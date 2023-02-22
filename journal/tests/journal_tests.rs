use block::Block;
use journal::{Header, Journal, Protocol, Stream};
use quickcheck::{quickcheck, Arbitrary, Gen, TestResult};
use spin_sleep::sleep;
use std::cell::UnsafeCell;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn test_journal_not_exists() {
    // create named temp file and delete
    let name = &tempfile::NamedTempFile::new().unwrap();
    std::fs::remove_file(name).unwrap();
    let res = Journal::try_from(name);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.journal_not_exists());
}

#[derive(Debug, Clone, PartialEq)]
struct TestPage {
    offset: u64,
    data: Vec<u8>,
}

impl Arbitrary for TestPage {
    fn arbitrary(gen: &mut Gen) -> Self {
        Self {
            offset: u64::arbitrary(gen),
            data: Vec::<u8>::arbitrary(gen),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TestSnapshot {
    pages: Vec<TestPage>,
}

impl Arbitrary for TestSnapshot {
    fn arbitrary(gen: &mut Gen) -> Self {
        // limit min/max pages per snapshot
        let page_count = 1 + usize::arbitrary(gen) % 49;
        let pages = (0..page_count)
            .enumerate()
            .fold(vec![], |mut acc, (pos, _)| {
                let mut page = TestPage::arbitrary(gen);
                // *edge case*
                // quickcheck is able to quickly find a way to insert 'last page' as a first page of snapshot
                // last page is a page where all values are set to 0 and technically it's not possible
                // to insert such page from sqlite calls
                // for now we just override such scenario, but pages with zero sizes are still part of
                // the test case, even though empty page as a concept doesn't make sense.
                if pos == 0 && page.data.is_empty() {
                    page.data = vec![0];
                }
                acc.push(page);
                acc
            });
        TestSnapshot { pages }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            self.pages
                .shrink()
                .filter(|pages| !pages.is_empty()) // snapshot with no pages is not valid input
                .map(|pages| TestSnapshot { pages }),
        )
    }
}

#[test]
fn test_journal_snapshotting() {
    fn check(input: Vec<TestSnapshot>) {
        let mut journal = Journal::new(Header::default(), Cursor::new(vec![]), None).unwrap();
        for snapshot in input.iter() {
            for page in snapshot.pages.iter() {
                journal.new_page(page.offset, page.data.as_slice()).unwrap();
            }
            journal.commit().unwrap();
        }
        // iteration over journal always should return same input
        let restored_input = (&mut journal)
            .into_iter()
            .map(Result::unwrap)
            .fold(
                (vec![], None),
                |(mut acc, mut snapshot_id), (snapshot_h, page_h, page)| {
                    if snapshot_id != Some(snapshot_h.id) {
                        snapshot_id = Some(snapshot_h.id);
                        acc.push(TestSnapshot { pages: vec![] });
                    };
                    acc.last_mut().unwrap().pages.push(TestPage {
                        offset: page_h.offset,
                        data: page,
                    });
                    (acc, snapshot_id)
                },
            )
            .0;
        assert_eq!(restored_input, input);
    }
    quickcheck(check as fn(Vec<TestSnapshot>));
}

#[derive(Debug, Clone)]
struct XorShift {
    state: u64,
}

impl XorShift {
    fn new(seed: u64) -> Self {
        // seed should never be zero
        Self { state: seed.max(1) }
    }

    fn next(&mut self) -> u64 {
        let mut s = self.state;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.state = s;
        s
    }
}

impl Arbitrary for XorShift {
    fn arbitrary(g: &mut Gen) -> Self {
        Self::new(u64::arbitrary(g))
    }
}

// test journal serialization into Protocol stream (from scratch)
#[test]
fn test_journal_stream() {
    fn check(input: Vec<TestSnapshot>, mut prng: XorShift) -> TestResult {
        let mut journal = Journal::new(Header::default(), Cursor::new(vec![]), None).unwrap();
        let mut expected_len = 4; // end of stream
        for snapshot in input.iter() {
            expected_len += journal::SnapshotHeader::block_size() + 4;
            for page in snapshot.pages.iter() {
                expected_len += journal::PageHeader::block_size() + 4 + page.data.len();
                journal.new_page(page.offset, page.data.as_slice()).unwrap();
            }
            journal.commit().unwrap();
        }

        let mut stream: Stream<_> = Stream::from(&mut journal);
        let mut writer = Cursor::new(vec![]);
        loop {
            let buf_size = (prng.next() % 100) as usize;
            // intermidiate buffer of variable size, including 0
            let mut buf = vec![0; buf_size];
            let read = stream.read(&mut buf).unwrap();
            if read == 0 && buf_size != 0 {
                break;
            }
            writer.write_all(&buf[..read]).unwrap();
        }
        let buf = writer.into_inner();
        if expected_len != buf.len() {
            return TestResult::error(format!(
                "expected len: {}, got: {}",
                expected_len,
                buf.len()
            ));
        }

        let mut reader = Cursor::new(buf.as_slice());
        let mut expected = vec![];
        loop {
            match serde_sqlite::from_reader::<Protocol, _>(&mut reader) {
                Ok(Protocol::SnapshotHeader(_)) => expected.push(TestSnapshot { pages: vec![] }),
                Ok(Protocol::PageHeader(p)) => {
                    let mut buf = vec![0; p.page_size as usize];
                    reader.read_exact(buf.as_mut_slice()).unwrap();
                    expected.last_mut().unwrap().pages.push(TestPage {
                        offset: p.offset,
                        data: buf,
                    });
                }
                Ok(Protocol::EndOfStream(_)) => break,
                Err(e) => return TestResult::error(format!("unexpected error: {e}")),
            }
        }
        TestResult::from_bool(input.eq(&expected))
    }
    quickcheck(check as fn(Vec<TestSnapshot>, XorShift) -> TestResult);
}

// test journal serialization into Protocol stream with random offset
#[test]
fn test_journal_stream_with_offset() {
    fn check(input: Vec<TestSnapshot>, mut prng: XorShift) -> TestResult {
        // init journal
        let mut journal = Journal::new(Header::default(), Cursor::new(vec![]), None).unwrap();
        for snapshot in input.iter() {
            for page in snapshot.pages.iter() {
                journal.new_page(page.offset, page.data.as_slice()).unwrap();
            }
            journal.commit().unwrap();
        }

        // count how many serialized bytes are expected
        let skip = prng.next() % input.len().max(1) as u64;
        let mut expected_len = 4; // end of stream
        for snapshot in input.iter().skip(skip as usize) {
            expected_len += journal::SnapshotHeader::block_size() + 4;
            for page in snapshot.pages.iter() {
                expected_len += journal::PageHeader::block_size() + 4 + page.data.len();
            }
        }
        let mut stream: Stream<_> = Stream::from(journal.into_iter().skip_snapshots(skip));
        let mut writer = Cursor::new(vec![]);
        loop {
            let buf_size = (prng.next() % 100) as usize;
            // intermidiate buffer of variable size, including 0 sized
            let mut buf = vec![0; buf_size];
            let read = stream.read(&mut buf).unwrap();
            if read == 0 && buf_size != 0 {
                break;
            }
            writer.write_all(&buf[..read]).unwrap();
        }
        let buf = writer.into_inner();
        if expected_len != buf.len() {
            return TestResult::error(format!(
                "expected len: {}, got: {}",
                expected_len,
                buf.len()
            ));
        }

        let mut reader = Cursor::new(buf.as_slice());
        let mut expected = vec![];
        loop {
            match serde_sqlite::from_reader::<Protocol, _>(&mut reader) {
                Ok(Protocol::SnapshotHeader(_)) => expected.push(TestSnapshot { pages: vec![] }),
                Ok(Protocol::PageHeader(p)) => {
                    let mut buf = vec![0; p.page_size as usize];
                    reader.read_exact(buf.as_mut_slice()).unwrap();
                    expected.last_mut().unwrap().pages.push(TestPage {
                        offset: p.offset,
                        data: buf,
                    });
                }
                Ok(Protocol::EndOfStream(_)) => break,
                Err(e) => return TestResult::error(format!("unexpected error: {e}")),
            }
        }
        TestResult::from_bool(input[skip as usize..].eq(&expected))
    }
    quickcheck(check as fn(Vec<TestSnapshot>, XorShift) -> TestResult);
}

// check journal rebuild from stream
// journals should be identical in size and contents
#[test]
fn test_journal_rebuild_from_stream() {
    fn check(input: Vec<TestSnapshot>, mut prng: XorShift) {
        let mut journal = Journal::new(Header::default(), Cursor::new(vec![]), None).unwrap();
        for snapshot in input.iter() {
            for page in snapshot.pages.iter() {
                journal.new_page(page.offset, page.data.as_slice()).unwrap();
            }
            journal.commit().unwrap();
        }

        let mut stream: Stream<_> = Stream::from(&mut journal);
        let mut writer = Cursor::new(vec![]);
        loop {
            let buf_size = (prng.next() % 100) as usize;
            // intermidiate buffer of variable size, including 0
            let mut buf = vec![0; buf_size];
            let read = stream.read(&mut buf).unwrap();
            if read == 0 && buf_size != 0 {
                break;
            }
            writer.write_all(&buf[..read]).unwrap();
        }
        let buf = writer.into_inner();

        let mut reader = Cursor::new(buf.as_slice());
        let mut recovered_journal =
            Journal::new(Header::default(), Cursor::new(vec![]), None).unwrap();
        loop {
            match serde_sqlite::from_reader::<Protocol, _>(&mut reader) {
                Ok(Protocol::SnapshotHeader(s)) => {
                    recovered_journal.commit().unwrap();
                    recovered_journal.add_snapshot(&s).unwrap();
                }
                Ok(Protocol::PageHeader(p)) => {
                    let mut buf = vec![0; p.page_size as usize];
                    reader.read_exact(buf.as_mut_slice()).unwrap();
                    recovered_journal.add_page(&p, buf.as_slice()).unwrap();
                }
                Ok(Protocol::EndOfStream(_)) => {
                    recovered_journal.commit().unwrap();
                    break;
                }
                Err(e) => panic!("unexpected stream error: {e}"),
            }
        }
        assert_eq!(
            journal.into_iter().count(),
            recovered_journal.into_iter().count()
        );
        assert!(journal
            .into_iter()
            .map(Result::unwrap)
            .zip(recovered_journal.into_iter().map(Result::unwrap))
            .all(|(left, right)| left.eq(&right)));
        assert_eq!(journal.get_header(), recovered_journal.get_header());
    }
    quickcheck(check as fn(Vec<TestSnapshot>, XorShift));
}

#[derive(Debug)]
struct ShareableBuffer {
    buf: Arc<UnsafeCell<(Mutex<()>, Vec<u8>)>>,
}

impl ShareableBuffer {
    fn new() -> Self {
        Self {
            buf: Arc::new(UnsafeCell::new((Mutex::new(()), vec![]))),
        }
    }

    fn cursor(&self) -> ShareableBufferCursor {
        ShareableBufferCursor::new(Arc::clone(&self.buf))
    }
}

struct ShareableBufferCursor<'a> {
    buf: Arc<UnsafeCell<(Mutex<()>, Vec<u8>)>>,
    cur: Cursor<&'a mut Vec<u8>>,
}

impl ShareableBufferCursor<'_> {
    fn new(buf: Arc<UnsafeCell<(Mutex<()>, Vec<u8>)>>) -> Self {
        let buf_ref = unsafe { &mut (*buf.get()).1 };
        Self {
            buf,
            cur: Cursor::new(buf_ref),
        }
    }
}

