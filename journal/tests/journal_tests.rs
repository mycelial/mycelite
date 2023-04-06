use block::Block;
use journal::{Header, Journal, Protocol, Stream};
use quickcheck::{quickcheck, Arbitrary, Gen, TestResult};
use spin_sleep::sleep;
use std::cell::UnsafeCell;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(feature = "async")]
use {futures::pin_mut, journal::AsyncJournal, tokio_stream::StreamExt};

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
struct TestBlob {
    offset: u64,
    data: Vec<u8>,
}

impl Arbitrary for TestBlob {
    fn arbitrary(gen: &mut Gen) -> Self {
        Self {
            offset: u64::arbitrary(gen),
            data: Vec::<u8>::arbitrary(gen),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TestSnapshot {
    blobs: Vec<TestBlob>,
}

impl Arbitrary for TestSnapshot {
    fn arbitrary(gen: &mut Gen) -> Self {
        // limit min/max blob per snapshot
        let blob_count = 1 + usize::arbitrary(gen) % 49;
        let blobs = (0..blob_count)
            .enumerate()
            .fold(vec![], |mut acc, (pos, _)| {
                let mut blob = TestBlob::arbitrary(gen);
                // *edge case*
                // quickcheck is able to quickly find a way to insert 'last blob' as a first blob of snapshot
                // last blob is a blob where all values are set to 0 and technically it's not possible
                // to insert such blob from sqlite calls
                // for now we just override such scenario, but blobs with zero sizes are still part of
                // the test case, even though empty blob as a concept doesn't make sense.
                if pos == 0 && blob.data.is_empty() {
                    blob.data = vec![0];
                }
                acc.push(blob);
                acc
            });
        TestSnapshot { blobs }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            self.blobs
                .shrink()
                .filter(|blobs| !blobs.is_empty()) // snapshot with no blobs is not valid input
                .map(|blobs| TestSnapshot { blobs }),
        )
    }
}

#[test]
fn test_journal_snapshotting() {
    fn check(input: Vec<TestSnapshot>) {
        let mut journal = Journal::new(Header::default(), Cursor::new(vec![]), None).unwrap();
        for snapshot in input.iter() {
            for blob in snapshot.blobs.iter() {
                journal.new_snapshot(0).unwrap();
                journal.new_blob(blob.offset, blob.data.as_slice()).unwrap();
            }
            journal.commit().unwrap();
        }
        // iteration over journal always should return same input
        let restored_input = (&mut journal)
            .into_iter()
            .map(Result::unwrap)
            .fold(
                (vec![], None),
                |(mut acc, mut snapshot_id), (snapshot_h, blob_h, blob)| {
                    if snapshot_id != Some(snapshot_h.id) {
                        snapshot_id = Some(snapshot_h.id);
                        acc.push(TestSnapshot { blobs: vec![] });
                    };
                    acc.last_mut().unwrap().blobs.push(TestBlob {
                        offset: blob_h.offset,
                        data: blob,
                    });
                    (acc, snapshot_id)
                },
            )
            .0;
        assert_eq!(restored_input, input);
    }
    quickcheck(check as fn(Vec<TestSnapshot>));
}

#[cfg(feature = "async")]
#[test]
fn test_async_journal_snapshotting() {
    fn check(input: Vec<TestSnapshot>) {
        let rt = tokio::runtime::Builder::new_multi_thread().build().unwrap();

        // Call the asynchronous function using the `block_on` method
        let mut result = rt.block_on(async {
            let mut journal = AsyncJournal::new(Header::default(), Cursor::new(vec![]), None)
                .await
                .unwrap();
            for snapshot in input.iter() {
                for blob in snapshot.blobs.iter() {
                    journal.new_snapshot(0).await.unwrap();
                    journal
                        .new_blob(blob.offset, blob.data.as_slice())
                        .await
                        .unwrap();
                }
                journal.commit().await.unwrap();
            }
            journal
        });
        // iteration over journal always should return same input
        let restored_input = rt.block_on(async {
            let mut restored_input: Vec<TestSnapshot> = Vec::new();
            let stream = result.stream();
            pin_mut!(stream);
            let mut last_snapshot_header_id: Option<u64> = None;
            while let Some(Ok((snapshot_h, blob_h, blob))) = stream.next().await {
                if last_snapshot_header_id != Some(snapshot_h.id) {
                    last_snapshot_header_id = Some(snapshot_h.id);
                    restored_input.push(TestSnapshot { blobs: vec![] });
                }
                restored_input.last_mut().unwrap().blobs.push(TestBlob {
                    offset: blob_h.offset,
                    data: blob,
                });
            }
            restored_input
        });

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
        let mut expected_len = 12; // version + end of stream
        for snapshot in input.iter() {
            expected_len += journal::SnapshotHeader::block_size() + 4;
            for blob in snapshot.blobs.iter() {
                expected_len += journal::BlobHeader::block_size() + 4 + blob.data.len();
                journal.new_snapshot(0).unwrap();
                journal.new_blob(blob.offset, blob.data.as_slice()).unwrap();
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
        assert_eq!(
            serde_sqlite::from_reader::<Protocol, _>(&mut reader).unwrap(),
            Protocol::JournalVersion(1.into())
        );
        loop {
            match serde_sqlite::from_reader::<Protocol, _>(&mut reader) {
                Ok(Protocol::SnapshotHeader(_)) => expected.push(TestSnapshot { blobs: vec![] }),
                Ok(Protocol::BlobHeader(p)) => {
                    let mut buf = vec![0; p.blob_size as usize];
                    reader.read_exact(buf.as_mut_slice()).unwrap();
                    expected.last_mut().unwrap().blobs.push(TestBlob {
                        offset: p.offset,
                        data: buf,
                    });
                }
                Ok(Protocol::EndOfStream(_)) => break,
                Ok(msg) => panic!("unexpected {msg:?}"),
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
            for blob in snapshot.blobs.iter() {
                journal.new_snapshot(0).unwrap();
                journal.new_blob(blob.offset, blob.data.as_slice()).unwrap();
            }
            journal.commit().unwrap();
        }

        // count how many serialized bytes are expected
        let skip = prng.next() % input.len().max(1) as u64;
        let mut expected_len = 12; // version + end of stream
        for snapshot in input.iter().skip(skip as usize) {
            expected_len += journal::SnapshotHeader::block_size() + 4;
            for blob in snapshot.blobs.iter() {
                expected_len += journal::BlobHeader::block_size() + 4 + blob.data.len();
            }
        }
        let mut stream: Stream<_> = Stream::from((1, journal.into_iter().skip_snapshots(skip)));
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

        assert_eq!(
            serde_sqlite::from_reader::<Protocol, _>(&mut reader).unwrap(),
            Protocol::JournalVersion(1.into())
        );
        loop {
            match serde_sqlite::from_reader::<Protocol, _>(&mut reader) {
                Ok(Protocol::SnapshotHeader(_)) => expected.push(TestSnapshot { blobs: vec![] }),
                Ok(Protocol::BlobHeader(p)) => {
                    let mut buf = vec![0; p.blob_size as usize];
                    reader.read_exact(buf.as_mut_slice()).unwrap();
                    expected.last_mut().unwrap().blobs.push(TestBlob {
                        offset: p.offset,
                        data: buf,
                    });
                }
                Ok(Protocol::EndOfStream(_)) => break,
                Ok(msg) => panic!("unexpected {msg:?}"),
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
            for blob in snapshot.blobs.iter() {
                journal.new_snapshot(0).unwrap();
                journal.new_blob(blob.offset, blob.data.as_slice()).unwrap();
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

        assert_eq!(
            serde_sqlite::from_reader::<Protocol, _>(&mut reader).unwrap(),
            Protocol::JournalVersion(1.into())
        );
        loop {
            match serde_sqlite::from_reader::<Protocol, _>(&mut reader) {
                Ok(Protocol::SnapshotHeader(s)) => {
                    recovered_journal.commit().unwrap();
                    recovered_journal.add_snapshot(&s).unwrap();
                }
                Ok(Protocol::BlobHeader(p)) => {
                    let mut buf = vec![0; p.blob_size as usize];
                    reader.read_exact(buf.as_mut_slice()).unwrap();
                    recovered_journal.add_blob(&p, buf.as_slice()).unwrap();
                }
                Ok(Protocol::EndOfStream(_)) => {
                    recovered_journal.commit().unwrap();
                    break;
                }
                Ok(Protocol::JournalVersion(_)) => {
                    panic!("version header should not appear in loop")
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
                s1.iter().for_each(|blob| {
                    let guard = lock.lock().unwrap();
                    journal_1.new_snapshot(0).unwrap();
                    journal_1
                        .new_blob(blob.len() as u64, blob.as_slice())
                        .unwrap();
                    journal_1.commit().unwrap();
                    drop(guard);
                    sleep(Duration::from_micros(prng.lock().unwrap().next() % 10));
                });
            });
            s.spawn(|| {
                s2.iter().for_each(|blob| {
                    let guard = lock.lock().unwrap();
                    journal_2.new_snapshot(0).unwrap();
                    journal_2
                        .new_blob(blob.len() as u64, blob.as_slice())
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
        // it's matches only because we have one blob per snapshot
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
                    let (snapshot_h, blob_h, blob) = res.unwrap();
                    journal_1_re.add_snapshot(&snapshot_h).unwrap();
                    journal_1_re.add_blob(&blob_h, blob.as_slice()).unwrap();
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
                    let (snapshot_h, blob_h, blob) = res.unwrap();
                    journal_2_re.add_snapshot(&snapshot_h).unwrap();
                    journal_2_re.add_blob(&blob_h, blob.as_slice()).unwrap();
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

#[cfg(feature = "async")]
#[test]
fn test_async_journal_and_sync_journal_are_the_same() {
    // put the same things into a regular journal and an async journal.
    fn check_regular(input: Vec<TestSnapshot>) {
        let mut journal = Journal::new(Header::default(), Cursor::new(vec![]), None).unwrap();
        for snapshot in input.iter() {
            for blob in snapshot.blobs.iter() {
                journal.new_snapshot(0).unwrap();
                journal.new_blob(blob.offset, blob.data.as_slice()).unwrap();
            }
            journal.commit().unwrap();
        }
        // iteration over journal always should return same input
        let restored_input = (&mut journal)
            .into_iter()
            .map(Result::unwrap)
            .fold(
                (vec![], None),
                |(mut acc, mut snapshot_id), (snapshot_h, blob_h, blob)| {
                    if snapshot_id != Some(snapshot_h.id) {
                        snapshot_id = Some(snapshot_h.id);
                        acc.push(TestSnapshot { blobs: vec![] });
                    };
                    acc.last_mut().unwrap().blobs.push(TestBlob {
                        offset: blob_h.offset,
                        data: blob,
                    });
                    (acc, snapshot_id)
                },
            )
            .0;
        assert_eq!(restored_input, input);
    }

    fn check_async(input: Vec<TestSnapshot>) {
        let rt = tokio::runtime::Builder::new_multi_thread().build().unwrap();

        // Call the asynchronous function using the `block_on` method
        let mut result = rt.block_on(async {
            let mut async_journal = AsyncJournal::new(Header::default(), Cursor::new(vec![]), None)
                .await
                .unwrap();
            for snapshot in input.iter() {
                for blob in snapshot.blobs.iter() {
                    async_journal.new_snapshot(0).await.unwrap();
                    async_journal
                        .new_blob(blob.offset, blob.data.as_slice())
                        .await
                        .unwrap();
                }
                async_journal.commit().await.unwrap();
            }
            async_journal
        });
        // iteration over journal always should return same input
        let restored_input = rt.block_on(async {
            let mut restored_input: Vec<TestSnapshot> = Vec::new();
            let stream = result.stream();
            pin_mut!(stream);
            let mut last_snapshot_header_id: Option<u64> = None;
            while let Some(Ok((snapshot_h, blob_h, blob))) = stream.next().await {
                if last_snapshot_header_id != Some(snapshot_h.id) {
                    last_snapshot_header_id = Some(snapshot_h.id);
                    restored_input.push(TestSnapshot { blobs: vec![] });
                }
                restored_input.last_mut().unwrap().blobs.push(TestBlob {
                    offset: blob_h.offset,
                    data: blob,
                });
            }
            restored_input
        });

        assert_eq!(restored_input, input);
    }

    fn check(input: Vec<TestSnapshot>) {
        let input_clone = input.clone();
        check_async(input);
        check_regular(input_clone);
    }

    quickcheck(check as fn(Vec<TestSnapshot>));
}
