//! Journal (v1)

use crate::error::Error;
use block::{block, Block};
use serde::{Deserialize, Serialize};
use serde_sqlite::{from_reader, to_bytes};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path;

pub(crate) const MAGIC: u32 = 0x00907A70;
const DEFAULT_BUFFER_SIZE: usize = 65536;

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Journal<F = fs::File>
where
    F: Read + Write + Seek,
{
    /// Journal header
    header: Header,
    /// Wrapped into Fd reader/writer/seeker
    fd: Fd<F, BufWriter<F>, BufReader<F>>,
    /// snapshot page count
    blob_count: Option<u32>,
    /// Buffer size
    buffer_sz: usize,
}

#[derive(Debug)]
enum Fd<F, W, R> {
    Raw(F),
    Writer(W),
    Reader(R),
    // placeholder state to aid fd mode switching
    Nada,
}

impl<F> Fd<F, BufWriter<F>, BufReader<F>>
where
    F: Read + Write + Seek,
{
    fn as_fd(&mut self) -> F {
        match std::mem::replace(self, Self::Nada) {
            Self::Reader(fd) => fd.into_inner(),
            Self::Writer(fd) => fd.into_parts().0,
            Self::Raw(fd) => fd,
            Self::Nada => unreachable!(),
        }
    }

    /// Swith Fd to 'raw' mode
    pub fn as_raw(&mut self) {
        let fd = self.as_fd();
        let _ = std::mem::replace(self, Fd::Raw(fd));
    }

    /// Switch Fd to buffered write mode
    pub fn as_writer(&mut self, buf_size: usize) {
        let fd = self.as_fd();
        // FIXME: re-use buffer
        let _ = std::mem::replace(self, Fd::Writer(BufWriter::with_capacity(buf_size, fd)));
    }

    /// Switch Fd to buffered read mode
    pub fn as_reader(&mut self, buf_size: usize) {
        let fd = self.as_fd();
        // FIXME: re-use buffer
        let _ = std::mem::replace(self, Fd::Reader(BufReader::with_capacity(buf_size, fd)));
    }
}

impl<F: Write, W: Write, R> Write for Fd<F, W, R> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Raw(fd) => fd.write(buf),
            Self::Writer(fd) => fd.write(buf),
            Self::Reader(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "can't write into fd in read mode",
            )),
            Self::Nada => unreachable!(),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Raw(fd) => fd.flush(),
            Self::Writer(fd) => fd.flush(),
            Self::Reader(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "can't flush fd in read mode",
            )),
            Self::Nada => unreachable!(),
        }
    }
}

impl<F: Read, W, R: Read> Read for Fd<F, W, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Raw(fd) => fd.read(buf),
            Self::Reader(fd) => fd.read(buf),
            Self::Writer(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "can't read from fd in write mode",
            )),
            Self::Nada => unreachable!(),
        }
    }
}

impl<F: Seek, W: Seek, R: Seek> Seek for Fd<F, W, R> {
    fn seek(&mut self, seek: SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::Raw(fd) => fd.seek(seek),
            Self::Reader(fd) => fd.seek(seek),
            Self::Writer(fd) => fd.seek(seek),
            Self::Nada => unreachable!(),
        }
    }
}

impl Journal<fs::File> {
    /// Create new journal
    pub fn create<P: AsRef<path::Path>>(p: P) -> Result<Self> {
        let fd = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(p.as_ref())?;
        Self::new(Header::default(), fd, None)
    }

    /// Try to instantiate journal from given path
    pub fn try_from<P: AsRef<path::Path>>(p: P) -> Result<Self> {
        let mut fd = fs::OpenOptions::new().write(true).read(true).open(p)?;
        let header = Self::read_header(&mut fd)?;
        Ok(Self::from(header, fd, None))
    }
}

impl<F: Read + Write + Seek> Journal<F> {
    /// Instantiate journal & force header write
    pub fn new(header: Header, mut fd: F, blob_count: Option<u32>) -> Result<Self> {
        Self::write_header(&mut fd, &header)?;
        Ok(Self::from(header, fd, blob_count))
    }