impl Read for ShareableBufferCursor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mutex = unsafe { &(*self.buf.get()).0 };
        let _guard = mutex.lock().unwrap();
        self.cur.read(buf)
    }
}

impl Write for ShareableBufferCursor<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mutex = unsafe { &(*self.buf.get()).0 };
        let _guard = mutex.lock().unwrap();
        self.cur.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mutex = unsafe { &(*self.buf.get()).0 };
        let _guard = mutex.lock().unwrap();
        self.cur.flush()
    }
}

impl Seek for ShareableBufferCursor<'_> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let mutex = unsafe { &(*self.buf.get()).0 };
        let _guard = mutex.lock().unwrap();
        self.cur.seek(pos)
    }
}

unsafe impl Send for ShareableBufferCursor<'_> {}
unsafe impl Sync for ShareableBufferCursor<'_> {}

#[test]
fn test_shareablebuffer() {
    fn check(s: String) {
        let bytes = s.as_bytes();
        let sh_buf = ShareableBuffer::new();
        let mut cursor_1 = sh_buf.cursor();
        let mut cursor_2 = sh_buf.cursor();

        assert!(cursor_1.write_all(bytes).is_ok());
        let mut buf = vec![];
        assert!(cursor_2.read_to_end(&mut buf).is_ok());
        assert_eq!(buf, bytes);
    }
    quickcheck::quickcheck(check as fn(String))
}

