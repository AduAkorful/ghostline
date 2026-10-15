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

/// One directory entry for `channel list`. Never carries the deposit mask or key material.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChannelListRow {
    pub file: String,
    pub channel_id: String,
    pub deposit: u64,
    pub expiry_epoch: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cumulative: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remainder_at_accepted: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_settlement: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_latest_voucher: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settle_window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epochs_until_expiry: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_epoch: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_presence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ChannelListRow {
    pub fn identity(file_name: &str, channel_id: String, record: &ChannelFile) -> Self {
        Self {
            file: file_name.to_string(),
            channel_id,
            deposit: record.deposit,
            expiry_epoch: record.expiry_epoch,
            accepted: None,
            cumulative: None,
            remainder_at_accepted: None,
            pending_settlement: None,
            has_latest_voucher: None,
            settle_window: None,
            epochs_until_expiry: None,
            current_epoch: None,
            output_presence: None,
            output_version: None,
            note: None,
        }
    }

    fn cell(&self, column: &str) -> String {
        match column {
            "file" => self.file.clone(),
            "deposit" => self.deposit.to_string(),
            "accepted" => opt_u64(self.accepted),
            "remainder_at_accepted" => opt_u64(self.remainder_at_accepted),
            "expiry_epoch" => self.expiry_epoch.to_string(),
            "settle_window" => self.settle_window.clone().unwrap_or_default(),
            "pending_settlement" => opt_bool(self.pending_settlement),
            "output_presence" => self.output_presence.clone().unwrap_or_default(),
            "output_version" => self
                .output_version
                .map(|v| v.to_string())
                .unwrap_or_default(),
            "note" => self.note.clone().unwrap_or_default(),
            "channel_id" => self.channel_id.clone(),
            "cumulative" => opt_u64(self.cumulative),
            "has_latest_voucher" => opt_bool(self.has_latest_voucher),
            "epochs_until_expiry" => self
                .epochs_until_expiry
                .map(|v| v.to_string())
                .unwrap_or_default(),
            "current_epoch" => opt_u64(self.current_epoch),
            _ => String::new(),
        }
    }
}

const REPORT_DEFAULT: &[&str] = &[
    "file",
    "deposit",
    "accepted",
    "remainder_at_accepted",
    "expiry_epoch",
    "settle_window",
    "pending_settlement",
    "output_presence",
    "output_version",
    "note",
];

const REPORT_DISCLOSE: &[&str] = &[
    "channel_id",
    "cumulative",
    "has_latest_voucher",
    "epochs_until_expiry",
    "current_epoch",
];

/// Columns for an accounting export. Defaults omit channel id and keys. `--disclose` adds the named extras.
pub fn report_columns(disclose: &[String]) -> Result<Vec<String>, String> {
    let mut columns: Vec<String> = REPORT_DEFAULT.iter().map(|c| (*c).to_string()).collect();
    for name in disclose {
        if REPORT_DISCLOSE.contains(&name.as_str()) {
            if !columns.iter().any(|c| c == name) {
                columns.push(name.clone());
            }
            continue;
        }
        if REPORT_DEFAULT.contains(&name.as_str()) {
            continue;
        }
        return Err(format!(
            "disclose: {name} is not a report column (allowed: {})",
            REPORT_DISCLOSE.join(", ")
        ));
    }
    Ok(columns)
}

