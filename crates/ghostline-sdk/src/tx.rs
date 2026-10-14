// SPDX-License-Identifier: MIT

use tari_crypto::ristretto::RistrettoSecretKey;
use tari_ootle_transaction::{Epoch, Transaction};
use tari_template_lib::types::{ResourceAddress, stealth::StealthTransferStatement};

/// Wraps a stealth transfer statement in a localnet transaction and seals it.
///
/// This does not submit anything. A wallet/indexer client is still required to put the transaction on a network.
pub fn seal_localnet_stealth_transfer(
    resource: ResourceAddress,
    statement: StealthTransferStatement,
    max_epoch: Epoch,
    sealer: &RistrettoSecretKey,
) -> Transaction {
    Transaction::builder_localnet(max_epoch)
        .stealth_transfer(resource, statement)
        .finish()
        .seal(sealer)
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
        let statement = settle(&channel, funded, &voucher, service_public.to_byte_type()).unwrap();
        let tx = seal_localnet_stealth_transfer(
            STEALTH_TARI_RESOURCE_ADDRESS,
            statement,
            Epoch(1),
            &service_secret,
        );
        assert_eq!(tx.calculate_id(), tx.calculate_id());
    }
}
