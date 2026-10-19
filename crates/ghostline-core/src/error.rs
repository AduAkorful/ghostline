// SPDX-License-Identifier: MIT

use std::fmt;

/// Distinct reasons a voucher or ledger update is rejected. Money-path callers must match on these, not on strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoucherError {
    InvalidSignature,
    InvalidCommitment,
    InvalidServiceOutputKey,
    ExceedsDeposit { amount: u64, deposit: u64 },
    NotIncreased { amount: u64, accepted: u64 },
    ChannelMismatch,
    Overflow,
    ZeroAmount,
    Malformed,
}

impl fmt::Display for VoucherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "invalid voucher signature"),
            Self::InvalidCommitment => {
                write!(f, "voucher amount and mask do not open its commitment")
            }
            Self::InvalidServiceOutputKey => {
                write!(f, "voucher service output key is not a valid Ristretto key")
            }
            Self::ExceedsDeposit { amount, deposit } => {
                write!(
                    f,
                    "voucher exceeds channel spend limit ({amount} > {deposit})"
                )
            }
            Self::NotIncreased { amount, accepted } => {
                write!(
                    f,
                    "voucher amount {amount} does not exceed accepted {accepted}"
                )
            }
            Self::ChannelMismatch => write!(f, "voucher channel id does not match this channel"),
            Self::Overflow => write!(f, "amount overflow"),
            Self::ZeroAmount => write!(f, "voucher amount must be greater than zero"),
            Self::Malformed => write!(f, "voucher encoding malformed"),
        }
    }
}

impl std::error::Error for VoucherError {}
