// SPDX-License-Identifier: MIT

//! Test helper for funding channel UTXOs through the local faucet template.

use ootle_byte_type::ToByteType;
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_engine_types::component::derive_component_address_from_public_key;
use tari_ootle_common_types::{crypto::create_key_pair_from_seed, substate_type::SubstateType};
use tari_ootle_transaction::{Epoch, Transaction, args};
use tari_template_lib::types::{
    Amount, ComponentAddress, ResourceAddress, TemplateAddress, crypto::RistrettoPublicKeyBytes,
};
use tari_template_test_tooling::{
    TemplateTest,
    support::{
        spec::OutputAuthSpec,
        stealth::{self as tooling},
    },
};
use tari_utilities::hex::to_hex;

pub const FAUCET: &str = "TestFaucet";
pub const CHANNEL: &str = "GhostlineChannel";
pub const FEE_CONFIG: &str = "GhostlineFeeConfig";
pub const DEPOSIT: u64 = 1_000;
/// `Network::LocalNet`. Template tests execute with `Transaction::builder_localnet`, and a
/// template signature is bound to that network.
pub const LOCALNET_TAG: u8 = 0x10;

/// Mints a single `DEPOSIT`-valued stealth UTXO locked by `auth` and returns the resource address plus its mask.
#[allow(dead_code)]
pub fn fund(
    test: &mut TemplateTest,
    auth: OutputAuthSpec,
) -> (ResourceAddress, RistrettoSecretKey) {
    test.enable_auto_add_proofs_from_signers();
    let mint = tooling::generate_mint_statement(vec![(DEPOSIT, auth)], 0u64, None);
    let faucet = test.get_template_address(FAUCET);
    let tx = Transaction::builder_localnet(Epoch(1))
        .call_function(
            faucet,
            "new",
            args![Amount::from(DEPOSIT), mint.statement.clone()],
        )
        .build_and_seal(test.secret_key());
    test.execute_expect_success(tx, vec![]);
    let resource = test
        .get_previous_output_address(SubstateType::Resource)
        .as_resource_address()
        .unwrap();
    (resource, mint.output_masks[0].clone())
}

/// Owner and treasury of the fee config a test channel template is pinned to.
#[allow(dead_code)]
pub struct FeeSetup {
    pub channel_template: TemplateAddress,
    pub config_template: TemplateAddress,
    pub config: ComponentAddress,
    pub owner_secret: RistrettoSecretKey,
    pub owner: RistrettoPublicKeyBytes,
    pub treasury: RistrettoPublicKeyBytes,
}

/// Faucet, fee config and channel templates, with the config created by its owner and the channel built against it.
/// The channel template is compiled after the config exists because it pins the config address at build time.
#[allow(dead_code)]
pub fn templates() -> (TemplateTest, FeeSetup) {
    let mut test = TemplateTest::new(env!("CARGO_MANIFEST_DIR"), ["templates/faucet"]);
    let config_template = test.compile_new_template(
        FEE_CONFIG,
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../contracts/fee-config"),
        &[],
        Vec::<(&str, &str)>::new(),
    );
    let (owner_secret, owner_public) = create_key_pair_from_seed(201);
    let (_, treasury_public) = create_key_pair_from_seed(202);
    let owner: RistrettoPublicKeyBytes = owner_public.to_byte_type();
    let treasury: RistrettoPublicKeyBytes = treasury_public.to_byte_type();
    let tx = Transaction::builder_localnet(Epoch(1))
        .call_function(config_template, "new", args![treasury])
        .build_and_seal(&owner_secret);
    test.execute_expect_success(tx, vec![]);
    let config = derive_component_address_from_public_key(&config_template, &owner);
    let channel_template = test.compile_new_template(
        CHANNEL,
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../contracts/channel"),
        &[],
        [("GHOSTLINE_FEE_CONFIG", to_hex(config.as_bytes()))],
    );
    (
        test,
        FeeSetup {
            channel_template,
            config_template,
            config,
            owner_secret,
            owner,
            treasury,
        },
    )
}
