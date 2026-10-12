// SPDX-License-Identifier: MIT

//! Test helpers: building stealth transfers whose outputs carry a minimum-value promise, and funding a channel UTXO
//! through the test faucet template.
//!
//! The published test tooling fixes every output's minimum-value promise at zero, but the settle predicate's
//! "remainder returns to the payer" rule is only satisfiable when the returned output promises at least the remainder.
//! The transfer builder below sets the promise to each output's value.

use tari_crypto::{
    keys::SecretKey as _,
    ristretto::RistrettoSecretKey,
};
use tari_ootle_common_types::substate_type::SubstateType;
use tari_ootle_transaction::{Epoch, Transaction, args};
use tari_ootle_wallet_crypto::{OutputWitness, StealthInputWitness, StealthOutputWitness, stealth};
use tari_template_lib::types::{
    Amount,
    EncryptedData,
    ResourceAddress,
    crypto::UtxoTag,
    stealth::{SpendAuthorization, StealthTransferStatement},
};
use tari_template_test_tooling::{
    TemplateTest,
    support::{
        spec::{InputAuthSpec, InputSpec, OutputAuthSpec},
        stealth::{self as tooling, test_sender_public_nonce},
    },
};

pub const FAUCET: &str = "TestFaucet";
pub const CHANNEL: &str = "GhostlineChannel";
pub const DEPOSIT: u64 = 100;

pub struct Transfer {
    pub statement: StealthTransferStatement,
}

/// Builds a stealth transfer from `inputs` into `outputs`, where each output promises at least its own value.
pub fn transfer(inputs: Vec<InputSpec>, outputs: Vec<(u64, OutputAuthSpec)>) -> Transfer {
    let outputs = outputs
        .into_iter()
        .filter(|(value, _)| *value > 0)
        .map(|(value, auth)| {
            let mask = RistrettoSecretKey::random(&mut rand::rng());
            let authorization = match auth {
                OutputAuthSpec::KeyPath(pk) => SpendAuthorization::Key(pk),
                OutputAuthSpec::Conditions(conditions) => {
                    SpendAuthorization::Script(stealth::condition_root(&conditions).unwrap())
                },
                _ => panic!("unsupported output auth in channel tests"),
            };
            StealthOutputWitness {
                witness: OutputWitness {
                    amount: value,
                    mask,
                    sender_public_nonce: test_sender_public_nonce(),
                    minimum_value_promise: value,
                    encrypted_data: EncryptedData::try_from(vec![0; EncryptedData::min_size()]).unwrap(),
                    resource_view_key: None,
                },
                auth: authorization,
                tag: UtxoTag::new(0),
            }
        })
        .collect::<Vec<_>>();

    let inputs = inputs.into_iter().map(|input| match input.auth() {
        InputAuthSpec::KeyPath => StealthInputWitness::new(input.mask_and_value().clone()),
        InputAuthSpec::ScriptPath { conditions, leaf, data } => {
            let (witness, root) = stealth::script_path_witness_with_data(conditions, leaf, data.clone()).unwrap();
            StealthInputWitness::with_script_path(input.mask_and_value().clone(), witness, root)
        },
    });

    let statement = stealth::create_transfer_statement(inputs, Amount::zero(), outputs.iter(), None).unwrap();
    Transfer { statement }
}

/// Mints a single `DEPOSIT`-valued stealth UTXO locked by `auth` and returns the resource address plus its mask.
pub fn fund(test: &mut TemplateTest, auth: OutputAuthSpec) -> (ResourceAddress, RistrettoSecretKey) {
    test.enable_auto_add_proofs_from_signers();
    let mint = tooling::generate_mint_statement(vec![(DEPOSIT, auth)], 0u64, None);
    let faucet = test.get_template_address(FAUCET);
    let tx = Transaction::builder_localnet(Epoch(1))
        .call_function(faucet, "new", args![
            Amount::from(DEPOSIT),
            mint.statement.clone()
        ])
        .build_and_seal(test.secret_key());
    test.execute_expect_success(tx, vec![]);
    let resource = test
        .get_previous_output_address(SubstateType::Resource)
        .as_resource_address()
        .unwrap();
    (resource, mint.output_masks[0].clone())
}
