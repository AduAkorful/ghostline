// SPDX-License-Identifier: MIT

//! Opt-in hosted demo relay. The wallet daemon remains loopback-only; the relay controls it.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use ghostline_core::{from_hex, to_hex};
use ghostline_net::{
    IndexerClient, StealthTransfer, StealthTransferRequest, TransactionId, TransferFeeParams,
    UtxoInputSelection, WalletClient, account_name, execution_accepted, funding_output_spend_gate,
    indexed_receipt_status, indexed_transaction_status,
};
use ghostline_paywall::{ChannelStore, PaymentTerms};
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{
    EncryptedData,
    constants::STEALTH_TARI_RESOURCE_ADDRESS,
    crypto::{PedersenCommitmentBytes, RistrettoPublicKeyBytes, UtxoTag},
    stealth::UnspentOutput,
};
use tari_utilities::ByteArray;
use tokio::sync::Mutex;

use crate::AppState;

const DEFAULT_OPEN_FEE_CAP: u64 = 100_000;
// Five 10 µT calls leave only the mandatory 1 µT fixed-shape change output.
// This avoids accepting a 1,000 µT deposit and stranding almost all of it after settlement.
const DEFAULT_DEPOSIT: u64 = 51;
const FEE_FLOAT_WARNING_THRESHOLD: u128 = 100_000;

