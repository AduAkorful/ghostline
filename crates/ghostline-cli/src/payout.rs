// SPDX-License-Identifier: MIT

//! Service payout destinations: the one-time key a service quotes, and the secret nonce it needs at settlement to
//! address the payout so its own walletd finds it.

use std::path::Path;
use std::str::FromStr;

use ghostline_core::{from_hex, to_hex};
use ghostline_sdk::WalletDestination;
use ootle_network::Network;
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;
use tari_utilities::ByteArray;

const FORMAT: &str = "ghostline.payout-destination.v1";

#[derive(Serialize, Deserialize)]
struct PayoutDestinationFile {
    format: String,
    network: String,
    account_hex: String,
    view_hex: String,
    /// Secret: with the view key it opens this payout's value and mask.
    nonce_hex: String,
    /// The key to quote as `service_output_key`. Derived from the fields above; stored for review.
    owner_key_hex: String,
}

pub fn write(path: &Path, destination: &WalletDestination) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "refusing to overwrite payout destination: {}",
            path.display()
        ));
    }
    let file = PayoutDestinationFile {
        format: FORMAT.into(),
        network: destination.network().to_string(),
        account_hex: to_hex(destination.account_key().as_bytes()),
        view_hex: to_hex(destination.view_key().as_bytes()),
        nonce_hex: to_hex(destination.nonce().as_bytes()),
        owner_key_hex: to_hex(destination.owner_key().as_bytes()),
    };
    let json = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
    ghostline_core::write_private_file(path, &json).map_err(|e| format!("payout destination: {e}"))
}

pub fn read(path: &Path) -> Result<WalletDestination, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("payout destination {}: {e}", path.display()))?;
    let file: PayoutDestinationFile = serde_json::from_slice(&bytes)
        .map_err(|e| format!("payout destination {}: {e}", path.display()))?;
    if file.format != FORMAT {
        return Err(format!(
            "payout destination {}: unknown format {}",
            path.display(),
            file.format
        ));
    }
    let network = Network::from_str(&file.network)
        .map_err(|_| format!("payout destination: unknown network {}", file.network))?;
    let nonce = RistrettoSecretKey::from_canonical_bytes(&bytes32(&file.nonce_hex, "nonce")?)
        .map_err(|_| "payout destination: nonce is not a canonical scalar".to_string())?;
    let destination = WalletDestination::new(
        network,
        public_key(&file.account_hex, "account")?,
        public_key(&file.view_hex, "view")?,
        nonce,
    )
    .map_err(|e| e.to_string())?;
    if to_hex(destination.owner_key().as_bytes()) != file.owner_key_hex {
        return Err(
            "payout destination: stored owner key does not match its address and nonce".into(),
        );
    }
    Ok(destination)
}

fn bytes32(hex: &str, label: &str) -> Result<[u8; 32], String> {
    from_hex(hex.trim())
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| format!("payout destination: {label} must be 32 bytes of hex"))
}

fn public_key(hex: &str, label: &str) -> Result<RistrettoPublicKeyBytes, String> {
    RistrettoPublicKeyBytes::from_bytes(&bytes32(hex, label)?)
        .map_err(|_| format!("payout destination: {label} key is malformed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;

    fn destination() -> WalletDestination {
        WalletDestination::new(
            Network::MainNet,
            create_key_pair_from_seed(41).1.to_byte_type(),
            create_key_pair_from_seed(42).1.to_byte_type(),
            create_key_pair_from_seed(43).0,
        )
        .unwrap()
    }

    #[test]
    fn round_trips_and_refuses_overwrite() {
        let dir = std::env::temp_dir().join(format!("ghostline-payout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("payout.json");
        let original = destination();
        write(&path, &original).unwrap();
        assert_eq!(read(&path).unwrap(), original);
        assert!(
            write(&path, &original)
                .unwrap_err()
                .contains("refusing to overwrite")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_a_tampered_owner_key() {
        let dir = std::env::temp_dir().join(format!("ghostline-payout-t-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("payout.json");
        write(&path, &destination()).unwrap();
        let mut file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        file["owner_key_hex"] = serde_json::json!("00".repeat(32));
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        assert!(read(&path).unwrap_err().contains("does not match"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
