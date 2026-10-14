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
//! different path: `resolve-inputs` then `seal-unsigned`. That settle is proven live.
//! A faucet-funded derived fee account (not the opener component) is proven; same-seed
//! accounts remain correlated.

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
const ACCOUNTS_GET: &str = "accounts.get";
const ACCOUNTS_CREATE: &str = "accounts.create";
const TRANSACTIONS_GET: &str = "transactions.get";

/// Default `KeyId` JSON for the first derived account key (`tari_ootle_walletd_client` 0.45 `KeyId::Derived`).
///
/// Do not use this as the settle fee co-signer unless that account is the **service** fee
/// source. Using the opener's derived index 0 links open and settle on the public fee path.
pub const DEFAULT_ACCOUNT_SIGNER_JSON: &str = r#"{"Derived":{"key_branch":"account","index":0}}"#;

/// `KeyId::Derived` for the account branch at `index` (`tari_ootle_wallet_sdk` 0.45).
pub fn derived_account_key_id(index: u64) -> Value {
    serde_json::json!({ "Derived": { "key_branch": "account", "index": index } })
}

/// `ComponentAddressOrName::Name` (`tari_ootle_walletd_client` 0.45).
pub fn account_name(name: &str) -> Value {
    serde_json::json!({ "Name": name })
}

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

/// Fields from `AccountsCreateRequest` (`tari_ootle_walletd_client` 0.45).
#[derive(Debug, Clone, Serialize)]
pub struct AccountsCreateRequest {
    pub account_name: Option<String>,
    pub is_default: Option<bool>,
    pub key_index: Option<u64>,
}

/// Subset of `Account` needed to pay `pay_fee` (`tari_ootle_wallet_sdk` 0.45).
#[derive(Debug, Clone, Deserialize)]
pub struct WalletAccount {
    pub name: Option<String>,
    /// `ComponentAddress` as JSON (string in 0.45 serde).
    pub component_address: Value,
    pub owner_key_id: Option<Value>,
    #[serde(default)]
    pub is_default: bool,
}

impl WalletAccount {
    pub fn component_address_display(&self) -> String {
        json_as_plain(&self.component_address)
    }

    /// `KeyId` that must co-sign `pay_fee_from_component` for this account.
    pub fn fee_signer_key_id(&self) -> Result<&Value, NetError> {
        self.owner_key_id.as_ref().ok_or_else(|| NetError::Decode {
            path: ACCOUNTS_GET.to_string(),
            details: "account has no owner_key_id (view-only); cannot co-sign pay_fee".into(),
        })
    }
}

/// Fields from `AccountGetResponse` / `AccountsCreateResponse` (`tari_ootle_walletd_client` 0.45).
#[derive(Debug, Clone, Deserialize)]
pub struct AccountWithAddress {
    pub account: WalletAccount,
    /// `OotleAddress` as JSON (string or struct depending on serde).
    pub address: Value,
}

impl AccountWithAddress {
    pub fn address_display(&self) -> String {
        json_as_plain(&self.address)
    }
}

fn json_as_plain(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Other-signers for `transactions.submit` of a fee-paying unsigned tx.
///
/// Settle fees should be a service-owned account, not the opener. Passing neither
/// an explicit KeyId nor an account owner key is an error so index 0 is never implied.
pub fn settle_fee_signers(
    explicit: Vec<Value>,
    account_owner_key_id: Option<Value>,
) -> Result<Vec<Value>, NetError> {
    let mut signers = Vec::new();
    if let Some(key_id) = account_owner_key_id {
        signers.push(key_id);
    }
    for key_id in explicit {
        if !signers.contains(&key_id) {
            signers.push(key_id);
        }
    }
    if signers.is_empty() {
        return Err(NetError::Decode {
            path: SUBMIT.to_string(),
            details: "set --fee-account-name (service-owned pay_fee account) or --other-signer KeyId JSON; derived index 0 is not implied".into(),
        });
    }
    Ok(signers)
}

/// Stealth output fields from a wallet-stored transaction, matching `unblind --utxo-json`.
///
/// Shape proven against `tari_walletd` 0.45 `transactions.get` / `transactions.get_all`:
/// `V1.body.transaction.{instructions,fee_instructions}[].StealthTransfer.statement.outputs_statement.outputs[]`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WalletStealthUtxo {
    pub commitment: String,
    pub nonce: String,
    pub encrypted_hex: String,
    pub tag: i64,
    pub auth: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    pub from_fee_instruction: bool,
}

impl WalletStealthUtxo {
    /// JSON object accepted by `ghostline unblind --utxo-json`.
    pub fn unblind_json(&self) -> Value {
        serde_json::json!({
            "commitment": self.commitment,
            "nonce": self.nonce,
            "encrypted_hex": self.encrypted_hex,
            "tag": self.tag,
        })
    }
}

