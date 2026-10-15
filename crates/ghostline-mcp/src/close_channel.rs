// SPDX-License-Identifier: MIT

//! Close one channel file: settle while the window is open, refund at or after expiry.
//! Broadcasts only when `GHOSTLINE_CLOSE_SUBMIT=1`. `GHOSTLINE_CLOSE_DRY_RUN=1` asks the
//! wallet to price the transaction and does not broadcast.

use std::path::PathBuf;
use std::str::FromStr;

use ghostline_core::{SignedVoucher, from_hex, to_hex};
use ghostline_net::{
    IndexerClient, SubstatePresence, WalletClient, WalletSubmitRequest, execution_accepted,
    settle_fee_signers,
};
use ghostline_paywall::{
    ChannelStore, RefundDecision, SettleDecision, decide_refund, decide_settle,
};
use ghostline_policy::env_flag;
use ghostline_sdk::{
    Channel, ChannelSpec, MaskAndValue, OutputScan, refund, settle,
    unsigned_stealth_transfer_with_fee,
};
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{
    ComponentAddress, Hash32,
    constants::STEALTH_TARI_RESOURCE_ADDRESS,
    crypto::{PedersenCommitmentBytes, RistrettoPublicKeyBytes},
};
use tari_utilities::ByteArray;

/// Which close the channel is in. A down output wins over both windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseKind {
    OutputSpent,
    Refund(RefundDecision),
    Settle(SettleDecision),
}