#[derive(Clone)]
pub struct RelayRegistry(
    Arc<Mutex<RelayRegistryState>>,
    Arc<Mutex<()>>,
    Option<Arc<PathBuf>>,
);

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayRegistryState {
    channels: HashMap<String, RelayRecord>,
    funding_ids: HashSet<String>,
    #[serde(default)]
    opening_funding: HashSet<String>,
    completed_funding: HashMap<String, OpenResponse>,
    #[serde(default)]
    funding_refunds: HashMap<String, String>,
    #[serde(default)]
    refunding_funding: HashSet<String>,
    sessions: HashMap<String, RelaySession>,
    /// One prepared channel per Sapient funding transaction. A retry resumes it.
    #[serde(default)]
    open_attempts: HashMap<String, OpenAttempt>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelaySession {
    payer_public_key: String,
    refund_address: String,
    created_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayRecord {
    pub channel_file: PathBuf,
    pub service_signer_file: PathBuf,
    pub service_output_signer_file: PathBuf,
    pub refund_signer_file: PathBuf,
    pub refund_public_key: String,
    pub service_public_key: String,
    pub service_output_public_key: String,
    pub refund_address: String,
    pub settlement_tx: Option<String>,
    pub channel_refund_tx: Option<String>,
    pub refund_tx: Option<String>,
    pub expiry_epoch: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenAttempt {
    channel_file: PathBuf,
    ledger_file: PathBuf,
    service_signer_file: PathBuf,
    service_output_signer_file: PathBuf,
    refund_signer_file: PathBuf,
    service_public_key: String,
    service_output_public_key: String,
    refund_public_key: String,
    refund_address: String,
    expiry_epoch: u64,
    payer_root: String,
    nonce: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    pub funding_tx_id: String,
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest {
    pub payer_public_key: String,
    pub refund_address: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub session_id: String,
    pub funding_address: String,
    pub deposit: u64,
    pub price: u64,
    pub network: &'static str,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenResponse {
    pub channel_id: String,
    pub payer_root: String,
    pub nonce: String,
    pub network_tag: u8,
    pub deposit: u64,
    pub expiry_epoch: u64,
    pub service_public_key: String,
    pub service_output_public_key: String,
    pub funding_transaction_id: String,
    pub open_transaction_id: String,
    pub custody: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_receipt: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayStatus {
    pub channel_id: String,
    pub state: &'static str,
    pub settlement_transaction_id: Option<String>,
    pub explorer_url: Option<String>,
    pub expiry_epoch: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_receipt: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryBundle {
    version: u8,
    channel_id: String,
    response: OpenResponse,
    record: RelayRecord,
    channel_file_hex: String,
    service_signer_file_hex: String,
    service_output_signer_file_hex: String,
    refund_signer_file_hex: String,
    ledger_snapshot_hex: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRequest {
    receipt: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRefundRequest {
    funding_tx_id: String,
    session_id: String,
}

#[derive(Deserialize)]
struct ChannelCliFile {
    service_hex: String,
    refund_hex: Option<String>,
    expiry_epoch: u64,
}

#[derive(Deserialize)]
struct WalletKeyOutput {
    public_key_hex: String,
}

#[derive(Deserialize)]
struct WalletBalancesResponse {
    balances: Vec<WalletBalanceEntry>,
}

#[derive(Deserialize)]
struct WalletBalanceEntry {
    resource_address: String,
    /// Tari `Amount` is serialized as a decimal string in walletd's JSON-RPC response.
    balance: String,
}

pub async fn open(
    State(state): State<AppState>,
    Json(request): Json<OpenRequest>,
) -> impl IntoResponse {
    if !demo_mode() {
        return (StatusCode::NOT_FOUND, "demo relay is disabled").into_response();
    }
    match open_inner(&state, request).await {
        Ok(mut response) => {
            match state.mint_recovery_receipt(&response.channel_id).await {
                Ok(receipt) => response.recovery_receipt = Some(receipt),
                Err(error) => {
                    return (StatusCode::SERVICE_UNAVAILABLE, error).into_response();
                }
            }
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error).into_response(),
    }
}

/// Restore a channel after an ephemeral host restart, or refresh its recovery receipt.
pub async fn recover(
    State(state): State<AppState>,
    Json(request): Json<RecoveryRequest>,
) -> impl IntoResponse {
    if !demo_mode() {
        return (StatusCode::NOT_FOUND, "demo relay is disabled").into_response();
    }
    let response = match restore_from_receipt(&state, &request.receipt).await {
        Ok(response) => response,
        Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
    };
    let receipt = match state.mint_recovery_receipt(&response.channel_id).await {
        Ok(receipt) => receipt,
        Err(error) => return (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "channelId": response.channel_id,
            "state": "recovered",
            "recoveryReceipt": receipt,
        })),
    )
        .into_response()
}

/// Return a finalized Sapient deposit when channel creation failed.
/// The session id is a bearer capability; the destination remains the address bound at session creation.
pub async fn refund_funding(
    State(state): State<AppState>,
    Json(request): Json<FundingRefundRequest>,
) -> impl IntoResponse {
    if !demo_mode() {
        return (StatusCode::NOT_FOUND, "demo relay is disabled").into_response();
    }
    match refund_funding_inner(&state, request).await {
        Ok((transaction_id, status)) => {
            let body = serde_json::json!({
                "state": if status == "Finalized" { "refunded" } else { "pending" },
                "deposit": RelayConfig::load().map(|config| config.deposit).unwrap_or(DEFAULT_DEPOSIT),
                "refundTransactionId": transaction_id,
                "explorerUrl": format!("https://explorer.tari.mw/tx/{transaction_id}"),
            });
            let code = if status == "Finalized" {
                StatusCode::OK
            } else {
                StatusCode::ACCEPTED
            };
            (code, Json(body)).into_response()
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error).into_response(),
    }
}

async fn refund_funding_inner(
    state: &AppState,
    request: FundingRefundRequest,
) -> Result<(String, &'static str), String> {
    // Serialize with channel opens so one funding transaction cannot be both opened and refunded.
    let _guard = state.relay_registry.1.clone().lock_owned().await;
    let funding_tx = TransactionId::from_hex(request.funding_tx_id.trim())
        .map_err(|error| format!("invalid funding transaction id: {error}"))?;
    let funding_id = funding_tx.to_string();
    if state
        .relay_registry
        .completed_open(&funding_id)
        .await
        .is_some()
    {
        return Err("this funding transaction already opened a channel".into());
    }
    if state.relay_registry.is_opening(&funding_id).await {
        return Err("this funding transaction is already being used to open a channel; do not request a refund".into());
    }
    if let Some(refund_tx) = state.relay_registry.funding_refund(&funding_id).await {
        let id = TransactionId::from_hex(&refund_tx).map_err(|error| error.to_string())?;
        let status = transaction_status(&RelayConfig::load()?, id).await?;
        if status == "Finalized" {
            state
                .relay_registry
                .remove_session(&request.session_id)
                .await?;
        }
        return Ok((refund_tx, status));
    }

    if state.relay_registry.is_refunding_funding(&funding_id).await {
        return Err("a private refund was already submitted for this deposit; its transaction id is being recovered, so no second refund will be sent".into());
    }

    let config = RelayConfig::load()?;
    let (session, received) =
        load_funding_session(state, &config, funding_tx, &request.session_id).await?;
    if received.amount != config.deposit {
        return Err("the finalized transaction does not contain the exact demo deposit; the relay will not move its funds".into());
    }
    let wallet = connect_wallet(&config).await?;
    let transfer = StealthTransferRequest {
        owner_account: account_name(&config.opener_account),
        fee_params: TransferFeeParams {
            input_selection: UtxoInputSelection::PreferRevealed,
        },
        input_selection: UtxoInputSelection::PreferConfidential,
        resource_address: STEALTH_TARI_RESOURCE_ADDRESS.to_string(),
        badge_usage: "None",
        transfers: vec![StealthTransfer {
            destination_address: session.refund_address,
            blinded_output_amount: config.deposit,
            revealed_output_amount: 0,
            output_memo: None,
            pay_to: serde_json::Value::String("StealthPublicKey".into()),
            attach_sender_address: false,
            pay_ref: None,
        }],
        max_fee: config.refund_max_fee,
        dry_run: false,
    };
    // Persist an exclusive intent before submitting funds. A crash after submission must not
    // cause a retry to issue a second refund from the relay's shared deposit pool.
    state
        .relay_registry
        .mark_funding_refunding(&funding_id)
        .await?;
    let sent = wallet
        .stealth_transfer(&transfer)
        .await
        .map_err(|error| error.to_string())?;
    let refund_tx = sent.transaction_id.to_string();
    state
        .relay_registry
        .mark_funding_refund(&funding_id, refund_tx.clone())
        .await?;
    let status = transaction_status(&config, sent.transaction_id).await?;
    if status == "Finalized" {
        state
            .relay_registry
            .remove_session(&request.session_id)
            .await?;
    }
    warn_if_fee_float_low(&config).await;
    Ok((refund_tx, status))
}

pub async fn restore_from_receipt(state: &AppState, receipt: &str) -> Result<OpenResponse, String> {
    let config = RelayConfig::load()?;
    let bytes = crate::open_recovery_receipt(receipt, &config.recovery_key)
        .map_err(|error| error.to_string())?;
    let bundle: RecoveryBundle =
        serde_json::from_slice(&bytes).map_err(|_| "unsupported recovery receipt")?;
    if bundle.version != 1 {
        return Err("unsupported recovery receipt".into());
    }
    restore_bundle(state, &config, bundle).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfigResponse {
    pub enabled: bool,
    pub deposit: u64,
    pub price: u64,
    pub funding_address: String,
    pub network: &'static str,
    pub custody: &'static str,
}

pub async fn config(State(_state): State<AppState>) -> impl IntoResponse {
    if !demo_mode() {
        return (StatusCode::NOT_FOUND, "demo relay is disabled").into_response();
    }
    match RelayConfig::load() {
        Ok(config) => match connect_wallet(&config).await {
            Ok(wallet) => match relay_funding_account(&wallet, &config).await {
                Ok(account) => (
                    StatusCode::OK,
                    Json(RelayConfigResponse {
                        enabled: true,
                        deposit: config.deposit,
                        price: PRICE_PER_CALL,
                        funding_address: account.address_display(),
                        network: "esmeralda",
                        custody: "Custodial testnet demo. The relay controls channel settlement and expiry refund.",
                    }),
                )
                    .into_response(),
                Err(error) => (StatusCode::SERVICE_UNAVAILABLE, error.to_string()).into_response(),
            },
            Err(error) => (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
        },
        Err(error) => (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
    }
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<SessionRequest>,
) -> impl IntoResponse {
    if !demo_mode() {
        return (StatusCode::NOT_FOUND, "demo relay is disabled").into_response();
    }
    let config = match RelayConfig::load() {
        Ok(config) => config,
        Err(error) => return (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
    };
    if validate_demo_deposit(config.deposit).is_err() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "the current five-call demo requires a 51 µT deposit (50 µT spend plus 1 µT fixed-shape change)",
        )
            .into_response();
    }
    if RistrettoPublicKeyBytes::from_hex(request.payer_public_key.trim()).is_err() {
        return (StatusCode::BAD_REQUEST, "invalid payer public key").into_response();
    }
    let refund_address = request.refund_address.trim();
    if !refund_address.starts_with("otl_esm_1") {
        return (
            StatusCode::BAD_REQUEST,
            "refund_address must be an Esmeralda Sapient wallet address",
        )
            .into_response();
    }
    let session_id = match random_nonce_hex() {
        Ok(value) => value,
        Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    };
    let wallet = match connect_wallet(&config).await {
        Ok(wallet) => wallet,
        Err(error) => return (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
    };
    let account = match relay_funding_account(&wallet, &config).await {
        Ok(account) => account,
        Err(error) => return (StatusCode::SERVICE_UNAVAILABLE, error.to_string()).into_response(),
    };
    if let Err(error) = state
        .relay_registry
        .create_session(
            session_id.clone(),
            RelaySession {
                payer_public_key: request.payer_public_key,
                refund_address: refund_address.to_owned(),
                created_at: unix_now(),
            },
        )
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
    (
        StatusCode::CREATED,
        Json(SessionResponse {
            session_id,
            funding_address: account.address_display(),
            deposit: config.deposit,
            price: PRICE_PER_CALL,
            network: "esmeralda",
            expires_in_seconds: 600,
        }),
    )
        .into_response()
}

const PRICE_PER_CALL: u64 = 10;

pub async fn settle(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ChannelQuery>,
) -> impl IntoResponse {
    if !demo_mode() {
        return (StatusCode::NOT_FOUND, "demo relay is disabled").into_response();
    }
    let Some(record) = state.relay_registry.get(&query.channel).await else {
        return (StatusCode::NOT_FOUND, "unknown demo channel").into_response();
    };
    let result = if let Some(tx) = record.settlement_tx.clone() {
        match RelayConfig::load() {
            Ok(config) => match TransactionId::from_hex(&tx) {
                Ok(id) => transaction_status(&config, id)
                    .await
                    .map(|status| (tx, status)),
                Err(error) => Err(error.to_string()),
            },
            Err(error) => Err(error),
        }
    } else {
        settle_inner(&record).await
    };
    match result {
        Ok((tx_id, status)) => {
            if let Err(error) = state
                .relay_registry
                .settle(&query.channel, tx_id.clone())
                .await
            {
                return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
            }
            let recovery_receipt = match state.mint_recovery_receipt(&query.channel).await {
                Ok(receipt) => Some(receipt),
                Err(error) => {
                    tracing::error!(event = "recovery_receipt", outcome = "failed_after_settlement", error = %error);
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "settlement was submitted; retry status to refresh recovery state",
                    )
                        .into_response();
                }
            };
            (
                StatusCode::OK,
                Json(RelayStatus {
                    channel_id: query.channel,
                    state: status,
                    explorer_url: (status == "Finalized")
                        .then(|| format!("https://explorer.tari.mw/tx/{tx_id}")),
                    settlement_transaction_id: Some(tx_id),
                    expiry_epoch: record.expiry_epoch,
                    recovery_receipt,
                }),
            )
                .into_response()
        }
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error).into_response(),
    }
}

pub async fn status(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ChannelQuery>,
) -> impl IntoResponse {
    let Some(record) = state.relay_registry.get(&query.channel).await else {
        return (StatusCode::NOT_FOUND, "unknown demo channel").into_response();
    };
    let (state_name, explorer_url) = match record.settlement_tx.as_deref() {
        Some(tx) => {
            let state_name = match (RelayConfig::load(), TransactionId::from_hex(tx)) {
                (Ok(config), Ok(id)) => transaction_status(&config, id).await.unwrap_or("Pending"),
                _ => "Pending",
            };
            (
                state_name,
                Some(format!("https://explorer.tari.mw/tx/{tx}")),
            )
        }
        None if record.refund_tx.is_some() => {
            let tx = record.refund_tx.as_deref().unwrap_or_default();
            let finality = match (RelayConfig::load(), TransactionId::from_hex(tx)) {
                (Ok(config), Ok(id)) => transaction_status(&config, id).await.unwrap_or("Pending"),
                _ => "Pending",
            };
            (
                if finality == "Finalized" {
                    "Refunded"
                } else {
                    "RefundPending"
                },
                Some(format!("https://explorer.tari.mw/tx/{tx}")),
            )
        }
        None if record.channel_refund_tx.is_some() => {
            let tx = record.channel_refund_tx.as_deref().unwrap_or_default();
            let finality = match (RelayConfig::load(), TransactionId::from_hex(tx)) {
                (Ok(config), Ok(id)) => transaction_status(&config, id).await.unwrap_or("Pending"),
                _ => "Pending",
            };
            (
                if finality == "Finalized" {
                    "RefundPending"
                } else {
                    "Pending"
                },
                Some(format!("https://explorer.tari.mw/tx/{tx}")),
            )
        }
        None => ("Open", None),
    };
    let recovery_receipt = match state.mint_recovery_receipt(&query.channel).await {
        Ok(receipt) => Some(receipt),
        Err(error) => {
            tracing::error!(event = "recovery_receipt", outcome = "failed_on_status", error = %error);
            None
        }
    };
    (
        StatusCode::OK,
        Json(RelayStatus {
            channel_id: query.channel,
            state: state_name,
            settlement_transaction_id: record.settlement_tx,
            explorer_url,
            expiry_epoch: record.expiry_epoch,
            recovery_receipt,
        }),
    )
        .into_response()
}

pub async fn refund(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ChannelQuery>,
) -> impl IntoResponse {
    if !demo_mode() {
        return (StatusCode::NOT_FOUND, "demo relay is disabled").into_response();
    }
    let Some(record) = state.relay_registry.get(&query.channel).await else {
        return (StatusCode::NOT_FOUND, "unknown demo channel").into_response();
    };
    if record.settlement_tx.is_some() {
        return (StatusCode::CONFLICT, "channel has already settled").into_response();
    }
    if let Some(tx_id) = record.refund_tx.clone() {
        let finality = match (RelayConfig::load(), TransactionId::from_hex(&tx_id)) {
            (Ok(config), Ok(id)) => transaction_status(&config, id).await,
            (Err(error), _) => Err(error),
            (_, Err(error)) => Err(error.to_string()),
        };
        return match finality {
            Ok("Finalized") => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "channelId": query.channel,
                    "state": "refunded",
                    "refundTransactionId": tx_id,
                    "explorerUrl": format!("https://explorer.tari.mw/tx/{tx_id}"),
                })),
            )
                .into_response(),
            Ok(_) => (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({
                    "channelId": query.channel,
                    "state": "pending",
                    "refundTransactionId": tx_id,
                    "explorerUrl": format!("https://explorer.tari.mw/tx/{tx_id}"),
                })),
            )
                .into_response(),
            Err(error) => (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
        };
    }
    match refund_inner(&record, &state.relay_registry, &query.channel).await {
        Ok(tx_id) => {
            if let Err(error) = state
                .relay_registry
                .refund(&query.channel, tx_id.clone())
                .await
            {
                return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
            }
            let recovery_receipt = match state.mint_recovery_receipt(&query.channel).await {
                Ok(receipt) => receipt,
                Err(error) => {
                    tracing::error!(event = "recovery_receipt", outcome = "failed_after_refund", error = %error);
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "refund was submitted; retry status to refresh recovery state",
                    )
                        .into_response();
                }
            };
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "channelId": query.channel,
                    "state": "refunded",
                    "refundTransactionId": tx_id,
                    "explorerUrl": format!("https://explorer.tari.mw/tx/{tx_id}"),
                    "recoveryReceipt": recovery_receipt,
                })),
            )
                .into_response()
        }
        Err(error) => {
            if let Some(updated) = state.relay_registry.get(&query.channel).await
                && updated.channel_refund_tx.is_some()
                && updated.refund_tx.is_none()
                && error.contains("not finalized yet")
            {
                let tx = updated.channel_refund_tx.unwrap_or_default();
                return (
                    StatusCode::ACCEPTED,
                    Json(serde_json::json!({
                        "channelId": query.channel,
                        "state": "pending",
                        "refundTransactionId": tx,
                        "explorerUrl": format!("https://explorer.tari.mw/tx/{tx}"),
                    })),
                )
                    .into_response();
            }
            (StatusCode::UNPROCESSABLE_ENTITY, error).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct ChannelQuery {
    pub channel: String,
}

async fn open_inner(state: &AppState, request: OpenRequest) -> Result<OpenResponse, String> {
    // A wallet can finalize the transfer even when the browser loses the relay response.
    // Serialize opens and replay the original response for the same funding transaction so
    // a retry cannot create a second channel or prompt the visitor to fund again.
    let _open_guard = state.relay_registry.1.clone().lock_owned().await;
    let funding_tx = TransactionId::from_hex(request.funding_tx_id.trim())
        .map_err(|error| format!("invalid funding transaction id: {error}"))?;
    let funding_id = funding_tx.to_string();
    if let Some(response) = state.relay_registry.completed_open(&funding_id).await {
        return Ok(response);
    }
    let config = RelayConfig::load()?;
    if state.relay_registry.has_funding(&funding_id).await {
        return Err("this Sapient funding transaction was already used".into());
    }
    if state.relay_registry.is_refunding_funding(&funding_id).await {
        return Err(
            "this Sapient funding transaction is being refunded; it cannot also open a channel"
                .into(),
        );
    }
    let (session, verified) =
        load_funding_session(state, &config, funding_tx, &request.session_id).await?;
    let payer = RistrettoPublicKeyBytes::from_hex(session.payer_public_key.trim())
        .map_err(|error| format!("invalid payer public key: {error}"))?;
    let refund_address = session.refund_address;

    let current_epoch = run_cli(&config, &["indexer", "epoch", "--url", &config.indexer_url])
        .await?
        .trim()
        .parse::<u64>()
        .map_err(|_| "indexer epoch response was not an integer".to_string())?;
    let expiry_epoch = current_epoch
        .checked_add(config.expiry_epochs)
        .ok_or("expiry epoch overflow")?;
    validate_demo_deposit(config.deposit).map_err(str::to_owned)?;
    if config.expiry_epochs == 0 || config.expiry_epochs > 100 {
        return Err("demo expiry must be between 1 and 100 epochs".into());
    }
    if let Some(attempt) = state.relay_registry.open_attempt(&funding_id).await {
        return continue_saved_open(
            state,
            &config,
            &request.session_id,
            &funding_id,
            attempt,
            &verified.commitments,
        )
        .await;
    }
    require_funding_outputs_spendable(&config, &verified.commitments).await?;
    let max_epoch = current_epoch.checked_add(100).ok_or("max epoch overflow")?;
    let nonce = random_nonce_hex()?;

    let directory = config.state_dir.join("channels");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let temp_id = format!("{}-{}", current_epoch, &nonce[..12]);
    let channel_file = directory.join(format!("{temp_id}.json"));
    let ledger_file = directory.join(format!("{temp_id}.ledger.json"));
    let session_file = directory.join(format!("{temp_id}.session.json"));
    let service_signer_file = directory.join(format!("{temp_id}.service-key.json"));
    let output_signer_file = directory.join(format!("{temp_id}.output-key.json"));
    let refund_signer_file = directory.join(format!("{temp_id}.refund-key.json"));
    let service = create_wallet_key(&config, &service_signer_file).await?;
    let service_output = create_wallet_key(&config, &output_signer_file).await?;
    let refund = create_wallet_key(&config, &refund_signer_file).await?;
    let fee_component = run_cli(
        &config,
        &[
            "wallet",
            "account-get",
            "--url",
            &config.wallet_url,
            "--name",
            &config.fee_account_name,
        ],
    )
    .await?
    .lines()
    .find_map(|line| line.strip_prefix("component_address="))
    .map(str::to_owned)
    .ok_or("wallet account-get did not return component_address")?;

    let payer_hex = to_hex(payer.as_bytes());
    let spec_output = run_cli(
        &config,
        &[
            "spec",
            "--network-tag",
            &config.network_tag.to_string(),
            "--deposit",
            &config.deposit.to_string(),
            "--payer-hex",
            &payer_hex,
            "--service-hex",
            &service.public_key_hex,
            "--refund-hex",
            &refund.public_key_hex,
            "--expiry-epoch",
            &expiry_epoch.to_string(),
            "--nonce-hex",
            &nonce,
            "--template-hex",
            &config.template_hex,
        ],
    )
    .await?;
    let payer_root = spec_output
        .lines()
        .find_map(|line| line.strip_prefix("payer_root="))
        .ok_or("channel spec did not return payer_root")?
        .trim()
        .to_owned();
    let open_args = vec![
        "channel".to_string(),
        "init".into(),
        "--out".into(),
        channel_file.display().to_string(),
        "--network-tag".into(),
        config.network_tag.to_string(),
        "--deposit".into(),
        config.deposit.to_string(),
        "--payer-hex".into(),
        payer_hex,
        "--service-hex".into(),
        service.public_key_hex.clone(),
        "--refund-hex".into(),
        refund.public_key_hex.clone(),
        "--expiry-epoch".into(),
        expiry_epoch.to_string(),
        "--nonce-hex".into(),
        nonce.clone(),
        "--template-hex".into(),
        config.template_hex.clone(),
        "--indexer-url".into(),
        config.indexer_url.clone(),
        "--ootle-network".into(),
        "esmeralda".into(),
        "--max-epoch".into(),
        max_epoch.to_string(),
        "--opener-account".into(),
        config.opener_account.clone(),
        "--opener-wallet-url".into(),
        config.wallet_url.clone(),
        "--fee-account".into(),
        fee_component.clone(),
        "--fee-account-name".into(),
        config.fee_account_name.clone(),
        "--fee-wallet-url".into(),
        config.wallet_url.clone(),
        "--payer-fee-account".into(),
        fee_component,
        "--payer-fee-account-name".into(),
        config.fee_account_name.clone(),
        "--payer-wallet-url".into(),
        config.wallet_url.clone(),
        "--ledger".into(),
        ledger_file.display().to_string(),
        "--session".into(),
        session_file.display().to_string(),
    ];
    let init_args = open_args.iter().map(String::as_str).collect::<Vec<_>>();
    run_cli(&config, &init_args).await?;
    let attempt = OpenAttempt {
        channel_file: channel_file.clone(),
        ledger_file,
        service_signer_file,
        service_output_signer_file: output_signer_file,
        refund_signer_file,
        service_public_key: service.public_key_hex,
        service_output_public_key: service_output.public_key_hex,
        refund_public_key: refund.public_key_hex,
        refund_address,
        expiry_epoch,
        payer_root,
        nonce,
    };
    // The channel file is fixed before the wallet call. A retry resumes this attempt.
    state
        .relay_registry
        .save_open_attempt(&funding_id, attempt.clone())
        .await?;
    state.relay_registry.mark_opening(&funding_id).await?;
    let output = match run_channel_open(&config, &channel_file, &verified.commitments).await {
        Ok(output) => output,
        Err(error) => {
            return note_open_cli_failure(state, &funding_id, &channel_file, error).await;
        }
    };
    finish_registered_open(
        state,
        &config,
        &request.session_id,
        &funding_id,
        &attempt,
        &output,
    )
    .await
}

async fn continue_saved_open(
    state: &AppState,
    config: &RelayConfig,
    session_id: &str,
    funding_id: &str,
    attempt: OpenAttempt,
    commitments: &[String],
) -> Result<OpenResponse, String> {
    let existing = channel_open_tx_id(&attempt.channel_file)?;
    let output = if let Some(tx) = existing {
        match indexed_open_outcome(config, &tx).await? {
            SavedOpen::Finalized => {
                run_channel_open(config, &attempt.channel_file, commitments).await?
            }
            SavedOpen::Rejected(reason) => {
                state.relay_registry.release_opening(funding_id).await?;
                return Err(format!(
                    "channel open {tx} was rejected ({reason}). Refusing to create another output for this funding transaction."
                ));
            }
            SavedOpen::Pending => {
                return Err(format!(
                    "channel open {tx} is still pending. Retry this funding transaction without sending another deposit."
                ));
            }
        }
    } else {
        if let Err(error) = require_funding_outputs_spendable(config, commitments).await {
            state.relay_registry.release_opening(funding_id).await?;
            return Err(error);
        }
        state.relay_registry.mark_opening(funding_id).await?;
        match run_channel_open(config, &attempt.channel_file, commitments).await {
            Ok(output) => output,
            Err(error) => {
                return note_open_cli_failure(state, funding_id, &attempt.channel_file, error)
                    .await;
            }
        }
    };
    finish_registered_open(state, config, session_id, funding_id, &attempt, &output).await
}

async fn finish_registered_open(
    state: &AppState,
    config: &RelayConfig,
    session_id: &str,
    funding_id: &str,
    attempt: &OpenAttempt,
    output: &str,
) -> Result<OpenResponse, String> {
    let channel_id = output
        .lines()
        .find_map(|line| line.strip_prefix("channel_id="))
        .ok_or("channel open completed without a channel id")?
        .trim()
        .to_owned();
    let open_tx_id = output
        .lines()
        .find_map(|line| line.strip_prefix("transaction_id="))
        .ok_or("channel open completed without a transaction id")?
        .trim()
        .to_owned();
    let file_json = std::fs::read(&attempt.channel_file).map_err(|error| error.to_string())?;
    let file: ChannelCliFile = serde_json::from_slice(&file_json).map_err(|e| e.to_string())?;
    let ledger = ChannelStore::open(&attempt.ledger_file).map_err(|error| error.to_string())?;
    let terms = PaymentTerms::new(
        RistrettoPublicKeyBytes::from_hex(&file.service_hex).map_err(|e| e.to_string())?,
        file.expiry_epoch,
        file.expiry_epoch,
    )
    .map_err(|error| error.to_string())?
    .with_service_output_key(
        RistrettoPublicKeyBytes::from_hex(&attempt.service_output_public_key)
            .map_err(|error| error.to_string())?,
    );
    state
        .register_channel_with_terms(ledger, terms)
        .await
        .map_err(|error| error.to_string())?;
    state
        .relay_registry
        .insert(
            channel_id.clone(),
            RelayRecord {
                channel_file: attempt.channel_file.clone(),
                service_signer_file: attempt.service_signer_file.clone(),
                service_output_signer_file: attempt.service_output_signer_file.clone(),
                refund_signer_file: attempt.refund_signer_file.clone(),
                refund_public_key: file
                    .refund_hex
                    .clone()
                    .ok_or("channel file omitted refund key")?,
                service_public_key: file.service_hex.clone(),
                service_output_public_key: attempt.service_output_public_key.clone(),
                refund_address: attempt.refund_address.clone(),
                settlement_tx: None,
                channel_refund_tx: None,
                refund_tx: None,
                expiry_epoch: attempt.expiry_epoch,
            },
        )
        .await?;
    let response = OpenResponse {
        channel_id,
        payer_root: attempt.payer_root.clone(),
        nonce: attempt.nonce.clone(),
        network_tag: config.network_tag,
        deposit: config.deposit,
        expiry_epoch: attempt.expiry_epoch,
        service_public_key: file.service_hex,
        service_output_public_key: attempt.service_output_public_key.clone(),
        funding_transaction_id: funding_id.to_owned(),
        open_transaction_id: open_tx_id,
        custody: "Custodial testnet relay: the relay receives the deposit and controls the expiry-refund key.".into(),
        recovery_receipt: None,
    };
    state
        .relay_registry
        .mark_funding(funding_id, response.clone())
        .await?;
    state.relay_registry.remove_session(session_id).await?;
    warn_if_fee_float_low(config).await;
    Ok(response)
}

async fn note_open_cli_failure(
    state: &AppState,
    funding_id: &str,
    channel_file: &Path,
    error: String,
) -> Result<OpenResponse, String> {
    if channel_open_tx_id(channel_file).ok().flatten().is_none() {
        state.relay_registry.release_opening(funding_id).await?;
    }
    Err(error)
}

fn channel_open_tx_id(path: &Path) -> Result<Option<String>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("channel file: {error}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("channel file: {error}"))?;
    Ok(value
        .get("open_transaction_id")
        .and_then(serde_json::Value::as_str)
        .filter(|tx| !tx.is_empty())
        .map(str::to_owned))
}

enum SavedOpen {
    Finalized,
    Pending,
    Rejected(String),
}

async fn indexed_open_outcome(config: &RelayConfig, tx: &str) -> Result<SavedOpen, String> {
    let transaction_id = TransactionId::from_hex(tx).map_err(|error| error.to_string())?;
    let indexer = IndexerClient::connect(&config.indexer_url).map_err(|error| error.to_string())?;
    match indexer.transaction(transaction_id).await {
        Ok(entry) => match indexed_transaction_status(&entry) {
            "Finalized" => Ok(SavedOpen::Finalized),
            "Pending" | "Unknown" => Ok(SavedOpen::Pending),
            "FinalizedAbort" | "Rejected" => Ok(SavedOpen::Rejected(open_rejection_reason(&entry))),
            other => Err(format!(
                "indexer returned an unrecognized transaction status: {other}"
            )),
        },
        Err(error) if indexer_lookup_missing(&error) => {
            match indexer.transaction_receipt(transaction_id).await {
                Ok(receipt) => match indexed_receipt_status(&receipt) {
                    "Finalized" => Ok(SavedOpen::Finalized),
                    "FinalizedAbort" | "Rejected" => {
                        Ok(SavedOpen::Rejected("Ootle rejected the transaction".into()))
                    }
                    _ => Ok(SavedOpen::Pending),
                },
                Err(error) if indexer_lookup_missing(&error) => Ok(SavedOpen::Pending),
                Err(error) => Err(error.to_string()),
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

fn open_rejection_reason(entry: &serde_json::Value) -> String {
    let reason = entry
        .get("transaction")
        .unwrap_or(entry)
        .get("rejected_reason");
    match reason {
        Some(serde_json::Value::String(text)) if !text.is_empty() => text.clone(),
        Some(other) if !other.is_null() => other.to_string(),
        _ => "Ootle rejected the transaction".to_string(),
    }
}

/// The CLI then locks only these commitments. Other unspent outputs stay where they are.
async fn require_funding_outputs_spendable(
    config: &RelayConfig,
    commitments: &[String],
) -> Result<(), String> {
    let wallet = connect_wallet(config).await?;
    let utxos = wallet
        .stealth_utxos_list(
            &config.opener_account,
            &STEALTH_TARI_RESOURCE_ADDRESS.to_string(),
        )
        .await
        .map_err(|error| format!("could not list opener stealth outputs: {error}"))?;
    funding_output_spend_gate(&utxos, commitments)
}

async fn run_channel_open(
    config: &RelayConfig,
    channel_file: &Path,
    commitments: &[String],
) -> Result<String, String> {
    let file = channel_file
        .to_str()
        .ok_or("invalid channel file path")?
        .to_owned();
    let fee = config.open_max_fee.to_string();
    let mut args = vec![
        "channel".to_string(),
        "open".into(),
        "--file".into(),
        file,
        "--max-fee".into(),
        fee,
    ];
    for commitment in commitments {
        args.push("--funding-commitment".into());
        args.push(commitment.clone());
    }
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_cli(config, &refs).await
}

async fn restore_bundle(
    state: &AppState,
    config: &RelayConfig,
    mut bundle: RecoveryBundle,
) -> Result<OpenResponse, String> {
    if bundle.channel_id.len() != ghostline_core::CHANNEL_ID_LEN * 2
        || !bundle
            .channel_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || bundle.response.channel_id != bundle.channel_id
    {
        return Err("recovery receipt channel identity is invalid".into());
    }
    if let Some(existing) = state
        .relay_registry
        .response_for_channel(&bundle.channel_id)
        .await
    {
        return Ok(existing);
    }
    let directory = config.state_dir.join("channels");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let channel_file = directory.join(format!("{}.json", bundle.channel_id));
    let ledger_file = directory.join(format!("{}.ledger.json", bundle.channel_id));
    let service_signer_file = directory.join(format!("{}.service-key.json", bundle.channel_id));
    let service_output_signer_file =
        directory.join(format!("{}.payout-key.json", bundle.channel_id));
    let refund_signer_file = directory.join(format!("{}.refund-key.json", bundle.channel_id));
    write_file_hex(&channel_file, &bundle.channel_file_hex)?;
    write_file_hex(&service_signer_file, &bundle.service_signer_file_hex)?;
    write_file_hex(
        &service_output_signer_file,
        &bundle.service_output_signer_file_hex,
    )?;
    write_file_hex(&refund_signer_file, &bundle.refund_signer_file_hex)?;
    let ledger_snapshot = from_hex(&bundle.ledger_snapshot_hex)
        .map_err(|error| format!("invalid recovery ledger snapshot: {error}"))?;
    let store = ChannelStore::restore_recovery_snapshot(&ledger_file, &ledger_snapshot)
        .map_err(|error| format!("restore recovery ledger: {error}"))?;
    let service = RistrettoPublicKeyBytes::from_hex(&bundle.record.service_public_key)
        .map_err(|error| format!("invalid recovered service key: {error}"))?;
    let payout = RistrettoPublicKeyBytes::from_hex(&bundle.record.service_output_public_key)
        .map_err(|error| format!("invalid recovered payout key: {error}"))?;
    let terms = PaymentTerms::new(
        service,
        bundle.record.expiry_epoch,
        bundle.record.expiry_epoch,
    )
    .map_err(|error| format!("invalid recovered payment terms: {error}"))?
    .with_service_output_key(payout);
    state
        .register_channel_with_terms(store, terms)
        .await
        .map_err(|error| format!("register recovered channel: {error}"))?;
    bundle.record.channel_file = channel_file;
    bundle.record.service_signer_file = service_signer_file;
    bundle.record.service_output_signer_file = service_output_signer_file;
    bundle.record.refund_signer_file = refund_signer_file;
    let response = bundle.response;
    state
        .relay_registry
        .insert(bundle.channel_id, bundle.record)
        .await?;
    state
        .relay_registry
        .mark_funding(&response.funding_transaction_id, response.clone())
        .await?;
    Ok(response)
}

fn read_file_hex(path: &Path) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| to_hex(&bytes))
        .map_err(|error| format!("read recovery file {}: {error}", path.display()))
}

fn write_file_hex(path: &Path, value: &str) -> Result<(), String> {
    let bytes = from_hex(value).map_err(|error| format!("invalid recovery file data: {error}"))?;
    ghostline_core::write_private_file(path, &bytes)
        .map_err(|error| format!("restore private recovery file {}: {error}", path.display()))
}

async fn settle_inner(record: &RelayRecord) -> Result<(String, &'static str), String> {
    let config = RelayConfig::load()?;
    let channel = record
        .channel_file
        .to_str()
        .ok_or("invalid channel file path")?;
    let output = run_cli(
        &config,
        &[
            "channel",
            "settle",
            "--file",
            channel,
            "--force",
            "--submit",
            "--seal-key-id-file",
            record
                .service_signer_file
                .to_str()
                .ok_or("invalid key path")?,
            "--max-fee",
            &config.settle_max_fee.to_string(),
        ],
    )
    .await?;
    let tx = output
        .lines()
        .find_map(|line| line.strip_prefix("transaction_id="))
        .ok_or_else(|| {
            format!(
                "settle response had no transaction id: {}",
                safe_cli_output(&output)
            )
        })?
        .trim()
        .to_owned();
    let transaction_id = TransactionId::from_hex(&tx).map_err(|e| e.to_string())?;
    // Keep the submitted transaction id even if the indexer is temporarily unavailable. The
    // caller stores it before polling again, avoiding a second settlement submission.
    let status = transaction_status(&config, transaction_id)
        .await
        .unwrap_or("Pending");
    warn_if_fee_float_low(&config).await;
    Ok((tx, status))
}

async fn transaction_status(
    config: &RelayConfig,
    transaction_id: TransactionId,
) -> Result<&'static str, String> {
    let indexer = IndexerClient::connect(&config.indexer_url).map_err(|e| e.to_string())?;
    match indexer.transaction(transaction_id).await {
        Ok(entry) => match indexed_transaction_status(&entry) {
            "Finalized" => Ok("Finalized"),
            "Pending" | "Unknown" => Ok("Pending"),
            "FinalizedAbort" | "Rejected" => Err("Ootle rejected the transaction".into()),
            other => Err(format!(
                "indexer returned an unrecognized transaction status: {other}"
            )),
        },
        Err(error) if indexer_lookup_missing(&error) => {
            match indexer.transaction_receipt(transaction_id).await {
                Ok(receipt) => match indexed_receipt_status(&receipt) {
                    "Finalized" => Ok("Finalized"),
                    "FinalizedAbort" | "Rejected" => Err("Ootle rejected the transaction".into()),
                    _ => Ok("Pending"),
                },
                Err(error) if indexer_lookup_missing(&error) => Ok("Pending"),
                Err(error) => Err(error.to_string()),
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

#[derive(Deserialize)]
pub(crate) struct TxStatusQuery {
    id: String,
}

#[derive(Serialize)]
struct TxStatusResponse {
    state: &'static str,
}

/// Public Esmeralda 0.47 `/transactions/{id}/result` 404s while the transaction record remains.
/// The play page polls this instead of treating that 404 as a dropped deposit.
pub async fn tx_status(
    axum::extract::Query(query): axum::extract::Query<TxStatusQuery>,
) -> impl IntoResponse {
    if !demo_mode() {
        return (StatusCode::NOT_FOUND, "demo relay is disabled").into_response();
    }
    let config = match RelayConfig::load() {
        Ok(config) => config,
        Err(error) => return (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
    };
    let id = match TransactionId::from_hex(query.id.trim()) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "transaction id must be 32-byte hex",
            )
                .into_response();
        }
    };
    match transaction_status(&config, id).await {
        Ok(state) => (StatusCode::OK, Json(TxStatusResponse { state })).into_response(),
        Err(error) => (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
    }
}

const MISSING_SESSION_HINT: &str = "the relay no longer has this pre-funding session (usually after a host restart), and this older deposit memo cannot rebuild the payer key or refund address. Do not send another deposit. Return the funds from this page if the relay still has the session; otherwise abandon the saved transfer locally or contact the operator";

#[derive(Clone, Debug, PartialEq, Eq)]
struct FundingBinding {
    session_id: String,
    payer_public_key: Option<String>,
    refund_address: Option<String>,
}

struct VerifiedFunding {
    amount: u64,
    binding: FundingBinding,
    commitments: Vec<String>,
}

#[derive(Debug)]
struct RecoveredFundingOutput {
    recovered: ghostline_sdk::MaskAndValue,
    binding: FundingBinding,
}

fn parse_funding_memo_message(message: &str) -> Option<FundingBinding> {
    if let Some(rest) = message.strip_prefix("G2:") {
        let session_id = rest.get(..64)?.to_ascii_lowercase();
        let payer_public_key = rest.get(64..128)?.to_ascii_lowercase();
        if rest.as_bytes().get(128) != Some(&b':') {
            return None;
        }
        let refund_address = rest.get(129..)?.to_owned();
        if session_id.len() != 64
            || payer_public_key.len() != 64
            || !session_id.chars().all(|c| c.is_ascii_hexdigit())
            || !payer_public_key.chars().all(|c| c.is_ascii_hexdigit())
            || !refund_address.starts_with("otl_esm_1")
        {
            return None;
        }
        return Some(FundingBinding {
            session_id,
            payer_public_key: Some(payer_public_key),
            refund_address: Some(refund_address),
        });
    }
    message.strip_prefix("Ghostline:").and_then(|session_id| {
        (!session_id.is_empty()).then(|| FundingBinding {
            session_id: session_id.to_owned(),
            payer_public_key: None,
            refund_address: None,
        })
    })
}

#[cfg(test)]
fn funding_memo_for_binding(
    session_id: &str,
    payer_public_key: &str,
    refund_address: &str,
) -> Result<String, String> {
    let memo = format!("G2:{session_id}{payer_public_key}:{refund_address}");
    if memo.len() > tari_ootle_wallet_crypto::memo::Memo::MAX_BYTES_LENGTH {
        return Err("funding memo exceeds the stealth encrypted-data limit".into());
    }
    Ok(memo)
}

async fn load_funding_session(
    state: &AppState,
    config: &RelayConfig,
    funding_tx: TransactionId,
    session_id: &str,
) -> Result<(RelaySession, VerifiedFunding), String> {
    let verified = verify_relay_funding(config, funding_tx, session_id).await?;
    if let Some(session) = state.relay_registry.session(session_id).await {
        if let Some(payer) = verified.binding.payer_public_key.as_deref()
            && payer != session.payer_public_key.trim().to_ascii_lowercase()
        {
            return Err("funding memo payer does not match the relay session".into());
        }
        if let Some(refund) = verified.binding.refund_address.as_deref()
            && refund != session.refund_address
        {
            return Err("funding memo refund address does not match the relay session".into());
        }
        return Ok((session, verified));
    }
    let payer = verified
        .binding
        .payer_public_key
        .clone()
        .ok_or_else(|| MISSING_SESSION_HINT.to_owned())?;
    let refund = verified
        .binding
        .refund_address
        .clone()
        .ok_or_else(|| MISSING_SESSION_HINT.to_owned())?;
    let session = RelaySession {
        payer_public_key: payer,
        refund_address: refund,
        created_at: unix_now(),
    };
    state
        .relay_registry
        .create_session(session_id.to_owned(), session.clone())
        .await?;
    Ok((session, verified))
}

fn indexer_lookup_missing(error: &impl std::fmt::Display) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("not found") || text.contains("http 404")
}

#[cfg(test)]
fn json_microtari(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse::<u64>().ok())
}

#[cfg(test)]
fn listed_utxo_memo_message(utxo: &serde_json::Value) -> Option<&str> {
    utxo.get("memo")
        .and_then(|memo| memo.get("Message").or_else(|| memo.get("message")))
        .and_then(serde_json::Value::as_str)
}

#[cfg(test)]
fn listed_utxo_is_unspent(utxo: &serde_json::Value) -> bool {
    utxo.get("status")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|status| status.eq_ignore_ascii_case("unspent"))
}

#[cfg(test)]
fn verify_funding_from_listed_utxos(
    utxos: &[serde_json::Value],
    deposit: u64,
    session_id: &str,
) -> Result<VerifiedFunding, String> {
    let mut matches = Vec::new();
    for utxo in utxos {
        if !listed_utxo_is_unspent(utxo) {
            continue;
        }
        let Some(value) = json_microtari(utxo.get("value").unwrap_or(&serde_json::Value::Null))
        else {
            continue;
        };
        if value != deposit {
            continue;
        }
        let Some(message) = listed_utxo_memo_message(utxo) else {
            continue;
        };
        let Some(binding) = parse_funding_memo_message(message) else {
            continue;
        };
        if binding.session_id.eq_ignore_ascii_case(session_id) {
            matches.push((value, binding));
        }
    }
    if matches.len() != 1 {
        return Err(format!(
            "The Esmeralda indexer no longer has this funding transaction. The relay found {} unspent opener deposit(s) of {deposit} µT bound to this session. Keep the saved transfer; do not send another deposit.",
            matches.len()
        ));
    }
    let (amount, binding) = matches.pop().expect("exactly one matching unspent deposit");
    Ok(VerifiedFunding {
        amount,
        binding,
        commitments: Vec::new(),
    })
}

fn verify_unblinded_funding_outputs(
    outputs: &[ghostline_net::WalletStealthUtxo],
    config: &RelayConfig,
    session_id: &str,
) -> Result<VerifiedFunding, String> {
    let mut relay_values = Vec::new();
    let mut commitments = Vec::new();
    let mut candidate_count = 0usize;
    let mut binding = None;
    for output in outputs.iter().filter(|output| output.auth != "Script") {
        if output
            .resource
            .as_deref()
            .is_some_and(|r| r != STEALTH_TARI_RESOURCE_ADDRESS.to_string())
        {
            continue;
        }
        candidate_count += 1;
        if let Ok(recovered) = unblind_wallet_output(output, config, session_id) {
            relay_values.push(recovered.recovered.value);
            commitments.push(output.commitment.clone());
            if binding.is_none() {
                binding = Some(recovered.binding);
            }
        }
    }
    let owned_total = relay_values
        .iter()
        .try_fold(0u64, |total, value| total.checked_add(*value))
        .ok_or_else(|| {
            "relay-owned Sapient funding outputs overflow the supported amount range".to_owned()
        })?;
    if funding_output_total(&relay_values, config.deposit).is_err() {
        tracing::warn!(
            event = "sapient_funding_output_mismatch",
            transaction_output_count = outputs.len(),
            candidate_output_count = candidate_count,
            verified_output_count = relay_values.len(),
            verified_total_micro_xtr = owned_total,
            expected_micro_xtr = config.deposit,
            "could not verify the finalized Sapient deposit and its encrypted session binding"
        );
        return Err(format!(
            "Sapient funding is finalized, but the relay verified {} of {} candidate private outputs for this session ({} µT total); expected {} µT. The relay operator must check the opener account's matching view_only_key and encrypted session memo. Keep this pending transaction and do not send another deposit.",
            relay_values.len(),
            candidate_count,
            owned_total,
            config.deposit
        ));
    }
    Ok(VerifiedFunding {
        amount: owned_total,
        binding: binding.ok_or_else(|| {
            "Sapient funding is finalized, but the relay could not decrypt a session-binding memo"
                .to_owned()
        })?,
        commitments,
    })
}

async fn wait_indexed_funding_transaction(
    indexer: &IndexerClient,
    funding_tx: TransactionId,
) -> Result<serde_json::Value, String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        match indexer.transaction(funding_tx).await {
            Ok(entry) => match indexed_transaction_status(&entry) {
                "Finalized" => return Ok(entry),
                "Pending" | "Unknown" => {}
                "FinalizedAbort" | "Rejected" => {
                    return Err("Sapient funding transaction was rejected by Ootle".into());
                }
                other => {
                    return Err(format!(
                        "indexer returned an unrecognized Sapient funding result: {other}"
                    ));
                }
            },
            Err(error) if indexer_lookup_missing(&error) => {
                if let Ok(receipt) = indexer.transaction_receipt(funding_tx).await {
                    match indexed_receipt_status(&receipt) {
                        "FinalizedAbort" | "Rejected" => {
                            return Err("Sapient funding transaction was rejected by Ootle".into());
                        }
                        "Finalized" => {
                            tracing::info!(
                                event = "sapient_funding_receipt_without_body",
                                "indexer has a Commit receipt; waiting for the transaction body"
                            );
                        }
                        _ => {}
                    }
                }
            }
            Err(error) if tokio::time::Instant::now() >= deadline => {
                return Err(format!(
                    "could not confirm Sapient funding finality through the Esmeralda indexer within 90 seconds: {error}"
                ));
            }
            Err(_) => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Sapient funding transaction is still pending after 90 seconds. The indexer transaction record was not a Commit; retry without sending another deposit.".into());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn verify_relay_funding(
    config: &RelayConfig,
    funding_tx: TransactionId,
    session_id: &str,
) -> Result<VerifiedFunding, String> {
    // Sapient submits this transaction through the user's wallet, not relay walletd.
    // Public Esmeralda 0.47 `/transactions/{id}/result` 404s for committed transfers;
    // GET /transactions/{id} still has the body and summary.outcome used by the explorer.
    let indexer = IndexerClient::connect(&config.indexer_url).map_err(|e| e.to_string())?;
    let entry = wait_indexed_funding_transaction(&indexer, funding_tx).await?;
    let indexed_id = entry
        .pointer("/transaction/transaction_id")
        .and_then(serde_json::Value::as_str)
        .ok_or("indexer funding transaction response omitted its transaction id")?;
    if !indexed_id.eq_ignore_ascii_case(&funding_tx.to_string()) {
        return Err(
            "indexer returned a different transaction while verifying Sapient funding".into(),
        );
    }
    let tx = entry
        .pointer("/transaction/transaction")
        .ok_or("indexer funding transaction response omitted its transaction body")?;
    let outputs = ghostline_net::stealth_utxos_from_wallet_tx(tx);
    verify_unblinded_funding_outputs(&outputs, config, session_id)
}

fn funding_output_total(values: &[u64], expected: u64) -> Result<u64, &'static str> {
    let total = values
        .iter()
        .try_fold(0u64, |sum, value| sum.checked_add(*value))
        .ok_or("relay-owned funding output total overflow")?;
    if values.is_empty() || total != expected {
        return Err("relay-owned funding outputs do not equal the configured deposit");
    }
    Ok(total)
}

fn funding_memo_matches_session(
    memo: Option<&tari_ootle_wallet_crypto::memo::Memo>,
    session_id: &str,
) -> bool {
    memo.and_then(|memo| memo.as_memo_message())
        .and_then(parse_funding_memo_message)
        .is_some_and(|binding| binding.session_id.eq_ignore_ascii_case(session_id))
}

async fn refund_inner(
    record: &RelayRecord,
    registry: &RelayRegistry,
    channel_id: &str,
) -> Result<String, String> {
    let config = RelayConfig::load()?;
    let epoch = run_cli(&config, &["indexer", "epoch", "--url", &config.indexer_url])
        .await?
        .trim()
        .parse::<u64>()
        .map_err(|_| "indexer epoch response was not an integer".to_string())?;
    if epoch < record.expiry_epoch {
        return Err("expiry has not opened the refund branch yet".into());
    }
    let refund_id = if let Some(tx) = record.channel_refund_tx.as_deref() {
        TransactionId::from_hex(tx).map_err(|e| e.to_string())?
    } else {
        let output = run_cli(
            &config,
            &[
                "channel",
                "refund",
                "--file",
                record
                    .channel_file
                    .to_str()
                    .ok_or("invalid channel file path")?,
                "--submit",
                "--seal-key-id-file",
                record
                    .refund_signer_file
                    .to_str()
                    .ok_or("invalid refund key path")?,
                "--refund-output-hex",
                &record.refund_public_key,
            ],
        )
        .await?;
        let tx = output
            .lines()
            .find_map(|line| line.strip_prefix("transaction_id="))
            .ok_or_else(|| {
                format!(
                    "refund response had no transaction id: {}",
                    safe_cli_output(&output)
                )
            })?
            .trim()
            .to_owned();
        registry.set_channel_refund(channel_id, tx.clone()).await?;
        TransactionId::from_hex(&tx).map_err(|e| e.to_string())?
    };
    let status = transaction_status(&config, refund_id).await?;
    if status != "Finalized" {
        return Err("channel refund is not finalized yet".into());
    }
    let wallet = connect_wallet(&config).await?;
    let request = StealthTransferRequest {
        owner_account: account_name(&config.opener_account),
        fee_params: TransferFeeParams {
            input_selection: UtxoInputSelection::PreferRevealed,
        },
        input_selection: UtxoInputSelection::PreferConfidential,
        resource_address: STEALTH_TARI_RESOURCE_ADDRESS.to_string(),
        badge_usage: "None",
        transfers: vec![StealthTransfer {
            destination_address: record.refund_address.clone(),
            blinded_output_amount: config.deposit,
            revealed_output_amount: 0,
            output_memo: None,
            pay_to: serde_json::Value::String("StealthPublicKey".into()),
            attach_sender_address: false,
            pay_ref: None,
        }],
        max_fee: config.refund_max_fee,
        dry_run: false,
    };
    let sent = wallet
        .stealth_transfer(&request)
        .await
        .map_err(|e| e.to_string())?;
    registry
        .refund(channel_id, sent.transaction_id.to_string())
        .await?;
    let sent_result = wallet
        .wait_result(sent.transaction_id, Some(90))
        .await
        .map_err(|e| e.to_string())?;
    if !execution_accepted(sent_result.get("result").unwrap_or(&sent_result)) {
        return Err("private refund transfer was not accepted by walletd".into());
    }
    let status = transaction_status(&config, sent.transaction_id).await?;
    if status != "Finalized" {
        return Err("private refund transfer is not finalized yet".into());
    }
    warn_if_fee_float_low(&config).await;
    Ok(sent.transaction_id.to_string())
}

fn unblind_wallet_output(
    output: &ghostline_net::WalletStealthUtxo,
    config: &RelayConfig,
    session_id: &str,
) -> Result<RecoveredFundingOutput, String> {
    use ghostline_sdk::{OutputScan, unblind_output};
    use tari_crypto::ristretto::RistrettoPublicKey;

    let secret_bytes = from_hex(&config.claim_secret).map_err(|e| e.to_string())?;
    let secret = RistrettoSecretKey::from_canonical_bytes(&secret_bytes)
        .map_err(|e| format!("invalid relay claim secret: {e}"))?;
    let encrypted =
        EncryptedData::try_from(from_hex(&output.encrypted_hex).map_err(|e| e.to_string())?)
            .map_err(|length| format!("invalid encrypted output length: {length}"))?;
    let utxo = UnspentOutput {
        commitment: PedersenCommitmentBytes::from_hex(&output.commitment)
            .map_err(|e| e.to_string())?,
        sender_public_nonce: RistrettoPublicKeyBytes::from_hex(&output.nonce)
            .map_err(|e| e.to_string())?,
        encrypted_data: encrypted,
        minimum_value_promise: 0,
        viewable_balance_proof: None,
    };
    let recovered = unblind_output(
        &utxo,
        UtxoTag::new(output.tag as u32),
        &secret,
        OutputScan::esmeralda(STEALTH_TARI_RESOURCE_ADDRESS),
    )
    .map_err(|error| format!("funding output is not addressed to the relay: {error}"))?;
    let nonce = RistrettoPublicKey::from_canonical_bytes(utxo.sender_public_nonce.as_bytes())
        .map_err(|error| format!("invalid funding sender nonce: {error}"))?;
    // Stealth outputs use a DH-derived encryption key, not the raw recipient view key.
    // Use the same authenticated decryption as value recovery, with memo decoding enabled.
    let decrypted = tari_ootle_wallet_crypto::StealthCryptoApi::new()
        .decrypt_utxo_data(
            &utxo.encrypted_data,
            &utxo.commitment,
            &secret,
            &nonce,
            false,
        )
        .map_err(|error| format!("could not decrypt Sapient funding memo: {error}"))?;
    if !funding_memo_matches_session(decrypted.memo(), session_id) {
        return Err("funding output is not bound to this relay session".into());
    }
    let binding = decrypted
        .memo()
        .and_then(|memo| memo.as_memo_message())
        .and_then(parse_funding_memo_message)
        .ok_or_else(|| "funding output is not bound to this relay session".to_owned())?;
    Ok(RecoveredFundingOutput { recovered, binding })
}

async fn create_wallet_key(config: &RelayConfig, path: &Path) -> Result<WalletKeyOutput, String> {
    let output = run_cli(
        config,
        &[
            "wallet",
            "key-create",
            "--url",
            &config.wallet_url,
            "--out",
            path.to_str().ok_or("invalid key path")?,
        ],
    )
    .await?;
    serde_json::from_str(&output).map_err(|error| format!("wallet key-create output: {error}"))
}

async fn connect_wallet(config: &RelayConfig) -> Result<WalletClient, String> {
    let mut wallet = WalletClient::connect(&config.wallet_url).map_err(|e| e.to_string())?;
    wallet.auth_none_admin().await.map_err(|e| e.to_string())?;
    Ok(wallet)
}

async fn relay_funding_account(
    wallet: &WalletClient,
    config: &RelayConfig,
) -> Result<ghostline_net::AccountWithAddress, String> {
    let account = wallet
        .account_get(&config.opener_account)
        .await
        .map_err(|error| error.to_string())?;
    let view_keys = wallet
        .keys_list(ghostline_net::VIEW_ONLY_KEY_BRANCH)
        .await
        .map_err(|error| error.to_string())?;
    validate_relay_claim_key(&account, &view_keys, &config.claim_secret)?;
    Ok(account)
}

fn validate_relay_claim_key(
    account: &ghostline_net::AccountWithAddress,
    view_keys: &[serde_json::Value],
    claim_secret: &str,
) -> Result<(), String> {
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    let owner = account
        .account
        .owner_key_id
        .as_ref()
        .and_then(|key| key.get("Derived"))
        .ok_or("relay opener must have a derived account key")?;
    if owner.get("key_branch").and_then(serde_json::Value::as_str)
        != Some(ghostline_net::ACCOUNT_KEY_BRANCH)
    {
        return Err("relay opener does not have a derived account signing key".into());
    }
    let index = owner
        .get("index")
        .and_then(serde_json::Value::as_u64)
        .ok_or("relay opener account omitted its key index")?;
    let expected = view_keys.iter().find_map(|row| {
        let key = row.get(0)?.get("Derived")?;
        (key.get("key_branch")?.as_str()? == ghostline_net::VIEW_ONLY_KEY_BRANCH
            && key.get("index")?.as_u64()? == index)
            .then(|| row.get(1)?.as_str()).flatten()
    }).ok_or_else(|| format!("relay operator must derive the opener account's view_only_key at index {index}; no deposit should be sent"))?;
    let secret = RistrettoSecretKey::from_canonical_bytes(
        &from_hex(claim_secret).map_err(|_| "relay claim key is not valid hex")?,
    )
    .map_err(|_| "relay claim key is not a canonical secret key")?;
    let actual = to_hex(RistrettoPublicKey::from_secret_key(&secret).as_bytes());
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "relay operator must correct GHOSTLINE_CLAIM_SECRET: it does not match the opener account's view_only_key at index {index}. No deposit should be sent."
        ));
    }
    Ok(())
}

