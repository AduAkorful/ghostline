// SPDX-License-Identifier: MIT

use ootle_byte_type::ToByteType;
use tari_crypto::{
    keys::SecretKey,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    tari_utilities::ByteArray,
};
use tari_ootle_common_types::{RistrettoSchnorrBlake2bVerifier, Verifier};
use tari_ootle_wallet_crypto::MaskAndValue;
use tari_template_lib::types::{
    bytes::Bytes,
    crypto::{
        NoSignatureDomain, PedersenCommitmentBytes, PublicKey, RistrettoPublicKeyBytes, Signature,
    },
};

use crate::{
    CHANNEL_ID_LEN, MESSAGE_LEN, VOUCHER_DOMAIN, VoucherError, params::ChannelParams,
    sign_with_domain,
};

/// Message signed by the payer: channel identity, authorized service output key, and its hidden-value commitment.
pub fn voucher_message(
    channel_id: &[u8; CHANNEL_ID_LEN],
    service_output_key: &RistrettoPublicKeyBytes,
    commitment: &PedersenCommitmentBytes,
) -> [u8; MESSAGE_LEN] {
    let mut message = [0u8; MESSAGE_LEN];
    message[..CHANNEL_ID_LEN].copy_from_slice(channel_id);
    message[CHANNEL_ID_LEN..CHANNEL_ID_LEN + 32].copy_from_slice(service_output_key.as_bytes());
    message[CHANNEL_ID_LEN + 32..].copy_from_slice(commitment.as_bytes());
    message
}

/// Off-chain opening of a signed service-output commitment. Amount and mask are never included in the on-chain
/// witness; they let the service verify the voucher and later open its stealth output.
pub struct SignedVoucher {
    pub channel_id: [u8; CHANNEL_ID_LEN],
    pub amount: u64,
    pub mask: RistrettoSecretKey,
    pub service_output_key: RistrettoPublicKeyBytes,
    pub output_commitment: PedersenCommitmentBytes,
    pub signature: Signature<NoSignatureDomain>,
}

impl std::fmt::Debug for SignedVoucher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedVoucher")
            .field("channel_id", &self.channel_id)
            .field("amount", &self.amount)
            .field("service_output_key", &self.service_output_key)
            .finish_non_exhaustive()
    }
}

impl Clone for SignedVoucher {
    fn clone(&self) -> Self {
        Self {
            channel_id: self.channel_id,
            amount: self.amount,
            mask: self.mask.clone(),
            service_output_key: self.service_output_key,
            output_commitment: self.output_commitment,
            signature: Signature::new(self.signature.payload().clone()),
        }
    }
}

impl SignedVoucher {
    pub fn sign(
        secret: &RistrettoSecretKey,
        params: &ChannelParams,
        amount: u64,
        service_output_key: RistrettoPublicKeyBytes,
    ) -> Result<Self, VoucherError> {
        Self::sign_with_nonce(secret, params, amount, service_output_key, None)
    }

