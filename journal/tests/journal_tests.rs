use block::Block;
use journal::{Header, Journal, Protocol, Stream};
use quickcheck::{quickcheck, Arbitrary, Gen, TestResult};
use std::io::{Cursor, Read, Write};
use tempfile;

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
                .filter(|pages| pages.len() > 0) // snapshot with zero pages is an invalid input
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
                Err(e) => return TestResult::error(format!("unexpected error: {}", e)),
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
                Err(e) => return TestResult::error(format!("unexpected error: {}", e)),
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
                Err(e) => panic!("unexpected stream error: {}", e),
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