async fn warn_if_fee_float_low(config: &RelayConfig) {
    let result = async {
        let wallet = connect_wallet(config).await?;
        let response = wallet
            .account_balances(&config.fee_account_name, true)
            .await
            .map_err(|error| error.to_string())?;
        revealed_native_balance(&response)
    }
    .await;

    match result {
        Ok(balance) if balance < FEE_FLOAT_WARNING_THRESHOLD => {
            tracing::warn!(
                event = "relay_fee_float_low",
                revealed_balance_micro_xtr = balance,
                threshold_micro_xtr = FEE_FLOAT_WARNING_THRESHOLD,
                "relay fee account is below the 0.1 XTR safety threshold; top it up from the Esmeralda faucet"
            );
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                event = "relay_fee_float_unavailable",
                error = %error,
                "could not verify the relay fee float"
            );
        }
    }
}

fn revealed_native_balance(response: &serde_json::Value) -> Result<u128, String> {
    let parsed: WalletBalancesResponse = serde_json::from_value(response.clone())
        .map_err(|error| format!("decode walletd account balances: {error}"))?;
    let native_resource = STEALTH_TARI_RESOURCE_ADDRESS.to_string();
    parsed
        .balances
        .iter()
        .filter(|entry| entry.resource_address == native_resource)
        .try_fold(0_u128, |total, entry| {
            let amount = entry
                .balance
                .parse::<u128>()
                .map_err(|error| format!("invalid walletd revealed balance: {error}"))?;
            total
                .checked_add(amount)
                .ok_or_else(|| "walletd revealed balance total overflow".to_owned())
        })
}

