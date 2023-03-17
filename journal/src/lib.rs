// #[cfg(feature = "async_bridge")]
mod async_bridge;
// #[cfg(feature = "async")]
mod async_journal;

mod error;
mod journal;
mod stream;

// #[cfg(feature = "async_bridge")]
pub use crate::async_bridge::{
    AsyncReadJournalStream, AsyncReadJournalStreamHandle, AsyncWriteJournalStream,
    AsyncWriteJournalStreamHandle,
};

// #[cfg(feature = "async")]
pub use crate::async_journal::{
    AsyncJournal,
};

pub use crate::error::Error;
pub use crate::journal::{BlobHeader, Header, Journal, SnapshotHeader};
pub use crate::stream::{JournalVersion, Protocol, Stream};
