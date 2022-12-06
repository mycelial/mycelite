//! Replicator prototype
//!
//! ** For demo use only! **

use journal::{de, se, Journal, PageHeader, SnapshotHeader};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::JoinHandle;
use ureq;

#[derive(Debug)]
enum Message {
    /// New snapshot added locally
    NewLocalSnapshot,
    /// New snapshot added remotely
    NewRemoteSnapshot,
    /// Notification from ReplicatorHandle about closed DB File
    Quit,
}

#[derive(Debug)]
pub struct Replicator {
    url: String,
    database_path: String,
    journal: Journal,
    read_only: bool,
}

impl Replicator {
    pub fn new<P: AsRef<Path>>(
        url: String,
        journal_path: P,
        database_path: String,
        read_only: bool,
    ) -> Self {
        Self {
            url,
            journal: Journal::try_from(journal_path).unwrap(),
            database_path,
            read_only,
        }
    }

    pub fn spawn(mut self) -> ReplicatorHandle {
        let (local_loop_tx, mut local_loop_rx) = channel();
        let (remote_loop_tx, mut remote_loop_rx) = channel();
        let (mut local_loop_clone, url_clone) = (local_loop_tx.clone(), self.url.clone());
        let read_only = self.read_only;
        let local_loop_h = Some(std::thread::spawn(move || {
            self.enter_local_loop(&mut local_loop_rx)
        }));
        let remote_loop_h = match !read_only {
            true => None,
            false => Some(std::thread::spawn(move || {
                Self::enter_remote_loop(&mut local_loop_clone, &mut remote_loop_rx, &url_clone)
            })),
        };
        ReplicatorHandle::new(local_loop_tx, remote_loop_tx, local_loop_h, remote_loop_h)
    }

