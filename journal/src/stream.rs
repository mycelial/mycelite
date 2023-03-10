//! Streaming protocol for journal

use crate::error::Error as JournalError;
use crate::journal::{BlobHeader, IntoIter, Journal, SnapshotHeader};
use block::{block, Block};
use serde::{Deserialize, Serialize};
use serde_sqlite::to_writer;
use std::io::{BufRead, Cursor, Read, Seek, Write};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[block(0)]
pub struct End {}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[block]
pub enum Protocol {
    SnapshotHeader(SnapshotHeader),
    BlobHeader(BlobHeader),
    EndOfStream(End),
    JournalVersion(JournalVersion),
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::SnapshotHeader(_) => write!(f, "SnapshotHeader"),
            Self::BlobHeader(_) => write!(f, "BlobHeader"),
            Self::EndOfStream(_) => write!(f, "EndOfStream"),
            Self::JournalVersion(v) => write!(f, "JournalVersion({})", v.version),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
#[repr(transparent)]
#[block(4)]
pub struct JournalVersion {
    version: u32,
}

impl From<u32> for JournalVersion {
    fn from(version: u32) -> Self {
        Self { version }
    }
}

impl From<JournalVersion> for u32 {
    fn from(val: JournalVersion) -> Self {
        val.version
    }
}

impl From<SnapshotHeader> for Protocol {
    fn from(s: SnapshotHeader) -> Self {
        Self::SnapshotHeader(s)
    }
}

impl From<BlobHeader> for Protocol {
    fn from(p: BlobHeader) -> Self {
        Self::BlobHeader(p)
    }
}

impl From<JournalVersion> for Protocol {
    fn from(v: JournalVersion) -> Self {
        Self::JournalVersion(v)
    }
}

impl Protocol {
    fn end() -> Self {
        Self::EndOfStream(End {})
    }
}

#[derive(Debug)]
/// Converts iteration over journal into serialized Protocol stream
pub struct Stream<'a, I: Iterator<Item = <IntoIter<'a> as Iterator>::Item>> {
    iter: I,
    version: u32,
    version_written: bool,
    buf: Vec<u8>,
    read: usize,
    cur_snapshot_id: Option<u64>,
    finished: bool,
    _marker: std::marker::PhantomData<&'a ()>,
}

// stream, which starts from 'scratch'
impl<'a, F: Read + Write + Seek> From<&'a mut Journal<F>> for Stream<'a, IntoIter<'a, F>> {
    fn from(journal: &'a mut Journal<F>) -> Self {
        let version = journal.get_header().version;
        Stream::new(journal.into_iter(), version)
    }
}

// stream with any iterator with same Item type
impl<'a, I: Iterator<Item = <IntoIter<'a> as Iterator>::Item>> From<(u32, I)> for Stream<'a, I> {
    fn from((version, iter): (u32, I)) -> Self {
        Stream::new(iter, version)
    }
}

impl<'a, I: Iterator<Item = <IntoIter<'a> as Iterator>::Item>> Stream<'a, I> {
    pub fn new(iter: I, version: u32) -> Self {
        Self {
            iter,
            version,
            version_written: false,
            buf: Vec::with_capacity(8192),
            read: 0,
            cur_snapshot_id: None,
            finished: false,
            _marker: std::marker::PhantomData,
        }
    }

    fn to_io_error<E: Into<JournalError>>(e: E) -> std::io::Error {
        let e: JournalError = e.into();
        // FIXME: does it make sense to unwrap error?
        match e {
            JournalError::IOError(e) => e,
            JournalError::SerdeSqliteError(serde_sqlite::Error::IoError(e)) => e,
            e => std::io::Error::new(std::io::ErrorKind::Other, e),
        }
    }

    /// resize own buffer before writting new data chunk into it
    fn resize_buf(&mut self, len: usize) {
        if self.buf.capacity() < len {
            self.buf.reserve(len);
        }
        // *safe*:
        // * reserved for at least <len>
        // * used for writing data, no zeroing required
        unsafe { self.buf.set_len(len) };
    }
}

impl<'a, I: Iterator<Item = <IntoIter<'a> as Iterator>::Item>> BufRead for Stream<'a, I> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.read != self.buf.len() {
            return Ok(&self.buf[self.read..]);
        } else {
            self.read = 0;
            self.buf.clear();
        }

        // always write version first
        if !self.version_written {
            let version: Protocol = JournalVersion::from(self.version).into();
            self.resize_buf(version.iblock_size());
            to_writer(self.buf.as_mut_slice(), &version).map_err(Self::to_io_error)?;
            self.version_written = true;
            return Ok(self.buf.as_slice());
        }

        // body write
        match self.iter.next() {
            Some(Ok((snapshot_h, page_h, page))) => {
                let snapshot_id = snapshot_h.id;
                let snapshot_h: Protocol = snapshot_h.into();
                let page_h: Protocol = page_h.into();

                // max possible len for given item
                let total_len = snapshot_h.iblock_size() + page_h.iblock_size() + page.len();
                self.resize_buf(total_len);

                let mut read_buf = Cursor::new(self.buf.as_mut_slice());

                if self.cur_snapshot_id != Some(snapshot_id) {
                    to_writer(&mut read_buf, &snapshot_h).map_err(Self::to_io_error)?;
                    self.cur_snapshot_id = Some(snapshot_id)
                }
                to_writer(&mut read_buf, &page_h).map_err(Self::to_io_error)?;
                read_buf.write_all(page.as_slice())?;

                // real written value with according buffer resize
                let written = read_buf.position();
                self.resize_buf(written as usize);
            }
            Some(Err(e)) => return Err(Self::to_io_error(e)),
            None if !self.finished => {
                self.finished = true;

                let eos = Protocol::end();
                self.resize_buf(eos.iblock_size());
                to_writer(self.buf.as_mut_slice(), &eos).map_err(Self::to_io_error)?;
            }
            None => (),
        };
        Ok(self.buf.as_slice())
    }

    fn consume(&mut self, amn: usize) {
        self.read += amn
    }
}

impl<'a, I: Iterator<Item = <IntoIter<'a> as Iterator>::Item>> Read for Stream<'a, I> {
    fn read(&mut self, write_buf: &mut [u8]) -> std::io::Result<usize> {
        let mut total = 0;
        let mut write_buf_len = write_buf.len();
        let mut write_buf = Cursor::new(write_buf);
        loop {
            if write_buf_len == 0 {
                break;
            }
            let mut read_buf = self.fill_buf()?;
            if read_buf.is_empty() {
                break;
            }
            if read_buf.len() >= write_buf_len {
                read_buf = &read_buf[..write_buf_len];
            };
            let written = write_buf.write(read_buf)?;
            total += written;
            write_buf_len -= written;
            self.consume(written);
        }
        Ok(total)
    }
}
