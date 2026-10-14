// SPDX-License-Identifier: MIT

use std::fmt;

use ghostline_core::VoucherError;
use tari_ootle_wallet_crypto::WalletCryptoError;

#[derive(Debug)]
pub enum SdkError {
    ZeroDeposit,
    ZeroInput,
    Overflow,
    InsufficientFunds { have: u64, need: u64 },
    DepositMismatch { expected: u64, actual: u64 },
    Voucher(VoucherError),
    Wallet(WalletCryptoError),
    InvalidKey,
    Encode,
    Encrypt(String),
    Decrypt(String),
    TagMismatch,
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDeposit => write!(f, "channel deposit must be greater than zero"),
            Self::ZeroInput => write!(f, "open inputs must each be greater than zero"),
            Self::Overflow => write!(f, "input values overflow u64"),
            Self::InsufficientFunds { have, need } => {
                write!(f, "inputs total {have} is below channel deposit {need}")
            }
            Self::DepositMismatch { expected, actual } => {
                write!(
                    f,
                    "UTXO value {actual} does not match channel deposit {expected}"
                )
            }
            Self::Voucher(err) => write!(f, "{err}"),
            Self::Wallet(err) => write!(f, "{err}"),
            Self::InvalidKey => write!(f, "invalid ristretto key bytes"),
            Self::Encode => write!(f, "failed to encode bound template arguments"),
            Self::Encrypt(details) => write!(f, "failed to encrypt output data: {details}"),
            Self::Decrypt(details) => write!(f, "failed to decrypt output data: {details}"),
            Self::TagMismatch => write!(f, "utxo tag does not match this claim key and resource"),
        }
    }
}

impl std::error::Error for SdkError {}

impl From<VoucherError> for SdkError {
    fn from(value: VoucherError) -> Self {
        Self::Voucher(value)
    }
}

impl From<WalletCryptoError> for SdkError {
    fn from(value: WalletCryptoError) -> Self {
        Self::Wallet(value)
    }
}
