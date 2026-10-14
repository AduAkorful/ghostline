// SPDX-License-Identifier: MIT

//! Submit sealed Ghostline transactions to an Ootle indexer REST API.
//!
//! Paths and JSON field names come from `tari_indexer_client` (README + 0.46 source).
//! This crate does not depend on that package: published versions pull `tari_ootle_transaction`
//! 0.46+, which would collide with the 0.45 engine suite.

mod client;
mod error;

pub use client::{EpochStats, IndexerClient};
pub use error::NetError;
pub use tari_ootle_transaction::{Epoch, Transaction, TransactionEnvelope, TransactionId};