async fn run_cli(config: &RelayConfig, args: &[&str]) -> Result<String, String> {
    let cli = config.cli_path.clone();
    let claim_secret = config.claim_secret.clone();
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    tokio::task::spawn_blocking(move || {
        let output = Command::new(cli)
            .args(args)
            .env("GHOSTLINE_CLAIM_SECRET", claim_secret)
            .output()
            .map_err(|error| format!("could not start Ghostline CLI: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Ghostline CLI failed: {}",
                safe_cli_output(&stderr)
            ));
        }
        String::from_utf8(output.stdout).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Clone)]
struct RelayConfig {
    cli_path: PathBuf,
    wallet_url: String,
    indexer_url: String,
    state_dir: PathBuf,
    claim_secret: String,
    recovery_key: [u8; 32],
    template_hex: String,
    network_tag: u8,
    deposit: u64,
    expiry_epochs: u64,
    open_max_fee: u64,
    settle_max_fee: u64,
    refund_max_fee: u64,
    opener_account: String,
    fee_account_name: String,
}

impl RelayConfig {
    fn load() -> Result<Self, String> {
        if !demo_mode() {
            return Err("GHOSTLINE_DEMO_MODE must be 1".into());
        }
        let required = |key: &str| {
            std::env::var(key).map_err(|_| format!("relay is not configured: set {key}"))
        };
        Ok(Self {
            cli_path: std::env::var_os("GHOSTLINE_CLI_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("ghostline")),
            wallet_url: required("GHOSTLINE_WALLET_URL")?,
            indexer_url: required("GHOSTLINE_INDEXER_URL")?,
            state_dir: std::env::var_os("GHOSTLINE_LEDGER_PATH")
                .map(PathBuf::from)
                .ok_or("relay is not configured: set GHOSTLINE_LEDGER_PATH")?,
            claim_secret: required("GHOSTLINE_CLAIM_SECRET")?,
            recovery_key: crate::recovery_key_from_hex(&required("GHOSTLINE_DEMO_STATE_KEY")?)
                .map_err(|error| format!("invalid GHOSTLINE_DEMO_STATE_KEY: {error}"))?,
            template_hex: template_hex_from_env(&required("GHOSTLINE_TEMPLATE")?)?,
            network_tag: std::env::var("GHOSTLINE_NETWORK_TAG")
                .unwrap_or_else(|_| "38".into())
                .parse()
                .map_err(|_| "GHOSTLINE_NETWORK_TAG must be a u8")?,
            deposit: std::env::var("GHOSTLINE_DEMO_DEPOSIT")
                .unwrap_or_else(|_| DEFAULT_DEPOSIT.to_string())
                .parse()
                .map_err(|_| "GHOSTLINE_DEMO_DEPOSIT must be an integer micro-unit amount")?,
            expiry_epochs: required("GHOSTLINE_DEMO_EXPIRY_EPOCHS")?
                .parse()
                .map_err(|_| "GHOSTLINE_DEMO_EXPIRY_EPOCHS must be a positive integer")?,
            open_max_fee: std::env::var("GHOSTLINE_DEMO_OPEN_MAX_FEE")
                .unwrap_or_else(|_| DEFAULT_OPEN_FEE_CAP.to_string())
                .parse()
                .map_err(|_| "GHOSTLINE_DEMO_OPEN_MAX_FEE must be an integer")?,
            settle_max_fee: std::env::var("GHOSTLINE_DEMO_SETTLE_MAX_FEE")
                .unwrap_or_else(|_| DEFAULT_OPEN_FEE_CAP.to_string())
                .parse()
                .map_err(|_| "GHOSTLINE_DEMO_SETTLE_MAX_FEE must be an integer")?,
            refund_max_fee: std::env::var("GHOSTLINE_DEMO_REFUND_MAX_FEE")
                .unwrap_or_else(|_| DEFAULT_OPEN_FEE_CAP.to_string())
                .parse()
                .map_err(|_| "GHOSTLINE_DEMO_REFUND_MAX_FEE must be an integer")?,
            opener_account: required("GHOSTLINE_DEMO_OPENER_ACCOUNT")?,
            fee_account_name: required("GHOSTLINE_DEMO_FEE_ACCOUNT_NAME")?,
        })
    }
}

impl Default for RelayRegistry {
    fn default() -> Self {
        Self(
            Arc::new(Mutex::new(RelayRegistryState::default())),
            Arc::new(Mutex::new(())),
            None,
        )
    }
}

impl AppState {
    pub(super) async fn mint_recovery_receipt(&self, channel_id: &str) -> Result<String, String> {
        let config = RelayConfig::load()?;
        let record = self
            .relay_registry
            .get(channel_id)
            .await
            .ok_or("cannot issue a receipt for an unknown channel")?;
        let response = self
            .relay_registry
            .response_for_channel(channel_id)
            .await
            .ok_or("cannot issue a receipt without the channel's open record")?;
        let directory = self
            .directory
            .as_ref()
            .ok_or("recovery requires the dynamic channel directory")?;
        let id_bytes = from_hex(channel_id)
            .map_err(|error| format!("invalid channel id for recovery: {error}"))?;
        let id: [u8; ghostline_core::CHANNEL_ID_LEN] = id_bytes
            .try_into()
            .map_err(|_| "invalid channel id length for recovery")?;
        let ledger_snapshot = directory
            .lock()
            .await
            .recovery_snapshot(&id)
            .map_err(|error| format!("snapshot channel ledger for recovery: {error}"))?;
        let bundle = RecoveryBundle {
            version: 1,
            channel_id: channel_id.to_owned(),
            response,
            channel_file_hex: read_file_hex(&record.channel_file)?,
            service_signer_file_hex: read_file_hex(&record.service_signer_file)?,
            service_output_signer_file_hex: read_file_hex(&record.service_output_signer_file)?,
            refund_signer_file_hex: read_file_hex(&record.refund_signer_file)?,
            ledger_snapshot_hex: to_hex(&ledger_snapshot),
            record,
        };
        let payload = serde_json::to_vec(&bundle).map_err(|error| error.to_string())?;
        crate::seal_recovery_receipt(&payload, &config.recovery_key)
            .map_err(|error| error.to_string())
    }
}

impl RelayRegistry {
    pub async fn open(path: PathBuf) -> Result<Self, String> {
        let state = if path.exists() {
            let bytes = std::fs::read(&path)
                .map_err(|error| format!("read relay state {}: {error}", path.display()))?;
            serde_json::from_slice(&bytes)
                .map_err(|error| format!("decode relay state {}: {error}", path.display()))?
        } else {
            RelayRegistryState::default()
        };
        let registry = Self(
            Arc::new(Mutex::new(state)),
            Arc::new(Mutex::new(())),
            Some(Arc::new(path)),
        );
        {
            let mut state = registry.0.lock().await;
            // Keep session-to-refund-address bindings for seven days so a finalized transfer
            // can still be returned after a delayed retry or a relay restart.
            state.sessions.retain(|_, session| {
                unix_now().saturating_sub(session.created_at) <= 7 * 24 * 60 * 60
            });
            registry.persist_locked(&state)?;
        }
        Ok(registry)
    }

