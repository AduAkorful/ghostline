// SPDX-License-Identifier: MIT

//! Test-only template that mints a stealth resource into arbitrary condition-locked UTXOs, so contract tests can
//! fund a channel output without a wallet.

use tari_template_lib::prelude::*;

#[template]
mod faucet {
    use super::*;

    pub struct TestFaucet {
        supply_vault: Vault,
    }

    impl TestFaucet {
        pub fn new(initial_supply: Amount, mint: StealthTransferStatement) -> Component<Self> {
            let bucket = ResourceBuilder::stealth()
                .mintable(rule!(allow_all), OWNER)
                .initial_supply(initial_supply);

            let revealed = bucket.stealth_transfer(mint);
            let supply_vault = Vault::from_bucket(revealed);

            Component::new(Self { supply_vault })
                .with_access_rules(AccessRules::allow_all())
                .create()
        }
    }
}