pub fn choose_close(
    current_epoch: u64,
    expiry_epoch: u64,
    output_down: bool,
    accepted: u64,
    has_voucher: bool,
    margin_epochs: u64,
    force: bool,
) -> CloseKind {
    if output_down {
        return CloseKind::OutputSpent;
    }
    if current_epoch >= expiry_epoch {
        return CloseKind::Refund(decide_refund(current_epoch, expiry_epoch));
    }
    CloseKind::Settle(decide_settle(
        accepted,
        has_voucher,
        current_epoch,
        expiry_epoch,
        margin_epochs,
        force,
    ))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CloseOutcome {
    pub action: String,
    pub current_epoch: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epochs_until_expiry: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epochs_overdue: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_presence: Option<String>,
    pub submitted: bool,
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
}

#[derive(Deserialize)]
struct CloseRecord {
    network_tag: u8,
    deposit: u64,
    payer_hex: String,
    service_hex: String,
    expiry_epoch: u64,
    nonce_hex: String,
    template_hex: String,
    indexer_url: String,
    ootle_network: String,
    max_epoch: u64,
    opener_wallet_url: String,
    #[serde(default)]
    fee_account: Option<String>,
    #[serde(default)]
    fee_account_name: Option<String>,
    #[serde(default)]
    fee_wallet_url: Option<String>,
    #[serde(default)]
    payer_fee_account: Option<String>,
    #[serde(default)]
    payer_fee_account_name: Option<String>,
    #[serde(default)]
    payer_wallet_url: Option<String>,
    #[serde(default)]
    ledger: Option<PathBuf>,
    #[serde(default)]
    utxo_json: Option<PathBuf>,
    #[serde(default)]
    deposit_mask_hex: Option<String>,
}

pub async fn close_channel() -> Result<CloseOutcome, String> {
    let path = std::env::var("GHOSTLINE_CHANNEL_FILE").map_err(|_| "set GHOSTLINE_CHANNEL_FILE")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("channel file: {e}"))?;
    let record: CloseRecord =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    let current_epoch = fetch_epoch(&record.indexer_url).await?;
    let output_down = match &record.utxo_json {
        Some(utxo) => output_is_down(&record.indexer_url, utxo).await?,
        None => false,
    };
    let (accepted, has_voucher) = ledger_totals(record.ledger.as_ref())?;
    let margin = env_u64("GHOSTLINE_SETTLE_MARGIN", 10);
    let force = env_flag("GHOSTLINE_SETTLE_FORCE");
    let kind = choose_close(
        current_epoch,
        record.expiry_epoch,
        output_down,
        accepted,
        has_voucher,
        margin,
        force,
    );
    let mut outcome = outcome_for(current_epoch, output_down, &kind);
    let broadcast = env_flag("GHOSTLINE_CLOSE_SUBMIT");
    let dry_run = env_flag("GHOSTLINE_CLOSE_DRY_RUN");
    if !should_submit(&kind) || (!broadcast && !dry_run) {
        return Ok(outcome);
    }
    if broadcast && dry_run {
        return Err("set only one of GHOSTLINE_CLOSE_SUBMIT and GHOSTLINE_CLOSE_DRY_RUN".into());
    }
    let unsigned = match &kind {
        CloseKind::Settle(SettleDecision::Due { .. }) => build_settle(
            &record,
            record.ledger.as_ref().ok_or("channel file has no ledger")?,
        )?,
        CloseKind::Refund(RefundDecision::Due { .. }) => build_refund(&record)?,
        _ => return Ok(outcome),
    };
    let (wallet_url, fee_name) = match &kind {
        CloseKind::Settle(_) => (
            record
                .fee_wallet_url
                .clone()
                .unwrap_or_else(|| record.opener_wallet_url.clone()),
            record
                .fee_account_name
                .clone()
                .ok_or("submit requires fee_account_name on the channel file")?,
        ),
        CloseKind::Refund(_) => (
            record
                .payer_wallet_url
                .clone()
                .unwrap_or_else(|| record.opener_wallet_url.clone()),
            record
                .payer_fee_account_name
                .clone()
                .ok_or("submit requires payer_fee_account_name on the channel file")?,
        ),
        CloseKind::OutputSpent => return Ok(outcome),
    };
    let seal_path = std::env::var("GHOSTLINE_SEAL_KEY_ID_FILE")
        .map_err(|_| "set GHOSTLINE_SEAL_KEY_ID_FILE")?;
    let seal_signer = read_key_id(std::path::Path::new(&seal_path))?;
    let mut wallet = WalletClient::connect(&wallet_url).map_err(|e| e.to_string())?;
    wallet.auth_none_admin().await.map_err(|e| e.to_string())?;
    let got = wallet
        .account_get(&fee_name)
        .await
        .map_err(|e| e.to_string())?;
    let from_account = got
        .account
        .fee_signer_key_id()
        .map_err(|e| e.to_string())?
        .clone();
    let request = WalletSubmitRequest::new(
        unsigned,
        seal_signer,
        settle_fee_signers(Vec::new(), Some(from_account)).map_err(|e| e.to_string())?,
    );
    let timeout = env_u64("GHOSTLINE_CLOSE_TIMEOUT_SECS", 120);
    if dry_run {
        let dry = wallet
            .submit_dry_run(&request)
            .await
            .map_err(|e| e.to_string())?;
        if !execution_accepted(&dry.result) {
            return Err(format!("dry-run did not Accept: {}", dry.result));
        }
        outcome.dry_run = true;
        outcome.transaction_id = Some(dry.transaction_id.to_string());
        return Ok(outcome);
    }
    let submitted = wallet.submit(&request).await.map_err(|e| e.to_string())?;
    let waited = wallet
        .wait_result(submitted.transaction_id, Some(timeout))
        .await
        .map_err(|e| e.to_string())?;
    let result = waited.get("result").unwrap_or(&waited);
    if !(execution_accepted(result) || execution_accepted(&waited)) {
        return Err(format!("wait did not Accept: {waited}"));
    }
    outcome.submitted = true;
    outcome.transaction_id = Some(submitted.transaction_id.to_string());
    Ok(outcome)
}

fn should_submit(kind: &CloseKind) -> bool {
    matches!(
        kind,
        CloseKind::Settle(SettleDecision::Due { .. })
            | CloseKind::Refund(RefundDecision::Due { .. })
    )
}

fn outcome_for(current_epoch: u64, output_down: bool, kind: &CloseKind) -> CloseOutcome {
    let mut outcome = CloseOutcome {
        action: String::new(),
        current_epoch,
        epochs_until_expiry: None,
        epochs_overdue: None,
        amount: None,
        output_presence: if output_down {
            Some("down".into())
        } else {
            None
        },
        submitted: false,
        dry_run: false,
        transaction_id: None,
    };
    match kind {
        CloseKind::OutputSpent => outcome.action = "output_spent".into(),
        CloseKind::Refund(RefundDecision::Wait {
            epochs_until_expiry,
        }) => {
            outcome.action = "wait".into();
            outcome.epochs_until_expiry = Some(*epochs_until_expiry);
        }
        CloseKind::Refund(RefundDecision::Due { epochs_overdue }) => {
            outcome.action = "refund".into();
            outcome.epochs_overdue = Some(*epochs_overdue);
        }
        CloseKind::Settle(SettleDecision::Due {
            epochs_until_expiry,
            amount,
        }) => {
            outcome.action = "settle".into();
            outcome.epochs_until_expiry = Some(*epochs_until_expiry);
            outcome.amount = Some(*amount);
        }
        CloseKind::Settle(SettleDecision::Wait {
            epochs_until_expiry,
        }) => {
            outcome.action = "wait".into();
            outcome.epochs_until_expiry = Some(*epochs_until_expiry);
        }
        CloseKind::Settle(SettleDecision::Expired { epochs_overdue }) => {
            outcome.action = "expired".into();
            outcome.epochs_overdue = Some(*epochs_overdue);
        }
        CloseKind::Settle(SettleDecision::NothingOwed) => outcome.action = "nothing_owed".into(),
        CloseKind::Settle(SettleDecision::MissingVoucher { .. }) => {
            outcome.action = "missing_voucher".into();
        }
    }
    outcome
}

