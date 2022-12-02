//! Journal (v1)

use crate::de;
use crate::se;
use block::{block, Block};
use chrono;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path;

pub(crate) const MAGIC: u32 = 0x00907A70;

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
    page_count: Option<u32>,
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
    fn to_fd(&mut self) -> F {
        match std::mem::replace(self, Self::Nada) {
            Self::Reader(fd) => fd.into_inner(),
            Self::Writer(fd) => fd.into_parts().0,
            Self::Raw(fd) => fd,
            Self::Nada => unreachable!(),
        }
    }

    /// Swith Fd to 'raw' mode
    pub fn to_raw(&mut self) {
        let fd = self.to_fd();
        let _ = std::mem::replace(self, Fd::Raw(fd));
    }

    /// Switch Fd to buffered write mode
    pub fn to_writer(&mut self) {
        let fd = self.to_fd();
        // FIXME: hardcoded buffer size (1 MB)
        // FIXME: buffer allocation is not checked
        let _ = std::mem::replace(self, Fd::Writer(BufWriter::with_capacity(0x100_000, fd)));
    }

    /// Switch Fd to buffered read mode
    pub fn to_reader(&mut self) {
        let fd = self.to_fd();
        // FIXME: hardcoded buffer size (1 MB)
        // FIXME: buffer capacity is not checked
        let _ = std::mem::replace(self, Fd::Reader(BufReader::with_capacity(0x100_000, fd)));
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

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

impl Journal<fs::File> {
    /// Create new journal
    pub fn create<P: AsRef<path::Path>>(p: P) -> Result<Self> {
        let mut fd = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(p.as_ref())?;
        let header = Header::default();
        Self::write_header(&mut fd, &header)?;
        Ok(Self {
            header,
            fd: Fd::Raw(fd),
            page_count: None,
        })
    }

    /// Try to instantiate journal from given path
    ///
    /// FIXME: move to TryFrom trait impl
    pub fn try_from<P: AsRef<path::Path>>(p: P) -> Result<Self> {
        let mut fd = fs::OpenOptions::new().write(true).read(true).open(p)?;
        let header = Self::read_header(&mut fd)?;
        Ok(Self {
            header,
            fd: Fd::Raw(fd),
            page_count: None,
        })
    }
}

impl<F: Read + Write + Seek> Journal<F> {
    /// Initiate snapshot
    ///
    /// * to initiate snapshot we seek to current end of the file (value stored in header)
    /// * switch fd to buffered mode
    /// * write snapshot header with current header counter number
    pub fn snapshot(&mut self) -> Result<()> {
        if self.page_count.is_some() {
            return Ok(());
        }
        self.fd.seek(SeekFrom::Start(self.header.eof))?;
        self.fd.to_writer();
        self.fd.write(&se::to_bytes(&SnapshotHeader::new(
            self.header.snapshot_counter,
            chrono::Utc::now().timestamp_micros(),
        ))?)?;
        self.page_count = Some(0);
        Ok(())
    }

    /// Add sqlite page
    ///
    /// Automatically starts new snapshot if there is none
    pub fn add_page(&mut self, offset: u64, page: &[u8]) -> Result<()> {
        if !self.snapshot_started() {
            self.snapshot()?;
        };
        let page_num = self.page_count.unwrap();
        self.page_count.as_mut().map(|x| *x += 1);
        self.fd.write_all(&se::to_bytes(&PageHeader::new(
            offset,
            page_num,
            page.len() as u32,
        ))?)?;
        self.fd.write_all(page)?;
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
        if !self.snapshot_started() {
            return Ok(());
        }
        // commit snapshot by writting final empty page
        self.fd.write_all(&se::to_bytes(&PageHeader::last())?)?;
        self.page_count = None;

        self.header.snapshot_counter += 1;
        self.header.eof = self.fd.seek(SeekFrom::Current(0))?;

        Self::write_header(&mut self.fd, &self.header)?;
        self.fd.flush()?;
        self.fd.to_raw();
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

    // FIXME: this function was added as a hack to speedup process of demo development
    // remove it once it's not needed anymore
    pub fn update_header(&mut self) -> Result<()> {
        self.fd.to_reader();
        self.header = Self::read_header(&mut self.fd)?;
        Ok(())
    }

    /// Read header from a given fd
    ///
    /// * seek to start of the file
    /// * read header
    fn read_header<R: Read + Seek>(fd: &mut R) -> Result<Header> {
        fd.seek(SeekFrom::Start(0))?;
        de::from_reader(BufReader::new(fd)).map_err(Into::into)
    }

    /// Write header to a given fd
    ///
    /// * seek to start of the file
    /// * write header
    fn write_header<W: Write + Seek>(fd: &mut W, header: &Header) -> Result<()> {
        fd.seek(SeekFrom::Start(0))?;
        fd.write_all(&se::to_bytes(header)?).map_err(Into::into)
    }

    /// Check if snapshot was already started
    fn snapshot_started(&self) -> bool {
        self.page_count.is_some()
    }
}

#[derive(Debug)]
pub struct IntoIter<'a> {
    journal: &'a mut Journal,
    current_snapshot: Option<SnapshotHeader>,
    eoi: bool,
}

impl<'a> IntoIterator for &'a mut Journal {
    type IntoIter = IntoIter<'a>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter<'b>(self) -> Self::IntoIter {
        // offset to first snapshot
        self.fd
            .seek(SeekFrom::Start(Header::block_size() as u64))
            .ok();
        self.fd.to_reader();
        let eoi = self.header.snapshot_counter == 0;
        IntoIter {
            journal: self,
            current_snapshot: None,
            eoi,
        }
    }
}

impl<'a> Iterator for IntoIter<'a> {
    type Item = Result<(SnapshotHeader, PageHeader, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.eoi {
            return None;
        }
        if self.current_snapshot.is_none() {
            self.current_snapshot = match de::from_reader::<SnapshotHeader, _>(&mut self.journal.fd)
            {
                Ok(s) => Some(s),
                Err(e) => {
                    self.eoi = true;
                    return Some(Err(e.into()));
                }
            };
        }
        let page_header = match de::from_reader::<PageHeader, _>(&mut self.journal.fd) {
            Ok(p) => p,
            Err(e) => {
                self.eoi = true;
                return Some(Err(e.into()));
            }
        };
        if page_header.is_last() {
            if self.current_snapshot.as_ref().unwrap().num + 1
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
        match buf.try_reserve(page_header.page_size as usize) {
            Ok(_) => (),
            Err(e) => {
                self.eoi = true;
                return Some(Err(e.into()));
            }
        }
        buf.resize(page_header.page_size as usize, 0);
        match self.journal.fd.read_exact(buf.as_mut_slice()) {
            Ok(_) => (),
            Err(e) => {
                self.eoi = true;
                return Some(Err(e.into()));
            }
        }
        Some(Ok((
            self.current_snapshot.as_ref().unwrap().clone(),
            page_header,
            buf,
        )))
    }
}

/// Journal Header
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub num: u64,
    pub timestamp: i64,
}

impl SnapshotHeader {
    pub fn new(num: u64, timestamp: i64) -> Self {
        Self { num, timestamp }
    }
}

/// Page Header
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[block(16)]
pub struct PageHeader {
    pub offset: u64,
    pub page_num: u32,
    pub page_size: u32,
}

impl PageHeader {
    fn new(offset: u64, page_num: u32, page_size: u32) -> Self {
        Self {
            offset,
            page_num,
            page_size,
        }
    }

    // FIXME: should not be public
    pub fn last() -> Self {
        Self {
            offset: 0,
            page_num: 0,
            page_size: 0,
        }
    }

    // FIXME: should not be public
    pub fn is_last(&self) -> bool {
        self.offset == 0 && self.page_num == 0 && self.page_size == 0
    }
}
