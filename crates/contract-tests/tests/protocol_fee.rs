// SPDX-License-Identifier: MIT

//! Protocol fee: settlement must pay `payout / FEE_DIVISOR` to the treasury key held by the pinned fee config, and
//! only the config owner can move the treasury.

mod support;

use ghostline_core::{NONCE_LEN, SignedVoucher, VoucherError};
use ghostline_sdk::{
    Channel, ChannelSpec, FeeOverride, MaskAndValue, OutputScan, SdkError, WalletDestination,
    settle, settle_to_wallet, settle_with_fee_override,
};
use ootle_byte_type::ToByteType;
use support::{DEPOSIT, FEE_CONFIG, FeeSetup, fund, templates};
use tari_crypto::{keys::SecretKey as _, ristretto::RistrettoSecretKey};
use tari_engine_types::component::derive_component_address_from_public_key;
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_ootle_transaction::{Epoch, Transaction, args};
use tari_template_lib::types::{
    ComponentAddress, crypto::RistrettoPublicKeyBytes, stealth::SpendAuthorization,
};
use tari_template_test_tooling::{TemplateTest, support::spec::OutputAuthSpec};

const NETWORK_TAG: u8 = support::LOCALNET_TAG;
const PAYOUT: u64 = 500;

struct Fixture {
    test: TemplateTest,
    fee: FeeSetup,
    channel: Channel,
    deposit: MaskAndValue,
    voucher: SignedVoucher,
    service: RistrettoPublicKeyBytes,
    service_secret: tari_crypto::ristretto::RistrettoSecretKey,
    scan: OutputScan,
}

fn fixture() -> Fixture {
    let (test, fee) = templates();
    let config = fee.config;
    fixture_with_config(test, fee, config)
}

fn fixture_with_config(
    mut test: TemplateTest,
    fee: FeeSetup,
    fee_config: ComponentAddress,
) -> Fixture {
    let (payer_secret, payer_public) = create_key_pair_from_seed(11);
    let (service_secret, service_public) = create_key_pair_from_seed(13);
    let service: RistrettoPublicKeyBytes = service_public.to_byte_type();
    let channel = Channel::open(ChannelSpec {
        network_tag: NETWORK_TAG,
        payer: payer_public.to_byte_type(),
        service,
        deposit: DEPOSIT,
        expiry_epoch: u64::MAX,
        nonce: [7; NONCE_LEN],
        channel_template: fee.channel_template,
        fee_config,
    })
    .unwrap();
    let (resource, mask) = fund(&mut test, OutputAuthSpec::Conditions(channel.leaves_vec()));
    let voucher = SignedVoucher::sign(&payer_secret, channel.params(), PAYOUT, service).unwrap();
    Fixture {
        test,
        fee,
        channel,
        deposit: MaskAndValue {
            mask,
            value: DEPOSIT,
        },
        voucher,
        service,
        service_secret,
        scan: OutputScan::localnet(resource),
    }
}

fn settle_to(f: &mut Fixture, treasury: RistrettoPublicKeyBytes) -> Result<(), String> {
    let statement = settle(
        &f.channel,
        f.deposit.clone(),
        &f.voucher,
        f.service,
        treasury,
        f.scan,
    )
    .map_err(|e| e.to_string())?;
    submit(f, statement)
}

fn settle_overriding(f: &mut Fixture, fee: FeeOverride) -> Result<(), String> {
    let statement = settle_with_fee_override(
        &f.channel,
        f.deposit.clone(),
        &f.voucher,
        f.service,
        fee,
        f.scan,
    )
    .map_err(|e| e.to_string())?;
    submit(f, statement)
}

fn submit(
    f: &mut Fixture,
    statement: tari_template_lib::types::stealth::StealthTransferStatement,
) -> Result<(), String> {
    let resource = f.scan.resource;
    let tx = Transaction::builder_localnet(Epoch(1))
        .stealth_transfer(resource, statement)
        .finish()
        .seal(&f.service_secret);
    let result = f
        .test
        .try_execute(tx, vec![])
        .map_err(|e| format!("{e:?}"))?;
    if result.finalize.result.is_accept() {
        Ok(())
    } else {
        Err(format!("{:?}", result.expect_failure()))
    }
}

