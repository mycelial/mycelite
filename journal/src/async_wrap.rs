//! Temporary async wrapping to sync journal

use crate::{Error as JournalError, Journal, Protocol, Stream as JournalStream};
use serde_sqlite::de;
use std::io::{BufRead, Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc::{channel, error::TryRecvError, Receiver, Sender};

fn to_err<E: std::error::Error + Send + Sync + 'static>(err: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, err)
}

pub struct AsyncReadJournalStream {
    rx: Receiver<Waker>,
    tx: Sender<Vec<u8>>,
    snapshot_id: u64,
    journal: Journal,
}

impl AsyncReadJournalStream {
    pub fn new(
        journal_path: &str,
        snapshot_id: u64,
    ) -> Result<(Self, AsyncReadJournalStreamHandle), JournalError> {
        let journal = Journal::try_from(&journal_path)?;
        let (waker_tx, waker_rx) = channel::<Waker>(1);
        let (buffer_tx, buffer_rx) = channel::<Vec<u8>>(1);
        Ok((
            AsyncReadJournalStream {
                tx: buffer_tx,
                rx: waker_rx,
                journal,
                snapshot_id,
            },
            AsyncReadJournalStreamHandle {
                tx: waker_tx,
                rx: buffer_rx,
                buf: None,
                read: 0,
            },
        ))
    }

    pub fn enter_loop(mut self) {
        let mut stream =
            JournalStream::new(self.journal.into_iter().skip_snapshots(self.snapshot_id));

        while let Some(waker) = self.rx.blocking_recv() {
            let mut buf = Vec::<u8>::with_capacity(0x0001_0000); // 65kb buffer
            unsafe { buf.set_len(buf.capacity()) };
            let read = match stream.read(buf.as_mut_slice()) {
                Ok(read) => read,
                // FIXME: ?
                Err(_) => {
                    waker.wake();
                    return;
                }
            };
            unsafe { buf.set_len(read) };
            self.tx.blocking_send(buf).ok();
            waker.wake();
        }
    }
}

#[derive(Debug)]
pub struct AsyncReadJournalStreamHandle {
    buf: Option<Vec<u8>>,
    read: usize,
    rx: Receiver<Vec<u8>>,
    tx: Sender<Waker>,
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
                Ok(buf) if buf.len() == 0 => return Poll::Ready(Ok(())),
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
    W(Waker),
    B(Vec<u8>),
}

struct ReadReceiver {
    buf: Vec<u8>,
    buf_pos: usize,
    waker: Option<Waker>,
    rx: Receiver<AsyncWriteProto>,
    tx: Sender<()>,
}

impl ReadReceiver {
    fn new(rx: Receiver<AsyncWriteProto>, tx: Sender<()>) -> Self {
        Self {
            buf: vec![],
            buf_pos: 0,
            waker: None,
            rx,
            tx,
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
        // buffer request
        self.tx.try_send(()).ok();

        loop {
            // wake up future
            self.waker.take().map(|waker| waker.wake());
            match self.rx.blocking_recv() {
                Some(AsyncWriteProto::W(waker)) => {
                    self.waker = Some(waker);
                }
                Some(AsyncWriteProto::B(buf)) => {
                    self.buf = buf;
                    self.buf_pos = 0;
                    break;
                }
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

pub struct AsyncWriteJournalStream {
    journal: Journal,
    read_receiver: ReadReceiver,
}

impl AsyncWriteJournalStream {
    pub fn new(journal_path: &str) -> Result<(Self, AsyncWriteJournalStreamHandle), JournalError> {
        let journal = match Journal::try_from(&journal_path) {
            Ok(j) => j,
            Err(e) if e.journal_not_exists() => Journal::create(&journal_path)?,
            Err(e) => return Err(e),
        };
        let (buf_tx, buf_rx) = channel(2); // enough space to store waker and buf
        let (req_tx, req_rx) = channel(1);
        Ok((
            Self {
                journal,
                read_receiver: ReadReceiver::new(buf_rx, req_tx),
            },
            AsyncWriteJournalStreamHandle {
                tx: buf_tx,
                rx: req_rx,
            },
        ))
    }

    pub fn enter_loop(&mut self) -> std::io::Result<()> {
        loop {
            match de::from_reader::<Protocol, _>(&mut self.read_receiver) {
                Ok(Protocol::SnapshotHeader(snapshot_header)) => {
                    self.journal.commit().map_err(to_err)?;
                    self.journal
                        .add_snapshot(&snapshot_header)
                        .map_err(to_err)?;
                }
                Ok(Protocol::PageHeader(page_header)) => {
                    let mut page = vec![0; page_header.page_size as usize];
                    self.read_receiver
                        .read_exact(page.as_mut_slice())
                        .map_err(to_err)?;
                    self.journal
                        .add_page(&page_header, page.as_slice())
                        .map_err(to_err)?;
                }
                Ok(Protocol::EndOfStream(_)) => {
                    self.journal.commit().map_err(to_err)?;
                    return Ok(());
                }
                Err(e) => return Err(to_err(e)),
            }
        }
    }
}

#[derive(Debug)]
pub struct AsyncWriteJournalStreamHandle {
    tx: Sender<AsyncWriteProto>,
    rx: Receiver<()>,
}

impl AsyncWrite for AsyncWriteJournalStreamHandle {
    fn poll_write(
        self: Pin<&mut Self>,
        ctx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let me = self.get_mut();
        match me.rx.try_recv() {
            Err(TryRecvError::Empty) => {
                me.tx
                    .try_send(AsyncWriteProto::W(ctx.waker().clone()))
                    .map_err(to_err)?;
                Poll::Pending
            }
            Err(e @ TryRecvError::Disconnected) => Poll::Ready(Err(to_err(e))),
            Ok(_) => {
                // eh
                let len = buf.len();
                let buf: Vec<u8> = buf.into();
                match me.tx.try_send(AsyncWriteProto::B(buf)) {
                    Ok(_) => Poll::Ready(Ok(len)),
                    Err(e) => Poll::Ready(Err(to_err(e))),
                }
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
