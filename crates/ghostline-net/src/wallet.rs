// SPDX-License-Identifier: MIT

//! JSON-RPC client for a local `tari_walletd`.
//!
//! Method names and request field names come from `tari_ootle_walletd_client` 0.45
//! (`WalletDaemonClient`, `AuthLoginRequest`, `TransactionSubmitRequest`). This crate
//! does not depend on that package: it pulls a newer `tari_ootle_transaction` than
//! the 0.45 engine suite Ghostline pins.
//!
//! Auth here is `credentials: "None"` plus `permissions: ["Admin"]`, which is what
//! a daemon started with `--authentication none` accepts. Other auth methods are
//! not implemented.
//!
//! Submitting Ghostline-sealed envelopes through the public indexer REST API is a
//! different path and remains unverified.

use std::str::FromStr;

use reqwest::{Client, IntoUrl};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use tari_ootle_transaction::{TransactionId, UnsignedTransaction};
use url::Url;

use crate::NetError;

const JSON_RPC: &str = "json_rpc";
const AUTH_REQUEST: &str = "auth.request";
const SUBMIT: &str = "transactions.submit";
const SUBMIT_DRY_RUN: &str = "transactions.submit_dry_run";
const WAIT_RESULT: &str = "transactions.wait_result";
const STEALTH_TRANSFER: &str = "accounts.stealth_transfer";
const GET_DEFAULT: &str = "accounts.get_default";

/// Default `KeyId` JSON for the first derived account key (`tari_ootle_walletd_client` 0.45 `KeyId::Derived`).
pub const DEFAULT_ACCOUNT_SIGNER_JSON: &str = r#"{"Derived":{"key_branch":"account","index":0}}"#;

#[derive(Debug, Clone, Serialize)]
struct RpcRequest<'a, T> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: T,
}

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    message: String,
}

#[derive(Debug, Serialize)]
struct AuthLoginRequest {
    permissions: Vec<&'static str>,
    credentials: &'static str,
}

#[derive(Debug, Deserialize)]
struct AuthLoginResponse {
    token: String,
}

/// Fields from `TransactionSubmitRequest` (`tari_ootle_walletd_client` 0.45).
#[derive(Debug, Clone, Serialize)]
pub struct WalletSubmitRequest {
    pub transaction: UnsignedTransaction,
    pub seal_signer: Value,
    pub other_signers: Vec<Value>,
    pub signatures: Vec<Value>,
    pub detect_inputs: bool,
    pub detect_inputs_use_unversioned: bool,
    pub lock_ids: Vec<u64>,
}

impl WalletSubmitRequest {
    pub fn new(
        transaction: UnsignedTransaction,
        seal_signer: Value,
        other_signers: Vec<Value>,
    ) -> Self {
        Self {
            transaction,
            seal_signer,
            other_signers,
            signatures: Vec::new(),
            detect_inputs: true,
            detect_inputs_use_unversioned: true,
            lock_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WalletSubmitResponse {
    pub transaction_id: TransactionId,
}

#[derive(Debug, Deserialize)]
pub struct WalletDryRunResponse {
    pub transaction_id: TransactionId,
    pub required_fees: u64,
    /// `ExecuteResult` as JSON so this crate does not take an engine-types dependency.
    pub result: Value,
}

#[derive(Debug, Serialize)]
struct WaitRequest {
    transaction_id: TransactionId,
    timeout_secs: Option<u64>,
}

/// `UtxoInputSelection` unit variants as JSON strings (`tari_ootle_walletd_client` 0.45).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UtxoInputSelection {
    PreferConfidential,
    PreferRevealed,
    ConfidentialOnly,
    RevealedOnly,
}

impl FromStr for UtxoInputSelection {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PreferConfidential" => Ok(Self::PreferConfidential),
            "PreferRevealed" => Ok(Self::PreferRevealed),
            "ConfidentialOnly" => Ok(Self::ConfidentialOnly),
            "RevealedOnly" => Ok(Self::RevealedOnly),
            other => Err(format!(
                "input-selection: expected PreferConfidential, PreferRevealed, ConfidentialOnly, or RevealedOnly, got {other}"
            )),
        }
    }
}

/// Fields from `TransferFeeParams` (`tari_ootle_wallet_sdk` / walletd 0.45). Optional swap is omitted.
#[derive(Debug, Clone, Serialize)]
pub struct TransferFeeParams {
    pub input_selection: UtxoInputSelection,
}

