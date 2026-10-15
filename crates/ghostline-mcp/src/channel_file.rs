// SPDX-License-Identifier: MIT

//! Read a channel file from the environment. The deposit mask and fee accounts are not returned.

use std::path::{Path, PathBuf};

use ghostline_net::{IndexerClient, SubstatePresence};
use ghostline_paywall::{ChannelStatus, ChannelStore};
use serde::{Deserialize, Serialize};
use tari_template_lib::types::{
    UtxoAddress, UtxoId, constants::STEALTH_TARI_RESOURCE_ADDRESS, crypto::PedersenCommitmentBytes,
};

#[derive(Debug, Clone, Serialize)]
pub struct ChannelFileView {
    pub file: String,
    #[serde(flatten)]
    pub status: ChannelStatus,
    pub output_presence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChannelCallView {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<u64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}

#[derive(Deserialize)]
struct ChannelRecord {
    expiry_epoch: u64,
    #[serde(default)]
    indexer_url: String,
    #[serde(default)]
    ledger: Option<PathBuf>,
    #[serde(default)]
    utxo_json: Option<PathBuf>,
}

#[derive(Deserialize)]
struct UtxoCommitment {
    commitment: String,
}

pub async fn channel_file_status() -> Result<ChannelFileView, String> {
    let path = std::env::var("GHOSTLINE_CHANNEL_FILE").map_err(|_| "set GHOSTLINE_CHANNEL_FILE")?;
    let path = PathBuf::from(path);
    let name = file_name(&path);
    read_view(&path, &name).await
}

pub async fn channel_file_list() -> Result<Vec<ChannelFileView>, String> {
    let dir = std::env::var("GHOSTLINE_CHANNEL_DIR").map_err(|_| "set GHOSTLINE_CHANNEL_DIR")?;
    let mut views = Vec::new();
    for path in channel_files(Path::new(&dir))? {
        let name = file_name(&path);
        views.push(read_view(&path, &name).await?);
    }
    Ok(views)
}

pub fn channel_file_calls() -> Result<Vec<ChannelCallView>, String> {
    let dir = std::env::var("GHOSTLINE_CHANNEL_DIR").map_err(|_| "set GHOSTLINE_CHANNEL_DIR")?;
    let mut rows = Vec::new();
    for path in channel_files(Path::new(&dir))? {
        let name = file_name(&path);
        let record = load_record(&path)?;
        let Some(ledger) = record.ledger else {
            continue;
        };
        let store = ChannelStore::open(&ledger).map_err(|e| format!("{name}: {e}"))?;
        if store.calls().is_empty() {
            if store.accepted() > 0 {
                rows.push(ChannelCallView {
                    file: name,
                    price: None,
                    accepted: Some(store.accepted()),
                    note: "no call log; accepted total predates it".into(),
                });
            }
            continue;
        }
        for call in store.calls() {
            rows.push(ChannelCallView {
                file: name.clone(),
                price: Some(call.price),
                accepted: Some(call.accepted),
                note: String::new(),
            });
        }
    }
    Ok(rows)
}

fn channel_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("channel dir: {e}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut files = Vec::new();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if serde_json::from_str::<ChannelRecord>(&text).is_ok() {
            files.push(path);
        }
    }
    Ok(files)
}

async fn read_view(path: &Path, name: &str) -> Result<ChannelFileView, String> {
    let record = load_record(path)?;
    let ledger = record
        .ledger
        .clone()
        .ok_or_else(|| format!("{name}: channel file has no ledger path"))?;
    let store = ChannelStore::open(&ledger).map_err(|e| format!("{name}: {e}"))?;
    let mut note = None;
    let current_epoch = if record.indexer_url.is_empty() {
        None
    } else {
        match fetch_epoch(&record.indexer_url).await {
            Ok(epoch) => Some(epoch),
            Err(e) => {
                note = Some(format!("indexer: {e}"));
                None
            }
        }
    };
    let status = ChannelStatus::from_store(&store, Some(record.expiry_epoch), current_epoch)
        .map_err(|e| e.to_string())?;
    let (output_presence, output_version) = match &record.utxo_json {
        None => ("not_checked".to_string(), None),
        Some(utxo) => match output_presence(&record.indexer_url, utxo).await {
            Ok(found) => found,
            Err(e) => {
                note = Some(match note {
                    Some(prev) => format!("{prev}; output: {e}"),
                    None => format!("output: {e}"),
                });
                ("not_checked".to_string(), None)
            }
        },
    };
    Ok(ChannelFileView {
        file: name.to_string(),
        status,
        output_presence,
        output_version,
        note,
    })
}

fn load_record(path: &Path) -> Result<ChannelRecord, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("channel file: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))
}

async fn fetch_epoch(indexer_url: &str) -> Result<u64, String> {
    Ok(IndexerClient::connect(indexer_url)
        .map_err(|e| e.to_string())?
        .epoch_stats()
        .await
        .map_err(|e| e.to_string())?
        .current_epoch
        .as_u64())
}

async fn output_presence(
    indexer_url: &str,
    utxo_json: &Path,
) -> Result<(String, Option<u32>), String> {
    if indexer_url.is_empty() {
        return Err("commitment lookup needs indexer_url".into());
    }
    let text = std::fs::read_to_string(utxo_json).map_err(|e| format!("utxo-json: {e}"))?;
    let parsed: UtxoCommitment =
        serde_json::from_str(&text).map_err(|e| format!("utxo-json: {e}"))?;
    let commitment = PedersenCommitmentBytes::from_hex(&parsed.commitment)
        .map_err(|e| format!("commitment: {e}"))?;
    let id = UtxoAddress::new(STEALTH_TARI_RESOURCE_ADDRESS, UtxoId::from(commitment)).to_string();
    let presence = IndexerClient::connect(indexer_url)
        .map_err(|e| e.to_string())?
        .substate_presence(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(match presence {
        SubstatePresence::Up { version } => ("up".into(), Some(version)),
        SubstatePresence::Down { version } => ("down".into(), Some(version)),
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("channel.json")
        .to_string()
}