#[test]
fn settlement_to_a_wallet_destination_is_accepted() {
    // The payout is locked to a one-time key derived from the service wallet's address, which the voucher names as
    // its service output key. The template only checks that key, so the walletd-detectable shape settles as before.
    let mut f = fixture();
    let (_, account) = create_key_pair_from_seed(31);
    let (_, view) = create_key_pair_from_seed(32);
    let payout = WalletDestination::new(
        f.scan.network,
        account.to_byte_type(),
        view.to_byte_type(),
        RistrettoSecretKey::random(&mut rand::rng()),
    )
    .unwrap();
    let (payer_secret, _) = create_key_pair_from_seed(11);
    f.voucher = SignedVoucher::sign(
        &payer_secret,
        f.channel.params(),
        PAYOUT,
        payout.owner_key(),
    )
    .unwrap();
    let statement = settle_to_wallet(
        &f.channel,
        f.deposit.clone(),
        &f.voucher,
        &payout,
        f.fee.treasury,
        f.scan,
    )
    .unwrap();
    submit(&mut f, statement).expect("wallet-addressed settle");
}

#[test]
fn settlement_pays_fee_to_config_treasury() {
    let mut f = fixture();
    let treasury = f.fee.treasury;
    settle_to(&mut f, treasury).expect("settle with the configured treasury");
}

fn call(
    test: &mut TemplateTest,
    signer: &RistrettoSecretKey,
    tx: tari_ootle_transaction::TransactionBuilder,
) -> Result<(), String> {
    test.enable_auto_add_proofs_from_signers();
    // Config changes must persist for the settlement that follows; `try_execute` alone does not commit.
    let result = test.execute_and_commit_on_success(tx.build_and_seal(signer), vec![]);
    if result.finalize.result.is_accept() {
        Ok(())
    } else {
        Err(format!("{:?}", result.expect_failure()))
    }
}

fn set_treasury(
    f: &mut Fixture,
    signer: &RistrettoSecretKey,
    treasury: RistrettoPublicKeyBytes,
) -> Result<(), String> {
    let tx = Transaction::builder_localnet(Epoch(1)).call_method(
        f.fee.config,
        "set_treasury",
        args![treasury],
    );
    call(&mut f.test, signer, tx)
}

fn honest_fee(f: &Fixture) -> FeeOverride {
    FeeOverride {
        amount: PAYOUT / ghostline_core::FEE_DIVISOR,
        mask: None,
        auth: Some(SpendAuthorization::Key(f.fee.treasury)),
        recipient: f.fee.treasury,
    }
}

fn assert_rejected(result: Result<(), String>, expected: &str) {
    let reason = result.expect_err("settlement must be rejected");
    assert!(
        reason.contains(expected),
        "expected `{expected}` in rejection, got: {reason}"
    );
}

#[test]
fn honest_override_matches_the_standard_builder() {
    let mut f = fixture();
    let fee = honest_fee(&f);
    settle_overriding(&mut f, fee).expect("honest fee through the override builder");
}

#[test]
fn fee_to_any_other_key_is_rejected() {
    let mut f = fixture();
    let (_, other) = create_key_pair_from_seed(77);
    assert_rejected(
        settle_to(&mut f, other.to_byte_type()),
        "unauthorized output",
    );
}

#[test]
fn fee_below_the_quotient_is_rejected() {
    let mut f = fixture();
    let fee = FeeOverride {
        amount: PAYOUT / ghostline_core::FEE_DIVISOR - 1,
        ..honest_fee(&f)
    };
    assert_rejected(settle_overriding(&mut f, fee), "fee output does not match");
}

