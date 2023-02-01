//! Replicator prototype
//!
//! ** For demo use only! **

use crate::config::{Config, ConfigRegistry};
use journal::{Journal, Protocol, Stream};
use serde_sqlite::de;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

enum Message {
    /// New snapshot added locally
    NewLocalSnapshot,
    /// Notification from ReplicatorHandle about closed DB File
    Quit,
}

pub struct Replicator {
    database_path: String,
    journal: Journal,
    read_only: bool,
    lock: Arc<Mutex<()>>,
    config: Arc<Mutex<Config>>,
}

impl Replicator {
    pub fn new<P: AsRef<Path>>(
        journal_path: P,
        database_path: String,
        read_only: bool,
        lock: Arc<Mutex<()>>,
    ) -> Self {
        let config = ConfigRegistry::new().get(database_path.as_str());
        Self {
            journal: Journal::try_from(journal_path).unwrap(),
            database_path,
            read_only,
            lock,
            config,
        }
    }

    pub fn spawn(mut self) -> ReplicatorHandle {
        let (tx, mut rx) = channel();
        let local_h = Some(std::thread::spawn(move || self.enter_loop(&mut rx)));
        ReplicatorHandle::new(tx, local_h)
    }

    /// local loop
    ///
    /// listens for notifications pulls/pushes snapshots, restores underlying database to latest
    /// snapshot
    fn enter_loop(&mut self, rx: &mut Receiver<Message>) {
        loop {
            match self.read_only {
                true => {
                    match self.maybe_pull_snapshots() {
                        Ok((last, new)) if last < new => {
                            self.restore_latest_snapshot().ok();
                        }
                        Ok(_) => (),
                        Err(_e) => (),
                    };
                }
                false => {
                    self.maybe_push_snapshots().ok();
                }
            }
            match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => (),
                Ok(Message::Quit) => return,
                Ok(Message::NewLocalSnapshot) => (),
            };
        }
    }

    /// Push local snapshots, if any
    fn maybe_push_snapshots(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // FIXME: unwrap
        self.journal.update_header().unwrap();
        let local_snapshot_id = match self.journal.current_snapshot() {
            None => return Ok(()),
            Some(v) => v,
        };
        let url = Self::get_url(&self.config);
        let domain = Self::get_domain(&self.config);
        let (url, domain) = match (url.as_ref(), domain.as_ref()) {
            (Some(u), Some(d)) => (u, d),
            _ => return Ok(()),
        };
        let remote_snapshot_id = match Self::get_backend_current_snapshot(url, domain) {
            Ok(Some(v)) if v >= local_snapshot_id => {
                return Ok(());
            }
            Ok(Some(v)) => v,
            Ok(None) => 0,
            Err(_) => return Err("error".into()),
        };
        // FIXME: status code are not checked
        let stream = Stream::from(self.journal.into_iter().skip_snapshots(remote_snapshot_id));
        ureq::post(url).set("x-mcl-to", domain).send(stream)?;
        Ok(())
    }

    /// Pulls remove snapshots, if any
    fn maybe_pull_snapshots(
        &mut self,
    ) -> Result<(Option<u64>, Option<u64>), Box<dyn std::error::Error>> {
        let local_snapshot_id = self.journal.current_snapshot();
        let url = Self::get_url(&self.config);
        let domain = Self::get_domain(&self.config);
        if url.is_none() || domain.is_none() {
            return Ok((local_snapshot_id, local_snapshot_id));
        };
        let (url, domain) = (&url.unwrap(), &domain.unwrap());

        match Self::get_backend_current_snapshot(url, domain)? {
            Some(v) if local_snapshot_id < Some(v) => (),
            v => return Ok((local_snapshot_id, v)),
        };

        let res = ureq::get(url)
            .set("x-mcl-to", domain)
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
        let lock = self.lock.lock().map_err(|_e| "failed to lock")?;
        let mut output = std::io::BufWriter::with_capacity(
            0x0010_0000,
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&self.database_path)?,
        );
        for data in self.journal.into_iter() {
            let (_snapshot_header, page_header, page) = data?;
            output.seek(SeekFrom::Start(page_header.offset))?;
            output.write_all(&page)?;
        }
        drop(lock);
        Ok(())
    }

    /// Fetch last snapshot id seen by sync backend
    fn get_backend_current_snapshot(
        url: &str,
        domain: &str,
    ) -> Result<Option<u64>, Box<dyn std::error::Error>> {
        let res = ureq::head(url)
            .set("x-mcl-to", domain)
            .timeout(std::time::Duration::from_secs(5))
            .call()?;

        match res.header("x-snapshot-id") {
            Some(value) if value.is_empty() => Ok(None),
            Some(value) => Ok(Some(value.parse()?)),
            None => Err("backend didn't return x-snapshot-id".into()),
        }
    }

    fn get_domain(config: &Arc<Mutex<Config>>) -> Option<String> {
        config.lock().unwrap().get("domain").map(|s| s.to_owned())
    }

    fn get_url(config: &Arc<Mutex<Config>>) -> Option<String> {
        config
            .lock()
            .unwrap()
            .get("endpoint")
            .map(|s| format!("{s}/api/v0/snapshots"))
    }
}

#[derive(Debug)]
pub struct ReplicatorHandle {
    tx: Sender<Message>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for ReplicatorHandle {
    fn drop(&mut self) {
        self.tx.send(Message::Quit).ok();
        self.handle.take().map(|h| h.join());
    }
}

impl ReplicatorHandle {
    fn new(tx: Sender<Message>, handle: Option<JoinHandle<()>>) -> Self {
        Self { tx, handle }
    }

    pub fn new_snapshot(&mut self) {
        self.tx.send(Message::NewLocalSnapshot).ok();
    }
}
