// SPDX-License-Identifier: MIT

//! Inputs for the protocol fee: which fee config a channel names and which treasury key a settlement pays.

use std::str::FromStr;

use tari_engine_types::substate::SubstateValue;
use tari_template_lib::types::{ComponentAddress, crypto::RistrettoPublicKeyBytes};

/// Treasury key a settlement pays its fee to. It must match the fee config's current state, or the template rejects
/// the settlement and the network fee is still charged.
pub const TREASURY_ENV: &str = "GHOSTLINE_TREASURY_KEY";

/// Accepts raw hex or an Ootle `component_<hex>` address. An empty value means the record predates the protocol fee.
pub fn parse_fee_config(value: &str) -> Result<ComponentAddress, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(
            "fee_config is not set; the channel record predates the protocol fee template".into(),
        );
    }
    ComponentAddress::from_str(trimmed).map_err(|e| format!("fee_config: {e}"))
}

pub fn parse_treasury(value: &str) -> Result<RistrettoPublicKeyBytes, String> {
    RistrettoPublicKeyBytes::from_hex(value.trim()).map_err(|e| format!("treasury: {e}"))
}

/// Reads the current treasury key from a fee config component's substate: the CBOR `SubstateValue` that the indexer
/// serves base64-encoded at `state.up.substate`. `GhostlineFeeConfig` keeps `treasury` at field 0 and `owner` at
/// field 1, the same layout the channel template decodes at settlement.
pub fn treasury_from_config_substate(cbor: &[u8]) -> Result<RistrettoPublicKeyBytes, String> {
    let value: SubstateValue =
        tari_bor::decode(cbor).map_err(|e| format!("fee config substate: {e}"))?;
    let component = value
        .into_component()
        .ok_or("fee config substate is not a component")?;
    let (treasury, _owner): (RistrettoPublicKeyBytes, RistrettoPublicKeyBytes) =
        tari_bor::from_value(component.state())
            .map_err(|e| format!("fee config state is not (treasury, owner): {e}"))?;
    Ok(treasury)
}

/// The treasury from configuration only. Prefer the on-chain value; see `ghostline-net` `fee_config_treasury`.
pub fn treasury_from_env() -> Result<RistrettoPublicKeyBytes, String> {
    let value = std::env::var(TREASURY_ENV)
        .map_err(|_| format!("set {TREASURY_ENV} to the current treasury key"))?;
    parse_treasury(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_treasury_from_the_mainnet_fee_config() {
        // `GET /substates/component_37df6af5…0bb5` from the mainnet indexer on 13 Nov 2026 (public chain data).
        let hex = include_str!("fixtures/mainnet-fee-config-substate.hex").trim();
        let bytes = tari_utilities::hex::from_hex(hex).unwrap();
        let treasury = treasury_from_config_substate(&bytes).unwrap();
        assert_eq!(
            treasury,
            parse_treasury("728870aee9b9fb66e69fcf45510440e54b59387a7efe3816713e054014d3b230")
                .unwrap()
        );
    }

    #[test]
    fn rejects_bytes_that_are_not_a_component() {
        assert!(treasury_from_config_substate(&[0x00]).is_err());
    }

    #[test]
    fn fee_config_accepts_prefixed_and_raw_hex() {
        let hex = "ab".repeat(32);
        let raw = parse_fee_config(&hex).unwrap();
        assert_eq!(parse_fee_config(&format!("component_{hex}")).unwrap(), raw);
    }

    #[test]
    fn empty_fee_config_names_the_legacy_record() {
        assert!(parse_fee_config("").unwrap_err().contains("predates"));
    }
}
