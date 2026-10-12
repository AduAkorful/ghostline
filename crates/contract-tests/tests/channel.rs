// SPDX-License-Identifier: MIT

//! Engine-level tests for the Ghostline channel: one stealth UTXO with a settle leaf and a refund leaf.
//!
//! - settle leaf: voucher predicate AND before-expiry AND service key signs
//! - refund leaf: at-or-after-expiry AND payer key signs
//!
//! `BeforeEpoch(e)` admits epochs strictly below `e` and `AfterEpoch(e)` admits `e` and above, so exactly one leaf is
//! open at any epoch. An expiry of `u64::MAX` is "not yet expired" and `0` is "already expired" for every test epoch.

mod support;

use ghostline_core::{ChannelParams, NONCE_LEN, sign_voucher, witness_data};
use ootle_byte_type::ToByteType;
use support::{CHANNEL, DEPOSIT, fund, transfer};
use tari_engine_types::stealth::MerkleTree;
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_ootle_transaction::{Epoch, Transaction};
use tari_template_lib::types::{
    FunctionName,
    Hash32,
    bytes::Bytes,
    crypto::{RistrettoPublicKeyBytes, Signature},
    rule,
    stealth::{BuiltinPredicate, SpendCondition, TemplateFunction},
};
use tari_template_test_tooling::{
    TemplateTest,
    support::spec::{InputAuthSpec, InputSpec, OutputAuthSpec},
    wallet_crypto::MaskAndValue,
};

const NETWORK_TAG: u8 = 1;
const NONCE: [u8; NONCE_LEN] = [42u8; NONCE_LEN];
const SIGNED: u64 = 30;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Settle,
    Refund,
}

#[derive(Clone, Copy, PartialEq)]
enum Signer {
    Payer,
    Service,
    Other,
}

struct Scenario {
    expiry: u64,
    action: Action,
    signer: Signer,
    signed_amount: u64,
    claimed_amount: u64,
    forged: bool,
    voucher_nonce: [u8; NONCE_LEN],
    voucher_network: u8,
    payer_gets: Option<u64>,
}

impl Scenario {
    fn settle() -> Self {
        Self {
            expiry: u64::MAX,
            action: Action::Settle,
            signer: Signer::Service,
            signed_amount: SIGNED,
            claimed_amount: SIGNED,
            forged: false,
            voucher_nonce: NONCE,
            voucher_network: NETWORK_TAG,
            payer_gets: None,
        }
    }

    fn refund() -> Self {
        Self {
            expiry: 0,
            action: Action::Refund,
            signer: Signer::Payer,
            ..Self::settle()
        }
    }
}

fn encode_arg<T: tari_bor::Encode<()>>(value: &T) -> Bytes {
    Bytes::from_vec(tari_bor::encode(value).unwrap())
}

fn all(atoms: Vec<SpendCondition>) -> SpendCondition {
    SpendCondition::all(atoms.into_iter().flat_map(|c| c.into_conditions().into_vec()))
}

fn builtin(predicate: BuiltinPredicate) -> SpendCondition {
    SpendCondition::builtin(predicate)
}

