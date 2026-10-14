// SPDX-License-Identifier: MIT

//! Submit Ghostline transactions to an Ootle indexer REST API or a local wallet daemon.
//!
//! Indexer paths come from `tari_indexer_client` (README + 0.46 source). Wallet JSON-RPC
//! names come from `tari_ootle_walletd_client` 0.45. This crate does not depend on those
//! packages: published versions pull `tari_ootle_transaction` 0.46+, which would collide
//! with the 0.45 engine suite.

mod client;
mod error;
mod wallet;

pub use client::{EpochStats, IndexerClient};
pub use error::NetError;
pub use tari_ootle_transaction::{Epoch, Transaction, TransactionEnvelope, TransactionId};
pub use wallet::{
    DEFAULT_ACCOUNT_SIGNER_JSON, WalletClient, WalletDryRunResponse, WalletSubmitRequest,
    WalletSubmitResponse, execution_accepted,
};