/// Fields from `StealthTransferRequest` (`tari_ootle_walletd_client` 0.45).
#[derive(Debug, Clone, Serialize)]
pub struct StealthTransferRequest {
    pub owner_account: Value,
    pub fee_params: TransferFeeParams,
    pub input_selection: UtxoInputSelection,
    pub resource_address: String,
    pub badge_usage: &'static str,
    pub transfers: Vec<StealthTransfer>,
    pub max_fee: u64,
    pub dry_run: bool,
}

/// Fields from `StealthTransfer` (`tari_ootle_walletd_client` 0.45).
#[derive(Debug, Clone, Serialize)]
pub struct StealthTransfer {
    pub destination_address: String,
    pub blinded_output_amount: u64,
    pub revealed_output_amount: u64,
    pub output_memo: Option<Value>,
    pub pay_to: Value,
    pub attach_sender_address: bool,
    pub pay_ref: Option<String>,
}

impl StealthTransferRequest {
    /// One `PayTo::Conditions` output, no badge, fees from revealed UTXOs by default.
    pub fn channel_open(
        account_name: &str,
        destination_address: String,
        resource_address: String,
        deposit: u64,
        pay_to: Value,
        input_selection: UtxoInputSelection,
        fee_input_selection: UtxoInputSelection,
        max_fee: u64,
        dry_run: bool,
    ) -> Self {
        Self {
            owner_account: serde_json::json!({ "Name": account_name }),
            fee_params: TransferFeeParams {
                input_selection: fee_input_selection,
            },
            input_selection,
            resource_address,
            badge_usage: "None",
            transfers: vec![StealthTransfer {
                destination_address,
                blinded_output_amount: deposit,
                revealed_output_amount: 0,
                output_memo: None,
                pay_to,
                attach_sender_address: false,
                pay_ref: None,
            }],
            max_fee,
            dry_run,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AccountDefaultResponse {
    pub address: String,
}

/// JSON-RPC client for a wallet daemon you run yourself.
#[derive(Debug, Clone)]
pub struct WalletClient {
    http: Client,
    endpoint: Url,
    token: Option<String>,
}

impl WalletClient {
    pub fn connect(endpoint: impl IntoUrl) -> Result<Self, NetError> {
        let mut endpoint = endpoint.into_url()?;
        let path = endpoint.path().trim_end_matches('/');
        if path.is_empty() {
            endpoint.set_path(&format!("/{JSON_RPC}"));
        } else if !path.ends_with(JSON_RPC) {
            endpoint.set_path(&format!("{path}/{JSON_RPC}"));
        }
        Ok(Self {
            http: Client::new(),
            endpoint,
            token: None,
        })
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    pub async fn auth_none_admin(&mut self) -> Result<(), NetError> {
        let resp: AuthLoginResponse = self
            .rpc(
                AUTH_REQUEST,
                AuthLoginRequest {
                    permissions: vec!["Admin"],
                    credentials: "None",
                },
            )
            .await?;
        self.token = Some(resp.token);
        Ok(())
    }

    pub async fn submit_dry_run(
        &self,
        request: &WalletSubmitRequest,
    ) -> Result<WalletDryRunResponse, NetError> {
        self.rpc(SUBMIT_DRY_RUN, request).await
    }

    pub async fn submit(
        &self,
        request: &WalletSubmitRequest,
    ) -> Result<WalletSubmitResponse, NetError> {
        self.rpc(SUBMIT, request).await
    }

    pub async fn wait_result(
        &self,
        transaction_id: TransactionId,
        timeout_secs: Option<u64>,
    ) -> Result<Value, NetError> {
        self.rpc(
            WAIT_RESULT,
            WaitRequest {
                transaction_id,
                timeout_secs,
            },
        )
        .await
    }

    pub async fn default_account_address(&self) -> Result<String, NetError> {
        let resp: AccountDefaultResponse = self.rpc(GET_DEFAULT, serde_json::json!({})).await?;
        Ok(resp.address)
    }

    pub async fn stealth_transfer(
        &self,
        request: &StealthTransferRequest,
    ) -> Result<WalletSubmitResponse, NetError> {
        self.rpc(STEALTH_TRANSFER, request).await
    }

    async fn rpc<T: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: T,
    ) -> Result<R, NetError> {
        let mut builder = self.http.post(self.endpoint.clone()).json(&RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method,
            params,
        });
        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(NetError::Rejected {
                path: method.to_string(),
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        let parsed: RpcResponse<R> =
            serde_json::from_slice(&bytes).map_err(|err| NetError::Decode {
                path: method.to_string(),
                details: err.to_string(),
            })?;
        if let Some(err) = parsed.error {
            return Err(NetError::WalletRpc {
                method: method.to_string(),
                message: err.message,
            });
        }
        parsed.result.ok_or_else(|| NetError::Decode {
            path: method.to_string(),
            details: "JSON-RPC response had neither result nor error".into(),
        })
    }
}

/// True when the dry-run / wait payload contains an `Accept` finalize (not `Reject`).
pub fn execution_accepted(result: &Value) -> bool {
    let finalize = result.get("finalize").unwrap_or(result);
    match finalize.get("result") {
        Some(Value::Object(map)) => map.contains_key("Accept") && !map.contains_key("Reject"),
        _ => result.get("Accept").is_some() && result.get("Reject").is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_appends_json_rpc() {
        let client = WalletClient::connect("http://127.0.0.1:5100").unwrap();
        assert_eq!(client.endpoint().as_str(), "http://127.0.0.1:5100/json_rpc");
    }

    #[test]
    fn submit_body_matches_walletd_0_45_field_names() {
        let tx: UnsignedTransaction = serde_json::from_str(
            r#"{"V1":{"network":38,"fee_instructions":[],"instructions":[],"inputs":[],"min_epoch":null,"max_epoch":1,"is_seal_signer_authorized":true,"dry_run":false,"blobs":[],"nonce":0}}"#,
        )
        .unwrap();
        let req = WalletSubmitRequest::new(
            tx,
            serde_json::from_str(r#"{"Imported":{"local_key_id":1}}"#).unwrap(),
            vec![serde_json::from_str(DEFAULT_ACCOUNT_SIGNER_JSON).unwrap()],
        );
        let json = serde_json::to_value(&req).unwrap();
        assert!(json["transaction"]["V1"].is_object());
        assert_eq!(json["seal_signer"]["Imported"]["local_key_id"], 1);
        assert_eq!(json["other_signers"][0]["Derived"]["key_branch"], "account");
        assert_eq!(json["detect_inputs"], true);
        assert_eq!(json["detect_inputs_use_unversioned"], true);
        assert_eq!(json["signatures"], serde_json::json!([]));
        assert_eq!(json["lock_ids"], serde_json::json!([]));
    }

    #[test]
    fn execution_accepted_reads_finalize_accept() {
        let v = serde_json::json!({"finalize":{"result":{"Accept":{}}}});
        assert!(execution_accepted(&v));
        let v = serde_json::json!({"finalize":{"result":{"Reject":"nope"}}});
        assert!(!execution_accepted(&v));
    }

    #[test]
    fn auth_login_serializes_none_credentials() {
        let json = serde_json::to_value(&AuthLoginRequest {
            permissions: vec!["Admin"],
            credentials: "None",
        })
        .unwrap();
        assert_eq!(json["permissions"][0], "Admin");
        assert_eq!(json["credentials"], "None");
    }

    #[test]
    fn stealth_open_body_matches_proven_walletd_shape() {
        let pay_to = serde_json::json!({ "Conditions": [[]] });
        let req = StealthTransferRequest::channel_open(
            "ghostline",
            "otl_esm_example".into(),
            "resource_0101010101010101010101010101010101010101010101010101010101010101".into(),
            1_000_000,
            pay_to.clone(),
            UtxoInputSelection::PreferConfidential,
            UtxoInputSelection::PreferRevealed,
            100_000,
            true,
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["owner_account"]["Name"], "ghostline");
        assert_eq!(json["fee_params"]["input_selection"], "PreferRevealed");
        assert!(json["fee_params"].get("max_fee").is_none());
        assert_eq!(json["input_selection"], "PreferConfidential");
        assert_eq!(json["badge_usage"], "None");
        assert_eq!(json["max_fee"], 100_000);
        assert_eq!(json["dry_run"], true);
        let xfer = &json["transfers"][0];
        assert_eq!(xfer["blinded_output_amount"], 1_000_000);
        assert_eq!(xfer["revealed_output_amount"], 0);
        assert_eq!(xfer["attach_sender_address"], false);
        assert_eq!(xfer["pay_to"], pay_to);
        assert!(xfer["output_memo"].is_null());
        assert!(xfer["pay_ref"].is_null());
    }
}
