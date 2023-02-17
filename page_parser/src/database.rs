//! Sqlite Database
use crate::header::Header;
use crate::page::RawPage;
use serde_sqlite::from_bytes;
use std::io::BufReader;
use std::io::{Read, Seek};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn new<P: Into<PathBuf>>(p: P) -> Self {
        Self { path: p.into() }
    }

    /// Initialize iterator over raw sqlite pages
    pub fn into_raw_page_iter(&self) -> Result<RawPageIter, Box<dyn std::error::Error>> {
        let mut fd = std::fs::OpenOptions::new()
            .read(true)
            .open(self.path.as_path())?;
        let db_size = fd.metadata()?.len();
        let (page_size, pages_left) = match db_size {
            0 => (0, 0),
            _ => {
                let mut buf = [0_u8; 100];
                fd.read_exact(buf.as_mut_slice())?;
                let header = from_bytes::<Header>(buf.as_slice())?;
                let page_size = header.page_size() as u64;
                (page_size, db_size / page_size)
            }
        };
        fd.rewind()?;
        Ok(RawPageIter {
            fd: BufReader::new(fd),
            page_size,
            pages_left,
        })
    }
}

#[derive(Debug)]
pub struct RawPageIter {
    // for now only file iter, but in-memory option also can be supported
    fd: BufReader<std::fs::File>,
    page_size: u64,
    pages_left: u64,
}

impl Iterator for RawPageIter {
    type Item = Result<(u64, RawPage), std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pages_left == 0 {
            return None;
        };
        self.pages_left -= 1;
        let offset = match self.fd.stream_position() {
            Err(e) => return Some(Err(e)),
            Ok(offset) => offset,
        };
        let mut page = vec![0; self.page_size as usize];
        match self.fd.read_exact(page.as_mut_slice()) {
            Ok(_) => Some(Ok((offset, RawPage::new(page)))),
            Err(e) => {
                self.pages_left = 0;
                Some(Err(e))
            }
        }
    }
}