fn ledger_totals(ledger: Option<&PathBuf>) -> Result<(u64, bool), String> {
    let Some(path) = ledger else {
        return Ok((0, false));
    };
    let store = ChannelStore::open(path).map_err(|e| e.to_string())?;
    Ok((store.accepted(), store.latest_voucher_hex().is_some()))
}

fn build_settle(
    record: &CloseRecord,
    ledger: &std::path::Path,
) -> Result<ghostline_sdk::UnsignedTransaction, String> {
    let store = ChannelStore::open(ledger).map_err(|e| e.to_string())?;
    let voucher_hex = store
        .latest_voucher_hex()
        .ok_or("due settle is missing a stored voucher")?;
    let bytes = from_hex(voucher_hex).map_err(|_| "stored voucher: expected even-length hex")?;
    let voucher = SignedVoucher::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let channel = channel_from(record)?;
    let deposit = funded(record, &channel)?;
    let resource = STEALTH_TARI_RESOURCE_ADDRESS;
    let scan = scan(record)?;
    let statement = settle(&channel, deposit, &voucher, channel.spec().service, scan)
        .map_err(|e| e.to_string())?;
    let fee = parse_component(
        "fee-account",
        record
            .fee_account
            .as_deref()
            .ok_or("due settle requires fee_account")?,
    )?;
    Ok(unsigned_stealth_transfer_with_fee(
        scan.network,
        resource,
        statement,
        record.max_epoch,
        Some((fee, 100_000)),
    ))
}

fn build_refund(record: &CloseRecord) -> Result<ghostline_sdk::UnsignedTransaction, String> {
    let channel = channel_from(record)?;
    let deposit = funded(record, &channel)?;
    let resource = STEALTH_TARI_RESOURCE_ADDRESS;
    let scan = scan(record)?;
    let statement =
        refund(&channel, deposit, channel.spec().payer, scan).map_err(|e| e.to_string())?;
    let fee = parse_component(
        "payer-fee-account",
        record
            .payer_fee_account
            .as_deref()
            .ok_or("due refund requires payer_fee_account")?,
    )?;
    Ok(unsigned_stealth_transfer_with_fee(
        scan.network,
        resource,
        statement,
        record.max_epoch,
        Some((fee, 100_000)),
    ))
}

fn funded(record: &CloseRecord, channel: &Channel) -> Result<MaskAndValue, String> {
    let mask_hex = record
        .deposit_mask_hex
        .as_deref()
        .filter(|mask| !mask.is_empty())
        .ok_or("channel file has no deposit mask")?;
    let bytes = from_hex(mask_hex).map_err(|_| "deposit mask: expected even-length hex")?;
    let mask = RistrettoSecretKey::from_canonical_bytes(&bytes)
        .map_err(|e| format!("deposit mask: {e}"))?;
    if to_hex(mask.as_bytes()) != mask_hex {
        return Err("deposit mask: not canonical".into());
    }
    Ok(MaskAndValue {
        value: channel.params().deposit,
        mask,
    })
}

fn channel_from(record: &CloseRecord) -> Result<Channel, String> {
    let nonce_bytes = from_hex(&record.nonce_hex).map_err(|e| format!("nonce: {e}"))?;
    let nonce: [u8; 32] = nonce_bytes
        .try_into()
        .map_err(|_| "nonce must be 32 bytes".to_string())?;
    Channel::open(ChannelSpec {
        network_tag: record.network_tag,
        payer: RistrettoPublicKeyBytes::from_hex(&record.payer_hex)
            .map_err(|e| format!("payer: {e}"))?,
        service: RistrettoPublicKeyBytes::from_hex(&record.service_hex)
            .map_err(|e| format!("service: {e}"))?,
        deposit: record.deposit,
        expiry_epoch: record.expiry_epoch,
        nonce,
        channel_template: Hash32::from_hex(&record.template_hex)
            .map_err(|e| format!("template: {e}"))?,
    })
    .map_err(|e| e.to_string())
}