pub fn report_csv(rows: &[ChannelListRow], columns: &[String]) -> String {
    let mut out = columns.join(",");
    out.push('\n');
    for row in rows {
        let cells: Vec<String> = columns
            .iter()
            .map(|column| csv_cell(&row.cell(column)))
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

pub fn report_json(rows: &[ChannelListRow], columns: &[String]) -> serde_json::Value {
    serde_json::Value::Array(
        rows.iter()
            .map(|row| {
                let mut object = serde_json::Map::new();
                for column in columns {
                    object.insert(column.clone(), json_cell(row, column));
                }
                serde_json::Value::Object(object)
            })
            .collect(),
    )
}

fn json_cell(row: &ChannelListRow, column: &str) -> serde_json::Value {
    match column {
        "deposit" => serde_json::json!(row.deposit),
        "expiry_epoch" => serde_json::json!(row.expiry_epoch),
        "accepted" => opt_u64_json(row.accepted),
        "remainder_at_accepted" => opt_u64_json(row.remainder_at_accepted),
        "cumulative" => opt_u64_json(row.cumulative),
        "current_epoch" => opt_u64_json(row.current_epoch),
        "output_version" => match row.output_version {
            Some(v) => serde_json::json!(v),
            None => serde_json::Value::Null,
        },
        "epochs_until_expiry" => match row.epochs_until_expiry {
            Some(v) => serde_json::json!(v),
            None => serde_json::Value::Null,
        },
        "pending_settlement" => opt_bool_json(row.pending_settlement),
        "has_latest_voucher" => opt_bool_json(row.has_latest_voucher),
        _ => serde_json::Value::String(row.cell(column)),
    }
}

fn opt_u64_json(value: Option<u64>) -> serde_json::Value {
    match value {
        Some(v) => serde_json::json!(v),
        None => serde_json::Value::Null,
    }
}

fn opt_bool_json(value: Option<bool>) -> serde_json::Value {
    match value {
        Some(v) => serde_json::Value::Bool(v),
        None => serde_json::Value::Null,
    }
}

fn opt_u64(value: Option<u64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn opt_bool(value: Option<bool>) -> String {
    match value {
        Some(true) => "true".into(),
        Some(false) => "false".into(),
        None => String::new(),
    }
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
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

    #[test]
    fn list_row_omits_mask_and_keys() {
        let record = ChannelFile {
            network_tag: 1,
            deposit: 100,
            payer_hex: "11".repeat(32),
            service_hex: "22".repeat(32),
            expiry_epoch: 50,
            nonce_hex: "33".repeat(32),
            template_hex: "44".repeat(32),
            indexer_url: "http://127.0.0.1:1/".into(),
            ootle_network: "esmeralda".into(),
            max_epoch: 10,
            opener_account: "ghostline".into(),
            opener_wallet_url: "http://127.0.0.1:5100".into(),
            fee_account: Some("component_secret".into()),
            fee_account_name: None,
            fee_wallet_url: None,
            payer_fee_account: None,
            payer_fee_account_name: None,
            payer_wallet_url: None,
            ledger: None,
            utxo_json: None,
            deposit_mask_hex: Some("55".repeat(32)),
        };
        let row = ChannelListRow::identity("due.file.json", "aa".repeat(32), &record);
        let text = serde_json::to_string(&row).unwrap();
        assert!(!text.contains("deposit_mask"));
        assert!(!text.contains(&record.payer_hex));
        assert!(!text.contains(&record.service_hex));
        assert!(!text.contains(&record.nonce_hex));
        assert!(!text.contains(&record.template_hex));
        assert!(!text.contains("component_secret"));
        assert!(!text.contains(&"55".repeat(32)));
        assert!(text.contains("due.file.json"));
        assert!(text.contains("\"deposit\":100"));
    }

    #[test]
    fn report_default_omits_channel_id_and_refuses_secrets() {
        let mut row = ChannelListRow::identity("due.file.json", "ab".repeat(32), &sample());
        row.accepted = Some(20);
        row.remainder_at_accepted = Some(80);
        row.pending_settlement = Some(true);
        row.settle_window = Some("settle_open".into());
        row.output_presence = Some("down".into());
        row.note = Some("comma, here".into());
        let columns = report_columns(&[]).unwrap();
        let csv = report_csv(&[row], &columns);
        assert!(csv.starts_with("file,deposit,accepted,"));
        assert!(csv.contains("due.file.json,100,20,80,"));
        assert!(csv.contains("\"comma, here\""));
        assert!(!csv.contains(&"ab".repeat(32)));
        assert!(!csv.contains(&sample().payer_hex));
        assert!(
            report_columns(&["channel_id".into()])
                .unwrap()
                .contains(&"channel_id".to_string())
        );
        assert!(report_columns(&["deposit_mask".into()]).is_err());
        assert!(report_columns(&["payer_hex".into()]).is_err());
    }

    fn sample() -> ChannelFile {
        ChannelFile {
            network_tag: 1,
            deposit: 100,
            payer_hex: "11".repeat(32),
            service_hex: "22".repeat(32),
            expiry_epoch: 50,
            nonce_hex: "33".repeat(32),
            template_hex: "44".repeat(32),
            indexer_url: "http://127.0.0.1:1/".into(),
            ootle_network: "esmeralda".into(),
            max_epoch: 10,
            opener_account: "ghostline".into(),
            opener_wallet_url: "http://127.0.0.1:5100".into(),
            fee_account: Some("component_secret".into()),
            fee_account_name: None,
            fee_wallet_url: None,
            payer_fee_account: None,
            payer_fee_account_name: None,
            payer_wallet_url: None,
            ledger: None,
            utxo_json: None,
            deposit_mask_hex: Some("55".repeat(32)),
        }
    }
}
