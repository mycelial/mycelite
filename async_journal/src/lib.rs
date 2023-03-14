use block::block;
use std::{path, pin::Pin};

use serde::{Deserialize, Serialize};
use serde_sqlite::{from_reader, to_bytes};
use tokio::runtime::Runtime;

use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, BufWriter, SeekFrom},
};

trait AsyncWriteAndSeekExts: AsyncWriteExt + AsyncSeekExt {}

impl<T: AsyncWriteExt + AsyncSeekExt> AsyncWriteAndSeekExts for T {}

const DEFAULT_BUFFER_SIZE: usize = 65536;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[block(128)]
pub struct Header {}

#[derive(Debug)]
enum Fd<F, W, R> {
    Raw(F),
    Writer(W),
    Reader(R),
    // placeholder state to aid fd mode switching
    Nada,
}

#[derive(Debug)]
pub struct AsyncJournal<F = tokio::fs::File>
where
    F: AsyncReadExt + AsyncWriteExt + AsyncSeekExt,
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
        Self::new(Header {}, fd, None).await
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

    /// Read header from a given fd
    ///
    /// * seek to start of the file
    /// * read header
    async fn read_header<R: AsyncReadExt + AsyncSeekExt + std::marker::Unpin>(
        fd: &mut R,
    ) -> Header {
        fd.rewind().await.unwrap();
        Header {}
        // from_reader(BufReader::new(fd)).map_err(Into::into).unwrap()
    }

    /// Write header to a given fd
    ///
    /// * seek to start of the file
    /// * write header
    async fn write_header<W: AsyncWriteAndSeekExts>(
        mut fd: Pin<Box<W>>,
        header: &Header,
    ) -> Result<(), std::io::Error> {
        fd.seek(SeekFrom::Start(0)).await?;
        let x = to_bytes(header).unwrap();
        fd.write_all(&x).await?;
        Ok(())
    }
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }

    #[test]
    fn journal_works() {
        let mut rt = Runtime::new().unwrap();

        // Call the asynchronous function using the `block_on` method
        let result = rt.block_on(async {
            let journal = AsyncJournal::create("/tmp/asdf.txt");
            let result = journal.await;
            assert_eq!(result.blob_count, None)
        });
        println!("{result:?}");

        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
