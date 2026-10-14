// SPDX-License-Identifier: MIT

use ootle_network::Network;
use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
use tari_ootle_transaction::{Instruction, ResourceAddressRef, Transaction};
use tari_ootle_wallet_crypto::{MaskAndValue, StealthCryptoApi};
use tari_template_lib::types::{ResourceAddress, crypto::UtxoTag, stealth::UnspentOutput};
use tari_utilities::ByteArray;

use crate::{OutputScan, SdkError};

/// Value and mask recovered from one stealth output in a sealed transaction.
#[derive(Debug, Clone)]
pub struct RecoveredOutput {
    pub resource: ResourceAddress,
    pub tag: UtxoTag,
    pub funds: MaskAndValue,
}

/// Public stealth output fields from a sealed transaction. No secret required.
#[derive(Debug, Clone)]
pub struct StealthOutputView {
    pub resource: ResourceAddress,
    pub tag: UtxoTag,
    pub output: UnspentOutput,
}

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

/// Unblinds every tagged stealth output in `tx` that was encrypted to `claim_secret`.
///
/// Skips outputs whose tag does not match (other recipients) and transfers whose resource is a
/// workspace ref rather than a concrete address. Uses the transaction's Ootle network byte, not
/// Ghostline's voucher `network_tag`.
pub fn unblind_transaction(
    tx: &Transaction,
    claim_secret: &RistrettoSecretKey,
) -> Result<Vec<RecoveredOutput>, SdkError> {
    let network =
        Network::try_from(tx.network()).map_err(|_| SdkError::UnknownNetwork(tx.network()))?;
    let mut recovered = Vec::new();
    for view in inspect_transaction(tx) {
        let scan = OutputScan {
            network,
            resource: view.resource,
        };
        match unblind_output(&view.output, view.tag, claim_secret, scan) {
            Ok(funds) => recovered.push(RecoveredOutput {
                resource: view.resource,
                tag: view.tag,
                funds,
            }),
            Err(SdkError::TagMismatch) => {}
            Err(err) => return Err(err),
        }
    }
    Ok(recovered)
}

/// Lists stealth outputs in fee and body instructions. Workspace resource refs are skipped.
pub fn inspect_transaction(tx: &Transaction) -> Vec<StealthOutputView> {
    let mut views = Vec::new();
    for instruction in tx.fee_instructions().iter().chain(tx.instructions()) {
        let Instruction::StealthTransfer {
            resource_address_ref: ResourceAddressRef::Address(resource),
            statement,
            ..
        } = instruction
        else {
            continue;
        };
        for out in &statement.outputs_statement.outputs {
            views.push(StealthOutputView {
                resource: *resource,
                tag: out.tag,
                output: out.output.clone(),
            });
        }
    }
    views
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, ChannelSpec, fund_open};
    use ghostline_core::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use tari_crypto::keys::SecretKey as _;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_ootle_wallet_crypto::MaskAndValue;
    use tari_template_lib::types::Hash32;
    use tari_template_lib::types::constants::STEALTH_TARI_RESOURCE_ADDRESS;
    use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

    fn setup() -> (RistrettoSecretKey, crate::OpenStatement, OutputScan) {
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

    #[test]
    fn payer_unblinds_deposit_from_a_sealed_open() {
        use crate::seal_localnet_stealth_transfer;
        use tari_ootle_transaction::Epoch;

        let (secret, opened, scan) = setup();
        let tx = seal_localnet_stealth_transfer(
            scan.resource,
            opened.statement.clone(),
            Epoch(1),
            &secret,
        );
        let found = unblind_transaction(&tx, &secret).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].funds.value, 100);
        assert_eq!(found[0].funds.mask, opened.deposit.mask);
        assert_eq!(found[0].resource, scan.resource);
    }

    #[test]
    fn inspect_lists_outputs_that_unblind_can_take() {
        use crate::seal_localnet_stealth_transfer;
        use tari_ootle_transaction::Epoch;

        let (secret, opened, scan) = setup();
        let tx = seal_localnet_stealth_transfer(
            scan.resource,
            opened.statement.clone(),
            Epoch(1),
            &secret,
        );
        let views = inspect_transaction(&tx);
        assert_eq!(views.len(), 1);
        assert_eq!(
            views[0].tag,
            opened.statement.outputs_statement.outputs[0].tag
        );
        let recovered = unblind_output(&views[0].output, views[0].tag, &secret, scan).unwrap();
        assert_eq!(recovered.value, 100);
        assert_eq!(recovered.mask, opened.deposit.mask);
    }
}
