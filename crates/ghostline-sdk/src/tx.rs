// SPDX-License-Identifier: MIT

use ootle_network::Network;
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_ootle_transaction::{Epoch, Transaction, TransactionEnvelope};
use tari_template_lib::types::{ResourceAddress, stealth::StealthTransferStatement};

use crate::SdkError;

/// Wraps a stealth transfer statement in a transaction for `network` and seals it.
///
/// This does not submit anything. A wallet/indexer client is still required to put the transaction on a network.
pub fn seal_stealth_transfer(
    network: Network,
    resource: ResourceAddress,
    statement: StealthTransferStatement,
    max_epoch: Epoch,
    sealer: &RistrettoSecretKey,
) -> Transaction {
    Transaction::builder(network, max_epoch)
        .stealth_transfer(resource, statement)
        .finish()
        .seal(sealer)
}

/// Wraps a stealth transfer statement in a localnet transaction and seals it.
///
/// This does not submit anything. A wallet/indexer client is still required to put the transaction on a network.
pub fn seal_localnet_stealth_transfer(
    resource: ResourceAddress,
    statement: StealthTransferStatement,
    max_epoch: Epoch,
    sealer: &RistrettoSecretKey,
) -> Transaction {
    seal_stealth_transfer(Network::LocalNet, resource, statement, max_epoch, sealer)
}

/// BOR-encode a sealed transaction as the indexer `SubmitTransactionRequest.transaction` envelope.
pub fn encode_transaction_envelope(
    transaction: Transaction,
) -> Result<TransactionEnvelope, SdkError> {
    TransactionEnvelope::encode(transaction).map_err(|_| SdkError::Encode)
}

/// Hex of the BOR envelope, for `ghostline indexer submit --tx-hex`.
pub fn transaction_envelope_hex(transaction: Transaction) -> Result<String, SdkError> {
    let envelope = encode_transaction_envelope(transaction)?;
    Ok(ghostline_core::to_hex(&envelope.0))
}

/// Seal a stealth transfer for `network` and return the submit hex.
pub fn seal_hex(
    network: Network,
    resource: ResourceAddress,
    statement: StealthTransferStatement,
    max_epoch: u64,
    sealer: &RistrettoSecretKey,
) -> Result<String, SdkError> {
    transaction_envelope_hex(seal_stealth_transfer(
        network,
        resource,
        statement,
        Epoch(max_epoch),
        sealer,
    ))
}

/// Seal a localnet stealth transfer and return the submit hex.
pub fn seal_localnet_hex(
    resource: ResourceAddress,
    statement: StealthTransferStatement,
    max_epoch: u64,
    sealer: &RistrettoSecretKey,
) -> Result<String, SdkError> {
    seal_hex(Network::LocalNet, resource, statement, max_epoch, sealer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, ChannelSpec, MaskAndValue, settle};
    use ghostline_core::{NONCE_LEN, SignedVoucher};
    use ootle_byte_type::ToByteType;
    use tari_crypto::keys::SecretKey as _;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;
    use tari_template_lib::types::constants::STEALTH_TARI_RESOURCE_ADDRESS;

    #[test]
    fn seals_a_settle_statement() {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (service_secret, service_public) = create_key_pair_from_seed(13);
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            deposit: 100,
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let voucher = SignedVoucher::sign(&payer_secret, channel.params(), 30).unwrap();
        let funded = MaskAndValue {
            value: 100,
            mask: tari_crypto::ristretto::RistrettoSecretKey::random(&mut rand::rng()),
        };
        let statement = settle(
            &channel,
            funded,
            &voucher,
            service_public.to_byte_type(),
            crate::OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
        )
        .unwrap();
        let tx = seal_localnet_stealth_transfer(
            STEALTH_TARI_RESOURCE_ADDRESS,
            statement,
            Epoch(1),
            &service_secret,
        );
        assert_eq!(tx.network(), Network::LocalNet.as_byte());
        assert_eq!(tx.calculate_id(), tx.calculate_id());
        let hex = transaction_envelope_hex(tx).unwrap();
        assert!(hex.len() > 2);
        assert!(hex.len().is_multiple_of(2));
    }

    #[test]
    fn seals_esmeralda_network_byte() {
        use crate::{OutputScan, fund_open};

        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            deposit: 100,
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let opened = fund_open(
            &channel,
            [MaskAndValue {
                value: 100,
                mask: tari_crypto::ristretto::RistrettoSecretKey::random(&mut rand::rng()),
            }],
            channel.spec().payer,
            OutputScan::esmeralda(STEALTH_TARI_RESOURCE_ADDRESS),
        )
        .unwrap();
        let tx = seal_stealth_transfer(
            Network::Esmeralda,
            STEALTH_TARI_RESOURCE_ADDRESS,
            opened.statement,
            Epoch(1),
            &payer_secret,
        );
        assert_eq!(tx.network(), Network::Esmeralda.as_byte());
    }
}