/// Channel deposit: first `Script` output (`PayTo::Conditions` is stored as Script).
///
/// Wallet-built `accounts.stealth_transfer` opens put that output in `fee_instructions`
/// (proven on Esmeralda walletd 0.45). Unsigned settle puts it in body `instructions`.
pub fn channel_open_utxo(utxos: &[WalletStealthUtxo]) -> Option<&WalletStealthUtxo> {
    utxos
        .iter()
        .find(|u| u.auth == "Script")
        .or_else(|| utxos.first())
}

/// Walk a `transactions.get` / `get_all` transaction object (or `{ "transaction": ... }`).
pub fn stealth_utxos_from_wallet_tx(tx: &Value) -> Vec<WalletStealthUtxo> {
    let inner = tx.get("transaction").unwrap_or(tx);
    let body = inner
        .pointer("/V1/body/transaction")
        .or_else(|| inner.pointer("/V1"))
        .unwrap_or(inner);
    let mut out = Vec::new();
    collect_stealth_outputs(body.get("instructions"), false, &mut out);
    collect_stealth_outputs(body.get("fee_instructions"), true, &mut out);
    out
}

fn collect_stealth_outputs(
    instructions: Option<&Value>,
    from_fee: bool,
    out: &mut Vec<WalletStealthUtxo>,
) {
    let Some(Value::Array(list)) = instructions else {
        return;
    };
    for instruction in list {
        let Some(transfer) = instruction.get("StealthTransfer") else {
            continue;
        };
        let resource = transfer
            .pointer("/resource_address_ref/Address")
            .and_then(json_string);
        let Some(Value::Array(outputs)) = transfer.pointer("/statement/outputs_statement/outputs")
        else {
            continue;
        };
        for output in outputs {
            if let Some(utxo) = wallet_stealth_utxo(output, resource.clone(), from_fee) {
                out.push(utxo);
            }
        }
    }
}

fn wallet_stealth_utxo(
    entry: &Value,
    resource: Option<String>,
    from_fee: bool,
) -> Option<WalletStealthUtxo> {
    let output = entry.get("output")?;
    Some(WalletStealthUtxo {
        commitment: json_string(output.get("commitment")?)?,
        nonce: json_string(output.get("sender_public_nonce")?)?,
        encrypted_hex: json_string(output.get("encrypted_data")?)?,
        tag: json_tag(entry.get("tag")?)?,
        auth: json_variant_name(entry.get("auth")?).unwrap_or_else(|| "unknown".into()),
        resource,
        from_fee_instruction: from_fee,
    })
}

fn json_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn json_tag(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_u64().map(|u| u as i64)),
        _ => None,
    }
}