#[test]
fn fee_above_the_quotient_is_rejected() {
    let mut f = fixture();
    let fee = FeeOverride {
        amount: PAYOUT / ghostline_core::FEE_DIVISOR + 1,
        ..honest_fee(&f)
    };
    assert_rejected(settle_overriding(&mut f, fee), "fee output does not match");
}

#[test]
fn fee_with_an_unrelated_mask_is_rejected() {
    let mut f = fixture();
    let fee = FeeOverride {
        mask: Some(RistrettoSecretKey::random(&mut rand::rng())),
        ..honest_fee(&f)
    };
    assert_rejected(settle_overriding(&mut f, fee), "fee output does not match");
}

#[test]
fn missing_fee_output_is_rejected() {
    let mut f = fixture();
    let fee = FeeOverride {
        auth: None,
        ..honest_fee(&f)
    };
    assert_rejected(settle_overriding(&mut f, fee), "exactly three outputs");
}

#[test]
fn owner_moves_the_treasury_for_open_channels() {
    let mut f = fixture();
    let old = f.fee.treasury;
    let (_, new_treasury) = create_key_pair_from_seed(203);
    let new_treasury: RistrettoPublicKeyBytes = new_treasury.to_byte_type();
    let owner = f.fee.owner_secret.clone();
    set_treasury(&mut f, &owner, new_treasury).expect("owner sets the treasury");

    assert_rejected(settle_to(&mut f, old), "unauthorized output");
    settle_to(&mut f, new_treasury).expect("settle pays the new treasury");
}

#[test]
fn non_owner_cannot_move_the_treasury() {
    let mut f = fixture();
    let (attacker, attacker_public) = create_key_pair_from_seed(66);
    let err = set_treasury(&mut f, &attacker, attacker_public.to_byte_type())
        .expect_err("non-owner must not set the treasury");
    assert!(
        err.contains("AccessDenied") || err.contains("denied"),
        "{err}"
    );

    let treasury = f.fee.treasury;
    settle_to(&mut f, treasury).expect("treasury is unchanged");
}

#[test]
fn channel_naming_another_config_is_rejected() {
    let (mut test, fee) = templates();
    let config_template = test.get_template_address(FEE_CONFIG);
    let (attacker, attacker_public) = create_key_pair_from_seed(66);
    let attacker_key: RistrettoPublicKeyBytes = attacker_public.to_byte_type();
    let tx = Transaction::builder_localnet(Epoch(1)).call_function(
        config_template,
        "new",
        args![attacker_key],
    );
    call(&mut test, &attacker, tx).expect("anyone can create their own config component");
    let rogue = derive_component_address_from_public_key(&config_template, &attacker_key);
    assert_ne!(rogue, fee.config);

    let mut f = fixture_with_config(test, fee, rogue);
    assert_rejected(settle_to(&mut f, attacker_key), "wrong fee config");
}

#[test]
fn builder_refuses_payouts_that_are_not_fee_aligned() {
    let f = fixture();
    let (payer_secret, _) = create_key_pair_from_seed(11);
    let voucher =
        SignedVoucher::sign(&payer_secret, f.channel.params(), PAYOUT + 50, f.service).unwrap();
    let err = settle(
        &f.channel,
        f.deposit.clone(),
        &voucher,
        f.service,
        f.fee.treasury,
        f.scan,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        SdkError::Voucher(VoucherError::NotFeeAligned {
            amount: 550,
            divisor: 100
        })
    ));
}

#[test]
fn builder_refuses_payouts_whose_fee_exhausts_the_deposit() {
    let f = fixture();
    let (payer_secret, _) = create_key_pair_from_seed(11);
    // 900 + 9 fits; 1_000 + 10 does not, and 1_000 itself already exceeds the 1 µT reserve.
    let ok = SignedVoucher::sign(&payer_secret, f.channel.params(), 900, f.service).unwrap();
    settle(
        &f.channel,
        f.deposit.clone(),
        &ok,
        f.service,
        f.fee.treasury,
        f.scan,
    )
    .unwrap();
    assert!(SignedVoucher::sign(&payer_secret, f.channel.params(), DEPOSIT, f.service).is_err());
}
