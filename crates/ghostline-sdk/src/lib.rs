// SPDX-License-Identifier: MIT

//! Client builders for Ghostline stealth channels.
//!
//! This crate produces condition trees, stealth transfer statements, and sealed transactions.
//! Submitting the envelope through an indexer is `ghostline-net`.

mod channel;
mod error;
mod fee;
mod recover;
mod spend;
mod tx;

pub use channel::{BATCH_SETTLE_FUNCTION, Channel, ChannelSpec, SETTLE_FUNCTION};
pub use error::SdkError;
pub use fee::{TREASURY_ENV, parse_fee_config, parse_treasury, treasury_from_env};
pub use ghostline_core::{
    ChannelParams, PaymentSession, ServiceLedger, SignedVoucher, VoucherError, sign_voucher,
};
pub use recover::{
    RecoveredOutput, StealthOutputView, inspect_transaction, unblind_output, unblind_transaction,
};
#[cfg(feature = "trusted-local-batch-prototype")]
pub use spend::settle_batch;
#[cfg(feature = "adversarial-test-builders")]
pub use spend::{FeeOverride, settle_with_fee_override};
pub use spend::{
    OpenStatement, OutputScan, fund_open, fund_open_batched, refund, refund_batched, settle,
    settle_batched_individually,
};
pub use tari_ootle_transaction::{TransactionEnvelope, UnsignedTransaction};
pub use tari_ootle_wallet_crypto::MaskAndValue;
pub use tx::{
    AccountFee, encode_transaction_envelope, seal_hex, seal_hex_with_fee, seal_localnet_hex,
    seal_localnet_stealth_transfer, seal_stealth_transfer, seal_stealth_transfer_with_fee,
    transaction_envelope_hex, unsigned_stealth_transfer_with_fee,
};