fn json_variant_name(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => map.keys().next().cloned(),
        _ => None,
    }
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

    pub async fn account_get(&self, name: &str) -> Result<AccountWithAddress, NetError> {
        self.rpc(
            ACCOUNTS_GET,
            serde_json::json!({ "name_or_address": account_name(name) }),
        )
        .await
    }

    pub async fn account_create(
        &self,
        request: &AccountsCreateRequest,
    ) -> Result<AccountWithAddress, NetError> {
        self.rpc(ACCOUNTS_CREATE, request).await
    }

    pub async fn stealth_transfer(
        &self,
        request: &StealthTransferRequest,
    ) -> Result<WalletSubmitResponse, NetError> {
        self.rpc(STEALTH_TRANSFER, request).await
    }

    pub async fn transaction_get(&self, transaction_id: TransactionId) -> Result<Value, NetError> {
        self.rpc(
            TRANSACTIONS_GET,
            serde_json::json!({ "transaction_id": transaction_id }),
        )
        .await
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

    #[test]
    fn accounts_create_body_matches_walletd_0_45() {
        let json = serde_json::to_value(&AccountsCreateRequest {
            account_name: Some("ghostline-service".into()),
            is_default: Some(false),
            key_index: Some(1),
        })
        .unwrap();
        assert_eq!(json["account_name"], "ghostline-service");
        assert_eq!(json["is_default"], false);
        assert_eq!(json["key_index"], 1);
    }

    #[test]
    fn accounts_get_name_or_address_is_internally_tagged() {
        let json = serde_json::json!({ "name_or_address": account_name("ghostline-service") });
        assert_eq!(json["name_or_address"]["Name"], "ghostline-service");
    }

    #[test]
    fn derived_account_key_id_matches_key_id_derived() {
        let json = derived_account_key_id(1);
        assert_eq!(json["Derived"]["key_branch"], "account");
        assert_eq!(json["Derived"]["index"], 1);
        assert_eq!(
            serde_json::from_str::<Value>(DEFAULT_ACCOUNT_SIGNER_JSON).unwrap(),
            derived_account_key_id(0)
        );
    }

    #[test]
    fn settle_fee_signers_refuse_implied_opener() {
        assert!(settle_fee_signers(Vec::new(), None).is_err());
        let from_account = settle_fee_signers(Vec::new(), Some(derived_account_key_id(1))).unwrap();
        assert_eq!(from_account, vec![derived_account_key_id(1)]);
        let explicit = settle_fee_signers(vec![derived_account_key_id(1)], None).unwrap();
        assert_eq!(explicit, vec![derived_account_key_id(1)]);
    }

    #[test]
    fn wallet_account_deserializes_owner_key_id() {
        let account: WalletAccount = serde_json::from_value(serde_json::json!({
            "name": "ghostline-service",
            "component_address": "component_00",
            "owner_key_id": { "Derived": { "key_branch": "account", "index": 1 } },
            "is_default": false
        }))
        .unwrap();
        assert_eq!(
            account.fee_signer_key_id().unwrap(),
            &derived_account_key_id(1)
        );
        assert_eq!(account.component_address_display(), "component_00");
    }

    #[test]
    fn stealth_utxos_from_proven_walletd_transaction_shape() {
        let tx = serde_json::json!({
            "transaction": {
                "V1": {
                    "body": {
                        "transaction": {
                            "instructions": [{
                                "StealthTransfer": {
                                    "resource_address_ref": { "Address": "resource_0101" },
                                    "statement": {
                                        "outputs_statement": {
                                            "outputs": [{
                                                "auth": { "Script": "aabb" },
                                                "tag": 454119041,
                                                "output": {
                                                    "commitment": "11",
                                                    "sender_public_nonce": "22",
                                                    "encrypted_data": "33",
                                                    "minimum_value_promise": 0,
                                                    "viewable_balance_proof": null
                                                }
                                            }]
                                        }
                                    }
                                }
                            }],
                            "fee_instructions": [{
                                "StealthTransfer": {
                                    "resource_address_ref": { "Address": "resource_0101" },
                                    "statement": {
                                        "outputs_statement": {
                                            "outputs": [{
                                                "auth": { "Key": "cc" },
                                                "tag": 1,
                                                "output": {
                                                    "commitment": "aa",
                                                    "sender_public_nonce": "bb",
                                                    "encrypted_data": "cc",
                                                    "minimum_value_promise": 0,
                                                    "viewable_balance_proof": null
                                                }
                                            }]
                                        }
                                    }
                                }
                            }]
                        }
                    }
                }
            }
        });
        let utxos = stealth_utxos_from_wallet_tx(&tx);
        assert_eq!(utxos.len(), 2);
        assert_eq!(utxos[0].auth, "Script");
        assert_eq!(utxos[0].tag, 454119041);
        assert_eq!(utxos[0].nonce, "22");
        assert!(!utxos[0].from_fee_instruction);
        assert!(utxos[1].from_fee_instruction);
        let channel = channel_open_utxo(&utxos).unwrap();
        assert_eq!(channel.unblind_json()["commitment"], "11");
        assert_eq!(channel.unblind_json()["encrypted_hex"], "33");
    }

    #[test]
    fn channel_open_utxo_accepts_script_in_fee_instructions() {
        let tx = serde_json::json!({
            "V1": {
                "body": {
                    "transaction": {
                        "instructions": [],
                        "fee_instructions": [{
                            "StealthTransfer": {
                                "resource_address_ref": { "Address": "resource_0101" },
                                "statement": {
                                    "outputs_statement": {
                                        "outputs": [
                                            {
                                                "auth": { "Script": "aa" },
                                                "tag": 7,
                                                "output": {
                                                    "commitment": "c1",
                                                    "sender_public_nonce": "n1",
                                                    "encrypted_data": "e1"
                                                }
                                            },
                                            {
                                                "auth": { "Key": "bb" },
                                                "tag": 8,
                                                "output": {
                                                    "commitment": "c2",
                                                    "sender_public_nonce": "n2",
                                                    "encrypted_data": "e2"
                                                }
                                            }
                                        ]
                                    }
                                }
                            }
                        }]
                    }
                }
            }
        });
        let utxos = stealth_utxos_from_wallet_tx(&tx);
        assert_eq!(utxos.len(), 2);
        assert!(utxos[0].from_fee_instruction);
        let channel = channel_open_utxo(&utxos).unwrap();
        assert_eq!(channel.auth, "Script");
        assert_eq!(channel.commitment, "c1");
    }
}
