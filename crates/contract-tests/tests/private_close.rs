// SPDX-License-Identifier: MIT

//! The channel does not use partition-balance covenants. Omit their optional public outflow claim without removing
//! the script witness, native timelocks, signed payout commitment, or global conservation proof.

mod support;

use ghostline_core::{SignedVoucher, NONCE_LEN};
use ghostline_sdk::{
    refund, settle, unblind_output, Channel, ChannelSpec, MaskAndValue, OutputScan,
};
use ootle_byte_type::ToByteType;
use support::{fund, CHANNEL, DEPOSIT};
use tari_crypto::{
    keys::{PublicKey as _, SecretKey as _},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_engine_types::virtual_substate::{VirtualSubstate, VirtualSubstateId};
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_ootle_transaction::{Epoch, Transaction};
use tari_ootle_wallet_crypto::{
    stealth::{condition_root, create_transfer_statement, script_path_witness_with_data},
    OutputWitness, StealthCryptoApi, StealthInputWitness, StealthOutputWitness,
};
use tari_template_lib::types::{
    bytes::Bytes,
    crypto::RistrettoPublicKeyBytes,
    stealth::{
        Covenant, RevealedOutput, SpendAuthorization, SpendCondition, SpendWitness,
        StealthTransferStatement,
    },
    Amount, ResourceAddress,
};
use tari_template_test_tooling::{support::spec::OutputAuthSpec, TemplateTest};
use tari_utilities::ByteArray;

struct Fixture {
    test: TemplateTest,
    channel: Channel,
    resource: ResourceAddress,
    deposit: MaskAndValue,
    payer: RistrettoSecretKey,
    service: RistrettoSecretKey,
    other: RistrettoSecretKey,
}

impl Fixture {
    fn new(expiry: u64) -> Self {
        let mut test = TemplateTest::new(
            env!("CARGO_MANIFEST_DIR"),
            ["templates/faucet", "../../contracts/channel"],
        );
        let (payer, payer_public) = create_key_pair_from_seed(11);
        let (service, service_public) = create_key_pair_from_seed(13);
        let (other, _) = create_key_pair_from_seed(14);
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            deposit: DEPOSIT,
            expiry_epoch: expiry,
            nonce: [45; NONCE_LEN],
            channel_template: test.get_template_address(CHANNEL),
        })
        .unwrap();
        let (resource, mask) = fund(&mut test, OutputAuthSpec::Conditions(channel.leaves_vec()));
        Self {
            test,
            channel,
            resource,
            deposit: MaskAndValue {
                mask,
                value: DEPOSIT,
            },
            payer,
            service,
            other,
        }
    }

    fn epoch(&mut self, epoch: u64) {
        self.test.set_virtual_substate(
            VirtualSubstateId::CurrentEpoch,
            VirtualSubstate::CurrentEpoch(epoch),
        );
    }

    fn voucher(&self, amount: u64) -> SignedVoucher {
        SignedVoucher::sign(
            &self.payer,
            self.channel.params(),
            amount,
            self.channel.spec().service,
        )
        .unwrap()
    }

    fn outputs(&self, voucher: &SignedVoucher, remainder: u64) -> Vec<StealthOutputWitness> {
        vec![
            output(
                self.resource,
                voucher.amount,
                voucher.mask.clone(),
                SpendAuthorization::Key(voucher.service_output_key),
                voucher.service_output_key,
            ),
            output(
                self.resource,
                remainder,
                RistrettoSecretKey::random(&mut rand::rng()),
                SpendAuthorization::Script(self.channel.params().payer_root),
                self.channel.spec().payer,
            ),
        ]
    }

    fn raw_settle(
        &self,
        data: Bytes,
        outputs: &[StealthOutputWitness],
        revealed: Option<RevealedOutput>,
    ) -> StealthTransferStatement {
        let (witness, _) = script_path_witness_with_data(
            &self.channel.leaves_vec(),
            self.channel.settle_leaf(),
            data,
        )
        .unwrap();
        no_claim_statement(self.deposit.clone(), witness, outputs, revealed)
    }
}