// Test journal ability to work concurrently on same underlying IO resource
#[test]
fn test_journal_concurrent_updates() {
    fn check(size: usize, mut prng: XorShift) -> TestResult {
        // limit max number of snapshots
        let size = (size % 1000).max(1);
        let buf = ShareableBuffer::new();

        let journal_1 = &mut Journal::new(Header::default(), buf.cursor(), None).unwrap();
        journal_1.set_buffer_size((prng.next() % 0x0001_0000).max(1) as usize);
        let journal_2 = &mut Journal::new(Header::default(), buf.cursor(), None).unwrap();
        journal_2.set_buffer_size((prng.next() % 0x0001_0000).max(1) as usize);
        let lock = Mutex::new(());

        let snapshots = (0..size).map(|s| vec![0; s + 1]).collect::<Vec<Vec<u8>>>();
        let (s1, s2) = snapshots.as_slice().split_at(snapshots.len() / 2);

        let prng = Mutex::new(prng);

        // test concurrent snapshot creation
        std::thread::scope(|s| {
            s.spawn(|| {
                s1.iter().for_each(|page| {
                    let guard = lock.lock().unwrap();
                    journal_1
                        .new_page(page.len() as u64, page.as_slice())
                        .unwrap();
                    journal_1.commit().unwrap();
                    drop(guard);
                    sleep(Duration::from_micros(prng.lock().unwrap().next() % 10));
                });
            });
            s.spawn(|| {
                s2.iter().for_each(|page| {
                    let guard = lock.lock().unwrap();
                    journal_2
                        .new_page(page.len() as u64, page.as_slice())
                        .unwrap();
                    journal_2.commit().unwrap();
                    drop(guard);
                    sleep(Duration::from_micros(prng.lock().unwrap().next() % 10));
                });
            });
        });

        assert!(journal_1
            .into_iter()
            .zip(journal_2.into_iter())
            .all(|(left, right)| left.unwrap() == right.unwrap()));

        assert_eq!(journal_1.into_iter().count(), journal_2.into_iter().count());
        // it's matches only because we have one page per snapshot
        assert_eq!(journal_1.into_iter().count(), size);
        assert_eq!(journal_1.get_header().snapshot_counter, size as u64);

        // test concurrent snapshot addition
        let buf_re = ShareableBuffer::new();
        let journal_1_re = &mut Journal::new(Header::default(), buf_re.cursor(), None).unwrap();
        let journal_2_re = &mut Journal::new(Header::default(), buf_re.cursor(), None).unwrap();

        let iter = Mutex::new(journal_1.into_iter());
        std::thread::scope(|s| {
            s.spawn(|| loop {
                let mut i = iter.lock().unwrap();
                if let Some(res) = i.next() {
                    let (snapshot_h, page_h, page) = res.unwrap();
                    journal_1_re.add_snapshot(&snapshot_h).unwrap();
                    journal_1_re.add_page(&page_h, page.as_slice()).unwrap();
                    journal_1_re.commit().unwrap();
                } else {
                    break;
                }
                drop(i);
                sleep(Duration::from_micros(prng.lock().unwrap().next() % 10));
            });
            s.spawn(|| loop {
                let mut i = iter.lock().unwrap();
                if let Some(res) = i.next() {
                    let (snapshot_h, page_h, page) = res.unwrap();
                    journal_2_re.add_snapshot(&snapshot_h).unwrap();
                    journal_2_re.add_page(&page_h, page.as_slice()).unwrap();
                    journal_2_re.commit().unwrap();
                } else {
                    break;
                }
                drop(i);
                sleep(Duration::from_micros(prng.lock().unwrap().next() % 10));
            });
        });
        assert!(journal_1
            .into_iter()
            .zip(journal_1_re.into_iter())
            .all(|(left, right)| left.unwrap() == right.unwrap()));

        assert_eq!(
            journal_1.into_iter().count(),
            journal_1_re.into_iter().count(),
        );

        assert!(journal_1_re
            .into_iter()
            .zip(journal_2_re.into_iter())
            .all(|(left, right)| left.unwrap() == right.unwrap()));

        assert_eq!(
            journal_1_re.into_iter().count(),
            journal_2_re.into_iter().count(),
        );

        TestResult::from_bool(true)
    }
    quickcheck(check as fn(usize, XorShift) -> TestResult)
}
