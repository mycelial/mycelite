//! Journal Error
use serde_sqlite::Error as SerdeSqliteError;
use std::collections::TryReserveError;
use std::io::Error as IOError;

#[derive(Debug)]
pub enum Error {
    /// std::io::Error
    IOError(IOError),
    /// std::collections::TryReserveError
    TryReserveError(TryReserveError),
    /// serde_sqlite error
    SerdeSqliteError(SerdeSqliteError),
    /// attemt to add out of order snapshot
    OutOfOrderSnapshot {
        snapshot_id: u64,
        journal_snapshot_id: u64,
    },
    /// Snapshot not started
    SnapshotNotStarted,
    /// attemt to add out of order blob
    OutOfOrderBlob {
        blob_num: u32,
        blob_count: Option<u32>,
    },
}

impl From<IOError> for Error {
    fn from(e: IOError) -> Self {
        Self::IOError(e)
    }
}

impl From<TryReserveError> for Error {
    fn from(e: TryReserveError) -> Self {
        Self::TryReserveError(e)
    }
}

impl From<SerdeSqliteError> for Error {
    fn from(e: SerdeSqliteError) -> Self {
        Self::SerdeSqliteError(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Check if error caused by absense of journal
    pub fn journal_not_exists(&self) -> bool {
        match self {
            Self::IOError(e) => e.kind() == std::io::ErrorKind::NotFound,
            _ => false,
        }
    }
}
