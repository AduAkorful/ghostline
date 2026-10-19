// SPDX-License-Identifier: MIT

//! Test helpers used only by pooled settlement integration tests.

use tari_crypto::ristretto::RistrettoSecretKey;
use tari_ootle_common_types::substate_type::SubstateType;
use tari_ootle_transaction::{Epoch, Transaction, args};
use tari_template_lib::types::{Amount, ResourceAddress};
use tari_template_test_tooling::{
    TemplateTest,
    support::{spec::OutputAuthSpec, stealth as tooling},
};

use crate::support::{DEPOSIT, FAUCET};

/// Mints multiple stealth outputs for the same resource in one faucet call.
pub fn fund_many(
    test: &mut TemplateTest,
    auths: Vec<OutputAuthSpec>,
) -> (ResourceAddress, Vec<RistrettoSecretKey>) {
    test.enable_auto_add_proofs_from_signers();
    let outputs: Vec<(u64, OutputAuthSpec)> =
        auths.into_iter().map(|auth| (DEPOSIT, auth)).collect();
    let mint = tooling::generate_mint_statement(outputs, 0u64, None);
    let faucet = test.get_template_address(FAUCET);
    let tx = Transaction::builder_localnet(Epoch(1))
        .call_function(
            faucet,
            "new",
            args![
                Amount::from(DEPOSIT * mint.output_masks.len() as u64),
                mint.statement.clone()
            ],
        )
        .build_and_seal(test.secret_key());
    test.execute_expect_success(tx, vec![]);
    let resource = test
        .get_previous_output_address(SubstateType::Resource)
        .as_resource_address()
        .unwrap();
    (resource, mint.output_masks)
}
