// SPDX-License-Identifier: MIT

use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
use tari_ootle_wallet_crypto::{MaskAndValue, StealthCryptoApi};
use tari_template_lib::types::{crypto::UtxoTag, stealth::UnspentOutput};
use tari_utilities::ByteArray;

use crate::{OutputScan, SdkError};

/// Recovers value and mask if `tag` matches the Ootle scan KDF for `claim_secret`.
///
/// The claim secret is the recipient spend/view key the output was encrypted to (payer for a
/// channel deposit, change, remainder, or refund; service for a settle payout). This does not talk
/// to a wallet daemon.
pub fn unblind_output(
    output: &UnspentOutput,
    tag: UtxoTag,
    claim_secret: &RistrettoSecretKey,
    scan: OutputScan,
) -> Result<MaskAndValue, SdkError> {
    let nonce = RistrettoPublicKey::from_canonical_bytes(output.sender_public_nonce.as_bytes())
        .map_err(|_| SdkError::InvalidKey)?;
    let expected = StealthCryptoApi::new().derive_stealth_output_tag(
        scan.network,
        claim_secret,
        &nonce,
        &scan.resource,
    );
    if expected != tag {
        return Err(SdkError::TagMismatch);
    }
    let decrypted = StealthCryptoApi::new()
        .decrypt_utxo_data(
            &output.encrypted_data,
            &output.commitment,
            claim_secret,
            &nonce,
            true,
        )
        .map_err(|err| SdkError::Decrypt(err.to_string()))?;
    Ok(decrypted.into_mask_and_value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, ChannelSpec, fund_open};
    use tari_ootle_wallet_crypto::MaskAndValue;
    use ghostline_core::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use tari_crypto::keys::SecretKey as _;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;
    use tari_template_lib::types::constants::STEALTH_TARI_RESOURCE_ADDRESS;
    use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

    fn setup() -> (
        RistrettoSecretKey,
        crate::OpenStatement,
        OutputScan,
    ) {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let payer: RistrettoPublicKeyBytes = payer_public.to_byte_type();
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer,
            service: service_public.to_byte_type(),
            deposit: 100,
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let scan = OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS);
        let funded = MaskAndValue {
            value: 100,
            mask: RistrettoSecretKey::random(&mut rand::rng()),
        };
        let opened = fund_open(&channel, [funded], payer, scan).unwrap();
        (payer_secret, opened, scan)
    }

    #[test]
    fn payer_unblinds_the_deposit() {
        let (secret, opened, scan) = setup();
        let out = &opened.statement.outputs_statement.outputs[0];
        let recovered = unblind_output(&out.output, out.tag, &secret, scan).unwrap();
        assert_eq!(recovered.value, 100);
        assert_eq!(recovered.mask, opened.deposit.mask);
    }

    #[test]
    fn wrong_secret_does_not_match_the_tag() {
        let (_secret, opened, scan) = setup();
        let other = RistrettoSecretKey::random(&mut rand::rng());
        let out = &opened.statement.outputs_statement.outputs[0];
        let err = unblind_output(&out.output, out.tag, &other, scan).unwrap_err();
        assert!(matches!(err, SdkError::TagMismatch));
    }
}
