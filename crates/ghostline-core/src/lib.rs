// SPDX-License-Identifier: MIT

//! Voucher and channel-identity primitives.
//!
//! The encodings here must match the channel template byte for byte: the template recomputes the channel id from its
//! bound arguments and verifies the voucher signature over `channel_id || amount_le`.

use blake2::{Blake2b, Digest, digest::consts::U32};
use ootle_byte_type::ToByteType;
use tari_crypto::{
    keys::PublicKey as _,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
};
use tari_ootle_common_types::RistrettoSchnorrBlake2bVerifier;
use tari_template_lib::types::{
    Hash32,
    bytes::Bytes,
    crypto::{NoSignatureDomain, RistrettoPublicKeyBytes, Signature},
};

pub const VOUCHER_DOMAIN: &[u8] = b"ghostline.channel.voucher.v1";
pub const CHANNEL_ID_DOMAIN: &[u8] = b"ghostline.channel.id.v1";
pub const NONCE_LEN: usize = 32;
pub const CHANNEL_ID_LEN: usize = 32;
pub const MESSAGE_LEN: usize = CHANNEL_ID_LEN + 8;

type Blake2b256 = Blake2b<U32>;

/// The parameters a payer fixes when opening a channel. Every field is bound into the channel id.
#[derive(Debug, Clone)]
pub struct ChannelParams {
    pub network_tag: u8,
    pub payer: RistrettoPublicKeyBytes,
    pub deposit: u64,
    pub payer_root: Hash32,
    pub nonce: [u8; NONCE_LEN],
}

impl ChannelParams {
    pub fn channel_id(&self) -> [u8; CHANNEL_ID_LEN] {
        let mut hasher = Blake2b256::new();
        hasher.update(CHANNEL_ID_DOMAIN);
        hasher.update([self.network_tag]);
        hasher.update(self.payer.as_bytes());
        hasher.update(self.deposit.to_le_bytes());
        hasher.update(self.payer_root.as_slice());
        hasher.update(self.nonce);
        hasher.finalize().into()
    }
}

pub fn voucher_message(channel_id: &[u8; CHANNEL_ID_LEN], amount: u64) -> [u8; MESSAGE_LEN] {
    let mut message = [0u8; MESSAGE_LEN];
    message[..CHANNEL_ID_LEN].copy_from_slice(channel_id);
    message[CHANNEL_ID_LEN..].copy_from_slice(&amount.to_le_bytes());
    message
}

/// Signs a voucher for the cumulative `amount` owed on the channel described by `params`.
pub fn sign_voucher(
    secret: &RistrettoSecretKey,
    params: &ChannelParams,
    amount: u64,
) -> Signature<NoSignatureDomain> {
    let message = voucher_message(&params.channel_id(), amount);
    sign_with_domain(secret, VOUCHER_DOMAIN, &message)
}

fn sign_with_domain(secret: &RistrettoSecretKey, domain: &[u8], message: &[u8]) -> Signature<NoSignatureDomain> {
    let (nonce, nonce_public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
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

/// The witness data a settling spend must carry: the cumulative amount and the payer's signature over it.
pub fn witness_data(amount: u64, signature: &Signature<NoSignatureDomain>) -> Bytes {
    Bytes::from_vec(tari_bor::encode(&(amount, signature)).expect("voucher witness encodes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ChannelParams {
        ChannelParams {
            network_tag: 1,
            payer: RistrettoPublicKeyBytes::from_bytes(&[7u8; 32]).unwrap(),
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        }
    }

    #[test]
    fn channel_id_is_deterministic() {
        assert_eq!(params().channel_id(), params().channel_id());
    }

    #[test]
    fn channel_id_binds_every_field() {
        let base = params().channel_id();

        let mut other = params();
        other.network_tag = 2;
        assert_ne!(base, other.channel_id());

        let mut other = params();
        other.deposit = 101;
        assert_ne!(base, other.channel_id());

        let mut other = params();
        other.payer_root = Hash32::from_array([8u8; 32]);
        assert_ne!(base, other.channel_id());

        let mut other = params();
        other.nonce = [4u8; NONCE_LEN];
        assert_ne!(base, other.channel_id());
    }

    #[test]
    fn voucher_message_has_one_parse() {
        let message = voucher_message(&[5u8; CHANNEL_ID_LEN], 0x0102_0304_0506_0708);
        assert_eq!(&message[..CHANNEL_ID_LEN], &[5u8; CHANNEL_ID_LEN]);
        assert_eq!(&message[CHANNEL_ID_LEN..], &0x0102_0304_0506_0708u64.to_le_bytes());
    }
}
