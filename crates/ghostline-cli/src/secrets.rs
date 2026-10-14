// SPDX-License-Identifier: MIT

use std::io::{self, Read};

use tari_crypto::ristretto::RistrettoSecretKey;

use crate::seal;

pub const PAYER_SECRET_ENV: &str = "GHOSTLINE_PAYER_SECRET";
pub const SEAL_SECRET_ENV: &str = "GHOSTLINE_SEAL_SECRET";
pub const INPUT_MASK_ENV: &str = "GHOSTLINE_INPUT_MASK";
pub const DEPOSIT_MASK_ENV: &str = "GHOSTLINE_DEPOSIT_MASK";
pub const SESSION_ENV: &str = "GHOSTLINE_SESSION";
pub const VENDOR_ENV: &str = "GHOSTLINE_POLICY_VENDOR_HEX";
pub const UNSAFE_ENV: &str = "GHOSTLINE_POLICY_UNSAFE";

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
