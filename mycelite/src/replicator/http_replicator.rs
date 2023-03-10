//! Replicator prototype
//!
//! ** For demo use only! **

use crate::config::{Config, ConfigRegistry};
use base64::engine::{general_purpose::STANDARD as BASE64, Engine};
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
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
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
        let url = match self.get_url() {
            Some(url) => url,
            None => return Ok(()),
        };
        // snapshot push always requires authorization (for now)
        let client_id = self.get_key("client_id");
        let secret = self.get_key("secret");
        if client_id.is_none() || secret.is_none() {
            return Ok(());
        };
        let remote_snapshot_id = match self.get_backend_current_snapshot(
            &url,
            client_id.as_deref(),
            secret.as_deref(),
        ) {
            Ok(Some(v)) if v >= local_snapshot_id => {
                return Ok(());
            }
            Ok(Some(v)) => v,
            Ok(None) => 0,
            Err(_) => return Err("error".into()),
        };

        let mut req = ureq::post(&url);
        if let Some(b) = self.get_basic_auth_header(client_id.as_deref(), secret.as_deref()) {
            req = req.set("Authorization", &b)
        }

        let version = self.journal.get_header().version;
        let stream = Stream::from((
            version,
            self.journal.into_iter().skip_snapshots(remote_snapshot_id),
        ));

        // FIXME: status code are not checked
        req.send(stream)?;
        Ok(())
    }

    /// Pulls remove snapshots, if any
    fn maybe_pull_snapshots(
        &mut self,
    ) -> Result<(Option<u64>, Option<u64>), Box<dyn std::error::Error>> {
        let local_snapshot_id = self.journal.current_snapshot();
        let url = match self.get_url() {
            Some(url) => url,
            None => return Ok((local_snapshot_id, local_snapshot_id)),
        };

        let client_id = self.get_key("client_id");
        let secret = self.get_key("secret");

        match self.get_backend_current_snapshot(&url, client_id.as_deref(), secret.as_deref())? {
            Some(v) if local_snapshot_id < Some(v) => (),
            v => return Ok((local_snapshot_id, v)),
        };

        let mut req =
            ureq::get(&url).query("snapshot-id", &local_snapshot_id.unwrap_or(0).to_string());

        if let Some(b) = self.get_basic_auth_header(client_id.as_deref(), secret.as_deref()) {
            req = req.set("Authorization", &b)
        }
        let res = req.call()?;

        let mut reader = res.into_reader();

        match de::from_reader::<Protocol, _>(&mut reader)? {
            Protocol::JournalVersion(v) if v == 1_u32.into() => (),
            Protocol::JournalVersion(v) => {
                return Err(format!("unexpected journal version: {v:?}").into())
            }
            _ => return Err("expected version header".into()),
        };
        loop {
            match de::from_reader::<Protocol, _>(&mut reader)? {
                Protocol::SnapshotHeader(snapshot_header) => {
                    self.journal.commit()?;
                    self.journal.add_snapshot(&snapshot_header)?
                }
                Protocol::BlobHeader(blob_header) => {
                    let mut blob = vec![0; blob_header.blob_size as usize];
                    reader.read_exact(blob.as_mut_slice())?;
                    self.journal.add_blob(&blob_header, blob.as_slice())?;
                }
                Protocol::EndOfStream(_) => {
                    self.journal.commit()?;
                    break;
                }
                Protocol::JournalVersion(_) => return Err("version header was not expected".into()),
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
        &self,
        url: &str,
        client_id: Option<&str>,
        secret: Option<&str>,
    ) -> Result<Option<u64>, Box<dyn std::error::Error>> {
        let mut req = ureq::head(url).timeout(std::time::Duration::from_secs(5));

        if let Some(b) = self.get_basic_auth_header(client_id, secret) {
            req = req.set("Authorization", &b)
        }
        let res = req.call()?;

        match res.header("x-snapshot-id") {
            Some(value) if value.is_empty() => Ok(None),
            Some(value) => Ok(Some(value.parse()?)),
            None => Err("backend didn't return x-snapshot-id".into()),
        }
    }

    fn get_key(&self, key: &str) -> Option<String> {
        self.config.lock().unwrap().get(key).map(|s| s.to_owned())
    }

    fn get_url(&self) -> Option<String> {
        if let (Some(endpoint), Some(domain)) = (self.get_key("endpoint"), self.get_key("domain")) {
            return Some(format!("{endpoint}/domain/{domain}"));
        }
        None
    }

    fn get_basic_auth_header(
        &self,
        client_id: Option<&str>,
        secret: Option<&str>,
    ) -> Option<String> {
        if let (Some(client_id), Some(secret)) = (client_id, secret) {
            return Some(format!(
                "Basic {}",
                BASE64.encode(format!("{client_id}:{secret}"))
            ));
        } else {
            None
        }
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
