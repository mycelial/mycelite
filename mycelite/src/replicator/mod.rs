#[cfg(feature="replicator")]
mod replicator;
#[cfg(not(feature="replicator"))]
mod noop_replicator;

#[cfg(feature="replicator")]
pub use replicator::*;

#[cfg(not(feature="replicator"))]
pub use noop_replicator::*;
