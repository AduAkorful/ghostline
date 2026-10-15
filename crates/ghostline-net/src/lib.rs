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

pub use client::{
    EpochStats, IndexerClient, ResolvedInputs, SubstatePresence, indexer_result_status,
};
pub use error::NetError;
pub use tari_ootle_transaction::{Epoch, Transaction, TransactionEnvelope, TransactionId};
pub use wallet::{
    ACCOUNT_KEY_BRANCH, AccountWithAddress, AccountsCreateRequest, ChannelOpenParams,
    DEFAULT_ACCOUNT_SIGNER_JSON, StealthTransferRequest, UtxoInputSelection, VIEW_ONLY_KEY_BRANCH,
    WalletAccount, WalletClient, WalletDryRunResponse, WalletStealthUtxo, WalletSubmitRequest,
    WalletSubmitResponse, account_name, channel_open_utxo, derived_account_key_id,
    execution_accepted, settle_fee_signers, stealth_utxos_from_wallet_tx,
};
