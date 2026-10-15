// SPDX-License-Identifier: MIT

//! Operator record for one channel. After `deposit_mask_hex` is set, treat the file as sensitive.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelFile {
    pub network_tag: u8,
    pub deposit: u64,
    pub payer_hex: String,
    pub service_hex: String,
    pub expiry_epoch: u64,
    pub nonce_hex: String,
    pub template_hex: String,
    pub indexer_url: String,
    pub ootle_network: String,
    pub max_epoch: u64,
    pub opener_account: String,
    pub opener_wallet_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_wallet_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payer_fee_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payer_fee_account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payer_wallet_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ledger: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utxo_json: Option<PathBuf>,
    /// View-key mask of the channel output. Not a wallet seed, but enough to build settle and refund.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deposit_mask_hex: Option<String>,
}

impl ChannelFile {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("channel file: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("channel file: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| format!("channel file: {e}"))?;
        }
        fs::write(path, json).map_err(|e| format!("channel file: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_omits_empty_mask() {
        let dir = std::env::temp_dir().join(format!("ghostline-ch-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("channel.json");
        let file = ChannelFile {
            network_tag: 1,
            deposit: 100,
            payer_hex: "aa".into(),
            service_hex: "bb".into(),
            expiry_epoch: 50,
            nonce_hex: "cc".into(),
            template_hex: "dd".into(),
            indexer_url: "http://127.0.0.1:1/".into(),
            ootle_network: "esmeralda".into(),
            max_epoch: 10,
            opener_account: "ghostline".into(),
            opener_wallet_url: "http://127.0.0.1:5100".into(),
            fee_account: None,
            fee_account_name: None,
            fee_wallet_url: None,
            payer_fee_account: None,
            payer_fee_account_name: None,
            payer_wallet_url: None,
            ledger: None,
            utxo_json: None,
            deposit_mask_hex: None,
        };
        file.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("deposit_mask"));
        assert_eq!(ChannelFile::load(&path).unwrap(), file);
    }
}
