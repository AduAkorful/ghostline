// SPDX-License-Identifier: MIT

use ghostline_core::{CHANNEL_ID_LEN, SignedVoucher, from_hex, to_hex};
use serde::{Deserialize, Serialize};

use crate::PaywallError;

/// Ghostline's own payment-required scheme. Not EVM x402.
pub const SCHEME: &str = "ghostline.channel.voucher.v1";
pub const VOUCHER_HEADER: &str = "ghostline-voucher";
/// Hex channel id. Required on a 402 when the service holds more than one ledger.
pub const CHANNEL_HEADER: &str = "ghostline-channel";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentRequired {
    pub scheme: String,
    /// Amount due for this call, in micro-units. For a metered quote this is `unit_price * units`.
    pub price: u64,
    pub channel_id: String,
    /// `fixed`, `per_request`, or `metered`. Absent on quotes built before pricing bases existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_price: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<u64>,
}

impl PaymentRequired {
    pub fn new(price: u64, channel_id: &[u8; CHANNEL_ID_LEN]) -> Self {
        Self {
            scheme: SCHEME.to_string(),
            price,
            channel_id: to_hex(channel_id),
            basis: None,
            unit_price: None,
            units: None,
        }
    }

    pub fn from_quote(quote: &crate::Quote, channel_id: &[u8; CHANNEL_ID_LEN]) -> Self {
        Self {
            scheme: SCHEME.to_string(),
            price: quote.price,
            channel_id: to_hex(channel_id),
            basis: Some(quote.basis.to_string()),
            unit_price: quote.unit_price,
            units: quote.units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentRejection {
    pub scheme: String,
    pub status: u16,
    pub reason: String,
}

impl PaymentRejection {
    pub fn from_error(err: &PaywallError) -> Self {
        Self {
            scheme: SCHEME.to_string(),
            status: err.status(),
            reason: err.to_string(),
        }
    }
}

pub fn encode_voucher_header(voucher: &SignedVoucher) -> String {
    to_hex(&voucher.to_bytes())
}

pub fn decode_voucher_header(hex: &str) -> Result<SignedVoucher, PaywallError> {
    let bytes = from_hex(hex.trim())
        .map_err(|_| PaywallError::Voucher(ghostline_core::VoucherError::Malformed))?;
    SignedVoucher::from_bytes(&bytes).map_err(PaywallError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_required_round_trips_json() {
        let body = PaymentRequired::new(10, &[0xab; CHANNEL_ID_LEN]);
        let json = serde_json::to_string(&body).unwrap();
        let back: PaymentRequired = serde_json::from_str(&json).unwrap();
        assert_eq!(back.price, 10);
        assert_eq!(back.scheme, SCHEME);
        assert_eq!(back.channel_id.len(), CHANNEL_ID_LEN * 2);
    }
}