    fn persist_locked(&self, state: &RelayRegistryState) -> Result<(), String> {
        let Some(path) = &self.2 else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(state).map_err(|error| error.to_string())?;
        ghostline_core::write_private_file(path, &bytes)
            .map_err(|error| format!("write relay state {}: {error}", path.display()))
    }

    pub async fn payment_terms(&self) -> Result<Vec<(String, PaymentTerms)>, String> {
        let state = self.0.lock().await;
        state
            .channels
            .iter()
            .map(|(channel, record)| {
                let service = RistrettoPublicKeyBytes::from_hex(&record.service_public_key)
                    .map_err(|error| format!("invalid saved service key for {channel}: {error}"))?;
                let output = RistrettoPublicKeyBytes::from_hex(&record.service_output_public_key)
                    .map_err(|error| {
                    format!("invalid saved payout key for {channel}: {error}")
                })?;
                let terms = PaymentTerms::new(service, record.expiry_epoch, record.expiry_epoch)
                    .map_err(|error| format!("invalid saved payment terms for {channel}: {error}"))?
                    .with_service_output_key(output);
                Ok((channel.clone(), terms))
            })
            .collect()
    }

    async fn completed_open(&self, tx: &str) -> Option<OpenResponse> {
        self.0.lock().await.completed_funding.get(tx).cloned()
    }