fn run(s: Scenario) -> Result<(), String> {
    let mut test = TemplateTest::new(env!("CARGO_MANIFEST_DIR"), [
        "templates/faucet",
        "../../contracts/channel",
    ]);
    let channel_template = test.get_template_address(CHANNEL);

    let (payer_secret, payer_public) = create_key_pair_from_seed(11);
    let (service_secret, service_public) = create_key_pair_from_seed(13);
    let (other_secret, _) = create_key_pair_from_seed(14);
    let payer: RistrettoPublicKeyBytes = payer_public.to_byte_type();
    let service: RistrettoPublicKeyBytes = service_public.to_byte_type();

    let payer_target = SpendCondition::access_rule(rule!(public_key(payer)));
    let payer_root: Hash32 = MerkleTree::from_conditions([&payer_target]).unwrap().root();

    let settle_leaf = all(vec![
        SpendCondition::template_function(TemplateFunction::new(
            channel_template,
            FunctionName::try_from("settle").unwrap(),
            vec![
                encode_arg(&NETWORK_TAG),
                encode_arg(&payer),
                encode_arg(&DEPOSIT),
                encode_arg(&payer_root),
                encode_arg(&NONCE.to_vec()),
            ],
        )),
        builtin(BuiltinPredicate::BeforeEpoch(s.expiry)),
        SpendCondition::access_rule(rule!(public_key(service))),
    ]);
    let refund_leaf = all(vec![
        builtin(BuiltinPredicate::AfterEpoch(s.expiry)),
        SpendCondition::access_rule(rule!(public_key(payer))),
    ]);
    let tree = vec![settle_leaf.clone(), refund_leaf.clone()];

    let (resource, mask) = fund(&mut test, OutputAuthSpec::Conditions(tree.clone()));
    let funded = MaskAndValue { mask, value: DEPOSIT };

    let spend = match s.action {
        Action::Settle => {
            let params = ChannelParams {
                network_tag: s.voucher_network,
                payer,
                deposit: DEPOSIT,
                payer_root,
                nonce: s.voucher_nonce,
            };
            let signing_key = if s.forged { &other_secret } else { &payer_secret };
            let signature: Signature<_> = sign_voucher(signing_key, &params, s.signed_amount);
            let data = witness_data(s.claimed_amount, &signature);
            let input = InputSpec::with_auth(funded, InputAuthSpec::script_path(tree, settle_leaf, data));
            let payer_gets = s.payer_gets.unwrap_or(DEPOSIT.saturating_sub(s.claimed_amount));
            let service_gets = DEPOSIT - payer_gets;
            transfer(vec![input], vec![
                (payer_gets, OutputAuthSpec::Conditions(vec![payer_target])),
                (service_gets, OutputAuthSpec::KeyPath(service)),
            ])
        },
        Action::Refund => {
            let input = InputSpec::with_auth(
                funded,
                InputAuthSpec::script_path(tree, refund_leaf, Bytes::from_vec(vec![])),
            );
            transfer(vec![input], vec![(DEPOSIT, OutputAuthSpec::KeyPath(payer))])
        },
    };

    let secret = match s.signer {
        Signer::Payer => &payer_secret,
        Signer::Service => &service_secret,
        Signer::Other => &other_secret,
    };
    let tx = Transaction::builder_localnet(Epoch(1))
        .stealth_transfer(resource, spend.statement)
        .finish()
        .seal(secret);

    let result = test.try_execute(tx, vec![]).map_err(|e| format!("{e:?}"))?;
    if result.finalize.result.is_accept() {
        Ok(())
    } else {
        Err(format!("{:?}", result.expect_failure()))
    }
}

fn assert_rejected(result: Result<(), String>, expected: &str) {
    let reason = result.expect_err("scenario must be rejected");
    assert!(reason.contains(expected), "expected `{expected}` in rejection, got: {reason}");
}

// ---------------------------------- Settlement ---------------------------------- //

#[test]
fn settle_valid_voucher_pays_service_and_returns_remainder() {
    run(Scenario::settle()).expect("valid voucher must settle");
}

#[test]
fn settle_forged_signature_is_rejected() {
    assert_rejected(
        run(Scenario {
            forged: true,
            ..Scenario::settle()
        }),
        "invalid voucher signature",
    );
}

#[test]
fn settle_claim_above_signed_amount_is_rejected() {
    assert_rejected(
        run(Scenario {
            claimed_amount: 60,
            ..Scenario::settle()
        }),
        "invalid voucher signature",
    );
}

#[test]
fn settle_amount_above_deposit_is_rejected() {
    assert_rejected(
        run(Scenario {
            signed_amount: 150,
            claimed_amount: 150,
            ..Scenario::settle()
        }),
        "voucher exceeds deposit",
    );
}

#[test]
fn settle_remainder_not_returned_to_payer_is_rejected() {
    assert_rejected(
        run(Scenario {
            payer_gets: Some(0),
            ..Scenario::settle()
        }),
        "required output not present",
    );
}

#[test]
fn settle_voucher_for_another_channel_is_rejected() {
    assert_rejected(
        run(Scenario {
            voucher_nonce: [43u8; NONCE_LEN],
            ..Scenario::settle()
        }),
        "invalid voucher signature",
    );
}

#[test]
fn settle_voucher_for_another_network_is_rejected() {
    assert_rejected(
        run(Scenario {
            voucher_network: NETWORK_TAG + 1,
            ..Scenario::settle()
        }),
        "invalid voucher signature",
    );
}

// ------------------------------ Access and deadlines ------------------------------ //

#[test]
fn settle_by_non_service_is_rejected() {
    assert_rejected(
        run(Scenario {
            signer: Signer::Other,
            ..Scenario::settle()
        }),
        "Access Denied",
    );
}

#[test]
fn settle_after_expiry_is_rejected() {
    assert_rejected(
        run(Scenario {
            expiry: 0,
            ..Scenario::settle()
        }),
        "BeforeEpoch",
    );
}

#[test]
fn refund_by_payer_after_expiry_succeeds() {
    run(Scenario::refund()).expect("payer refund after expiry");
}

#[test]
fn refund_before_expiry_is_rejected() {
    assert_rejected(
        run(Scenario {
            expiry: u64::MAX,
            ..Scenario::refund()
        }),
        "AfterEpoch",
    );
}

#[test]
fn refund_by_non_payer_is_rejected() {
    assert_rejected(
        run(Scenario {
            signer: Signer::Service,
            ..Scenario::refund()
        }),
        "Access Denied",
    );
}
