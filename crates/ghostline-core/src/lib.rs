// SPDX-License-Identifier: MIT

//! Voucher and channel-identity primitives.
//!
//! The encodings here must match the channel template byte for byte: the template recomputes the channel id from its
//! bound arguments and verifies the voucher signature over `channel_id || service_key || output_commitment`.

mod error;
mod params;
mod private_file;
mod session;
mod voucher;

use ootle_byte_type::ToByteType;
use ootle_network::Network;
use tari_crypto::{
    keys::PublicKey as _,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
};
use tari_ootle_common_types::{RistrettoSchnorrBlake2bVerifier, TemplateSignatureBinding};
use tari_template_lib::types::crypto::{NoSignatureDomain, Signature};

pub use error::VoucherError;
pub use params::ChannelParams;
pub use private_file::write_private_file;
pub use session::{PaymentSession, ServiceLedger};
pub use voucher::{SignedVoucher, sign_voucher, voucher_message};

pub const VOUCHER_DOMAIN: &[u8] = b"ghostline.channel.voucher.v1";
pub const BATCH_BALANCE_DOMAIN: &[u8] = b"ghostline.channel.batch-balance.v1";
pub const CHANNEL_ID_DOMAIN: &[u8] = b"ghostline.channel.id.v1";
pub const NONCE_LEN: usize = 32;
pub const CHANNEL_ID_LEN: usize = 32;
pub const MESSAGE_LEN: usize = CHANNEL_ID_LEN + 32 + 32;
/// Keep one micro-unit of payer change so every settlement has the same public output count.
pub const SETTLEMENT_RESERVE: u64 = 1;
/// Protocol fee divisor; must equal `FEE_DIVISOR` in the channel template. The fee is `payout / FEE_DIVISOR`.
pub const FEE_DIVISOR: u64 = 100;

/// Protocol fee owed on a settled service payout. Payouts that are not a multiple of [`FEE_DIVISOR`] cannot settle,
/// because the template checks the fee as an exact quotient on commitments.
pub fn settlement_fee(payout: u64) -> Result<u64, VoucherError> {
    if !payout.is_multiple_of(FEE_DIVISOR) {
        return Err(VoucherError::NotFeeAligned {
            amount: payout,
            divisor: FEE_DIVISOR,
        });
    }
    Ok(payout / FEE_DIVISOR)
}

fn sign_with_domain(
    secret: &RistrettoSecretKey,
    network_tag: u8,
    domain: &[u8],
    message: &[u8],
) -> Signature<NoSignatureDomain> {
    let (nonce, _) = RistrettoPublicKey::random_keypair(&mut rand::rng());
    sign_with_nonce(secret, nonce, network_tag, domain, message)
}

/// Schnorr challenge for a template signature in the shared scope (`tari_ootle_common_types` 0.49).
///
/// `signature.verify` in the channel template uses that scope, so the challenge binds `network_tag`
/// and the domain. `network_tag` must be an Ootle network byte (mainnet is 0, Esmeralda is 38).
pub fn sign_with_nonce(
    secret: &RistrettoSecretKey,
    nonce: RistrettoSecretKey,
    network_tag: u8,
    domain: &[u8],
    message: &[u8],
) -> Signature<NoSignatureDomain> {
    let network =
        Network::try_from(network_tag).expect("channel network tag is not an Ootle network byte");
    let binding = TemplateSignatureBinding::shared(network);
    let nonce_public = RistrettoPublicKey::from_secret_key(&nonce);
    let public_key = RistrettoPublicKey::from_secret_key(secret);
    let challenge = RistrettoSchnorrBlake2bVerifier::compute_challenge(
        &binding,
        domain,
        message,
        &public_key.to_byte_type(),
        &nonce_public.to_byte_type(),
    );
    RistrettoSchnorr::sign_raw_uniform(secret, nonce, &challenge)
        .expect("signing with a valid nonce and challenge cannot fail")
        .to_byte_type()
        .into()
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(hex: &str) -> Result<Vec<u8>, VoucherError> {
    if !hex.len().is_multiple_of(2) {
        return Err(VoucherError::Malformed);
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| VoucherError::Malformed))
        .collect()
}

#[cfg(test)]
mod fee_tests {
    use super::*;

    const TEMPLATE_SOURCE: &str = include_str!("../../../contracts/channel/src/lib.rs");

    fn template_const(name: &str) -> String {
        let start = TEMPLATE_SOURCE
            .find(&format!("pub const {name}"))
            .expect("template constant exists");
        let rest = &TEMPLATE_SOURCE[start..];
        let rest = &rest[rest.find('=').unwrap() + 1..];
        let value = &rest[..rest.find(';').unwrap()];
        value.split_whitespace().collect()
    }

    #[test]
    fn fee_divisor_matches_the_template() {
        assert_eq!(template_const("FEE_DIVISOR"), FEE_DIVISOR.to_string());
    }

    #[test]
    fn fee_is_exact_quotient() {
        assert_eq!(settlement_fee(100), Ok(1));
        assert_eq!(settlement_fee(10_000), Ok(100));
        assert_eq!(settlement_fee(0), Ok(0));
        assert_eq!(
            settlement_fee(150),
            Err(VoucherError::NotFeeAligned {
                amount: 150,
                divisor: FEE_DIVISOR
            })
        );
    }
}
