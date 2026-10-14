// SPDX-License-Identifier: MIT

//! Client builders for Ghostline stealth channels.
//!
//! This crate produces condition trees and stealth transfer statements. Submitting those statements through a wallet
//! daemon is a separate slice.

mod channel;
mod error;
mod spend;
mod tx;

pub use channel::{Channel, ChannelSpec, SETTLE_FUNCTION};
pub use error::SdkError;
pub use ghostline_core::{
    ChannelParams, PaymentSession, ServiceLedger, SignedVoucher, VoucherError, sign_voucher,
    witness_data,
};
pub use spend::{refund, settle};
pub use tari_ootle_wallet_crypto::MaskAndValue;
pub use tx::seal_localnet_stealth_transfer;
