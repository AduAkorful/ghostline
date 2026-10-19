// SPDX-License-Identifier: MIT

use std::fmt;

use ghostline_core::VoucherError;
use ghostline_policy::PolicyError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaywallError {
    MissingVoucher,
    WrongPrice {
        paid: u64,
        price: u64,
    },
    BadPrice(String),
    InvalidTerms(String),
    InvalidQuote(String),
    WrongServiceOutputKey,
    /// The voucher names a channel this service does not have a ledger for.
    UnknownChannel,
    /// More than one ledger, and the request did not name a channel.
    AmbiguousChannel,
    Voucher(VoucherError),
    Storage(String),
}

impl fmt::Display for PaywallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVoucher => write!(f, "ghostline voucher required"),
            Self::WrongPrice { paid, price } => {
                write!(f, "voucher delta {paid} does not match price {price}")
            }
            Self::BadPrice(details) => write!(f, "{details}"),
            Self::InvalidTerms(details) => write!(f, "invalid payment terms: {details}"),
            Self::InvalidQuote(details) => write!(f, "invalid payment quote: {details}"),
            Self::WrongServiceOutputKey => {
                write!(f, "voucher payout key does not match the service quote")
            }
            Self::UnknownChannel => write!(f, "no ledger for this channel"),
            Self::AmbiguousChannel => {
                write!(f, "more than one channel; set the ghostline-channel header")
            }
            Self::Voucher(err) => write!(f, "{err}"),
            Self::Storage(details) => write!(f, "channel store: {details}"),
        }
    }
}

impl std::error::Error for PaywallError {}

impl From<VoucherError> for PaywallError {
    fn from(value: VoucherError) -> Self {
        Self::Voucher(value)
    }
}

impl PaywallError {
    pub fn status(&self) -> u16 {
        match self {
            Self::MissingVoucher | Self::WrongPrice { .. } => 402,
            Self::BadPrice(_)
            | Self::InvalidTerms(_)
            | Self::InvalidQuote(_)
            | Self::AmbiguousChannel
            | Self::Voucher(VoucherError::Malformed | VoucherError::ZeroAmount) => 400,
            Self::Voucher(_)
            | Self::Storage(_)
            | Self::UnknownChannel
            | Self::WrongServiceOutputKey => 403,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentError {
    Policy(PolicyError),
    Voucher(VoucherError),
    Paywall(PaywallError),
    UnexpectedStatus { status: u16, body: String },
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy(err) => write!(f, "{err}"),
            Self::Voucher(err) => write!(f, "{err}"),
            Self::Paywall(err) => write!(f, "{err}"),
            Self::UnexpectedStatus { status, body } => {
                write!(f, "unexpected HTTP {status}: {body}")
            }
        }
    }
}

impl std::error::Error for AgentError {}

impl From<PolicyError> for AgentError {
    fn from(value: PolicyError) -> Self {
        Self::Policy(value)
    }
}

impl From<VoucherError> for AgentError {
    fn from(value: VoucherError) -> Self {
        Self::Voucher(value)
    }
}

impl From<PaywallError> for AgentError {
    fn from(value: PaywallError) -> Self {
        Self::Paywall(value)
    }
}
