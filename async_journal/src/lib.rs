use block::block;
use std::{path, pin::Pin};

use serde::{Deserialize, Serialize};
use serde_sqlite::to_bytes;

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};

type Result<T> = std::result::Result<T, Error>;

pub(crate) const MAGIC: u32 = 0x00907A70;
const DEFAULT_BUFFER_SIZE: usize = 65536;

impl Default for Header {
    fn default() -> Self {
        Self {
            magic: MAGIC,
            version: 1,
            snapshot_counter: 0,
            eof: <Self as block::Block>::block_size() as u64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[block(128)]
pub struct Header {
    /// magic header
    pub magic: u32,
    /// journal version
    pub version: u32,
    /// operation counter
    pub snapshot_counter: u64,
    /// end of last snapshot
    pub eof: u64,
}

use serde_sqlite::Error as SerdeSqliteError;
use std::collections::TryReserveError;
use std::io::Error as IOError;

#[derive(Debug)]
pub enum Error {
    /// std::io::Error
    IOError(IOError),
    /// std::collections::TryReserveError
    TryReserveError(TryReserveError),
    /// serde_sqlite error
    SerdeSqliteError(SerdeSqliteError),
    /// attemt to add out of order snapshot
    OutOfOrderSnapshot {
        snapshot_id: u64,
        journal_snapshot_id: u64,
    },
    /// Snapshot not started
    SnapshotNotStarted,
    /// Attemt to add out of order blob
    OutOfOrderBlob {
        blob_num: u32,
        blob_count: Option<u32>,
    },
    /// Unexpected Journal Version
    UnexpectedJournalVersion { expected: u32, got: u32 },
}

impl From<IOError> for Error {
    fn from(e: IOError) -> Self {
        Self::IOError(e)
    }
}

impl From<TryReserveError> for Error {
    fn from(e: TryReserveError) -> Self {
        Self::TryReserveError(e)
    }
}

impl From<SerdeSqliteError> for Error {
    fn from(e: SerdeSqliteError) -> Self {
        Self::SerdeSqliteError(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Check if error caused by absense of journal
    pub fn journal_not_exists(&self) -> bool {
        match self {
            Self::IOError(e) => e.kind() == std::io::ErrorKind::NotFound,
            _ => false,
        }
    }
}
#[derive(Debug)]
pub struct AsyncJournal<F = tokio::fs::File>
where
    F: AsyncReadExt + AsyncWriteExt + AsyncSeekExt,
{
    /// Journal header
    header: Header,
    /// Wrapped into Fd reader/writer/seeker
    // fd: Fd<F, BufWriter<F>, BufReader<F>>,
    /// snapshot page count
    blob_count: Option<u32>,
    /// Buffer size
    buffer_sz: usize,

    /// async f? fd?
    async_f: F,
}

impl AsyncJournal<tokio::fs::File> {
    /// Create new journal
    pub async fn create<P: AsRef<path::Path>>(p: P) -> Self {
        let fd = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(p.as_ref())
            .await
            .unwrap();
        // Self::new(Header::default(), fd, None)
        Self::new(
            Header {
                magic: 0,
                eof: 0,
                version: 0,
                snapshot_counter: 0,
            },
            fd,
            None,
        )
        .await
    }

    /// Try to instantiate journal from given path
    pub async fn try_from<P: AsRef<path::Path>>(p: P) -> Self {
        let mut fd = tokio::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(p)
            .await
            .unwrap();
        let header = Self::read_header(&mut fd).await;
        Self::from(header, fd, None)
    }
}

impl<F: AsyncReadExt + AsyncWriteExt + AsyncSeekExt + std::marker::Unpin> AsyncJournal<F> {
    /// Instantiate journal & force header write
    pub async fn new(header: Header, mut fd: F, blob_count: Option<u32>) -> Self {
        Self::write_header(Box::pin(&mut fd), &header)
            .await
            .unwrap();
        Self::from(header, fd, blob_count)
    }
    /// Instantiate journal
    pub fn from(header: Header, fd: F, blob_count: Option<u32>) -> Self {
        Self {
            header,
            blob_count,
            buffer_sz: DEFAULT_BUFFER_SIZE,
            async_f: fd,
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
        self.async_f.write_all(&to_bytes(blob_header)?).await?;
        self.async_f.write_all(blob).await?;
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
        self.async_f
            .write_all(&to_bytes(&BlobHeader::last())?)
            .await?;
        self.blob_count = None;

        self.header.snapshot_counter += 1;
        self.header.eof = self.async_f.stream_position().await?;

        Self::write_header(Box::pin(&mut self.async_f), &self.header).await?;
        self.async_f.flush().await?;
        // self.async_f.as_raw();
        Ok(())
    }

    /// Read header from a given fd
    ///
    /// * seek to start of the file
    /// * read header
    async fn read_header<R: AsyncReadExt + AsyncSeekExt + std::marker::Unpin>(
        fd: &mut R,
    ) -> Header {
        fd.rewind().await.unwrap();
        Header {
            magic: 0,
            eof: 0,
            version: 0,
            snapshot_counter: 0,
        }
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
        self.async_f.seek(SeekFrom::Start(self.header.eof)).await?;
        // self.async_f.as_writer(self.buffer_sz);
        self.async_f.write_all(&to_bytes(snapshot_header)?).await?;
        self.blob_count = Some(0);
        Ok(())
    }

    /// Write header to a given fd
    ///
    /// * seek to start of the file
    /// * write header
    async fn write_header<W: AsyncWriteExt + AsyncSeekExt>(
        mut fd: Pin<Box<W>>,
        header: &Header,
    ) -> Result<()> {
        fd.seek(SeekFrom::Start(0)).await?;
        let x = to_bytes(header).unwrap();
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
        // self.async_f.as_reader(self.buffer_sz);
        let h = Self::read_header(&mut self.async_f).await;
        self.header = h;
        Ok(())
    }
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }

    #[test]
    fn journal_works() {
        let rt = Runtime::new().unwrap();

        // Call the asynchronous function using the `block_on` method
        let result = rt.block_on(async {});
        println!("{result:?}");
    }

    #[test]
    fn journal_create_works() {
        let rt = Runtime::new().unwrap();

        // Call the asynchronous function using the `block_on` method
        let result = rt.block_on(async {
            let journal = AsyncJournal::create("/tmp/asdf.txt");
            let result = journal.await;
            println!("{result:?}");
            assert_eq!(result.blob_count, None);
        });
        println!("{result:?}");
    }
}
