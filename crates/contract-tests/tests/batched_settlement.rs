// SPDX-License-Identifier: MIT

mod support;
#[path = "support/batch.rs"]
mod batch_support;

use ghostline_core::{NONCE_LEN, SignedVoucher};
use ghostline_sdk::{
    Channel, ChannelSpec, MaskAndValue, OutputScan, settle_batch, settle_batched_individually,
    unblind_output,
};
use ootle_byte_type::ToByteType;
use batch_support::fund_many;
use support::{CHANNEL, DEPOSIT};
use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
use tari_engine_types::virtual_substate::{VirtualSubstate, VirtualSubstateId};
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_ootle_transaction::{Epoch, Transaction};
use tari_template_lib::types::{
    Hash32, ResourceAddress,
    bytes::Bytes,
    crypto::{NoSignatureDomain, PedersenCommitmentBytes, RistrettoPublicKeyBytes, Signature},
    stealth::{SpendWitness, StealthTransferStatement},
};
use tari_template_test_tooling::{TemplateTest, support::spec::OutputAuthSpec};
use tari_utilities::ByteArray;

type BatchWitnessEntry = (
    Hash32,
    PedersenCommitmentBytes,
    RistrettoPublicKeyBytes,
    Hash32,
    PedersenCommitmentBytes,
    Signature<NoSignatureDomain>,
    Signature<NoSignatureDomain>,
);

struct BatchFixture {
    test: TemplateTest,
    resource: ResourceAddress,
    relayer: RistrettoSecretKey,
    payout: RistrettoSecretKey,
    channels: [Channel; 2],
    deposits: [MaskAndValue; 2],
    vouchers: [SignedVoucher; 2],
}

impl BatchFixture {
    fn new() -> Self {
        let mut test = TemplateTest::new(
            env!("CARGO_MANIFEST_DIR"),
            ["templates/faucet", "../../contracts/channel"],
        );
        test.set_virtual_substate(
            VirtualSubstateId::CurrentEpoch,
            VirtualSubstate::CurrentEpoch(1),
        );
        let (payer1, payer1_public) = create_key_pair_from_seed(101);
        let (payer2, payer2_public) = create_key_pair_from_seed(102);
        let (_service, service_public) = create_key_pair_from_seed(103);
        let (relayer, _) = create_key_pair_from_seed(105);
        let (payout, payout_public) = create_key_pair_from_seed(104);
        let template = test.get_template_address(CHANNEL);
        let make_channel = |payer: &RistrettoPublicKey| {
            Channel::open(ChannelSpec {
                network_tag: 1,
                payer: payer.to_byte_type(),
                service: service_public.to_byte_type(),
                deposit: DEPOSIT,
                expiry_epoch: 100,
                nonce: [payer.as_bytes()[0]; NONCE_LEN],
                channel_template: template,
            })
            .unwrap()
        };
        let channels = [make_channel(&payer1_public), make_channel(&payer2_public)];
        let (resource, masks) = fund_many(
            &mut test,
            channels
                .iter()
                .map(|channel| OutputAuthSpec::Conditions(channel.batched_leaves_vec()))
                .collect(),
        );
        let deposits = [
            MaskAndValue {
                value: DEPOSIT,
                mask: masks[0].clone(),
            },
            MaskAndValue {
                value: DEPOSIT,
                mask: masks[1].clone(),
            },
        ];
        let payout_public = payout_public.to_byte_type();
        let vouchers = [
            SignedVoucher::sign(&payer1, channels[0].params(), 30, payout_public).unwrap(),
            SignedVoucher::sign(&payer2, channels[1].params(), 20, payout_public).unwrap(),
        ];
        Self {
            test,
            resource,
            relayer,
            payout,
            channels,
            deposits,
            vouchers,
        }
    }

    fn statement(&self) -> StealthTransferStatement {
        settle_batch(
            &[
                (
                    &self.channels[0],
                    self.deposits[0].clone(),
                    &self.vouchers[0],
                ),
                (
                    &self.channels[1],
                    self.deposits[1].clone(),
                    &self.vouchers[1],
                ),
            ],
            self.vouchers[0].service_output_key,
            OutputScan::localnet(self.resource),
        )
        .unwrap()
    }
}

