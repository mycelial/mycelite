//! Temporary async wrapping to sync journal

use crate::{Error as JournalError, Journal, Protocol, Stream as JournalStream};
use serde_sqlite::de;
use tokio::sync::mpsc::error::TrySendError;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc::{channel, error::TryRecvError, Receiver, Sender};

fn to_err<E: std::error::Error + Send + Sync + 'static>(err: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, err)
}

pub struct AsyncReadJournalStream {
    snapshot_id: u64,
    journal_path: PathBuf,
}

impl AsyncReadJournalStream {
    pub fn new<P: Into<std::path::PathBuf>>(journal_path: P, snapshot_id: u64) -> Self {
        AsyncReadJournalStream {
            journal_path: journal_path.into(),
            snapshot_id,
        }
    }

    pub fn spawn(self) -> AsyncReadJournalStreamHandle {
        let (waker_tx, mut waker_rx) = channel::<Waker>(1);
        let (mut buffer_tx, buffer_rx) = channel::<Vec<u8>>(1);
        let join_handle =
            tokio::task::spawn_blocking(move || self.enter_loop(&mut waker_rx, &mut buffer_tx));
        AsyncReadJournalStreamHandle {
            tx: waker_tx,
            rx: buffer_rx,
            buf: None,
            read: 0,
            join_handle,
        }
    }

    pub fn enter_loop(
        self,
        rx: &mut Receiver<Waker>,
        tx: &mut Sender<Vec<u8>>,
    ) -> Result<(), JournalError> {
        let mut journal = Journal::try_from(self.journal_path.as_path())?;
        let version = journal.get_header().version;
        let mut stream = JournalStream::new(
            journal.into_iter().skip_snapshots(self.snapshot_id),
            version,
        );

        while let Some(waker) = rx.blocking_recv() {
            let mut buf = Vec::<u8>::with_capacity(0x0001_0000); // 65kb buffer
            unsafe { buf.set_len(buf.capacity()) };
            let read = match stream.read(buf.as_mut_slice()) {
                Ok(read) => read,
                Err(e) => {
                    waker.wake();
                    return Err(e.into());
                }
            };
            unsafe { buf.set_len(read) };
            let res = tx.blocking_send(buf);
            waker.wake();
            if let Err(tokio::sync::mpsc::error::SendError(_)) = res {
                let err = std::io::Error::new(std::io::ErrorKind::Other, "channel closed");
                return Err(err.into());
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct AsyncReadJournalStreamHandle {
    buf: Option<Vec<u8>>,
    read: usize,
    rx: Receiver<Vec<u8>>,
    tx: Sender<Waker>,
    join_handle: tokio::task::JoinHandle<Result<(), JournalError>>,
}

impl AsyncReadJournalStreamHandle {
    pub async fn join(self) -> Result<Result<(), JournalError>, tokio::task::JoinError> {
        self.join_handle.await
    }
}

impl AsyncRead for AsyncReadJournalStreamHandle {
    fn poll_read(
        self: Pin<&mut Self>,
        ctx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let p = self.get_mut();
        if p.buf.is_none() {
            match p.rx.try_recv() {
                // EOF
                Ok(buf) if buf.is_empty() => return Poll::Ready(Ok(())),
                Ok(buf) => {
                    p.buf = Some(buf);
                    p.read = 0;
                }
                // stream thread quit, FIXME: distinction between thread error and EOF
                Err(TryRecvError::Disconnected) => return Poll::Ready(Ok(())),
                Err(TryRecvError::Empty) => {
                    p.tx.try_send(ctx.waker().clone()).map_err(to_err)?;
                    return Poll::Pending;
                }
            }
        }

        let inner_buf = p.buf.as_ref().unwrap();
        let start = p.read;
        let end = p.read + buf.remaining();
        match inner_buf.len() {
            len if len == start => {
                // inner buf was read to the end
                p.buf = None;
                p.tx.try_send(ctx.waker().clone()).map_err(to_err)?;
                Poll::Pending
            }
            len if len > end => {
                // inner buf have enough data to fill incoming buf to the end
                let slice = &inner_buf[start..end];
                p.read = end;
                buf.put_slice(slice);
                Poll::Ready(Ok(()))
            }
            len => {
                // inner buf doesn't have enough data, to fill incoming buffer completely
                let slice = &inner_buf[p.read..];
                p.read = len;
                buf.put_slice(slice);
                Poll::Ready(Ok(()))
            }
        }
    }
}

#[derive(Debug)]
enum AsyncWriteProto {
    WriteBuf(Vec<u8>, Waker),
    Shutdown(Waker),
}

pub struct ReadReceiver {
    buf: Vec<u8>,
    buf_pos: usize,
    waker: Option<Waker>,
    rx: Receiver<AsyncWriteProto>,
}

impl ReadReceiver {
    fn new(rx: Receiver<AsyncWriteProto>) -> Self {
        Self {
            buf: vec![],
            buf_pos: 0,
            waker: None,
            rx,
        }
    }
}

impl BufRead for ReadReceiver {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.buf_pos != self.buf.len() {
            return Ok(&self.buf[self.buf_pos..]);
        } else {
            self.buf_pos = 0;
            self.buf.clear();
        }

        loop {
            match self.rx.blocking_recv() {
                Some(AsyncWriteProto::WriteBuf(buf, waker)) => {
                    waker.wake();
                    self.buf = buf;
                    self.buf_pos = 0;
                    break;
                },
                Some(AsyncWriteProto::Shutdown(waker)) => {
                    self.waker = Some(waker);
                    break;
                },
                None => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "channel closed",
                    ))
                }
            }
        }
        Ok(self.buf.as_slice())
    }