    async fn response_for_channel(&self, channel: &str) -> Option<OpenResponse> {
        self.0
            .lock()
            .await
            .completed_funding
            .values()
            .find(|response| response.channel_id == channel)
            .cloned()
    }

    async fn get(&self, channel: &str) -> Option<RelayRecord> {
        self.0.lock().await.channels.get(channel).cloned()
    }

    pub(crate) async fn settle_pending_on_shutdown(&self) {
        let records = self.0.lock().await.channels.clone();
        for (channel_id, record) in records {
            if record.settlement_tx.is_some()
                || record.refund_tx.is_some()
                || record.channel_refund_tx.is_some()
            {
                continue;
            }
            let has_voucher = match channel_has_pending_voucher(&record.channel_file) {
                Ok(has_voucher) => has_voucher,
                Err(error) => {
                    tracing::error!(
                        event = "shutdown_settlement",
                        channel = %channel_id,
                        outcome = "ledger_unreadable",
                        error = %error
                    );
                    continue;
                }
            };
            if !has_voucher {
                continue;
            }
            match settle_inner(&record).await {
                Ok((transaction_id, state)) => {
                    if let Err(error) = self.settle(&channel_id, transaction_id.clone()).await {
                        tracing::error!(
                            event = "shutdown_settlement",
                            channel = %channel_id,
                            transaction_id = %transaction_id,
                            outcome = "registry_persist_failed",
                            error = %error
                        );
                    } else {
                        tracing::warn!(
                            event = "shutdown_settlement",
                            channel = %channel_id,
                            transaction_id = %transaction_id,
                            state,
                            outcome = "submitted"
                        );
                    }
                }
                Err(error) => tracing::error!(
                    event = "shutdown_settlement",
                    channel = %channel_id,
                    outcome = "failed",
                    error = %error
                ),
            }
        }
    }

    async fn insert(&self, channel: String, record: RelayRecord) -> Result<(), String> {
        let mut state = self.0.lock().await;
        state.channels.insert(channel, record);
        self.persist_locked(&state)
    }

    async fn settle(&self, channel: &str, tx: String) -> Result<(), String> {
        let mut state = self.0.lock().await;
        if let Some(record) = state.channels.get_mut(channel) {
            record.settlement_tx = Some(tx);
        }
        self.persist_locked(&state)
    }

    async fn set_channel_refund(&self, channel: &str, tx: String) -> Result<(), String> {
        let mut state = self.0.lock().await;
        if let Some(record) = state.channels.get_mut(channel) {
            record.channel_refund_tx = Some(tx);
        }
        self.persist_locked(&state)
    }

    async fn refund(&self, channel: &str, tx: String) -> Result<(), String> {
        let mut state = self.0.lock().await;
        if let Some(record) = state.channels.get_mut(channel) {
            record.refund_tx = Some(tx);
        }
        self.persist_locked(&state)
    }

    async fn has_funding(&self, tx: &str) -> bool {
        self.0.lock().await.funding_ids.contains(tx)
    }

    async fn is_opening(&self, tx: &str) -> bool {
        self.0.lock().await.opening_funding.contains(tx)
    }

    async fn open_attempt(&self, tx: &str) -> Option<OpenAttempt> {
        self.0.lock().await.open_attempts.get(tx).cloned()
    }

    async fn save_open_attempt(&self, tx: &str, attempt: OpenAttempt) -> Result<(), String> {
        let mut state = self.0.lock().await;
        if state.funding_refunds.contains_key(tx) || state.refunding_funding.contains(tx) {
            return Err(
                "this funding transaction is being refunded or has already been refunded".into(),
            );
        }
        state.open_attempts.insert(tx.to_owned(), attempt);
        self.persist_locked(&state)
    }

    async fn release_opening(&self, tx: &str) -> Result<(), String> {
        let mut state = self.0.lock().await;
        state.opening_funding.remove(tx);
        self.persist_locked(&state)
    }

    async fn mark_opening(&self, tx: &str) -> Result<(), String> {
        let mut state = self.0.lock().await;
        if state.funding_refunds.contains_key(tx) || state.refunding_funding.contains(tx) {
            return Err(
                "this funding transaction is being refunded or has already been refunded".into(),
            );
        }
        state.opening_funding.insert(tx.to_owned());
        self.persist_locked(&state)
    }

    async fn funding_refund(&self, tx: &str) -> Option<String> {
        self.0.lock().await.funding_refunds.get(tx).cloned()
    }

    async fn is_refunding_funding(&self, tx: &str) -> bool {
        self.0.lock().await.refunding_funding.contains(tx)
    }

    async fn mark_funding_refunding(&self, tx: &str) -> Result<(), String> {
        let mut state = self.0.lock().await;
        if state.opening_funding.contains(tx)
            || state.completed_funding.contains_key(tx)
            || state.funding_refunds.contains_key(tx)
        {
            return Err("cannot refund funding already used to open a channel".into());
        }
        state.refunding_funding.insert(tx.to_owned());
        self.persist_locked(&state)
    }

    async fn mark_funding_refund(&self, tx: &str, refund_tx: String) -> Result<(), String> {
        let mut state = self.0.lock().await;
        if state.opening_funding.contains(tx) || state.completed_funding.contains_key(tx) {
            return Err("cannot refund funding already used to open a channel".into());
        }
        state.funding_ids.insert(tx.to_owned());
        state.refunding_funding.remove(tx);
        state.funding_refunds.insert(tx.to_owned(), refund_tx);
        self.persist_locked(&state)
    }

