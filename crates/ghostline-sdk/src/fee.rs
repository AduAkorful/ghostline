// SPDX-License-Identifier: MIT

//! Inputs for the protocol fee: which fee config a channel names and which treasury key a settlement pays.

use std::str::FromStr;

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

// TODO: read the treasury from the fee config component through the indexer instead of trusting configuration. The
// indexer's JSON shape for component state has not been checked against a live 0.53 indexer.
pub fn treasury_from_env() -> Result<RistrettoPublicKeyBytes, String> {
    let value = std::env::var(TREASURY_ENV)
        .map_err(|_| format!("set {TREASURY_ENV} to the current treasury key"))?;
    parse_treasury(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

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