    fn consume(&mut self, read: usize) {
        self.buf_pos += read;
    }
}

impl Read for ReadReceiver {
    fn read(&mut self, write_buf: &mut [u8]) -> std::io::Result<usize> {
        let mut total = 0;
        let mut write_buf_len = write_buf.len();
        let mut write_buf = std::io::Cursor::new(write_buf);
        loop {
            if write_buf_len == 0 {
                break;
            };
            let mut read_buf = self.fill_buf()?;
            if read_buf.is_empty() {
                break;
            }
            if read_buf.len() >= write_buf_len {
                read_buf = &read_buf[..write_buf_len];
            }
            let written = write_buf.write(read_buf)?;
            total += written;
            write_buf_len -= written;
            self.consume(written)
        }
        Ok(total)
    }
}

impl Drop for ReadReceiver {
    fn drop(&mut self) {
        self.rx.close();
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        while let Ok(message) = self.rx.try_recv() {
            match message {
                AsyncWriteProto::WriteBuf(_buf, waker) => waker.wake(),
                AsyncWriteProto::Shutdown(waker) => waker.wake(),
            }
        }
    }
}

pub struct AsyncWriteJournalStream {
    journal_path: PathBuf,
}

impl AsyncWriteJournalStream {
    pub fn new<P: Into<PathBuf>>(journal_path: P) -> Self {
        Self {
            journal_path: journal_path.into(),
        }
    }

    pub fn spawn(mut self) -> AsyncWriteJournalStreamHandle {
        let (tx, rx) = channel(1); // enough space to store waker and buf
        let read_receiver = ReadReceiver::new(rx);
        let join_handle = tokio::task::spawn_blocking(move || self.enter_loop(read_receiver));
        AsyncWriteJournalStreamHandle { tx, join_handle }
    }

    pub fn enter_loop(&mut self, mut read_receiver: ReadReceiver) -> Result<(), JournalError> {
        let mut journal = match Journal::try_from(self.journal_path.as_path()) {
            Ok(j) => j,
            Err(e) if e.journal_not_exists() => Journal::create(self.journal_path.as_path())?,
            Err(e) => return Err(e),
        };

        let expected = Protocol::JournalVersion(1.into());
        match de::from_reader::<Protocol, _>(&mut read_receiver).map_err(to_err)? {
            msg if msg == expected => (),
            other => {
                let err = std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("expected {}, got: {}", expected, other),
                );
                return Err(err.into());
            }
        }
        loop {
            match de::from_reader::<Protocol, _>(&mut read_receiver).map_err(to_err)? {
                Protocol::SnapshotHeader(snapshot_header) => {
                    journal.commit().map_err(to_err)?;
                    journal.add_snapshot(&snapshot_header).map_err(to_err)?;
                }
                Protocol::BlobHeader(blob_header) => {
                    let mut blob = vec![0; blob_header.blob_size as usize];
                    read_receiver
                        .read_exact(blob.as_mut_slice())
                        .map_err(to_err)?;
                    journal
                        .add_blob(&blob_header, blob.as_slice())
                        .map_err(to_err)?;
                }
                Protocol::EndOfStream(_) => {
                    journal.commit().map_err(to_err)?;
                    drop(journal);
                    return Ok(());
                }
                msg => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("unexpected message: {msg:?}"),
                    )
                    .into())
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct AsyncWriteJournalStreamHandle {
    tx: Sender<AsyncWriteProto>,
    join_handle: tokio::task::JoinHandle<Result<(), JournalError>>,
}

impl AsyncWriteJournalStreamHandle {
    pub async fn join(self) -> Result<Result<(), JournalError>, tokio::task::JoinError> {
        self.join_handle.await
    }
}

impl AsyncWrite for AsyncWriteJournalStreamHandle {
    fn poll_write(
        self: Pin<&mut Self>,
        ctx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let me = self.get_mut();
        match me.tx.try_send(AsyncWriteProto::WriteBuf(buf.into(), ctx.waker().clone())) {
            Ok(_) => Poll::Ready(Ok(buf.len())),
            Err(TrySendError::Full(_)) => Poll::Pending,
            Err(e@TrySendError::Closed(_)) => Poll::Ready(Err(to_err(e))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let me = self.get_mut();
        match me.tx.try_send(AsyncWriteProto::Shutdown(ctx.waker().clone())) {
            Ok(_) => Poll::Pending,
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                Poll::Pending
            },
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                Poll::Ready(Ok(()))
            }
        }
    }
}
