// SPDX-License-Identifier: MIT

use tari_crypto::ristretto::RistrettoSecretKey;
use tari_ootle_common_types::{RistrettoSchnorrBlake2bVerifier, Verifier};
use tari_template_lib::types::{
    bytes::Bytes,
    crypto::{NoSignatureDomain, PublicKey, Signature},
};

use crate::{
    CHANNEL_ID_LEN, MESSAGE_LEN, VOUCHER_DOMAIN, VoucherError, params::ChannelParams,
    sign_with_domain,
};

pub fn voucher_message(channel_id: &[u8; CHANNEL_ID_LEN], amount: u64) -> [u8; MESSAGE_LEN] {
    let mut message = [0u8; MESSAGE_LEN];
    message[..CHANNEL_ID_LEN].copy_from_slice(channel_id);
    message[CHANNEL_ID_LEN..].copy_from_slice(&amount.to_le_bytes());
    message
}

/// A signed cumulative voucher. Sequence is not in the signed message; the contract binds amount only.
pub struct SignedVoucher {
    pub channel_id: [u8; CHANNEL_ID_LEN],
    pub amount: u64,
    pub signature: Signature<NoSignatureDomain>,
}

impl std::fmt::Debug for SignedVoucher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedVoucher")
            .field("channel_id", &self.channel_id)
            .field("amount", &self.amount)
            .finish_non_exhaustive()
    }
}

impl Clone for SignedVoucher {
    fn clone(&self) -> Self {
        Self {
            channel_id: self.channel_id,
            amount: self.amount,
            signature: Signature::new(self.signature.payload().clone()),
        }
    }
}

impl SignedVoucher {
    /// Signs a voucher for the cumulative `amount` owed on the channel described by `params`.
    pub fn sign(
        secret: &RistrettoSecretKey,
        params: &ChannelParams,
        amount: u64,
    ) -> Result<Self, VoucherError> {
        if amount == 0 {
            return Err(VoucherError::ZeroAmount);
        }
        if amount > params.deposit {
            return Err(VoucherError::ExceedsDeposit {
                amount,
                deposit: params.deposit,
            });
        }
        let channel_id = params.channel_id();
        let message = voucher_message(&channel_id, amount);
        Ok(Self {
            channel_id,
            amount,
            signature: sign_with_domain(secret, VOUCHER_DOMAIN, &message),
        })
    }

    pub fn verify(&self, params: &ChannelParams) -> Result<(), VoucherError> {
        if self.channel_id != params.channel_id() {
            return Err(VoucherError::ChannelMismatch);
        }
        if self.amount == 0 {
            return Err(VoucherError::ZeroAmount);
        }
        if self.amount > params.deposit {
            return Err(VoucherError::ExceedsDeposit {
                amount: self.amount,
                deposit: params.deposit,
            });
        }
        let message = voucher_message(&self.channel_id, self.amount);
        let ok = RistrettoSchnorrBlake2bVerifier.verify(
            VOUCHER_DOMAIN,
            &message,
            &PublicKey::from(params.payer),
            self.signature.payload(),
        );
        if ok {
            Ok(())
        } else {
            Err(VoucherError::InvalidSignature)
        }
    }

    /// The witness data a settling spend must carry: the cumulative amount and the payer's signature over it.
    pub fn witness_data(&self) -> Bytes {
        Bytes::from_vec(
            tari_bor::encode(&(self.amount, &self.signature)).expect("voucher witness encodes"),
        )
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        tari_bor::encode(&(self.channel_id, self.amount, &self.signature)).expect("voucher encodes")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VoucherError> {
        let (channel_id, amount, signature): (
            [u8; CHANNEL_ID_LEN],
            u64,
            Signature<NoSignatureDomain>,
        ) = tari_bor::decode(bytes).map_err(|_| VoucherError::Malformed)?;
        Ok(Self {
            channel_id,
            amount,
            signature,
        })
    }
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

/// The witness data a settling spend must carry: the cumulative amount and the payer's signature over it.
pub fn witness_data(amount: u64, signature: &Signature<NoSignatureDomain>) -> Bytes {
    Bytes::from_vec(tari_bor::encode(&(amount, signature)).expect("voucher witness encodes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;

    fn seeded_params(seed: u8) -> (RistrettoSecretKey, ChannelParams) {
        let (secret, public) = create_key_pair_from_seed(seed);
        let params = ChannelParams {
            network_tag: 1,
            payer: public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        (secret, params)
    }

    #[test]
    fn voucher_message_has_one_parse() {
        let message = voucher_message(&[5u8; CHANNEL_ID_LEN], 0x0102_0304_0506_0708);
        assert_eq!(&message[..CHANNEL_ID_LEN], &[5u8; CHANNEL_ID_LEN]);
        assert_eq!(
            &message[CHANNEL_ID_LEN..],
            &0x0102_0304_0506_0708u64.to_le_bytes()
        );
    }

    #[test]
    fn signed_voucher_verifies() {
        let (secret, params) = seeded_params(11);
        let voucher = SignedVoucher::sign(&secret, &params, 30).unwrap();
        voucher.verify(&params).unwrap();
    }

    #[test]
    fn forged_voucher_is_rejected() {
        let (_payer, params) = seeded_params(11);
        let (other, _) = seeded_params(12);
        let voucher = SignedVoucher::sign(&other, &params, 30).unwrap();
        assert_eq!(voucher.verify(&params), Err(VoucherError::InvalidSignature));
    }

    #[test]
    fn amount_above_deposit_is_rejected() {
        let (secret, params) = seeded_params(11);
        assert_eq!(
            SignedVoucher::sign(&secret, &params, 101).unwrap_err(),
            VoucherError::ExceedsDeposit {
                amount: 101,
                deposit: 100,
            }
        );
    }

    #[test]
    fn encode_round_trip() {
        let (secret, params) = seeded_params(11);
        let voucher = SignedVoucher::sign(&secret, &params, 30).unwrap();
        let decoded = SignedVoucher::from_bytes(&voucher.to_bytes()).unwrap();
        decoded.verify(&params).unwrap();
        assert_eq!(decoded.amount, 30);
        assert_eq!(decoded.channel_id, voucher.channel_id);
    }
}