fn output(
    resource: ResourceAddress,
    value: u64,
    mask: RistrettoSecretKey,
    auth: SpendAuthorization,
    recipient: RistrettoPublicKeyBytes,
) -> StealthOutputWitness {
    let recipient =
        tari_crypto::ristretto::RistrettoPublicKey::from_canonical_bytes(recipient.as_bytes())
            .unwrap();
    let (nonce, public_nonce) = RistrettoPublicKey::random_keypair(&mut rand::rng());
    let crypto = StealthCryptoApi::new();
    let encrypted_data = crypto
        .encrypt_value_and_mask(value, &mask, &recipient, &nonce, None)
        .unwrap();
    let tag = crypto.derive_stealth_output_tag(
        ootle_network::Network::LocalNet,
        &nonce,
        &recipient,
        &resource,
    );
    StealthOutputWitness {
        witness: OutputWitness {
            amount: value,
            mask,
            sender_public_nonce: public_nonce,
            minimum_value_promise: 1,
            encrypted_data,
            resource_view_key: None,
        },
        auth,
        tag,
    }
}

/// The test uses a different construction from the SDK: the optional root metadata solely controls the generic
/// wallet builder's claim generation. The real script witness is still included in the signed input statement.
fn no_claim_statement(
    deposit: MaskAndValue,
    witness: SpendWitness,
    outputs: &[StealthOutputWitness],
    revealed: Option<RevealedOutput>,
) -> StealthTransferStatement {
    create_transfer_statement(
        [StealthInputWitness {
            mask_and_value: deposit,
            witness,
            condition_root: None,
        }],
        Amount::zero(),
        outputs.iter(),
        revealed,
    )
    .unwrap()
}

fn reject(
    test: &mut TemplateTest,
    resource: ResourceAddress,
    statement: StealthTransferStatement,
    signer: &RistrettoSecretKey,
    expected: &str,
) {
    assert!(statement.covenant_claims.is_empty());
    let tx = Transaction::builder_localnet(Epoch(10_000))
        .stealth_transfer(resource, statement)
        .finish()
        .seal(signer);
    let reason = format!("{:?}", test.execute_expect_failure(tx, vec![]));
    assert!(
        reason.contains(expected),
        "expected {expected:?}, got {reason}"
    );
}

#[test]
fn omission_preserves_template_authorization_and_value_conservation() {
    let mut f = Fixture::new(10);
    f.epoch(1);
    let voucher = f.voucher(30);

    // Submit raw adversarial statements directly to the engine, bypassing SDK-side voucher validation.
    let forged =
        SignedVoucher::sign(&f.other, f.channel.params(), 30, f.channel.spec().service).unwrap();
    let statement = f.raw_settle(forged.witness_data(), &f.outputs(&forged, 70), None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "invalid voucher signature",
    );

    let mut params = f.channel.params().clone();
    params.network_tag = 2;
    let wrong_network =
        SignedVoucher::sign(&f.payer, &params, 30, f.channel.spec().service).unwrap();
    let statement = f.raw_settle(
        wrong_network.witness_data(),
        &f.outputs(&wrong_network, 70),
        None,
    );
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "invalid voucher signature",
    );

    let mut wrong_commitment = f.outputs(&voucher, 70);
    wrong_commitment[0].witness.mask = RistrettoSecretKey::random(&mut rand::rng());
    let statement = f.raw_settle(voucher.witness_data(), &wrong_commitment, None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "unauthorized output",
    );

    let mut diverted = f.outputs(&voucher, 70);
    diverted[1].auth = SpendAuthorization::Key(f.channel.spec().service);
    let statement = f.raw_settle(voucher.witness_data(), &diverted, None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "unauthorized output",
    );

    let mut extra_output = f.outputs(&voucher, 69);
    extra_output.push(output(
        f.resource,
        1,
        RistrettoSecretKey::random(&mut rand::rng()),
        SpendAuthorization::Key(f.channel.spec().service),
        f.channel.spec().service,
    ));
    let statement = f.raw_settle(voucher.witness_data(), &extra_output, None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "invalid settle output count",
    );

    let mut disclosing = f.outputs(&voucher, 70);
    disclosing[0].witness.minimum_value_promise = voucher.amount;
    let statement = f.raw_settle(voucher.witness_data(), &disclosing, None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "non-disclosing value promise",
    );

    let revealed = RevealedOutput::new(Amount::from_u64(1), f.channel.spec().service);
    let statement = f.raw_settle(
        voucher.witness_data(),
        &f.outputs(&voucher, 69),
        Some(revealed),
    );
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "cannot create revealed output value",
    );

    let statement = f.raw_settle(voucher.witness_data(), &f.outputs(&voucher, 71), None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "Balance proof signature verification failed",
    );

    let statement = f.raw_settle(voucher.witness_data(), &f.outputs(&voucher, 70), None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.payer,
        "Access Denied",
    );

    f.epoch(10);
    let statement = f.raw_settle(voucher.witness_data(), &f.outputs(&voucher, 70), None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "BeforeEpoch",
    );
    f.epoch(1);

    let statement = settle(
        &f.channel,
        f.deposit,
        &voucher,
        f.channel.spec().service,
        OutputScan::localnet(f.resource),
    )
    .unwrap();
    assert!(statement.covenant_claims.is_empty());
    let recovered: Vec<_> = statement
        .outputs_statement
        .outputs
        .iter()
        .map(|o| {
            let key = if o.auth == SpendAuthorization::Key(voucher.service_output_key) {
                &f.service
            } else {
                &f.payer
            };
            unblind_output(&o.output, o.tag, key, OutputScan::localnet(f.resource))
                .unwrap()
                .value
        })
        .collect();
    assert_eq!(recovered, vec![70, 30]);
    let tx = Transaction::builder_localnet(Epoch(2))
        .stealth_transfer(f.resource, statement)
        .finish()
        .seal(&f.service);
    f.test.execute_expect_success(tx.clone(), vec![]);
    f.test.execute_expect_failure(tx, vec![]);
}

