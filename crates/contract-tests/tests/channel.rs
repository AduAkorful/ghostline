// SPDX-License-Identifier: MIT

//! Engine-level tests for the Ghostline channel: one stealth UTXO with a settle leaf and a refund leaf.
//!
//! - settle leaf: voucher predicate AND before-expiry AND service key signs
//! - refund leaf: at-or-after-expiry AND payer key signs
//!
//! `BeforeEpoch(e)` admits epochs strictly below `e` and `AfterEpoch(e)` admits `e` and above, so exactly one leaf is
//! open at any epoch. An expiry of `u64::MAX` is "not yet expired" and `0` is "already expired" for every test epoch.

mod support;

use ghostline_core::{NONCE_LEN, SignedVoucher};
use ghostline_sdk::{Channel, ChannelSpec, MaskAndValue, OutputScan, fund_open, refund, settle};
use ootle_byte_type::ToByteType;
use support::{CHANNEL, DEPOSIT, fund};
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_ootle_transaction::{Epoch, Transaction};
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;
use tari_template_test_tooling::{TemplateTest, support::spec::OutputAuthSpec};

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

fn run(s: Scenario) -> Result<(), String> {
    let mut test = TemplateTest::new(
        env!("CARGO_MANIFEST_DIR"),
        ["templates/faucet", "../../contracts/channel"],
    );
    let channel_template = test.get_template_address(CHANNEL);

    let (payer_secret, payer_public) = create_key_pair_from_seed(11);
    let (service_secret, service_public) = create_key_pair_from_seed(13);
    let (other_secret, _) = create_key_pair_from_seed(14);
    let payer: RistrettoPublicKeyBytes = payer_public.to_byte_type();
    let service: RistrettoPublicKeyBytes = service_public.to_byte_type();

    let channel = Channel::open(ChannelSpec {
        network_tag: NETWORK_TAG,
        payer,
        service,
        deposit: DEPOSIT,
        expiry_epoch: s.expiry,
        nonce: NONCE,
        channel_template,
    })
    .map_err(|e| e.to_string())?;

    let (resource, mask) = fund(&mut test, OutputAuthSpec::Conditions(channel.leaves_vec()));
    let funded = MaskAndValue {
        mask,
        value: DEPOSIT,
    };

    let statement = match s.action {
        Action::Settle => {
            let signing_key = if s.forged {
                &other_secret
            } else {
                &payer_secret
            };
            let mut params = channel.params().clone();
            params.network_tag = s.voucher_network;
            params.nonce = s.voucher_nonce;
            let mut voucher = SignedVoucher::sign(signing_key, &params, s.signed_amount, service)
                .map_err(|e| e.to_string())?;
            voucher.amount = s.claimed_amount;
            settle(
                &channel,
                funded,
                &voucher,
                service,
                OutputScan::localnet(resource),
            )
            .map_err(|e| e.to_string())?
        }
        Action::Refund => refund(&channel, funded, payer, OutputScan::localnet(resource))
            .map_err(|e| e.to_string())?,
    };

    let secret = match s.signer {
        Signer::Payer => &payer_secret,
        Signer::Service => &service_secret,
        Signer::Other => &other_secret,
    };
    let tx = Transaction::builder_localnet(Epoch(1))
        .stealth_transfer(resource, statement)
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
    assert!(
        reason.contains(expected),
        "expected `{expected}` in rejection, got: {reason}"
    );
}

// ---------------------------------- Settlement ---------------------------------- //

#[test]
fn settle_valid_voucher_pays_service_and_returns_remainder() {
    run(Scenario::settle()).expect("valid voucher must settle");
}

#[test]
fn settle_full_deposit_is_rejected_to_preserve_fixed_output_shape() {
    let reason = run(Scenario {
        signed_amount: DEPOSIT,
        claimed_amount: DEPOSIT,
        ..Scenario::settle()
    })
    .expect_err("full-deposit voucher must not be accepted");
    assert!(reason.contains("channel spend limit"));
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
fn settle_changed_opening_is_rejected() {
    assert_rejected(
        run(Scenario {
            claimed_amount: 60,
            ..Scenario::settle()
        }),
        "do not open its commitment",
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
        "channel spend limit",
    );
}

#[test]
fn settle_voucher_for_another_channel_is_rejected() {
    assert_rejected(
        run(Scenario {
            voucher_nonce: [43u8; NONCE_LEN],
            ..Scenario::settle()
        }),
        "voucher channel id does not match this channel",
    );
}

#[test]
fn settle_voucher_for_another_network_is_rejected() {
    assert_rejected(
        run(Scenario {
            voucher_network: NETWORK_TAG + 1,
            ..Scenario::settle()
        }),
        "voucher channel id does not match this channel",
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

#[test]
fn sdk_open_from_key_path_then_settle() {
    let mut test = TemplateTest::new(
        env!("CARGO_MANIFEST_DIR"),
        ["templates/faucet", "../../contracts/channel"],
    );
    let channel_template = test.get_template_address(CHANNEL);
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
        channel_template,
    })
    .unwrap();

    let (resource, mask) = fund(&mut test, OutputAuthSpec::KeyPath(payer));
    let opened = fund_open(
        &channel,
        [MaskAndValue {
            mask,
            value: DEPOSIT,
        }],
        payer,
        OutputScan::localnet(resource),
    )
    .unwrap();
    let open_tx = Transaction::builder_localnet(Epoch(1))
        .stealth_transfer(resource, opened.statement)
        .finish()
        .seal(&payer_secret);
    test.execute_expect_success(open_tx, vec![]);

    let voucher = SignedVoucher::sign(&payer_secret, channel.params(), SIGNED, service).unwrap();
    let settle_stmt = settle(
        &channel,
        opened.deposit,
        &voucher,
        service,
        OutputScan::localnet(resource),
    )
    .unwrap();
    let settle_tx = Transaction::builder_localnet(Epoch(1))
        .stealth_transfer(resource, settle_stmt)
        .finish()
        .seal(&service_secret);
    test.execute_expect_success(settle_tx, vec![]);
}
