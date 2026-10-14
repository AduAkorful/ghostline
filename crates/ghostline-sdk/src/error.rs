// SPDX-License-Identifier: MIT

use std::fmt;

use ghostline_core::VoucherError;
use tari_ootle_wallet_crypto::WalletCryptoError;

#[derive(Debug)]
pub enum SdkError {
    ZeroDeposit,
    DepositMismatch { expected: u64, actual: u64 },
    Voucher(VoucherError),
    Wallet(WalletCryptoError),
    InvalidKey,
    Encode,
    Encrypt(String),
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDeposit => write!(f, "channel deposit must be greater than zero"),
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