fn scan(record: &CloseRecord) -> Result<OutputScan, String> {
    match record.ootle_network.as_str() {
        "localnet" => Ok(OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS)),
        "esmeralda" => Ok(OutputScan::esmeralda(STEALTH_TARI_RESOURCE_ADDRESS)),
        other => Err(format!(
            "ootle_network: expected localnet or esmeralda, got {other}"
        )),
    }
}

fn parse_component(label: &str, addr: &str) -> Result<ComponentAddress, String> {
    ComponentAddress::from_str(addr).map_err(|e| format!("{label}: {e}"))
}

fn read_key_id(path: &std::path::Path) -> Result<serde_json::Value, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("seal-key-id-file: {e}"))?;
    serde_json::from_str(raw.trim()).map_err(|e| format!("seal-key-id: {e}"))
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

async fn fetch_epoch(indexer_url: &str) -> Result<u64, String> {
    if indexer_url.is_empty() {
        return Err("channel file has no indexer_url".into());
    }
    Ok(IndexerClient::connect(indexer_url)
        .map_err(|e| e.to_string())?
        .epoch_stats()
        .await
        .map_err(|e| e.to_string())?
        .current_epoch
        .as_u64())
}

async fn output_is_down(indexer_url: &str, utxo_json: &std::path::Path) -> Result<bool, String> {
    if indexer_url.is_empty() {
        return Err("commitment lookup needs indexer_url".into());
    }
    let text = std::fs::read_to_string(utxo_json).map_err(|e| format!("utxo-json: {e}"))?;
    #[derive(Deserialize)]
    struct UtxoCommitment {
        commitment: String,
    }
    let parsed: UtxoCommitment =
        serde_json::from_str(&text).map_err(|e| format!("utxo-json: {e}"))?;
    let commitment = PedersenCommitmentBytes::from_hex(&parsed.commitment)
        .map_err(|e| format!("commitment: {e}"))?;
    let id = tari_template_lib::types::UtxoAddress::new(
        STEALTH_TARI_RESOURCE_ADDRESS,
        tari_template_lib::types::UtxoId::from(commitment),
    )
    .to_string();
    let presence = IndexerClient::connect(indexer_url)
        .map_err(|e| e.to_string())?
        .substate_presence(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(matches!(presence, SubstatePresence::Down { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_spends_only_the_open_window() {
        assert_eq!(
            choose_close(10, 50, true, 20, true, 10, true),
            CloseKind::OutputSpent
        );
        assert!(matches!(
            choose_close(10, 50, false, 0, false, 10, false),
            CloseKind::Settle(SettleDecision::NothingOwed)
        ));
        assert!(matches!(
            choose_close(10, 50, false, 20, true, 10, false),
            CloseKind::Settle(SettleDecision::Wait { .. })
        ));
        assert!(matches!(
            choose_close(40, 50, false, 20, true, 10, false),
            CloseKind::Settle(SettleDecision::Due { amount: 20, .. })
        ));
        assert!(matches!(
            choose_close(40, 50, false, 20, true, 5, true),
            CloseKind::Settle(SettleDecision::Due { amount: 20, .. })
        ));
        assert!(matches!(
            choose_close(50, 50, false, 20, true, 10, true),
            CloseKind::Refund(RefundDecision::Due { epochs_overdue: 0 })
        ));
        assert!(matches!(
            choose_close(49, 50, false, 0, false, 10, false),
            CloseKind::Settle(SettleDecision::NothingOwed)
        ));
    }

    #[test]
    fn close_outcome_has_no_mask_field() {
        let outcome = outcome_for(
            40,
            false,
            &CloseKind::Settle(SettleDecision::Due {
                epochs_until_expiry: 10,
                amount: 20,
            }),
        );
        let text = serde_json::to_string(&outcome).unwrap();
        assert!(text.contains("\"action\":\"settle\""));
        assert!(!text.contains("mask"));
        assert!(!outcome.submitted);
    }
}
