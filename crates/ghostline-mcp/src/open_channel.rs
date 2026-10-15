// SPDX-License-Identifier: MIT

//! Wallet dry-run for a channel file that does not yet hold a deposit mask.
//! A live deposit stays on `ghostline channel open`.

use ghostline_core::{NONCE_LEN, from_hex};
use ghostline_net::{StealthTransferRequest, UtxoInputSelection, WalletClient};
use ghostline_policy::env_flag;
use ghostline_sdk::{Channel, ChannelSpec};
use serde::{Deserialize, Serialize};
use tari_template_lib::types::{
    Hash32, constants::STEALTH_TARI_RESOURCE_ADDRESS, crypto::RistrettoPublicKeyBytes,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenOutcome {
    pub dry_run: bool,
    pub transaction_id: String,
}

#[derive(Deserialize)]
struct OpenRecord {
    network_tag: u8,
    deposit: u64,
    payer_hex: String,
    service_hex: String,
    expiry_epoch: u64,
    nonce_hex: String,
    template_hex: String,
    opener_account: String,
    opener_wallet_url: String,
}

pub async fn open_channel() -> Result<OpenOutcome, String> {
    let path = std::env::var("GHOSTLINE_CHANNEL_FILE").map_err(|_| "set GHOSTLINE_CHANNEL_FILE")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("channel file: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    if value
        .get("deposit_mask_hex")
        .and_then(|v| v.as_str())
        .is_some_and(|mask| !mask.is_empty())
    {
        return Err("channel file already holds a deposit mask; refusing to open again".into());
    }
    if env_flag("GHOSTLINE_OPEN_SUBMIT") {
        return Err(
            "MCP open does not broadcast. Unset GHOSTLINE_OPEN_SUBMIT, or use `ghostline channel open` for a live deposit."
                .into(),
        );
    }
    let record: OpenRecord =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    let nonce_bytes = from_hex(&record.nonce_hex).map_err(|e| format!("nonce: {e}"))?;
    let nonce: [u8; NONCE_LEN] = nonce_bytes
        .try_into()
        .map_err(|_| "nonce must be 32 bytes".to_string())?;
    let channel = Channel::open(ChannelSpec {
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
    .map_err(|e| e.to_string())?;
    let pay_to = serde_json::json!({ "Conditions": channel.leaves_vec() });
    let mut wallet = WalletClient::connect(&record.opener_wallet_url).map_err(|e| e.to_string())?;
    wallet.auth_none_admin().await.map_err(|e| e.to_string())?;
    let destination = wallet
        .default_account_address()
        .await
        .map_err(|e| e.to_string())?;
    let request = StealthTransferRequest::channel_open(
        &record.opener_account,
        destination,
        STEALTH_TARI_RESOURCE_ADDRESS.to_string(),
        record.deposit,
        pay_to,
        "PreferConfidential".parse::<UtxoInputSelection>()?,
        "PreferRevealed".parse::<UtxoInputSelection>()?,
        100_000,
        true,
    );
    let submitted = wallet
        .stealth_transfer(&request)
        .await
        .map_err(|e| e.to_string())?;
    Ok(OpenOutcome {
        dry_run: true,
        transaction_id: submitted.transaction_id.to_string(),
    })
}
