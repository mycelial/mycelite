//! Replicator prototype
//!
//! ** For demo use only! **

use journal::{Journal, PageHeader, Protocol, SnapshotHeader, Stream};
use serde_sqlite::{de, se};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

enum Message {
    /// New snapshot added locally
    NewLocalSnapshot,
    /// New snapshot added remotely
    NewRemoteSnapshot,
    /// Notification from ReplicatorHandle about closed DB File
    Quit,
}

pub struct Replicator {
    url: String,
    database_path: String,
    journal: Journal,
    read_only: bool,
    lock: Arc<Mutex<()>>,
}

impl Replicator {
    pub fn new<P: AsRef<Path>>(
        url: String,
        journal_path: P,
        database_path: String,
        read_only: bool,
        lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            url,
            journal: Journal::try_from(journal_path).unwrap(),
            database_path,
            read_only,
            lock,
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
            if !self.read_only {
                self.maybe_push_snapshots().ok();
            }
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
        let url = &format!("{url}/api/v0/snapshots");
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
            None => return Ok(()),
            Some(v) => v,
        };
        let url = &format!("{}/api/v0/snapshots", self.url);
        let remote_snapshot_id = match Self::get_backend_current_snapshot(url) {
            Ok(Some(v)) if v >= local_snapshot_id => {
                return Ok(());
            }
            Ok(Some(v)) => v,
            Ok(None) => 0,
            Err(_) => return Err("error".into()),
        };
        // FIXME: status code are not checked
        let stream = Stream::from(self.journal.into_iter().skip_snapshots(remote_snapshot_id));
        ureq::post(url)
            .set("x-mcl-to", "domain@mycelial.com")
            .send(stream)?;
        Ok(())
    }

    /// Pulls remove snapshots, if any
    fn maybe_pull_snapshots(
        &mut self,
    ) -> Result<(Option<u64>, Option<u64>), Box<dyn std::error::Error>> {
        let local_snapshot_id = self.journal.current_snapshot();
        let url = &format!("{}/api/v0/snapshots", self.url);
        match Self::get_backend_current_snapshot(url)? {
            Some(v) if local_snapshot_id < Some(v) => (),
            v => return Ok((local_snapshot_id, v)),
        };

        let url = &format!("{}/api/v0/snapshots", self.url);
        let res = ureq::get(url)
            .set("x-mcl-to", "domain@mycelial.com")
            .query("snapshot-id", &local_snapshot_id.unwrap_or(0).to_string())
            .call()?;

        let mut reader = res.into_reader();
        loop {
            match de::from_reader::<Protocol, _>(&mut reader)? {
                Protocol::SnapshotHeader(snapshot_header) => {
                    self.journal.commit()?;
                    self.journal.add_snapshot(&snapshot_header)?
                }
                Protocol::PageHeader(page_header) => {
                    let mut page = vec![0; page_header.page_size as usize];
                    reader.read_exact(page.as_mut_slice())?;
                    self.journal.add_page(&page_header, page.as_slice())?;
                }
                Protocol::EndOfStream(_) => {
                    self.journal.commit()?;
                    break;
                }
            }
        }
        Ok((local_snapshot_id, self.journal.current_snapshot()))
    }

    // FIXME: move to journal API
    // FIXME: snapshot is recovered from scratch each time
    fn restore_latest_snapshot(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let lock = self.lock.lock().map_err(|e| "failed to lock")?;
        let mut output = std::io::BufWriter::with_capacity(
            0x0010_0000,
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
            .set("x-mcl-to", "domain@mycelial.com")
            .timeout(std::time::Duration::from_secs(5))
            .call()?;

        match res.header("x-snapshot-id") {
            Some(value) if value.is_empty() => Ok(None),
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