    /// local loop
    ///
    /// listens for notifications pulls/pushes snapshots, restores underlying database to latest
    /// snapshot
    fn enter_local_loop(&mut self, rx: &mut Receiver<Message>) {
        loop {
            self.maybe_push_snapshots().ok();
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => (),
                Ok(Message::Quit) => return,
                Ok(Message::NewLocalSnapshot) => (),
                Ok(Message::NewRemoteSnapshot) => match self.maybe_pull_snapshots() {
                    Ok((last, new)) if last < new => {
                        self.restore_latest_snapshot().ok();
                    }
                    Ok(_) => (),
                    Err(_e) => (),
                },
            };
        }
    }

    /// remote loop
    ///
    /// just dumbly polls remote backend and bothers main thread. A lot.
    fn enter_remote_loop(tx: &mut Sender<Message>, rx: &mut Receiver<Message>, url: &str) {
        let url = &format!("{}/snapshot", url);
        loop {
            if let Ok(v) = Self::get_backend_current_snapshot(url) {
                tx.send(Message::NewRemoteSnapshot).ok();
            };
            match rx.try_recv() {
                Err(TryRecvError::Empty) => (),
                _ => return,
            };
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    /// Push local snapshots, if any
    fn maybe_push_snapshots(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.journal.update_header().unwrap();
        let local_snapshot_id = match self.journal.current_snapshot() {
            // no snapshots yet
            None => return Ok(()),
            Some(v) => v,
        };
        let url = &format!("{}/snapshot", self.url);
        let remote_snapshot_id = match Self::get_backend_current_snapshot(url) {
            Ok(Some(v)) if v >= local_snapshot_id => {
                // up to date, maybe some new stuff, but poller will take care of that
                // eprintln!("everything is up to date");
                return Ok(());
            }
            Ok(Some(v)) => v,
            Ok(None) => 0,
            Err(_) => return Err("error".into()),
        };
        // ureq doesn't allow to send chunked payload through multiple calls
        // so for now - just accumulate everything into single blob
        // even though journal implements std::io::Read and ureq accepts Read as input - journal
        // doesn't provide public API for seeking at pos
        let mut buf = vec![];
        let mut last_seen = None;
        // FIXME: linear scan, since journal doesn't have index maps
        let iter = self
            .journal
            .into_iter()
            .map(Result::unwrap)
            .filter(|(snapshot_header, _, _)| snapshot_header.num >= remote_snapshot_id);
        for (snapshot_header, page_header, page) in iter {
            if last_seen != Some(snapshot_header.num) {
                if last_seen.is_some() {
                    // write last page, FIXME: iterator interface omits last page
                    buf.extend(se::to_bytes(&PageHeader::last())?);
                }
                buf.extend(se::to_bytes(&snapshot_header)?);
                last_seen = Some(snapshot_header.num)
            }
            buf.extend(se::to_bytes(&page_header)?);
            buf.extend(page);
        }
        // FIXME: status code are not checked
        ureq::post(url).send_bytes(&buf)?;
        Ok(())
    }

    /// Pulls remove snapshots, if any
    fn maybe_pull_snapshots(
        &mut self,
    ) -> Result<(Option<u64>, Option<u64>), Box<dyn std::error::Error>> {
        let local_snapshot_id = self.journal.current_snapshot();
        let url = &format!("{}/snapshot", self.url);
        match Self::get_backend_current_snapshot(url)? {
            Some(v) if local_snapshot_id < Some(v) => (),
            v => return Ok((local_snapshot_id, v)),
        };

        let snapshot_path = match local_snapshot_id {
            Some(v) => format!("/{}", v),
            None => "".into(),
        };
        let url = &format!("{}/snapshot{}", self.url, snapshot_path);
        let res = ureq::get(url).call()?;
        let mut reader = res.into_reader();
        while let Ok(snapshot_header) = de::from_reader::<SnapshotHeader, _>(&mut reader) {
            while let Ok(page_header) = de::from_reader::<PageHeader, _>(&mut reader) {
                if page_header.is_last() {
                    break;
                }
                // FIXME: check for allocation
                let mut buf = Vec::with_capacity(page_header.page_size as usize);
                buf.resize(page_header.page_size as usize, 0);
                (&mut reader).read_exact(buf.as_mut_slice())?;
                self.journal.add_page(page_header.offset, &buf)?;
            }
            self.journal.commit()?;
        }
        Ok((local_snapshot_id, self.journal.current_snapshot()))
    }

    // FIXME: move to journal API
    // FIXME: lock sqlite database before restoration (see sqlite3_db_mutex)
    // NOTE: snapshot is recovered from scratch each time. Is there a way to store info about last
    // applied snapshot?
    fn restore_latest_snapshot(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut output = std::io::BufWriter::with_capacity(
            0x100_000,
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&self.database_path)?,
        );
        for data in self.journal.into_iter() {
            let (snapshot_header, page_header, page) = data?;
            output.seek(SeekFrom::Start(page_header.offset))?;
            output.write_all(&page)?;
        }
        Ok(())
    }

    /// Fetch last snapshot id seen by sync backend
    fn get_backend_current_snapshot(url: &str) -> Result<Option<u64>, Box<dyn std::error::Error>> {
        let res = ureq::head(url)
            .timeout(std::time::Duration::from_secs(5))
            .call()?;
        match res.header("x-snapshot-id") {
            Some(value) if value.len() == 0 => Ok(None),
            Some(value) => Ok(Some(value.parse()?)),
            None => Err("backend didn't return x-snapshot-id".into()),
        }
    }
}

#[derive(Debug)]
pub struct ReplicatorHandle {
    local_loop_tx: Sender<Message>,
    remote_loop_tx: Sender<Message>,
    local_loop: Option<JoinHandle<()>>,
    remote_loop: Option<JoinHandle<()>>,
}

impl Drop for ReplicatorHandle {
    fn drop(&mut self) {
        self.local_loop_tx.send(Message::Quit).ok();
        self.remote_loop_tx.send(Message::Quit).ok();
        self.local_loop.take().map(|h| h.join());
        self.remote_loop.take().map(|h| h.join());
    }
}

impl ReplicatorHandle {
    fn new(
        local_loop_tx: Sender<Message>,
        remote_loop_tx: Sender<Message>,
        local_loop: Option<JoinHandle<()>>,
        remote_loop: Option<JoinHandle<()>>,
    ) -> Self {
        Self {
            local_loop_tx,
            remote_loop_tx,
            local_loop,
            remote_loop,
        }
    }

    pub fn new_snapshot(&mut self) {
        self.local_loop_tx.send(Message::NewLocalSnapshot).ok();
    }
}
