// SPDX-License-Identifier: MIT

use ghostline_core::{CHANNEL_ID_LEN, SignedVoucher, from_hex, to_hex};
use serde::{Deserialize, Serialize};
use tari_crypto::{ristretto::RistrettoPublicKey, tari_utilities::ByteArray};
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

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
    /// Public key authorized to settle a channel opened for this service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient_key: Option<String>,
    /// Optional one-time key that receives the hidden settlement output. The service authorization key still
    /// authorizes the channel close; this key is independently selected by the service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_output_key: Option<String>,
    /// Inclusive expiry range accepted by this service for a new channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_expiry_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_expiry_epoch: Option<u64>,
}

/// Public channel-opening terms advertised by a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentTerms {
    pub recipient_key: String,
    pub service_output_key: Option<String>,
    pub min_expiry_epoch: u64,
    pub max_expiry_epoch: u64,
}

impl PaymentTerms {
    pub fn new(
        recipient_key: RistrettoPublicKeyBytes,
        min_expiry_epoch: u64,
        max_expiry_epoch: u64,
    ) -> Result<Self, PaywallError> {
        if min_expiry_epoch > max_expiry_epoch {
            return Err(PaywallError::InvalidTerms(
                "minimum expiry epoch must be at or before maximum expiry epoch".into(),
            ));
        }
        Ok(Self {
            recipient_key: to_hex(recipient_key.as_bytes()),
            service_output_key: None,
            min_expiry_epoch,
            max_expiry_epoch,
        })
    }

    /// Set the fresh public key that receives this channel's eventual settlement output.
    pub fn with_service_output_key(mut self, key: RistrettoPublicKeyBytes) -> Self {
        self.service_output_key = Some(to_hex(key.as_bytes()));
        self
    }

    pub fn service_output_public_key(
        &self,
    ) -> Result<Option<RistrettoPublicKeyBytes>, PaywallError> {
        self.service_output_key
            .as_deref()
            .map(|hex| {
                RistrettoPublicKeyBytes::from_hex(hex)
                    .map_err(|_| PaywallError::InvalidTerms("malformed service output key".into()))
            })
            .transpose()
    }
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
            recipient_key: None,
            service_output_key: None,
            min_expiry_epoch: None,
            max_expiry_epoch: None,
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
            recipient_key: None,
            service_output_key: None,
            min_expiry_epoch: None,
            max_expiry_epoch: None,
        }
    }

    pub fn with_terms(mut self, terms: &PaymentTerms) -> Self {
        self.recipient_key = Some(terms.recipient_key.clone());
        self.service_output_key = terms.service_output_key.clone();
        self.min_expiry_epoch = Some(terms.min_expiry_epoch);
        self.max_expiry_epoch = Some(terms.max_expiry_epoch);
        self
    }

    /// Return the advertised settlement-output key, if the service supplied one.
    pub fn service_output_public_key(
        &self,
    ) -> Result<Option<RistrettoPublicKeyBytes>, PaywallError> {
        self.service_output_key
            .as_deref()
            .map(|hex| {
                let bytes = from_hex(hex).map_err(|_| {
                    PaywallError::InvalidQuote("malformed service output key".into())
                })?;
                RistrettoPublicKey::from_canonical_bytes(&bytes).map_err(|_| {
                    PaywallError::InvalidQuote("malformed service output key".into())
                })?;
                RistrettoPublicKeyBytes::from_bytes(&bytes)
                    .map_err(|_| PaywallError::InvalidQuote("malformed service output key".into()))
            })
            .transpose()
    }

    /// Check that a quote is for the local channel and allowed recipient before signing it.
    /// Quotes predating opening terms remain valid; when terms are present, all three fields are required.
    pub fn validate_for(
        &self,
        channel_id: &[u8; CHANNEL_ID_LEN],
        recipient: &RistrettoPublicKeyBytes,
    ) -> Result<(), PaywallError> {
        self.validate_opening_terms(recipient)?;
        let quoted_id = from_hex(&self.channel_id)
            .map_err(|_| PaywallError::InvalidQuote("malformed channel id".into()))?;
        if quoted_id.as_slice() != channel_id {
            return Err(PaywallError::InvalidQuote(
                "channel id does not match the payer session".into(),
            ));
        }
        Ok(())
    }

    /// Validate the scheme and any advertised recipient/expiry terms before opening a channel.
    pub fn validate_opening_terms(
        &self,
        recipient: &RistrettoPublicKeyBytes,
    ) -> Result<(), PaywallError> {
        if self.scheme != SCHEME {
            return Err(PaywallError::InvalidQuote(format!(
                "unsupported scheme {}",
                self.scheme
            )));
        }
        if let Some(output_key_hex) = &self.service_output_key {
            let output_key = from_hex(output_key_hex)
                .map_err(|_| PaywallError::InvalidQuote("malformed service output key".into()))?;
            RistrettoPublicKey::from_canonical_bytes(&output_key)
                .map_err(|_| PaywallError::InvalidQuote("malformed service output key".into()))?;
        }
        let quoted_id = from_hex(&self.channel_id)
            .map_err(|_| PaywallError::InvalidQuote("malformed channel id".into()))?;
        if <[u8; CHANNEL_ID_LEN]>::try_from(quoted_id.as_slice()).is_err() {
            return Err(PaywallError::InvalidQuote(
                "channel id has the wrong length".into(),
            ));
        }
        match (
            self.recipient_key.as_deref(),
            self.min_expiry_epoch,
            self.max_expiry_epoch,
        ) {
            (None, None, None) => Ok(()),
            (Some(key), Some(min), Some(max)) => {
                let key = RistrettoPublicKeyBytes::from_hex(key)
                    .map_err(|_| PaywallError::InvalidQuote("malformed recipient key".into()))?;
                if &key != recipient {
                    return Err(PaywallError::InvalidQuote(
                        "recipient key does not match the allowed vendor".into(),
                    ));
                }
                if min > max {
                    return Err(PaywallError::InvalidQuote(
                        "minimum expiry exceeds maximum expiry".into(),
                    ));
                }
                Ok(())
            }
            _ => Err(PaywallError::InvalidQuote(
                "recipient key and both expiry bounds must appear together".into(),
            )),
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

    #[test]
    fn payment_terms_validate_range_and_appear_in_quote() {
        let key = RistrettoPublicKeyBytes::from_bytes(&[7u8; 32]).unwrap();
        let output_key = RistrettoPublicKeyBytes::from_bytes(&[8u8; 32]).unwrap();
        assert!(PaymentTerms::new(key, 8, 7).is_err());
        let terms = PaymentTerms::new(key, 8, 12)
            .unwrap()
            .with_service_output_key(output_key);
        let quote = PaymentRequired::new(10, &[0xab; CHANNEL_ID_LEN]).with_terms(&terms);
        let json = serde_json::to_value(quote).unwrap();
        assert_eq!(json["recipient_key"], to_hex(key.as_bytes()));
        assert_eq!(json["service_output_key"], to_hex(output_key.as_bytes()));
        assert_eq!(json["min_expiry_epoch"], 8);
        assert_eq!(json["max_expiry_epoch"], 12);
    }
}
