// SPDX-License-Identifier: MIT

//! Prototype proof that the published Ootle engine accepts a commitment-authorized settlement while keeping output
//! amounts out of the spend predicate and public minimum-value promises.

mod support;

use ghostline_core::{NONCE_LEN, SignedVoucher};
use ghostline_sdk::{Channel, ChannelSpec, MaskAndValue, OutputScan, refund, settle};
use ootle_byte_type::ToByteType;
use support::{CHANNEL, DEPOSIT, fund};
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_ootle_transaction::{Epoch, Transaction};
use tari_template_lib::types::{
    Amount,
    crypto::{NoSignatureDomain, PedersenCommitmentBytes, RistrettoPublicKeyBytes, Signature},
};
use tari_template_test_tooling::{TemplateTest, support::spec::OutputAuthSpec};

const NETWORK_TAG: u8 = 1;
const NONCE: [u8; NONCE_LEN] = [42; NONCE_LEN];

#[test]
fn signed_commitment_settlement_accepts_hidden_outputs() {
    let mut test = TemplateTest::new(
        env!("CARGO_MANIFEST_DIR"),
        ["templates/faucet", "../../contracts/channel"],
    );
    let template = test.get_template_address(CHANNEL);
    let (payer_secret, payer_public) = create_key_pair_from_seed(11);
    let (service_secret, service_public) = create_key_pair_from_seed(13);
    let payer: RistrettoPublicKeyBytes = payer_public.to_byte_type();
    let service: RistrettoPublicKeyBytes = service_public.to_byte_type();
    let channel = Channel::open(ChannelSpec {
        network_tag: NETWORK_TAG,
        payer,
        service,
        deposit: DEPOSIT,
        expiry_epoch: u64::MAX,
        nonce: NONCE,
        channel_template: template,
    })
    .unwrap();

    let (resource, deposit_mask) =
        fund(&mut test, OutputAuthSpec::Conditions(channel.leaves_vec()));
    let voucher = SignedVoucher::sign(&payer_secret, channel.params(), 30, service).unwrap();
    let witness_tuple: (
        PedersenCommitmentBytes,
        RistrettoPublicKeyBytes,
        Signature<NoSignatureDomain>,
    ) = tari_bor::decode(&voucher.witness_data()).unwrap();
    assert_eq!(witness_tuple.0, voucher.output_commitment);
    assert_eq!(witness_tuple.1, service);
    let statement = settle(
        &channel,
        MaskAndValue {
            mask: deposit_mask.clone(),
            value: DEPOSIT,
        },
        &voucher,
        service,
        OutputScan::localnet(resource),
    )
    .unwrap();
    assert_eq!(statement.covenant_claims.len(), 1);
    assert_eq!(
        statement.covenant_claims[0].revealed_amount,
        Amount::from_u64(DEPOSIT),
        "the current script-path claim publicly reveals the full deposit"
    );
    let refund_statement = refund(
        &channel,
        MaskAndValue {
            mask: deposit_mask,
            value: DEPOSIT,
        },
        payer,
        OutputScan::localnet(resource),
    )
    .unwrap();
    assert_eq!(refund_statement.covenant_claims.len(), 1);
    assert_eq!(
        refund_statement.covenant_claims[0].revealed_amount,
        Amount::from_u64(DEPOSIT),
        "the current script-path refund claim also publicly reveals the full deposit"
    );
    let tx = Transaction::builder_localnet(Epoch(1))
        .stealth_transfer(resource, statement)
        .finish()
        .seal(&service_secret);
    test.execute_expect_success(tx, vec![]);
}
