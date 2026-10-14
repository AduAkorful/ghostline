// SPDX-License-Identifier: MIT

use ghostline_core::{SignedVoucher, VoucherError};
use tari_crypto::{
    keys::{PublicKey as _, SecretKey as _},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_ootle_wallet_crypto::{
    MaskAndValue, OutputWitness, StealthCryptoApi, StealthInputWitness, StealthOutputWitness,
    pay_to::PayTo,
    stealth::{self, create_transfer_statement},
};
use tari_template_lib::types::{
    Amount,
    crypto::{RistrettoPublicKeyBytes, UtxoTag},
    stealth::{SpendAuthorization, StealthTransferStatement},
};
use tari_utilities::ByteArray;

use crate::{Channel, SdkError};

/// A funded channel UTXO plus the transfer that created it.
#[derive(Clone)]
pub struct OpenStatement {
    pub statement: StealthTransferStatement,
    pub deposit: MaskAndValue,
}

impl std::fmt::Debug for OpenStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenStatement")
            .field("deposit_value", &self.deposit.value)
            .finish_non_exhaustive()
    }
}

/// Spends key-path stealth inputs into the channel's two-leaf condition tree (`PayTo::Conditions`).
///
/// Input value above the deposit is returned as a key-path change output to `payer_change_key`.
/// Every output's minimum-value promise equals its value.
pub fn fund_open(
    channel: &Channel,
    inputs: impl IntoIterator<Item = MaskAndValue>,
    payer_change_key: RistrettoPublicKeyBytes,
) -> Result<OpenStatement, SdkError> {
    let inputs: Vec<MaskAndValue> = inputs.into_iter().collect();
    let mut have: u64 = 0;
    for input in &inputs {
        if input.value == 0 {
            return Err(SdkError::ZeroInput);
        }
        have = have.checked_add(input.value).ok_or(SdkError::Overflow)?;
    }
    let need = channel.params().deposit;
    if have < need {
        return Err(SdkError::InsufficientFunds { have, need });
    }
    let change = have - need;

    let auth =
        stealth::pay_to_output_authorization(&PayTo::Conditions(channel.leaves_vec()), || {
            unreachable!("PayTo::Conditions does not derive a one-time spend key")
        })?;

    let deposit_out = output(need, auth, &channel.spec().payer)?;
    let deposit = MaskAndValue {
        value: need,
        mask: deposit_out.witness.mask.clone(),
    };

    let mut outputs = vec![deposit_out];
    if change > 0 {
        outputs.push(output(
            change,
            SpendAuthorization::Key(payer_change_key),
            &payer_change_key,
        )?);
    }

    let statement = create_transfer_statement(
        inputs.into_iter().map(StealthInputWitness::new),
        Amount::zero(),
        outputs.iter(),
        None,
    )?;
    Ok(OpenStatement { statement, deposit })
}

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

    #[test]
    fn open_commits_the_channel_tree() {
        let (_secret, channel, _service) = channel_and_keys();
        let opened = fund_open(&channel, [funded(100)], channel.spec().payer).unwrap();
        assert_eq!(opened.statement.outputs_statement.outputs.len(), 1);
        assert_eq!(
            opened.statement.outputs_statement.outputs[0]
                .output
                .minimum_value_promise,
            100
        );
        let expected = stealth::pay_to_output_authorization(
            &PayTo::Conditions(channel.leaves_vec()),
            || unreachable!(),
        )
        .unwrap();
        assert_eq!(opened.statement.outputs_statement.outputs[0].auth, expected);
        assert_eq!(opened.deposit.value, 100);
    }

    #[test]
    fn open_change_promises_its_value() {
        let (_secret, channel, _service) = channel_and_keys();
        let opened = fund_open(&channel, [funded(130)], channel.spec().payer).unwrap();
        let promises: Vec<u64> = opened
            .statement
            .outputs_statement
            .outputs
            .iter()
            .map(|o| o.output.minimum_value_promise)
            .collect();
        assert_eq!(promises, vec![100, 30]);
    }

    #[test]
    fn open_rejects_short_inputs() {
        let (_secret, channel, _service) = channel_and_keys();
        let err = fund_open(&channel, [funded(40), funded(50)], channel.spec().payer).unwrap_err();
        assert!(matches!(
            err,
            SdkError::InsufficientFunds {
                have: 90,
                need: 100
            }
        ));
    }

    #[test]
    fn open_statement_validates() {
        let (_secret, channel, _service) = channel_and_keys();
        let opened = fund_open(&channel, [funded(100)], channel.spec().payer).unwrap();
        tari_engine_types::stealth::validate_transfer(&opened.statement, None).unwrap();
    }
}