    /// `nonce = None` draws a fresh Schnorr nonce. The output mask is always fresh and independent.
    pub fn sign_with_nonce(
        secret: &RistrettoSecretKey,
        params: &ChannelParams,
        amount: u64,
        service_output_key: RistrettoPublicKeyBytes,
        nonce: Option<RistrettoSecretKey>,
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
        let mask = RistrettoSecretKey::random(&mut rand::rng());
        let output_commitment = MaskAndValue::new(amount, mask.clone())
            .to_commitment()
            .to_byte_type();
        let message = voucher_message(&channel_id, &service_output_key, &output_commitment);
        let signature = match nonce {
            Some(n) => crate::sign_with_nonce(secret, n, VOUCHER_DOMAIN, &message),
            None => sign_with_domain(secret, VOUCHER_DOMAIN, &message),
        };
        Ok(Self {
            channel_id,
            amount,
            mask,
            service_output_key,
            output_commitment,
            signature,
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
        if RistrettoPublicKey::from_canonical_bytes(self.service_output_key.as_bytes()).is_err() {
            return Err(VoucherError::InvalidServiceOutputKey);
        }
        let expected = MaskAndValue::new(self.amount, self.mask.clone())
            .to_commitment()
            .to_byte_type();
        if expected != self.output_commitment {
            return Err(VoucherError::InvalidCommitment);
        }
        let message = voucher_message(
            &self.channel_id,
            &self.service_output_key,
            &self.output_commitment,
        );
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

    /// On-chain witness: only the commitment, recipient authorization key, and payer signature.
    pub fn witness_data(&self) -> Bytes {
        Bytes::from_vec(
            tari_bor::encode(&(
                self.output_commitment,
                self.service_output_key,
                &self.signature,
            ))
            .expect("voucher witness encodes"),
        )
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        tari_bor::encode(&(
            self.channel_id,
            self.amount,
            self.mask.as_bytes(),
            self.service_output_key,
            self.output_commitment,
            &self.signature,
        ))
        .expect("voucher encodes")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VoucherError> {
        let (channel_id, amount, mask_bytes, service_output_key, output_commitment, signature): (
            [u8; CHANNEL_ID_LEN],
            u64,
            Vec<u8>,
            RistrettoPublicKeyBytes,
            PedersenCommitmentBytes,
            Signature<NoSignatureDomain>,
        ) = tari_bor::decode(bytes).map_err(|_| VoucherError::Malformed)?;
        let mask = RistrettoSecretKey::from_canonical_bytes(&mask_bytes)
            .map_err(|_| VoucherError::Malformed)?;
        Ok(Self {
            channel_id,
            amount,
            mask,
            service_output_key,
            output_commitment,
            signature,
        })
    }
}

pub fn sign_voucher(
    secret: &RistrettoSecretKey,
    params: &ChannelParams,
    amount: u64,
    service_output_key: RistrettoPublicKeyBytes,
) -> Result<SignedVoucher, VoucherError> {
    SignedVoucher::sign(secret, params, amount, service_output_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;

    fn seeded_params(seed: u8) -> (RistrettoSecretKey, ChannelParams, RistrettoPublicKeyBytes) {
        let (secret, public) = create_key_pair_from_seed(seed);
        let (_service_secret, service_public) = create_key_pair_from_seed(seed + 20);
        let params = ChannelParams {
            network_tag: 1,
            payer: public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        (secret, params, service_public.to_byte_type())
    }

    #[test]
    fn message_binds_channel_service_and_commitment() {
        let service = RistrettoPublicKeyBytes::from_bytes(&[6u8; 32]).unwrap();
        let commitment = PedersenCommitmentBytes::from_array([7u8; 32]);
        let message = voucher_message(&[5u8; CHANNEL_ID_LEN], &service, &commitment);
        assert_eq!(&message[..CHANNEL_ID_LEN], &[5u8; 32]);
        assert_eq!(
            &message[CHANNEL_ID_LEN..CHANNEL_ID_LEN + 32],
            service.as_bytes()
        );
        assert_eq!(&message[CHANNEL_ID_LEN + 32..], commitment.as_bytes());
    }

    #[test]
    fn signed_voucher_verifies_and_witness_omits_opening() {
        let (secret, params, service) = seeded_params(11);
        let voucher = SignedVoucher::sign(&secret, &params, 30, service).unwrap();
        voucher.verify(&params).unwrap();
        let witness = tari_bor::decode::<(
            PedersenCommitmentBytes,
            RistrettoPublicKeyBytes,
            Signature<NoSignatureDomain>,
        )>(&voucher.witness_data())
        .unwrap();
        assert_eq!(witness.0, voucher.output_commitment);
        let decoded = SignedVoucher::from_bytes(&voucher.to_bytes()).unwrap();
        decoded.verify(&params).unwrap();
        assert_eq!(decoded.amount, voucher.amount);
    }

    #[test]
    fn wrong_opening_fails_validation() {
        let (secret, params, service) = seeded_params(11);
        let mut voucher = SignedVoucher::sign(&secret, &params, 30, service).unwrap();
        voucher.amount = 31;
        assert_eq!(
            voucher.verify(&params),
            Err(VoucherError::InvalidCommitment)
        );
    }

    #[test]
    fn forged_signature_is_rejected() {
        let (_, params, service) = seeded_params(11);
        let (other, _) = create_key_pair_from_seed(12);
        let voucher = SignedVoucher::sign(&other, &params, 30, service).unwrap();
        assert_eq!(voucher.verify(&params), Err(VoucherError::InvalidSignature));
    }

    #[test]
    fn amount_above_deposit_is_rejected() {
        let (secret, params, service) = seeded_params(11);
        assert!(matches!(
            SignedVoucher::sign(&secret, &params, 101, service),
            Err(VoucherError::ExceedsDeposit { .. })
        ));
    }
}
