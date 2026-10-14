// SPDX-License-Identifier: MIT

use blake2::{Blake2b, Digest, digest::consts::U32};
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};

use crate::{CHANNEL_ID_DOMAIN, CHANNEL_ID_LEN, NONCE_LEN};

type Blake2b256 = Blake2b<U32>;

/// The parameters a payer fixes when opening a channel. Every field is bound into the channel id.
#[derive(Debug, Clone, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NONCE_LEN;

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
}
