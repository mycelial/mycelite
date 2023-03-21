use crate::error::Error;
use crate::{journal::DEFAULT_BUFFER_SIZE, BlobHeader, Header, SnapshotHeader};
use async_stream::try_stream;
use block::Block;

use futures::Stream;
use std::{path, pin::Pin};

use serde_sqlite::{from_bytes, to_bytes};

use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt, SeekFrom,
};

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Copy, Clone)]
pub struct AsyncJournal<F = tokio::fs::File>
where
    F: AsyncRead + AsyncWrite + AsyncSeek,
{
    /// Journal header
    header: Header,
    /// File
    fd: F,
    /// snapshot page count
    blob_count: Option<u32>,
    /// Buffer size
    buffer_sz: usize,
}

impl AsyncJournal<tokio::fs::File> {
    /// Create new journal
    pub async fn create<P: AsRef<path::Path>>(p: P) -> Result<Self> {
        let fd = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(p.as_ref())
            .await?;
        Self::new(Header::default(), fd, None).await
    }

    /// Try to instantiate journal from given path
    pub async fn try_from<P: AsRef<path::Path>>(p: P) -> Result<Self> {
        let mut fd = tokio::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(p)
            .await?;
        let header = Self::read_header(&mut fd).await?;
        Ok(Self::from(header, fd, None))
    }
}

impl<F: AsyncRead + AsyncWrite + AsyncSeek + std::marker::Unpin> AsyncJournal<F> {
    /// Instantiate journal & force header write
    pub async fn new(header: Header, mut fd: F, blob_count: Option<u32>) -> Result<Self> {
        Self::write_header(Box::pin(&mut fd), &header).await?;
        Ok(Self::from(header, fd, blob_count))
    }
    /// Instantiate journal
    pub fn from(header: Header, fd: F, blob_count: Option<u32>) -> Self {
        Self {
            header,
            blob_count,
            buffer_sz: DEFAULT_BUFFER_SIZE,
            fd,
        }
    }

    /// Set buffer size
    pub fn set_buffer_size(&mut self, buffer_sz: usize) {
        self.buffer_sz = buffer_sz;
    }

