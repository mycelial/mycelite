#[cfg(feature = "async")]
mod async_wrap;
mod error;
mod journal;
mod stream;

pub use crate::error::Error;
pub use crate::journal::{Header, Journal, PageHeader, SnapshotHeader};
pub use crate::stream::{Protocol, Stream};
#[cfg(feature = "async")]
pub use crate::async_wrap::{
    AsyncReadJournalStream,
    AsyncReadJournalStreamHandle,
    AsyncWriteJournalStream,
    AsyncWriteJournalStreamHandle
};