fn reject(f: &mut BatchFixture, statement: StealthTransferStatement, expected: &str) {
    let tx = Transaction::builder_localnet(Epoch(2))
        .stealth_transfer(f.resource, statement)
        .finish()
        .seal(&f.relayer);
    let reason = format!("{:?}", f.test.execute_expect_failure(tx, vec![]));
    assert!(
        reason.contains(expected),
        "expected {expected:?}, got {reason}"
    );
}

#[test]
fn two_channels_settle_to_one_pooled_confidential_output_and_rejects_tampering() {
    let mut f = BatchFixture::new();
    let statement = f.statement();
    assert_eq!(statement.inputs_statement.inputs.len(), 2);
    assert_eq!(statement.outputs_statement.outputs.len(), 3);
    let pool = &statement.outputs_statement.outputs[0];
    assert_eq!(
        pool.auth,
        tari_template_lib::types::stealth::SpendAuthorization::Key(
            f.vouchers[0].service_output_key
        )
    );
    assert_eq!(pool.output.minimum_value_promise, 1);
    assert_eq!(
        unblind_output(
            &pool.output,
            pool.tag,
            &f.payout,
            OutputScan::localnet(f.resource)
        )
        .unwrap()
        .value,
        50
    );

    let mut wrong_index = statement.clone();
    let (leaf, proof, data) = wrong_index.inputs_statement.inputs[0]
        .witness
        .as_script_path()
        .unwrap();
    let (_, payout_key, entries): (
        u32,
        RistrettoPublicKeyBytes,
        Vec<BatchWitnessEntry>,
    ) = tari_bor::decode(data.as_ref()).unwrap();
    let tampered_data = tari_bor::encode(&(1u32, payout_key, entries)).unwrap();
    wrong_index.inputs_statement.inputs[0].witness = SpendWitness::script_path_with_data(
        leaf.clone(),
        proof.clone(),
        Bytes::from_vec(tampered_data),
    );
    reject(&mut f, wrong_index, "batch index mismatch");

    let mut wrong_pool = statement.clone();
    wrong_pool.outputs_statement.outputs[0].output.commitment =
        wrong_pool.outputs_statement.outputs[1].output.commitment;
    reject(&mut f, wrong_pool, "pooled service commitment mismatch");

    let mut cross_subsidy = statement.clone();
    let first_root = cross_subsidy.outputs_statement.outputs[1].auth.clone();
    cross_subsidy.outputs_statement.outputs[1].auth =
        cross_subsidy.outputs_statement.outputs[2].auth.clone();
    cross_subsidy.outputs_statement.outputs[2].auth = first_root;
    reject(&mut f, cross_subsidy, "invalid per-channel balance proof");

    let tx = Transaction::builder_localnet(Epoch(2))
        .stealth_transfer(f.resource, statement)
        .finish()
        .seal(&f.relayer);
    f.test.execute_expect_success(tx, vec![]);
}

#[test]
fn batch_enabled_channel_can_settle_individually_as_an_offline_participant_fallback() {
    let mut f = BatchFixture::new();
    let service_key = f.vouchers[0].service_output_key;
    let statement = settle_batched_individually(
        &f.channels[0],
        f.deposits[0].clone(),
        &f.vouchers[0],
        service_key,
        OutputScan::localnet(f.resource),
    )
    .unwrap();

    assert_eq!(statement.inputs_statement.inputs.len(), 1);
    assert_eq!(statement.outputs_statement.outputs.len(), 2);
    let service_output = statement
        .outputs_statement
        .outputs
        .iter()
        .find(|output| {
            output.auth == tari_template_lib::types::stealth::SpendAuthorization::Key(service_key)
        })
        .unwrap();
    assert_eq!(
        unblind_output(
            &service_output.output,
            service_output.tag,
            &f.payout,
            OutputScan::localnet(f.resource)
        )
        .unwrap()
        .value,
        f.vouchers[0].amount
    );

    let tx = Transaction::builder_localnet(Epoch(2))
        .stealth_transfer(f.resource, statement)
        .finish()
        .seal(&f.relayer);
    f.test.execute_expect_success(tx, vec![]);
}