    /// Get buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer_sz
    }

    /// Initiate new snapshot
    ///
    /// * update journal header to correctly setup offset
    /// * to initiate snapshot we seek to current end of the file (value stored in header)
    /// * switch fd to buffered mode
    /// * write snapshot header with current header counter number
    pub async fn new_snapshot(&mut self, page_size: u32) -> Result<()> {
        if self.blob_count.is_some() {
            return Ok(());
        }
        self.update_header().await?;
        let snapshot_header = SnapshotHeader::new(
            self.header.snapshot_counter,
            chrono::Utc::now().timestamp_micros(),
            Some(page_size),
        );
        self.write_snapshot(&snapshot_header).await
    }

    /// Add new blob
    pub async fn new_blob(&mut self, offset: u64, blob: &[u8]) -> Result<()> {
        let blob_num = match self.blob_count {
            Some(c) => c,
            None => return Err(Error::SnapshotNotStarted),
        };
        let blob_header = BlobHeader::new(offset, blob_num, blob.len() as u32);
        self.add_blob(&blob_header, blob).await
    }

    /// Add blob
    pub async fn add_blob(&mut self, blob_header: &BlobHeader, blob: &[u8]) -> Result<()> {
        if Some(blob_header.blob_num) != self.blob_count {
            return Err(Error::OutOfOrderBlob {
                blob_num: blob_header.blob_num,
                blob_count: self.blob_count,
            });
        }
        self.blob_count.as_mut().map(|x| {
            *x += 1;
            *x
        });
        self.fd.write_all(&to_bytes(blob_header)?).await?;
        self.fd.write_all(blob).await?;
        Ok(())
    }

    pub async fn read_blob_header(&mut self) -> Result<BlobHeader> {
        let mut buf: Vec<u8> = Vec::with_capacity(BlobHeader::block_size());
        self.fd.read_buf(&mut buf).await?;
        from_bytes::<BlobHeader>(&buf).map_err(Into::into)
    }

    pub async fn read_blob(&mut self, size: u32) -> Result<Vec<u8>> {
        if size == 0 {
            let result: Vec<u8> = Vec::new();
            return Ok(result);
        }
        let mut buf: Vec<u8> = Vec::with_capacity(size as usize);
        self.fd.read_buf(&mut buf).await?;
        Ok(buf)
    }

    fn snapshot_started(&self) -> bool {
        self.blob_count.is_some()
    }

    /// Commit snapshot
    ///
    /// * write final empty page to indicate end of snapshot
    /// * flush bufwriter (seek() on BufWriter will force flush)
    /// * write new header
    /// * flush bufwriter
    /// * switch fd back to raw mode
    pub async fn commit(&mut self) -> Result<()> {
        if !self.snapshot_started() {
            return Ok(());
        }
        // commit snapshot by writting final empty page
        self.fd.write_all(&to_bytes(&BlobHeader::last())?).await?;
        self.blob_count = None;

        self.header.snapshot_counter += 1;
        self.header.eof = self.fd.stream_position().await?;

        Self::write_header(Box::pin(&mut self.fd), &self.header).await?;
        self.fd.flush().await?;
        Ok(())
    }

    /// Read header from a given fd
    ///
    /// * seek to start of the file
    /// * read header
    async fn read_header<R: AsyncRead + AsyncSeek + std::marker::Unpin>(
        fd: &mut R,
    ) -> Result<Header> {
        fd.rewind().await?;
        let mut buf = Vec::with_capacity(Header::block_size());
        fd.read_buf(&mut buf).await?;

        from_bytes::<Header>(&buf).map_err(Into::into)
        // from_reader(BufReader::new(fd)).map_err(Into::into).unwrap()
    }

    /// Write snapshot to journal
    ///
    /// This function assumes journal header is up to date
    async fn write_snapshot(&mut self, snapshot_header: &SnapshotHeader) -> Result<()> {
        if snapshot_header.id != self.header.snapshot_counter {
            return Err(Error::OutOfOrderSnapshot {
                snapshot_id: snapshot_header.id,
                journal_snapshot_id: self.header.snapshot_counter,
            });
        }
        self.fd.seek(SeekFrom::Start(self.header.eof)).await?;
        self.fd.write_all(&to_bytes(snapshot_header)?).await?;
        self.blob_count = Some(0);
        Ok(())
    }

    pub async fn read_snapshot(&mut self) -> Result<SnapshotHeader> {
        let mut buf = Vec::with_capacity(SnapshotHeader::block_size());
        self.fd.read_buf(&mut buf).await?;

        from_bytes::<SnapshotHeader>(&buf).map_err(Into::into)
    }

    /// Write header to a given fd
    ///
    /// * seek to start of the file
    /// * write header
    async fn write_header<W: AsyncWrite + AsyncSeek>(
        mut fd: Pin<Box<W>>,
        header: &Header,
    ) -> Result<()> {
        fd.seek(SeekFrom::Start(0)).await?;
        let x = to_bytes(header)?;
        fd.write_all(&x).await?;
        Ok(())
    }

    /// Return current snapshot counter
    pub async fn current_snapshot(&self) -> Option<u64> {
        match self.header.snapshot_counter {
            0 => None,
            v => Some(v),
        }
    }

    /// Update journal header
    pub async fn update_header(&mut self) -> Result<()> {
        let h = Self::read_header(&mut self.fd).await?;
        self.header = h;
        Ok(())
    }

    pub fn stream(
        &mut self,
    ) -> impl Stream<Item = Result<Option<(SnapshotHeader, BlobHeader, Vec<u8>)>>> + '_ {
        let mut initialized = false;
        let mut eoi = false;
        try_stream! {
            loop {
                // step 1: early exit
                if eoi {
                    yield None
                }
                // step 1: update header.
                if !initialized {
                    self.update_header().await?;
                    initialized = true;
                }

                // step 2: read snapshot header
                let snapshot_header = self.read_snapshot().await?;

                loop {
                    // step 3: read blob header
                    let blob_header = self.read_blob_header().await?;

                    if !blob_header.is_last() {
                        // step 4: read the blob bytes
                        let blob = self.read_blob(blob_header.blob_size).await?;

                        // step 5: yield the results
                        yield Some((snapshot_header, blob_header, blob))
                    } else {
                        if snapshot_header.id + 1 == self.header.snapshot_counter {
                            eoi = true;
                            yield None
                        } else {
                            break
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;

    #[tokio::test]
    async fn journal_create_works() {
        let future = AsyncJournal::create("/tmp/asdf.txt");
        let result = future.await;
        assert!(result.is_ok());
        let journal = result.unwrap();
        assert_eq!(journal.blob_count, None);
        assert_eq!(journal.header, Header::default());
    }

    async fn get_test_journal() -> AsyncJournal {
        let j = AsyncJournal::create("/tmp/asdf.txt").await;
        assert!(j.is_ok());
        let mut journal = j.unwrap();
        assert_eq!(journal.blob_count, None);
        assert_eq!(journal.header, Header::default());

        let result = journal.new_snapshot(10).await;
        assert!(result.is_ok());
        let result = journal.new_blob(1, &[1, 1, 1]).await;
        assert!(result.is_ok());
        assert_eq!(journal.blob_count, Some(1));
        let result = journal.new_blob(2, &[2, 2, 2]).await;
        assert!(result.is_ok());
        assert_eq!(journal.blob_count, Some(2));
        let result = journal.new_blob(3, &[3, 3, 3]).await;
        assert!(result.is_ok());
        assert_eq!(journal.blob_count, Some(3));
        let result = journal.new_blob(4, &[4, 4, 4]).await;
        assert!(result.is_ok());
        assert_eq!(journal.blob_count, Some(4));
        assert_eq!(journal.header, Header::default());

        let result = journal.commit().await;
        assert!(result.is_ok());

        let result = journal.new_snapshot(10).await;
        assert!(result.is_ok());
        let result = journal.new_blob(5, &[5, 5, 5]).await;
        assert!(result.is_ok());
        let result = journal.new_blob(6, &[6, 6, 6]).await;
        assert!(result.is_ok());

        let result = journal.commit().await;
        assert!(result.is_ok());
        journal
    }

    #[tokio::test]
    async fn journal_add_and_commit_works() {
        let result = AsyncJournal::create("/tmp/asdf.txt").await;
        assert!(result.is_ok());
        let mut journal = result.unwrap();
        assert_eq!(journal.blob_count, None);
        assert_eq!(journal.header, Header::default());

        let result = journal.new_snapshot(10).await;
        assert!(result.is_ok());
        let result = journal.new_blob(300, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]).await;
        assert!(result.is_ok());
        assert_eq!(journal.blob_count, Some(1));
        assert_eq!(journal.header, Header::default());

        let result = journal.commit().await;
        assert!(result.is_ok());
        assert_ne!(journal.header, Header::default());
    }
}
