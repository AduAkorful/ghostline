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
use tari_crypto::{
    keys::PublicKey as _,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
};
use tari_ootle_common_types::RistrettoSchnorrBlake2bVerifier;
use tari_template_lib::types::crypto::{NoSignatureDomain, Signature};

pub use error::VoucherError;
pub use params::ChannelParams;
pub use private_file::write_private_file;
pub use session::{PaymentSession, ServiceLedger};
pub use voucher::{SignedVoucher, sign_voucher, voucher_message};

pub const VOUCHER_DOMAIN: &[u8] = b"ghostline.channel.voucher.v1";
pub const CHANNEL_ID_DOMAIN: &[u8] = b"ghostline.channel.id.v1";
pub const NONCE_LEN: usize = 32;
pub const CHANNEL_ID_LEN: usize = 32;
pub const MESSAGE_LEN: usize = CHANNEL_ID_LEN + 32 + 32;

fn sign_with_domain(
    secret: &RistrettoSecretKey,
    domain: &[u8],
    message: &[u8],
) -> Signature<NoSignatureDomain> {
    let (nonce, _) = RistrettoPublicKey::random_keypair(&mut rand::rng());
    sign_with_nonce(secret, nonce, domain, message)
}

/// Same challenge as `RistrettoSchnorrBlake2bVerifier` (`tari_ootle_common_types` 0.45).
pub fn sign_with_nonce(
    secret: &RistrettoSecretKey,
    nonce: RistrettoSecretKey,
    domain: &[u8],
    message: &[u8],
) -> Signature<NoSignatureDomain> {
    let nonce_public = RistrettoPublicKey::from_secret_key(&nonce);
    let public_key = RistrettoPublicKey::from_secret_key(secret);
    let challenge = RistrettoSchnorrBlake2bVerifier::compute_challenge(
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