#[test]
fn native_refund_is_private_and_remains_unilateral_long_after_expiry() {
    let mut f = Fixture::new(10);
    let statement = refund(
        &f.channel,
        f.deposit.clone(),
        f.channel.spec().payer,
        OutputScan::localnet(f.resource),
    )
    .unwrap();
    f.epoch(9);
    reject(
        &mut f.test,
        f.resource,
        statement.clone(),
        &f.payer,
        "AfterEpoch",
    );
    f.epoch(10);
    reject(
        &mut f.test,
        f.resource,
        statement.clone(),
        &f.service,
        "Access Denied",
    );
    assert!(statement.covenant_claims.is_empty());
    assert_eq!(statement.outputs_statement.outputs.len(), 1);
    let out = &statement.outputs_statement.outputs[0];
    assert_eq!(out.output.minimum_value_promise, 1);
    assert_eq!(
        unblind_output(
            &out.output,
            out.tag,
            &f.payer,
            OutputScan::localnet(f.resource)
        )
        .unwrap()
        .value,
        DEPOSIT
    );

    // Unlike a pre-signed joint-key refund, the payer creates a fresh transaction at recovery time. The script's
    // native AfterEpoch condition has no maximum recovery epoch and needs no service signature.
    f.epoch(10_000);
    let tx = Transaction::builder_localnet(Epoch(10_001))
        .stealth_transfer(f.resource, statement)
        .finish()
        .seal(&f.payer);
    f.test.execute_expect_success(tx.clone(), vec![]);
    f.test.execute_expect_failure(tx, vec![]);
}

#[test]
fn genuine_balance_covenant_still_requires_a_partition_claim() {
    let mut test = TemplateTest::new(
        env!("CARGO_MANIFEST_DIR"),
        ["templates/faucet", "../../contracts/channel"],
    );
    let leaf = SpendCondition::covenant(Covenant::BalancePreserved(0));
    let leaves = vec![leaf.clone()];
    let (resource, mask) = fund(&mut test, OutputAuthSpec::Conditions(leaves.clone()));
    let deposit = MaskAndValue {
        mask,
        value: DEPOSIT,
    };
    let (witness, root) = script_path_witness_with_data(&leaves, &leaf, Bytes::default()).unwrap();
    assert_eq!(root, condition_root(&leaves).unwrap());
    let recipient = test.to_public_key_bytes();
    let outputs = vec![output(
        resource,
        DEPOSIT,
        RistrettoSecretKey::random(&mut rand::rng()),
        SpendAuthorization::Script(root),
        recipient,
    )];
    let without_claim = no_claim_statement(deposit.clone(), witness.clone(), &outputs, None);
    let signer = test.secret_key().clone();
    reject(
        &mut test,
        resource,
        without_claim,
        &signer,
        "Spend condition not met",
    );

    let with_claim = create_transfer_statement(
        [StealthInputWitness::with_script_path(
            deposit, witness, root,
        )],
        Amount::zero(),
        outputs.iter(),
        None,
    )
    .unwrap();
    assert_eq!(with_claim.covenant_claims.len(), 1);
    assert_eq!(
        with_claim.covenant_claims[0].revealed_amount,
        Amount::zero()
    );
    let tx = Transaction::builder_localnet(Epoch(1))
        .stealth_transfer(resource, with_claim)
        .finish()
        .seal(&signer);
    test.execute_expect_success(tx, vec![]);
}
