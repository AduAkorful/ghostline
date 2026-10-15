// SPDX-License-Identifier: MIT

//! HTTP 402 paywall and agent-side payer for Ghostline vouchers.
//!
//! The paywall never talks to the chain. It verifies cumulative vouchers against a [`ServiceLedger`].

mod agent;
mod error;
mod gate;
mod price;
mod protocol;
mod status;
mod store;

pub use agent::{AgentPayer, gated_sign};
pub use error::{AgentError, PaywallError};
pub use gate::{channel_id_hex, charge, ledger_channel_id};
pub use ghostline_core::{PaymentSession, ServiceLedger, SignedVoucher};
pub use ghostline_policy::{AuditEvent, Policy, PolicyEngine, PolicyError, RateLimit};
pub use price::{PriceKind, Quote, quote};
pub use protocol::{
    CHANNEL_HEADER, PaymentRejection, PaymentRequired, SCHEME, VOUCHER_HEADER,
    decode_voucher_header, encode_voucher_header,
};
pub use status::{
    ChannelStatus, ObserverView, RefundDecision, SettleDecision, SettleWindow, WatchStep,
    decide_refund, decide_settle, watch_step,
};
pub use store::{AcceptedCall, ChannelDirectory, ChannelStore};
