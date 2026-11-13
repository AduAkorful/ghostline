// SPDX-License-Identifier: MIT

//! Ghostline protocol fee configuration.
//!
//! One component holds the treasury key that channel settlements pay their fee to. The channel template pins this
//! component's address at build time and reads its state, so the treasury can move without republishing the channel
//! template. The creator is the owner and is the only key allowed to change the treasury.
//!
//! The component address is derived from the owner's public key, and the constructor uses the transaction signer as
//! that key. Nobody else can create a component at the owner's derived address of this template.

use tari_template_lib::prelude::*;

#[template]
mod fee_config {
    use super::*;

    /// Field order is part of the channel template's decoding contract: the treasury key must stay at index 0.
    pub struct GhostlineFeeConfig {
        treasury: RistrettoPublicKeyBytes,
        owner: RistrettoPublicKeyBytes,
    }

    impl GhostlineFeeConfig {
        pub fn new(treasury: RistrettoPublicKeyBytes) -> Component<Self> {
            let owner = CallerContext::transaction_signer_public_key();
            Component::new(Self { treasury, owner })
                .with_public_key_address(owner)
                .with_owner_rule(OwnerRule::ByPublicKey(owner))
                .with_access_rules(
                    AccessRules::allow_all().add_method_rule("set_treasury", rule!(public_key(owner))),
                )
                .create()
        }

        pub fn treasury(&self) -> RistrettoPublicKeyBytes {
            self.treasury
        }

        pub fn owner(&self) -> RistrettoPublicKeyBytes {
            self.owner
        }

        pub fn set_treasury(&mut self, treasury: RistrettoPublicKeyBytes) {
            self.treasury = treasury;
        }
    }
}
