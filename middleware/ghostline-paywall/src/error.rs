// SPDX-License-Identifier: MIT

use std::fmt;

use ghostline_core::VoucherError;
use ghostline_policy::PolicyError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaywallError {
    MissingVoucher,
    WrongPrice { paid: u64, price: u64 },
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
            Self::Voucher(VoucherError::Malformed | VoucherError::ZeroAmount) => 400,
            Self::Voucher(_) | Self::Storage(_) => 403,
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
