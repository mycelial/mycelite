#[cfg_attr(not(feature = "replicator"), path = "noop_replicator.rs")]
#[cfg_attr(feature = "replicator", path = "http_replicator.rs")]
mod replicator_impl;

pub use replicator_impl::*;
