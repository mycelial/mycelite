use crate::error::Error;
use crate::Header;
use block::{block, Block};
use std::{path, pin::Pin};

use serde::{Deserialize, Serialize};
use serde_sqlite::{from_bytes, to_bytes};

use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt, SeekFrom,
};

const DEFAULT_BUFFER_SIZE: usize = 65536;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[block(32)]
pub struct SnapshotHeader {
    pub id: u64,
    pub timestamp: i64,
    #[serde(
        serialize_with = "serde_sqlite::se::none_as_zero",
        deserialize_with = "serde_sqlite::de::zero_as_none"
    )]
    pub page_size: Option<u32>,
}

impl SnapshotHeader {
    pub fn new(id: u64, timestamp: i64, page_size: Option<u32>) -> Self {
        Self {
            id,
            timestamp,
            page_size,
        }
    }
}

/// Blob Header
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[block(16)]
pub struct BlobHeader {
    pub offset: u64,
    pub blob_num: u32,
    pub blob_size: u32,
}

impl BlobHeader {
    fn new(offset: u64, blob_num: u32, blob_size: u32) -> Self {
        Self {
            offset,
            blob_num,
            blob_size,
        }
    }

    // FIXME: should not be public
    pub fn last() -> Self {
        Self {
            offset: 0,
            blob_num: 0,
            blob_size: 0,
        }
    }

    // FIXME: should not be public
    pub fn is_last(&self) -> bool {
        self.offset == 0 && self.blob_num == 0 && self.blob_size == 0
    }
}
pub struct IntoIter<'a, F = tokio::fs::File>
where
    F: AsyncRead + AsyncWrite + AsyncSeek,
{
    journal: &'a mut AsyncJournal<F>,
    current_snapshot: Option<SnapshotHeader>,
    initialized: bool,
    eoi: bool,
    rt: tokio::runtime::Runtime,
}

impl<'a, F: AsyncRead + AsyncWrite + AsyncSeek + std::marker::Unpin> IntoIter<'a, F> {
    pub fn skip_snapshots(
        self,
        skip: u64,
    ) -> impl Iterator<Item = <IntoIter<'a, F> as Iterator>::Item> {
        self.filter(move |s| match s {
            Ok((ref snapshot_h, _, _)) => snapshot_h.id >= skip,
            _ => false,
        })
    }
}

impl<'a, F: AsyncRead + AsyncWrite + AsyncSeek + std::marker::Unpin> IntoIterator
    for &'a mut AsyncJournal<F>
{
    type IntoIter = IntoIter<'a, F>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter<'b>(self) -> Self::IntoIter {
        let eoi = self.header.snapshot_counter == 0;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        IntoIter {
            journal: self,
            initialized: false,
            current_snapshot: None,
            eoi,
            rt,
        }
    }
}

impl<'a, F> Iterator for IntoIter<'a, F>
where
    F: AsyncRead + AsyncWrite + AsyncSeek + std::marker::Unpin,
{
    type Item = Result<(SnapshotHeader, BlobHeader, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.initialized {
            if let Err(e) = self.rt.block_on(self.journal.update_header()) {
                self.eoi = true;
                return Some(Err(e));
            };

            match self.rt.block_on(
                self.journal
                    .fd
                    .seek(SeekFrom::Start(Header::block_size() as u64)),
            ) {
                Ok(_) => (),
                Err(e) => {
                    self.eoi = true;
                    return Some(Err(e.into()));
                }
            };
            self.initialized = true;
        }
        if self.eoi {
            return None;
        }
        if self.current_snapshot.is_none() {
            let mut buf: Vec<u8> = Vec::with_capacity(SnapshotHeader::block_size());
            let r = self.rt.block_on(self.journal.fd.read_buf(&mut buf));

            self.current_snapshot = match r {
                Ok(_) => {
                    let s = from_bytes::<SnapshotHeader>(&buf).unwrap();
                    Some(s)
                }
                Err(e) => {
                    self.eoi = true;
                    return Some(Err(e.into()));
                }
            };
        }
        let mut buf: Vec<u8> = Vec::with_capacity(BlobHeader::block_size());
        let r = self.rt.block_on(self.journal.fd.read_buf(&mut buf));

        let blob_header = match r {
            Ok(_) => {
                let b: BlobHeader = from_bytes::<BlobHeader>(&buf).unwrap();
                b
            }
            Err(e) => {
                self.eoi = true;
                return Some(Err(e.into()));
            }
        };
        if blob_header.is_last() {
            if self.current_snapshot.as_ref().unwrap().id + 1
                == self.journal.header.snapshot_counter
            {
                self.eoi = true;
                return None;
            } else {
                self.current_snapshot = None;
                return self.next();
            }
        }
        let mut buf = vec![];
        match buf.try_reserve(blob_header.blob_size as usize) {
            Ok(_) => (),
            Err(e) => {
                self.eoi = true;
                return Some(Err(e.into()));
            }
        }
        buf.resize(blob_header.blob_size as usize, 0);
        match self
            .rt
            .block_on(self.journal.fd.read_exact(buf.as_mut_slice()))
        {
            Ok(_) => (),
            Err(e) => {
                self.eoi = true;
                return Some(Err(e.into()));
            }
        }
        Some(Ok((
            self.current_snapshot.as_ref().unwrap().clone(),
            blob_header,
            buf,
        )))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn journal_create_works() {
        let rt = Runtime::new().unwrap();

        let result = rt.block_on(async {
            let future = AsyncJournal::create("/tmp/asdf.txt");
            let result = future.await;
            assert!(result.is_ok());
            let journal = result.unwrap();
            assert_eq!(journal.blob_count, None);
            assert_eq!(journal.header, Header::default());
        });
    }

    #[test]
    fn journal_add_and_commit_works() {
        let rt = Runtime::new().unwrap();

        let result = rt.block_on(async {
            let result = AsyncJournal::create("/tmp/asdf.txt").await;
            assert!(result.is_ok());
            let mut journal = result.unwrap();
            assert_eq!(journal.blob_count, None);
            assert_eq!(journal.header, Header::default());

            let result = journal.new_snapshot(10).await;
            assert!(result.is_ok());
            let result = journal.new_blob(300, &[0,1,2,3,4,5,6,7,8,9]).await;
            assert!(result.is_ok());
            assert_eq!(journal.blob_count, Some(1));
            assert_eq!(journal.header, Header::default());

            let result = journal.commit().await;
            assert!(result.is_ok());
            assert_ne!(journal.header, Header::default());

        });
        println!("{result:?}");
    }

}
