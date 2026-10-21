// SPDX-License-Identifier: MIT

//! The channel does not use partition-balance covenants. Omit their optional public outflow claim without removing
//! the script witness, native timelocks, signed payout commitment, or global conservation proof.

mod support;

use ghostline_core::{NONCE_LEN, SignedVoucher, VOUCHER_DOMAIN, sign_with_nonce, voucher_message};
use ghostline_sdk::{
    Channel, ChannelSpec, MaskAndValue, OutputScan, refund, settle, unblind_output,
};
use ootle_byte_type::ToByteType;
use support::{CHANNEL, DEPOSIT, fund};
use tari_crypto::{
    keys::{PublicKey as _, SecretKey as _},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_engine_types::virtual_substate::{VirtualSubstate, VirtualSubstateId};
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_ootle_transaction::{Epoch, Transaction, args};
use tari_ootle_wallet_crypto::{
    OutputWitness, StealthCryptoApi, StealthInputWitness, StealthOutputWitness,
    stealth::{condition_root, create_transfer_statement, script_path_witness_with_data},
};
use tari_template_lib::types::{
    Amount, ResourceAddress,
    bytes::Bytes,
    constants::TARI_TOKEN,
    crypto::RistrettoPublicKeyBytes,
    stealth::{
        Covenant, RevealedOutput, SpendAuthorization, SpendCondition, SpendWitness,
        StealthTransferStatement,
    },
};
use tari_template_test_tooling::{
    TemplateTest,
    support::{
        spec::OutputAuthSpec,
        stealth::{generate_transfer_data, generate_transfer_data_to},
    },
};
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

    fn raw_full_deposit_voucher(&self) -> SignedVoucher {
        let mask = RistrettoSecretKey::random(&mut rand::rng());
        let output_commitment = MaskAndValue {
            value: DEPOSIT,
            mask: mask.clone(),
        }
        .to_commitment()
        .to_byte_type();
        let message = voucher_message(
            &self.channel.params().channel_id(),
            &self.channel.spec().service,
            &output_commitment,
        );
        let signature = sign_with_nonce(
            &self.payer,
            RistrettoSecretKey::random(&mut rand::rng()),
            VOUCHER_DOMAIN,
            &message,
        );
        SignedVoucher {
            channel_id: self.channel.params().channel_id(),
            amount: DEPOSIT,
            mask,
            service_output_key: self.channel.spec().service,
            output_commitment,
            signature,
        }
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
        "settle must have exactly two outputs",
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
fn template_rejects_single_output_full_deposit_settlement() {
    let mut f = Fixture::new(u64::MAX);
    let voucher = f.raw_full_deposit_voucher();
    assert_eq!(DEPOSIT - voucher.amount, 0);
    let only_service_output = [output(
        f.resource,
        DEPOSIT,
        voucher.mask.clone(),
        SpendAuthorization::Key(voucher.service_output_key),
        voucher.service_output_key,
    )];
    let statement = f.raw_settle(voucher.witness_data(), &only_service_output, None);
    reject(
        &mut f.test,
        f.resource,
        statement,
        &f.service,
        "exactly two outputs",
    );
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
fn relay_refund_key_can_refund_after_expiry_without_replacing_payer_signature() {
    let mut test = TemplateTest::new(
        env!("CARGO_MANIFEST_DIR"),
        ["templates/faucet", "../../contracts/channel"],
    );
    let (payer, payer_public) = create_key_pair_from_seed(101);
    let (refund_authority, refund_public) = create_key_pair_from_seed(102);
    let (service, service_public) = create_key_pair_from_seed(103);
    let channel = Channel::open_with_refund_key(
        ChannelSpec {
            network_tag: 1,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            deposit: DEPOSIT,
            expiry_epoch: 10,
            nonce: [46; NONCE_LEN],
            channel_template: test.get_template_address(CHANNEL),
        },
        refund_public.to_byte_type(),
    )
    .unwrap();
    let (resource, mask) = fund(&mut test, OutputAuthSpec::Conditions(channel.leaves_vec()));
    let deposit = MaskAndValue {
        value: DEPOSIT,
        mask,
    };
    let refund_statement = refund(
        &channel,
        deposit.clone(),
        refund_public.to_byte_type(),
        OutputScan::localnet(resource),
    )
    .unwrap();

    // Even though the relay holds the refund key, it cannot forge the payer's off-chain voucher.
    let forged = SignedVoucher::sign(
        &refund_authority,
        channel.params(),
        30,
        channel.spec().service,
    )
    .unwrap();
    let outputs = [
        output(
            resource,
            forged.amount,
            forged.mask.clone(),
            SpendAuthorization::Key(forged.service_output_key),
            forged.service_output_key,
        ),
        output(
            resource,
            DEPOSIT - forged.amount,
            RistrettoSecretKey::random(&mut rand::rng()),
            SpendAuthorization::Script(channel.params().payer_root),
            channel.spec().payer,
        ),
    ];
    let (settle_witness, _) = script_path_witness_with_data(
        &channel.leaves_vec(),
        channel.settle_leaf(),
        forged.witness_data(),
    )
    .unwrap();
    let forged_settle = no_claim_statement(deposit, settle_witness, &outputs, None);
    let tx = Transaction::builder_localnet(Epoch(9))
        .stealth_transfer(resource, forged_settle)
        .finish()
        .seal(&service);
    let reason = format!("{:?}", test.execute_expect_failure(tx, vec![]));
    assert!(reason.contains("invalid voucher signature"), "{reason}");

    test.set_virtual_substate(
        VirtualSubstateId::CurrentEpoch,
        VirtualSubstate::CurrentEpoch(10),
    );

    reject(
        &mut test,
        resource,
        refund_statement.clone(),
        &payer,
        "Access Denied",
    );

    let tx = Transaction::builder_localnet(Epoch(10_001))
        .stealth_transfer(resource, refund_statement)
        .finish()
        .seal(&refund_authority);
    test.execute_expect_success(tx, vec![]);
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

#[test]
fn settlement_can_pay_its_fee_from_a_separate_stealth_input() {
    use tari_engine::fees::FeeTable;
    use tari_engine_types::fees::FeeSource;

    let mut f = Fixture::new(10);
    f.epoch(1);
    let (_settlement_key, settlement_public) = create_key_pair_from_seed(91);
    let settlement_public = settlement_public.to_byte_type();
    let voucher = SignedVoucher::sign(&f.payer, f.channel.params(), 30, settlement_public).unwrap();

    // Create a separate native-TARI stealth UTXO for fee funding. The fee amount is revealed, but
    // this route needs no public Account.pay_fee component in the settlement transaction.
    let (fee_account, fee_owner, fee_wallet_key) = f.test.create_funded_account();
    f.test.disable_fees();
    let fee_input_value = 10_000_000u64;
    let fee_mint = generate_transfer_data(
        tari_template_test_tooling::support::stealth::NO_INPUTS,
        Amount::from(fee_input_value),
        [fee_input_value],
        Amount::zero(),
    );
    let fee_funding_tx = Transaction::builder_localnet(Epoch(2))
        .call_method(
            fee_account,
            "withdraw",
            args![TARI_TOKEN, Amount::from(fee_input_value)],
        )
        .put_last_instruction_output_on_workspace("fee_input")
        .stealth_transfer_with_input_bucket(TARI_TOKEN, fee_mint.statement.clone(), "fee_input")
        .build_and_seal(&fee_wallet_key);
    f.test
        .execute_expect_success(fee_funding_tx, vec![fee_owner]);

    let fee_mask = fee_mint.output_masks[0].clone();
    let fee_reserve = 100u64;
    let fee_transfer = generate_transfer_data_to(
        [MaskAndValue {
            mask: fee_mask.clone(),
            value: fee_input_value,
        }],
        Amount::zero(),
        [fee_input_value - fee_reserve],
        Amount::from(fee_reserve),
        f.channel.spec().service,
    );

    f.test.enable_fees();
    let mut fee_table = FeeTable::zero_rated();
    fee_table.per_module_call_cost = 1;
    f.test.set_fee_table(fee_table);
    f.test.set_burn_rate_bps(0);

    let settlement = settle(
        &f.channel,
        f.deposit.clone(),
        &voucher,
        settlement_public,
        OutputScan::localnet(f.resource),
    )
    .unwrap();
    // This authorization is part of the transaction's public stealth output statement. It is
    // intentionally distinct from the channel's service key, which authorizes the close below.
    assert!(
        settlement
            .outputs_statement
            .outputs
            .iter()
            .any(|output| output.auth == SpendAuthorization::Key(settlement_public))
    );
    assert!(
        !settlement
            .outputs_statement
            .outputs
            .iter()
            .any(|output| output.auth == SpendAuthorization::Key(f.channel.spec().service))
    );
    let tx = Transaction::builder_localnet(Epoch(2))
        .with_fee_instructions_builder(|builder| {
            builder
                .stealth_transfer(TARI_TOKEN, fee_transfer.statement.clone())
                .put_last_instruction_output_on_workspace("fee")
                .pay_fee_from_bucket("fee")
        })
        .stealth_transfer(f.resource, settlement)
        .finish()
        .add_signer(&f.test.to_public_key_bytes(), &fee_mask)
        .seal(&f.service);

    let result = f.test.execute_expect_success(tx, vec![]);
    assert_eq!(
        result
            .finalize
            .fee_receipt
            .fee_breakdown()
            .get(FeeSource::RuntimeCall),
        11
    );
    assert_eq!(result.finalize.fee_receipt.unpaid_debt(), 0);
    assert_eq!(result.finalize.fee_receipt.total_fee_overcharge(), 89);
    // The channel's service authorization key closes the channel; the independent one-time key owns the payout.
    assert_ne!(settlement_public, f.channel.spec().service);
}
