use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct Replicator {}

impl Replicator {
    pub fn new<P: AsRef<Path>>(
        _journal_path: P,
        _database_path: String,
        _read_only: bool,
        _lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {}
    }

    pub fn spawn(self) -> ReplicatorHandle {
        ReplicatorHandle {}
    }
}

pub struct ReplicatorHandle {}

impl ReplicatorHandle {
    pub fn new_snapshot(&self) {}
}
