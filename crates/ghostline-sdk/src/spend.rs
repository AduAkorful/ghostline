// SPDX-License-Identifier: MIT

use ghostline_core::{SignedVoucher, VoucherError};
use tari_crypto::{
    keys::{PublicKey as _, SecretKey as _},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_ootle_wallet_crypto::{
    MaskAndValue, OutputWitness, StealthCryptoApi, StealthInputWitness, StealthOutputWitness,
    stealth::{self, create_transfer_statement},
};
use tari_template_lib::types::{
    Amount,
    crypto::{RistrettoPublicKeyBytes, UtxoTag},
    stealth::{SpendAuthorization, StealthTransferStatement},
};
use tari_utilities::ByteArray;

use crate::{Channel, SdkError};

/// Spends the channel UTXO with a voucher: service receives `amount`, remainder returns under `payer_root`.
///
/// Every output's minimum-value promise equals its value. Zero-value outputs are omitted (a full-deposit voucher
/// produces only the service output).
pub fn settle(
    channel: &Channel,
    deposit: MaskAndValue,
    voucher: &SignedVoucher,
    service_output_key: RistrettoPublicKeyBytes,
) -> Result<StealthTransferStatement, SdkError> {
    if voucher.amount > channel.params().deposit {
        return Err(VoucherError::ExceedsDeposit {
            amount: voucher.amount,
            deposit: channel.params().deposit,
        }
        .into());
    }
    if deposit.value != channel.params().deposit {
        return Err(SdkError::DepositMismatch {
            expected: channel.params().deposit,
            actual: deposit.value,
        });
    }

    let remainder = channel
        .params()
        .deposit
        .checked_sub(voucher.amount)
        .expect("verify rejected amounts above the deposit");

    let (witness, root) = stealth::script_path_witness_with_data(
        &channel.leaves_vec(),
        channel.settle_leaf(),
        voucher.witness_data(),
    )?;
    let input = StealthInputWitness::with_script_path(deposit, witness, root);

    let mut outputs = Vec::new();
    if remainder > 0 {
        outputs.push(output(
            remainder,
            SpendAuthorization::Script(channel.params().payer_root),
            &channel.spec().payer,
        )?);
    }
    if voucher.amount > 0 {
        outputs.push(output(
            voucher.amount,
            SpendAuthorization::Key(service_output_key),
            &service_output_key,
        )?);
    }

    Ok(create_transfer_statement(
        std::iter::once(input),
        Amount::zero(),
        outputs.iter(),
        None,
    )?)
}

/// Spends the channel UTXO on the refund leaf after expiry, paying the full deposit to `payer_output_key`.
pub fn refund(
    channel: &Channel,
    deposit: MaskAndValue,
    payer_output_key: RistrettoPublicKeyBytes,
) -> Result<StealthTransferStatement, SdkError> {
    if deposit.value != channel.params().deposit {
        return Err(SdkError::DepositMismatch {
            expected: channel.params().deposit,
            actual: deposit.value,
        });
    }

    let (witness, root) = stealth::script_path_witness_with_data(
        &channel.leaves_vec(),
        channel.refund_leaf(),
        Default::default(),
    )?;
    let input = StealthInputWitness::with_script_path(deposit, witness, root);
    let outputs = [output(
        channel.params().deposit,
        SpendAuthorization::Key(payer_output_key),
        &payer_output_key,
    )?];

    Ok(create_transfer_statement(
        std::iter::once(input),
        Amount::zero(),
        outputs.iter(),
        None,
    )?)
}

fn output(
    amount: u64,
    auth: SpendAuthorization,
    recipient: &RistrettoPublicKeyBytes,
) -> Result<StealthOutputWitness, SdkError> {
    let mask = RistrettoSecretKey::random(&mut rand::rng());
    let (nonce_secret, nonce_public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
    let recipient_pk = RistrettoPublicKey::from_canonical_bytes(recipient.as_bytes())
        .map_err(|_| SdkError::InvalidKey)?;
    let encrypted_data = StealthCryptoApi::new()
        .encrypt_value_and_mask(amount, &mask, &recipient_pk, &nonce_secret, None)
        .map_err(|err| SdkError::Encrypt(err.to_string()))?;

    Ok(StealthOutputWitness {
        witness: OutputWitness {
            amount,
            mask,
            sender_public_nonce: nonce_public,
            minimum_value_promise: amount,
            encrypted_data,
            resource_view_key: None,
        },
        auth,
        tag: UtxoTag::new(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, ChannelSpec};
    use ghostline_core::{NONCE_LEN, SignedVoucher};
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;

    fn channel_and_keys() -> (RistrettoSecretKey, Channel, RistrettoPublicKeyBytes) {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let payer: RistrettoPublicKeyBytes = payer_public.to_byte_type();
        let service: RistrettoPublicKeyBytes = service_public.to_byte_type();
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer,
            service,
            deposit: 100,
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        (payer_secret, channel, service)
    }

    fn funded(value: u64) -> MaskAndValue {
        MaskAndValue {
            value,
            mask: RistrettoSecretKey::random(&mut rand::rng()),
        }
    }

    #[test]
    fn settle_outputs_promise_their_own_value() {
        let (secret, channel, service) = channel_and_keys();
        let voucher = SignedVoucher::sign(&secret, channel.params(), 30).unwrap();
        let statement = settle(&channel, funded(100), &voucher, service).unwrap();
        let promises: Vec<u64> = statement
            .outputs_statement
            .outputs
            .iter()
            .map(|o| o.output.minimum_value_promise)
            .collect();
        assert_eq!(promises, vec![70, 30]);
    }

    #[test]
    fn full_deposit_voucher_omits_remainder() {
        let (secret, channel, service) = channel_and_keys();
        let voucher = SignedVoucher::sign(&secret, channel.params(), 100).unwrap();
        let statement = settle(&channel, funded(100), &voucher, service).unwrap();
        assert_eq!(statement.outputs_statement.outputs.len(), 1);
        assert_eq!(
            statement.outputs_statement.outputs[0]
                .output
                .minimum_value_promise,
            100
        );
    }

    #[test]
    fn refund_returns_the_deposit() {
        let (_secret, channel, _service) = channel_and_keys();
        let statement = refund(&channel, funded(100), channel.spec().payer).unwrap();
        assert_eq!(statement.outputs_statement.outputs.len(), 1);
        assert_eq!(
            statement.outputs_statement.outputs[0]
                .output
                .minimum_value_promise,
            100
        );
    }

    #[test]
    fn settle_rejects_wrong_utxo_value() {
        let (secret, channel, service) = channel_and_keys();
        let voucher = SignedVoucher::sign(&secret, channel.params(), 30).unwrap();
        let err = settle(&channel, funded(99), &voucher, service).unwrap_err();
        assert!(matches!(
            err,
            SdkError::DepositMismatch {
                expected: 100,
                actual: 99
            }
        ));
    }
}
