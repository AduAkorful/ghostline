// SPDX-License-Identifier: MIT

use ghostline_core::{ChannelParams, NONCE_LEN};
use tari_ootle_wallet_crypto::stealth::condition_root;
use tari_template_lib::types::{
    FunctionName, Hash32, TemplateAddress,
    bytes::Bytes,
    crypto::RistrettoPublicKeyBytes,
    rule,
    stealth::{BuiltinPredicate, SpendCondition, TemplateFunction},
};

use crate::SdkError;

pub const SETTLE_FUNCTION: &str = "settle";

/// All values the payer fixes when opening a stealth channel.
#[derive(Debug, Clone)]
pub struct ChannelSpec {
    pub network_tag: u8,
    pub payer: RistrettoPublicKeyBytes,
    pub service: RistrettoPublicKeyBytes,
    pub deposit: u64,
    pub expiry_epoch: u64,
    pub nonce: [u8; NONCE_LEN],
    pub channel_template: TemplateAddress,
}

/// The two-leaf condition tree committed by a stealth deposit, plus the bound channel params.
#[derive(Debug, Clone)]
pub struct Channel {
    spec: ChannelSpec,
    params: ChannelParams,
    payer_target: SpendCondition,
    settle_leaf: SpendCondition,
    refund_leaf: SpendCondition,
}

impl Channel {
    pub fn open(spec: ChannelSpec) -> Result<Self, SdkError> {
        if spec.deposit == 0 {
            return Err(SdkError::ZeroDeposit);
        }

        let payer_target = SpendCondition::access_rule(rule!(public_key(spec.payer)));
        let payer_root = condition_root(std::slice::from_ref(&payer_target))?;
        let params = ChannelParams {
            network_tag: spec.network_tag,
            payer: spec.payer,
            deposit: spec.deposit,
            payer_root,
            nonce: spec.nonce,
        };

        let settle_leaf = conjunction([
            SpendCondition::template_function(TemplateFunction::new(
                spec.channel_template,
                settle_function(),
                vec![
                    encode_arg(&spec.network_tag)?,
                    encode_arg(&spec.payer)?,
                    encode_arg(&spec.deposit)?,
                    encode_arg(&payer_root)?,
                    encode_arg(&spec.nonce.to_vec())?,
                ],
            )),
            SpendCondition::builtin(BuiltinPredicate::BeforeEpoch(spec.expiry_epoch)),
            SpendCondition::access_rule(rule!(public_key(spec.service))),
        ]);
        let refund_leaf = conjunction([
            SpendCondition::builtin(BuiltinPredicate::AfterEpoch(spec.expiry_epoch)),
            SpendCondition::access_rule(rule!(public_key(spec.payer))),
        ]);

        Ok(Self {
            spec,
            params,
            payer_target,
            settle_leaf,
            refund_leaf,
        })
    }

    pub fn params(&self) -> &ChannelParams {
        &self.params
    }

    pub fn spec(&self) -> &ChannelSpec {
        &self.spec
    }

    pub fn payer_target(&self) -> &SpendCondition {
        &self.payer_target
    }

    pub fn settle_leaf(&self) -> &SpendCondition {
        &self.settle_leaf
    }

    pub fn refund_leaf(&self) -> &SpendCondition {
        &self.refund_leaf
    }

    pub fn leaves(&self) -> [SpendCondition; 2] {
        [self.settle_leaf.clone(), self.refund_leaf.clone()]
    }

    pub fn leaves_vec(&self) -> Vec<SpendCondition> {
        self.leaves().into()
    }

    pub fn condition_root(&self) -> Result<Hash32, SdkError> {
        Ok(condition_root(&self.leaves_vec())?)
    }
}

fn settle_function() -> FunctionName {
    FunctionName::try_from(SETTLE_FUNCTION).expect("settle is a valid function name")
}

fn conjunction(parts: impl IntoIterator<Item = SpendCondition>) -> SpendCondition {
    SpendCondition::all(
        parts
            .into_iter()
            .flat_map(|c| c.into_conditions().into_vec()),
    )
}

fn encode_arg<T: tari_bor::Encode<()>>(value: &T) -> Result<Bytes, SdkError> {
    tari_bor::encode(value)
        .map(Bytes::from_vec)
        .map_err(|_| SdkError::Encode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;

    fn spec() -> ChannelSpec {
        ChannelSpec {
            network_tag: 1,
            payer: RistrettoPublicKeyBytes::from_bytes(&[1u8; 32]).unwrap(),
            service: RistrettoPublicKeyBytes::from_bytes(&[2u8; 32]).unwrap(),
            deposit: 100,
            expiry_epoch: 50,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        }
    }

    #[test]
    fn open_rejects_zero_deposit() {
        let mut s = spec();
        s.deposit = 0;
        assert!(matches!(Channel::open(s), Err(SdkError::ZeroDeposit)));
    }

    #[test]
    fn payer_root_is_the_payer_key_leaf() {
        let channel = Channel::open(spec()).unwrap();
        let expected = condition_root(std::slice::from_ref(channel.payer_target())).unwrap();
        assert_eq!(channel.params().payer_root, expected);
    }

    #[test]
    fn tree_has_two_leaves() {
        let channel = Channel::open(spec()).unwrap();
        assert_eq!(channel.leaves_vec().len(), 2);
        assert_eq!(channel.settle_leaf().conditions().len(), 3);
        assert_eq!(channel.refund_leaf().conditions().len(), 2);
    }
}