    async fn mark_funding(&self, tx: &str, response: OpenResponse) -> Result<(), String> {
        let mut state = self.0.lock().await;
        state.funding_ids.insert(tx.to_owned());
        state.opening_funding.remove(tx);
        state.completed_funding.insert(tx.to_owned(), response);
        self.persist_locked(&state)
    }

    async fn create_session(&self, id: String, session: RelaySession) -> Result<(), String> {
        let mut state = self.0.lock().await;
        state
            .sessions
            .retain(|_, session| unix_now().saturating_sub(session.created_at) <= 7 * 24 * 60 * 60);
        state.sessions.insert(id, session);
        self.persist_locked(&state)
    }

    async fn session(&self, id: &str) -> Option<RelaySession> {
        self.0.lock().await.sessions.get(id).cloned()
    }

    async fn remove_session(&self, id: &str) -> Result<(), String> {
        let mut state = self.0.lock().await;
        state.sessions.remove(id);
        self.persist_locked(&state)
    }
}

fn channel_has_pending_voucher(path: &Path) -> Result<bool, String> {
    ChannelStore::open(path)
        .map(|store| store.latest_voucher_hex().is_some())
        .map_err(|error| error.to_string())
}

fn demo_mode() -> bool {
    std::env::var("GHOSTLINE_DEMO_MODE").is_ok_and(|value| value == "1")
}

fn template_hex_from_env(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("template_")
        .or_else(|| trimmed.strip_prefix("Template_"))
        .unwrap_or(trimmed);
    let bytes = from_hex(hex).map_err(|_| {
        "GHOSTLINE_TEMPLATE must be 32-byte hex, optionally prefixed with template_".to_owned()
    })?;
    if bytes.len() != 32 {
        return Err(
            "GHOSTLINE_TEMPLATE must be 32-byte hex, optionally prefixed with template_".into(),
        );
    }
    Ok(to_hex(&bytes))
}

fn validate_demo_deposit(deposit: u64) -> Result<(), &'static str> {
    if deposit == DEFAULT_DEPOSIT {
        Ok(())
    } else {
        Err(
            "the current five-call demo requires a 51 µT deposit (50 µT spend plus 1 µT fixed-shape change)",
        )
    }
}