    /// Instantiate journal
    pub fn from(header: Header, fd: F, blob_count: Option<u32>) -> Self {
        Self {
            header,
            fd: Fd::Raw(fd),
            blob_count,
            buffer_sz: DEFAULT_BUFFER_SIZE,
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
    pub fn new_snapshot(&mut self, page_size: u32) -> Result<()> {
        println!("new_snapshot");
        if self.blob_count.is_some() {
            return Ok(());
        }
        self.update_header()?;
        let snapshot_header = SnapshotHeader::new(
            self.header.snapshot_counter,
            chrono::Utc::now().timestamp_micros(),
            Some(page_size),
        );
        self.write_snapshot(&snapshot_header)
    }

    /// Add new blob
    pub fn new_blob(&mut self, offset: u64, blob: &[u8]) -> Result<()> {
        println!("new_blob");
        let blob_num = match self.blob_count {
            Some(c) => c,
            None => return Err(Error::SnapshotNotStarted),
        };
        let blob_header = BlobHeader::new(offset, blob_num, blob.len() as u32);
        self.add_blob(&blob_header, blob)
    }

    /// Add existing snapshot
    ///
    /// Re-syncs journal header
    pub fn add_snapshot(&mut self, snapshot_header: &SnapshotHeader) -> Result<()> {
        println!("add_snapshot");
        self.update_header()?;
        self.write_snapshot(snapshot_header)
    }

    /// Write snapshot to journal
    ///
    /// This function assumes journal header is up to date
    fn write_snapshot(&mut self, snapshot_header: &SnapshotHeader) -> Result<()> {
        println!("write_snapshot");
        if snapshot_header.id != self.header.snapshot_counter {
            return Err(Error::OutOfOrderSnapshot {
                snapshot_id: snapshot_header.id,
                journal_snapshot_id: self.header.snapshot_counter,
            });
        }
        self.fd.seek(SeekFrom::Start(self.header.eof))?;
        self.fd.as_writer(self.buffer_sz);
        self.fd.write_all(&to_bytes(snapshot_header)?)?;
        self.blob_count = Some(0);
        Ok(())
    }

    /// Add blob
    pub fn add_blob(&mut self, blob_header: &BlobHeader, blob: &[u8]) -> Result<()> {
        println!("add_blob");
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
        self.fd.write_all(&to_bytes(blob_header)?)?;
        self.fd.write_all(blob)?;
        Ok(())
    }

    /// Commit snapshot
    ///
    /// * write final empty page to indicate end of snapshot
    /// * flush bufwriter (seek() on BufWriter will force flush)
    /// * write new header
    /// * flush bufwriter
    /// * switch fd back to raw mode
    pub fn commit(&mut self) -> Result<()> {
        println!("commit");
        if !self.snapshot_started() {
            return Ok(());
        }
        // commit snapshot by writting final empty page
        self.fd.write_all(&to_bytes(&BlobHeader::last())?)?;
        self.blob_count = None;

        self.header.snapshot_counter += 1;
        self.header.eof = self.fd.stream_position()?;

        Self::write_header(&mut self.fd, &self.header)?;
        self.fd.flush()?;
        self.fd.as_raw();
        Ok(())
    }

    /// Get journal header
    pub fn get_header(&self) -> &Header {
        &self.header
    }

    /// Return current snapshot counter
    pub fn current_snapshot(&self) -> Option<u64> {
        match self.header.snapshot_counter {
            0 => None,
            v => Some(v),
        }
    }

    /// Update journal header
    pub fn update_header(&mut self) -> Result<()> {
        println!("update_header");
        self.fd.as_reader(self.buffer_sz);
        self.header = Self::read_header(&mut self.fd)?;
        Ok(())
    }

    /// Read header from a given fd
    ///
    /// * seek to start of the file
    /// * read header
    fn read_header<R: Read + Seek>(fd: &mut R) -> Result<Header> {
        println!("read_header");
        fd.rewind()?;
        let header = from_reader(BufReader::new(fd)).map_err(Into::into);
        println!("{header:?}");
        header
    }

    /// Write header to a given fd
    ///
    /// * seek to start of the file
    /// * write header
    fn write_header<W: Write + Seek>(fd: &mut W, header: &Header) -> Result<()> {
        println!("write_header");
        fd.rewind()?;
        println!("{header:?}");
        fd.write_all(&to_bytes(header)?).map_err(Into::into)
    }

    /// Check if snapshot was already started
    fn snapshot_started(&self) -> bool {
        self.blob_count.is_some()
    }
}

#[derive(Debug)]
pub struct IntoIter<'a, F = fs::File>
where
    F: Read + Write + Seek,
{
    journal: &'a mut Journal<F>,
    current_snapshot: Option<SnapshotHeader>,
    initialized: bool,
    eoi: bool,
}

impl<'a, F: Write + Read + Seek> IntoIter<'a, F> {
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

impl<'a, F: Read + Write + Seek> IntoIterator for &'a mut Journal<F> {
    type IntoIter = IntoIter<'a, F>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter<'b>(self) -> Self::IntoIter {
        let eoi = self.header.snapshot_counter == 0;
        IntoIter {
            journal: self,
            initialized: false,
            current_snapshot: None,
            eoi,
        }
    }
}

impl<'a, F> Iterator for IntoIter<'a, F>
where
    F: Read + Write + Seek,
{
    type Item = Result<(SnapshotHeader, BlobHeader, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.initialized {
            if let Err(e) = self.journal.update_header() {
                self.eoi = true;
                return Some(Err(e));
            }
            match self
                .journal
                .fd
                .seek(SeekFrom::Start(Header::block_size() as u64))
            {
                Ok(_) => (),
                Err(e) => {
                    self.eoi = true;
                    return Some(Err(e.into()));
                }
            };
            self.journal.fd.as_reader(self.journal.buffer_sz);
            self.initialized = true;
        }
        if self.eoi {
            return None;
        }
        if self.current_snapshot.is_none() {
            self.current_snapshot = match from_reader::<SnapshotHeader, _>(&mut self.journal.fd) {
                Ok(s) => Some(s),
                Err(e) => {
                    self.eoi = true;
                    return Some(Err(e.into()));
                }
            };
        }
        let blob_header = match from_reader::<BlobHeader, _>(&mut self.journal.fd) {
            Ok(p) => p,
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
        match self.journal.fd.read_exact(buf.as_mut_slice()) {
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

/// Journal Header
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

/// Transaction Header
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
