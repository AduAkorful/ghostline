// SPDX-License-Identifier: MIT

use std::io::{self, Read};

use tari_crypto::ristretto::RistrettoSecretKey;

use crate::seal;

pub const PAYER_SECRET_ENV: &str = "GHOSTLINE_PAYER_SECRET";
/// Claim key for `unblind`. Wallet-built channel outputs are encrypted to the wallet view key, not the Ghostline payer key.
pub const CLAIM_SECRET_ENV: &str = "GHOSTLINE_CLAIM_SECRET";
pub const SEAL_SECRET_ENV: &str = "GHOSTLINE_SEAL_SECRET";
pub const INPUT_MASK_ENV: &str = "GHOSTLINE_INPUT_MASK";
pub const DEPOSIT_MASK_ENV: &str = "GHOSTLINE_DEPOSIT_MASK";
pub const FEE_SECRET_ENV: &str = "GHOSTLINE_FEE_SECRET";
pub const SESSION_ENV: &str = "GHOSTLINE_SESSION";
pub const VENDOR_ENV: &str = "GHOSTLINE_POLICY_VENDOR_HEX";
pub const UNSAFE_ENV: &str = "GHOSTLINE_POLICY_UNSAFE";

/// Claim secret for unblinding: `--secret-hex`, then `GHOSTLINE_CLAIM_SECRET`, then `GHOSTLINE_PAYER_SECRET`.
pub fn load_claim_secret(flag: Option<&str>) -> Result<RistrettoSecretKey, String> {
    if flag.is_some() {
        return load_secret(flag, CLAIM_SECRET_ENV, "claim secret");
    }
    match std::env::var(CLAIM_SECRET_ENV) {
        Ok(v) if !v.trim().is_empty() => seal::parse_secret("claim secret", v.trim()),
        _ => load_secret(None, PAYER_SECRET_ENV, "claim secret"),
    }
}

/// CLI `--secret-hex` / env / stdin (`-`). Prints a warning when the value came from argv.
pub fn load_secret(
    flag: Option<&str>,
    env_name: &str,
    label: &str,
) -> Result<RistrettoSecretKey, String> {
    let hex = load_hex(flag, env_name, label)?;
    seal::parse_secret(label, &hex)
}

pub fn load_hex(flag: Option<&str>, env_name: &str, label: &str) -> Result<String, String> {
    if let Some(value) = flag {
        if value == "-" {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("{label} stdin: {e}"))?;
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                return Err(format!("{label}: stdin was empty"));
            }
            return Ok(trimmed.to_string());
        }
        eprintln!(
            "warning: {label} passed on the command line; it will appear in process lists. Prefer {env_name} or --{label}=- with stdin."
        );
        return Ok(value.to_string());
    }
    match std::env::var(env_name) {
        Ok(v) if !v.trim().is_empty() => Ok(v.trim().to_string()),
        _ => Err(format!(
            "missing {label}: set {env_name}, pass --flag -, or (leaky) --flag <hex>"
        )),
    }
}
