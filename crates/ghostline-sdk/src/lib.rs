// SPDX-License-Identifier: MIT

//! Client builders for Ghostline stealth channels.
//!
//! This crate produces condition trees, stealth transfer statements, and sealed transactions.
//! Submitting the envelope through an indexer is `ghostline-net`.

mod channel;
mod error;
mod recover;
mod spend;
mod tx;

pub use channel::{Channel, ChannelSpec, SETTLE_FUNCTION};
pub use error::SdkError;
pub use ghostline_core::{
    ChannelParams, PaymentSession, ServiceLedger, SignedVoucher, VoucherError, sign_voucher,
    witness_data,
};
pub use recover::{
    RecoveredOutput, StealthOutputView, inspect_transaction, unblind_output, unblind_transaction,
};
pub use spend::{OpenStatement, OutputScan, fund_open, refund, settle};
pub use tari_ootle_transaction::TransactionEnvelope;
pub use tari_ootle_wallet_crypto::MaskAndValue;
pub use tx::{
    encode_transaction_envelope, seal_hex, seal_localnet_hex, seal_localnet_stealth_transfer,
    seal_stealth_transfer, transaction_envelope_hex,
};
