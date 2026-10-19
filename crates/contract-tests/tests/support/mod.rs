// SPDX-License-Identifier: MIT

//! Test helper for funding channel UTXOs through the local faucet template.

use tari_crypto::ristretto::RistrettoSecretKey;
use tari_ootle_common_types::substate_type::SubstateType;
use tari_ootle_transaction::{Epoch, Transaction, args};
use tari_template_lib::types::{Amount, ResourceAddress};
use tari_template_test_tooling::{
    TemplateTest,
    support::{
        spec::OutputAuthSpec,
        stealth::{self as tooling},
    },
};

pub const FAUCET: &str = "TestFaucet";
pub const CHANNEL: &str = "GhostlineChannel";
pub const DEPOSIT: u64 = 100;

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