fn random_nonce_hex() -> Result<String, String> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce).map_err(|error| format!("secure random failed: {error}"))?;
    Ok(to_hex(&nonce))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn safe_cli_output(value: &str) -> String {
    value.trim().chars().take(1000).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_call_demo_requires_only_the_minimum_change_remainder() {
        assert!(validate_demo_deposit(51).is_ok());
        assert!(validate_demo_deposit(50).is_err());
        assert!(validate_demo_deposit(1_000).is_err());
    }

    #[test]
    fn fee_float_reads_only_revealed_native_tari_balance() {
        let response = serde_json::json!({
            "balances": [
                {
                    "resource_address": STEALTH_TARI_RESOURCE_ADDRESS.to_string(),
                    "balance": "99999",
                    "confidential_balance": "900000000000000000000"
                },
                {
                    "resource_address": "resource_other",
                    "balance": "700000"
                }
            ]
        });

        assert_eq!(revealed_native_balance(&response).unwrap(), 99_999);
    }

    #[test]
    fn fee_float_aggregates_native_vaults_and_rejects_bad_amounts() {
        let response = serde_json::json!({
            "balances": [
                { "resource_address": STEALTH_TARI_RESOURCE_ADDRESS.to_string(), "balance": "50000" },
                { "resource_address": STEALTH_TARI_RESOURCE_ADDRESS.to_string(), "balance": "50000" }
            ]
        });
        assert_eq!(
            revealed_native_balance(&response).unwrap(),
            FEE_FLOAT_WARNING_THRESHOLD
        );

        let malformed = serde_json::json!({
            "balances": [
                { "resource_address": STEALTH_TARI_RESOURCE_ADDRESS.to_string(), "balance": "not-an-integer" }
            ]
        });
        assert!(revealed_native_balance(&malformed).is_err());
    }

    #[test]
    fn shutdown_settlement_skips_channels_without_an_accepted_voucher() {
        let path = std::env::temp_dir().join(format!(
            "ghostline-empty-shutdown-ledger-{}-{}.json",
            std::process::id(),
            unix_now()
        ));
        let params = ghostline_core::ChannelParams {
            network_tag: 38,
            payer: RistrettoPublicKeyBytes::from_bytes(&[3u8; 32]).unwrap(),
            deposit: DEFAULT_DEPOSIT,
            payer_root: tari_template_lib::types::Hash32::from_array([7u8; 32]),
            nonce: [8u8; ghostline_core::NONCE_LEN],
        };
        ChannelStore::create(&path, params).unwrap();
        assert!(!channel_has_pending_voucher(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn a_funding_transaction_retry_returns_the_same_open_response() {
        let registry = RelayRegistry::default();
        let response = OpenResponse {
            channel_id: "channel".into(),
            payer_root: "root".into(),
            nonce: "nonce".into(),
            network_tag: 38,
            deposit: DEFAULT_DEPOSIT,
            expiry_epoch: 42,
            service_public_key: "service".into(),
            service_output_public_key: "payout".into(),
            funding_transaction_id: "funding".into(),
            open_transaction_id: "open".into(),
            custody: "test custody".into(),
            recovery_receipt: None,
        };
        registry
            .mark_funding("funding", response.clone())
            .await
            .unwrap();

        assert_eq!(
            registry.completed_open("funding").await.unwrap().channel_id,
            response.channel_id
        );
        assert!(registry.has_funding("funding").await);
    }

    #[test]
    fn relay_normalizes_template_addresses_from_env() {
        let hex = "61bad5eb1b599ba6351e1573958fb84c4aeb4dd47415e60c0b44b1c50d7157a1";
        assert_eq!(template_hex_from_env(hex).unwrap(), hex);
        assert_eq!(
            template_hex_from_env(&format!("template_{hex}")).unwrap(),
            hex
        );
        assert!(template_hex_from_env("template_test").is_err());
        assert!(template_hex_from_env("").is_err());
    }

    #[test]
    fn funding_output_validation_accepts_split_outputs_but_requires_exact_total() {
        assert_eq!(funding_output_total(&[20, 31], 51), Ok(51));
        assert!(funding_output_total(&[], 51).is_err());
        assert!(funding_output_total(&[50], 51).is_err());
        assert!(funding_output_total(&[51, 1], 51).is_err());
        assert!(funding_output_total(&[u64::MAX, 1], 51).is_err());
    }

    #[test]
    fn funding_memo_binds_a_transfer_to_one_session() {
        let memo =
            tari_ootle_wallet_crypto::memo::Memo::new_message("Ghostline:session-a").unwrap();
        assert!(funding_memo_matches_session(Some(&memo), "session-a"));
        assert!(!funding_memo_matches_session(Some(&memo), "session-b"));
        assert!(!funding_memo_matches_session(None, "session-a"));
        let session = "ab".repeat(32);
        let payer = "cd".repeat(32);
        let v2 = funding_memo_for_binding(&session, &payer, "otl_esm_1refundaddr").unwrap();
        let parsed = parse_funding_memo_message(&v2).unwrap();
        assert_eq!(parsed.session_id, session);
        assert_eq!(parsed.payer_public_key.as_deref(), Some(payer.as_str()));
        assert_eq!(
            parsed.refund_address.as_deref(),
            Some("otl_esm_1refundaddr")
        );
        let v2_memo = tari_ootle_wallet_crypto::memo::Memo::new_message(v2).unwrap();
        assert!(funding_memo_matches_session(Some(&v2_memo), &session));
        assert!(!funding_memo_matches_session(Some(&v2_memo), "session-a"));
        assert!(parse_funding_memo_message("Ghostline:").is_none());
    }

    #[test]
    fn indexer_404_is_a_missing_local_record_not_a_dropped_deposit() {
        assert!(indexer_lookup_missing(
            &"indexer rejected transactions/abc: HTTP 404: {\"error\":\"Transaction abc not found\"}"
        ));
        assert!(!indexer_lookup_missing(
            &"could not confirm Sapient funding finality through the Esmeralda indexer"
        ));
    }

    #[test]
    fn wallet_listed_unspent_deposit_matches_the_saved_session() {
        let session = "ab".repeat(32);
        let payer = "cd".repeat(32);
        let memo = funding_memo_for_binding(&session, &payer, "otl_esm_1refundaddr").unwrap();
        let utxos = vec![
            serde_json::json!({
                "status": "Spent",
                "value": "51",
                "memo": {"Message": memo},
            }),
            serde_json::json!({
                "status": "Unspent",
                "value": "51",
                "memo": {"Message": memo},
            }),
            serde_json::json!({
                "status": "Unspent",
                "value": "51",
                "memo": {"Message": "Ghostline:other-session"},
            }),
        ];
        let verified = verify_funding_from_listed_utxos(&utxos, 51, &session).unwrap();
        assert_eq!(verified.amount, 51);
        assert_eq!(verified.binding.session_id, session);
        assert_eq!(
            verified.binding.payer_public_key.as_deref(),
            Some(payer.as_str())
        );
        assert!(verify_funding_from_listed_utxos(&utxos[..1], 51, &session).is_err());
    }

    #[test]
    fn funding_preflight_requires_the_configured_opener_accounts_view_key() {
        use tari_ootle_common_types::crypto::create_key_pair_from_seed;
        let (other_secret, other_public) = create_key_pair_from_seed(37);
        let (opener_secret, opener_public) = create_key_pair_from_seed(41);
        let account = ghostline_net::AccountWithAddress {
            account: ghostline_net::WalletAccount {
                name: Some("relay-opener".into()),
                component_address: "component_test".into(),
                owner_key_id: Some(ghostline_net::derived_account_key_id(2)),
                is_default: true,
            },
            address: "otl_esm_test".into(),
        };
        let view_keys = vec![
            serde_json::json!([{"Derived":{"key_branch":"view_only_key","index":0}},to_hex(other_public.as_bytes()),true]),
            serde_json::json!([{"Derived":{"key_branch":"view_only_key","index":2}},to_hex(opener_public.as_bytes()),false]),
        ];
        assert!(
            validate_relay_claim_key(&account, &view_keys, &to_hex(opener_secret.as_bytes()))
                .is_ok()
        );
        assert!(
            validate_relay_claim_key(&account, &view_keys, &to_hex(other_secret.as_bytes()))
                .unwrap_err()
                .contains("view_only_key at index 2")
        );
        assert!(
            validate_relay_claim_key(&account, &view_keys[..1], &to_hex(opener_secret.as_bytes()))
                .unwrap_err()
                .contains("must derive")
        );
    }

    #[test]
    fn relay_decrypts_a_dh_encrypted_funding_memo_and_rejects_a_different_session() {
        use ghostline_sdk::{MaskAndValue, OutputScan};
        use ootle_byte_type::ToByteType;
        use tari_ootle_common_types::crypto::create_key_pair_from_seed;
        use tari_ootle_wallet_crypto::{StealthCryptoApi, memo::Memo};

        let (view_secret, view_public) = create_key_pair_from_seed(41);
        let (sender_secret, sender_public) = create_key_pair_from_seed(43);
        let (mask, _) = create_key_pair_from_seed(47);
        let scan = OutputScan::esmeralda(STEALTH_TARI_RESOURCE_ADDRESS);
        let crypto = StealthCryptoApi::new();
        let memo = Memo::new_message("Ghostline:session-a").unwrap();
        let encrypted = crypto
            .encrypt_value_and_mask(51, &mask, &view_public, &sender_secret, Some(&memo))
            .unwrap();
        let commitment: PedersenCommitmentBytes =
            MaskAndValue::new(51, mask).to_commitment().to_byte_type();
        let tag = crypto.derive_stealth_output_tag(
            scan.network,
            &view_secret,
            &sender_public,
            &scan.resource,
        );
        let output = ghostline_net::WalletStealthUtxo {
            commitment: to_hex(commitment.as_bytes()),
            nonce: to_hex(sender_public.as_bytes()),
            encrypted_hex: to_hex(encrypted.as_bytes()),
            tag: i64::from(tag.value()),
            auth: "Key".into(),
            resource: Some(scan.resource.to_string()),
            from_fee_instruction: false,
        };
        let config = RelayConfig {
            cli_path: PathBuf::new(),
            wallet_url: String::new(),
            indexer_url: String::new(),
            state_dir: PathBuf::new(),
            claim_secret: to_hex(view_secret.as_bytes()),
            recovery_key: [0u8; 32],
            template_hex: String::new(),
            network_tag: 38,
            deposit: DEFAULT_DEPOSIT,
            expiry_epochs: 10,
            open_max_fee: 0,
            settle_max_fee: 0,
            refund_max_fee: 0,
            opener_account: String::new(),
            fee_account_name: String::new(),
        };
        assert_eq!(
            unblind_wallet_output(&output, &config, "session-a")
                .unwrap()
                .recovered
                .value,
            51
        );
        assert!(
            unblind_wallet_output(&output, &config, "session-b")
                .unwrap_err()
                .contains("not bound to this relay session")
        );
    }

    #[test]
    fn relay_recovers_payer_and_refund_from_a_v2_funding_memo() {
        use ghostline_sdk::{MaskAndValue, OutputScan};
        use ootle_byte_type::ToByteType;
        use tari_ootle_common_types::crypto::create_key_pair_from_seed;
        use tari_ootle_wallet_crypto::{StealthCryptoApi, memo::Memo};

        let (view_secret, view_public) = create_key_pair_from_seed(41);
        let (sender_secret, sender_public) = create_key_pair_from_seed(43);
        let (mask, _) = create_key_pair_from_seed(47);
        let scan = OutputScan::esmeralda(STEALTH_TARI_RESOURCE_ADDRESS);
        let session = "ab".repeat(32);
        let payer = "cd".repeat(32);
        let memo_text = funding_memo_for_binding(&session, &payer, "otl_esm_1refundaddr").unwrap();
        let crypto = StealthCryptoApi::new();
        let memo = Memo::new_message(memo_text).unwrap();
        let encrypted = crypto
            .encrypt_value_and_mask(51, &mask, &view_public, &sender_secret, Some(&memo))
            .unwrap();
        let commitment: PedersenCommitmentBytes =
            MaskAndValue::new(51, mask).to_commitment().to_byte_type();
        let tag = crypto.derive_stealth_output_tag(
            scan.network,
            &view_secret,
            &sender_public,
            &scan.resource,
        );
        let output = ghostline_net::WalletStealthUtxo {
            commitment: to_hex(commitment.as_bytes()),
            nonce: to_hex(sender_public.as_bytes()),
            encrypted_hex: to_hex(encrypted.as_bytes()),
            tag: i64::from(tag.value()),
            auth: "Key".into(),
            resource: Some(scan.resource.to_string()),
            from_fee_instruction: false,
        };
        let config = RelayConfig {
            cli_path: PathBuf::new(),
            wallet_url: String::new(),
            indexer_url: String::new(),
            state_dir: PathBuf::new(),
            claim_secret: to_hex(view_secret.as_bytes()),
            recovery_key: [0u8; 32],
            template_hex: String::new(),
            network_tag: 38,
            deposit: DEFAULT_DEPOSIT,
            expiry_epochs: 10,
            open_max_fee: 0,
            settle_max_fee: 0,
            refund_max_fee: 0,
            opener_account: String::new(),
            fee_account_name: String::new(),
        };
        let recovered = unblind_wallet_output(&output, &config, &session).unwrap();
        assert_eq!(recovered.recovered.value, 51);
        assert_eq!(
            recovered.binding.payer_public_key.as_deref(),
            Some(payer.as_str())
        );
        assert_eq!(
            recovered.binding.refund_address.as_deref(),
            Some("otl_esm_1refundaddr")
        );
        assert!(unblind_wallet_output(&output, &config, "session-a").is_err());
    }

    #[tokio::test]
    async fn funding_refund_idempotency_state_survives_restart() {
        let root = std::env::temp_dir().join(format!(
            "ghostline-refund-recovery-{}-{}",
            std::process::id(),
            unix_now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state_path = root.join("relay-state.json");
        let registry = RelayRegistry::open(state_path.clone()).await.unwrap();
        registry
            .mark_funding_refund("funding", "refund-tx".into())
            .await
            .unwrap();
        let restored = RelayRegistry::open(state_path.clone()).await.unwrap();
        assert_eq!(
            restored.funding_refund("funding").await.as_deref(),
            Some("refund-tx")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn pending_funding_refund_intent_survives_restart_and_blocks_open() {
        let root = std::env::temp_dir().join(format!(
            "ghostline-refund-intent-{}-{}",
            std::process::id(),
            unix_now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state_path = root.join("relay-state.json");
        let registry = RelayRegistry::open(state_path.clone()).await.unwrap();
        registry.mark_funding_refunding("funding").await.unwrap();

        let restored = RelayRegistry::open(state_path.clone()).await.unwrap();
        assert!(restored.is_refunding_funding("funding").await);
        assert!(restored.mark_opening("funding").await.is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn opening_intent_blocks_refund_before_on_chain_open_finishes() {
        let registry = RelayRegistry::default();
        registry.mark_opening("funding").await.unwrap();
        assert!(registry.is_opening("funding").await);
        assert!(
            registry
                .mark_funding_refund("funding", "refund-tx".into())
                .await
                .is_err()
        );
    }

    #[test]
    fn open_rejection_reason_keeps_the_indexer_text() {
        let entry = serde_json::json!({
            "transaction": {
                "summary": null,
                "rejected_reason": "Failed to lock inputs: output already exists"
            }
        });
        assert_eq!(
            open_rejection_reason(&entry),
            "Failed to lock inputs: output already exists"
        );
    }

    #[tokio::test]
    async fn open_attempt_survives_restart() {
        let root = std::env::temp_dir().join(format!(
            "ghostline-open-attempt-{}-{}",
            std::process::id(),
            unix_now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state_path = root.join("relay-state.json");
        let registry = RelayRegistry::open(state_path.clone()).await.unwrap();
        let attempt = OpenAttempt {
            channel_file: root.join("channel.json"),
            ledger_file: root.join("channel.ledger.json"),
            service_signer_file: root.join("service.json"),
            service_output_signer_file: root.join("output.json"),
            refund_signer_file: root.join("refund.json"),
            service_public_key: "aa".into(),
            service_output_public_key: "bb".into(),
            refund_public_key: "cc".into(),
            refund_address: "otl_esm_1refund".into(),
            expiry_epoch: 9,
            payer_root: "dd".into(),
            nonce: "ee".into(),
        };
        registry
            .save_open_attempt("funding", attempt.clone())
            .await
            .unwrap();
        registry.mark_opening("funding").await.unwrap();
        let restored = RelayRegistry::open(state_path).await.unwrap();
        let loaded = restored.open_attempt("funding").await.unwrap();
        assert_eq!(loaded.channel_file, attempt.channel_file);
        assert_eq!(loaded.payer_root, attempt.payer_root);
        assert!(restored.is_opening("funding").await);
        restored.release_opening("funding").await.unwrap();
        assert!(!restored.is_opening("funding").await);
        assert!(restored.open_attempt("funding").await.is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn relay_state_restores_sessions_channels_and_idempotency_after_restart() {
        let root = std::env::temp_dir().join(format!(
            "ghostline-relay-recovery-{}-{}",
            std::process::id(),
            unix_now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state_path = root.join("relay-state.json");
        let channel_file = root.join("channel.json");
        let registry = RelayRegistry::open(state_path.clone()).await.unwrap();
        registry
            .create_session(
                "session".into(),
                RelaySession {
                    payer_public_key: to_hex(&[3u8; 32]),
                    refund_address: "otl_esm_1test".into(),
                    created_at: unix_now(),
                },
            )
            .await
            .unwrap();
        registry
            .insert(
                "channel".into(),
                RelayRecord {
                    channel_file: channel_file.clone(),
                    service_signer_file: root.join("service-key.json"),
                    service_output_signer_file: root.join("service-output-key.json"),
                    refund_signer_file: root.join("refund-key.json"),
                    refund_public_key: to_hex(&[4u8; 32]),
                    service_public_key: to_hex(&[5u8; 32]),
                    service_output_public_key: to_hex(&[6u8; 32]),
                    refund_address: "otl_esm_1test".into(),
                    settlement_tx: None,
                    channel_refund_tx: None,
                    refund_tx: None,
                    expiry_epoch: 42,
                },
            )
            .await
            .unwrap();
        let response = OpenResponse {
            channel_id: "channel".into(),
            payer_root: "root".into(),
            nonce: "nonce".into(),
            network_tag: 38,
            deposit: DEFAULT_DEPOSIT,
            expiry_epoch: 42,
            service_public_key: to_hex(&[5u8; 32]),
            service_output_public_key: to_hex(&[6u8; 32]),
            funding_transaction_id: "funding".into(),
            open_transaction_id: "open".into(),
            custody: "test custody".into(),
            recovery_receipt: None,
        };
        registry
            .mark_funding("funding", response.clone())
            .await
            .unwrap();

        let restored = RelayRegistry::open(state_path.clone()).await.unwrap();
        assert!(restored.session("session").await.is_some());
        assert!(restored.has_funding("funding").await);
        assert_eq!(
            restored
                .completed_open("funding")
                .await
                .unwrap()
                .open_transaction_id,
            response.open_transaction_id
        );
        assert_eq!(restored.get("channel").await.unwrap().expiry_epoch, 42);
        assert_eq!(restored.payment_terms().await.unwrap().len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn encrypted_recovery_bundle_rebuilds_channel_registry_and_ledger() {
        let root = std::env::temp_dir().join(format!(
            "ghostline-relay-receipt-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let source = root.join("source");
        let restored = root.join("restored");
        std::fs::create_dir_all(source.join("channels")).unwrap();
        std::fs::create_dir_all(restored.join("channels")).unwrap();

        let params = ghostline_core::ChannelParams {
            network_tag: 38,
            payer: RistrettoPublicKeyBytes::from_bytes(&[3u8; 32]).unwrap(),
            deposit: DEFAULT_DEPOSIT,
            payer_root: tari_template_lib::types::Hash32::from_array([7u8; 32]),
            nonce: [8u8; ghostline_core::NONCE_LEN],
        };
        let channel_id = to_hex(&params.channel_id());
        let channel_file_contents = br#"{"service_hex":"0505050505050505050505050505050505050505050505050505050505050505","refund_hex":"0404040404040404040404040404040404040404040404040404040404040404","expiry_epoch":42}"#;
        let ledger_file = source.join("channels/source-ledger.json");
        let store = ChannelStore::create(&ledger_file, params.clone()).unwrap();
        let ledger_snapshot_hex = to_hex(&store.recovery_snapshot().unwrap());
        let service_public_key = to_hex(&[5u8; 32]);
        let service_output_public_key = to_hex(&[6u8; 32]);
        let refund_public_key = to_hex(&[4u8; 32]);
        let response = OpenResponse {
            channel_id: channel_id.clone(),
            payer_root: to_hex(params.payer_root.as_slice()),
            nonce: to_hex(&params.nonce),
            network_tag: params.network_tag,
            deposit: params.deposit,
            expiry_epoch: 42,
            service_public_key: service_public_key.clone(),
            service_output_public_key: service_output_public_key.clone(),
            funding_transaction_id: "funding-id".into(),
            open_transaction_id: "open-id".into(),
            custody: "test custody".into(),
            recovery_receipt: None,
        };
        let bundle = RecoveryBundle {
            version: 1,
            channel_id: channel_id.clone(),
            response: response.clone(),
            record: RelayRecord {
                channel_file: source.join("channels/source.json"),
                service_signer_file: source.join("channels/service.json"),
                service_output_signer_file: source.join("channels/payout.json"),
                refund_signer_file: source.join("channels/refund.json"),
                refund_public_key,
                service_public_key,
                service_output_public_key,
                refund_address: "otl_esm_1test".into(),
                settlement_tx: None,
                channel_refund_tx: None,
                refund_tx: None,
                expiry_epoch: 42,
            },
            channel_file_hex: to_hex(channel_file_contents),
            service_signer_file_hex: to_hex(br#"{"key":"service"}"#),
            service_output_signer_file_hex: to_hex(br#"{"key":"payout"}"#),
            refund_signer_file_hex: to_hex(br#"{"key":"refund"}"#),
            ledger_snapshot_hex,
        };
        let state = AppState::with_directory(
            ghostline_paywall::ChannelDirectory::open(restored.join("channels")).unwrap(),
            PRICE_PER_CALL,
        )
        .unwrap();
        let config = RelayConfig {
            cli_path: PathBuf::new(),
            wallet_url: String::new(),
            indexer_url: String::new(),
            state_dir: restored.clone(),
            claim_secret: String::new(),
            recovery_key: [0u8; 32],
            template_hex: String::new(),
            network_tag: 38,
            deposit: DEFAULT_DEPOSIT,
            expiry_epochs: 10,
            open_max_fee: 0,
            settle_max_fee: 0,
            refund_max_fee: 0,
            opener_account: String::new(),
            fee_account_name: String::new(),
        };

        let recovered = restore_bundle(&state, &config, bundle).await.unwrap();
        assert_eq!(recovered.channel_id, channel_id);
        assert_eq!(
            state
                .relay_registry
                .response_for_channel(&channel_id)
                .await
                .unwrap()
                .open_transaction_id,
            "open-id"
        );
        let id: [u8; ghostline_core::CHANNEL_ID_LEN] =
            from_hex(&channel_id).unwrap().try_into().unwrap();
        let directory = state.directory.as_ref().unwrap().lock().await;
        assert_eq!(directory.len(), 1);
        assert_eq!(directory.accepted(&id).unwrap(), 0);
        assert!(directory.path_for_channel(&channel_id).is_ok());
        drop(directory);
        assert_eq!(
            std::fs::read(restored.join("channels").join(format!("{channel_id}.json"))).unwrap(),
            channel_file_contents
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
