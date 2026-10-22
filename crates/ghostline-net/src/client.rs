// SPDX-License-Identifier: MIT

use reqwest::{Client, IntoUrl};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use tari_ootle_transaction::{Epoch, Transaction, TransactionEnvelope, TransactionId};
use url::Url;

use crate::NetError;

/// Paths from `tari_indexer_client` README / `IndexerRestApiClient` (0.46 source).
const WAIT_UNTIL_READY: &str = "wait-until-ready";
const EPOCH_STATS: &str = "epoch-manager/stats";
const TRANSACTIONS: &str = "transactions";
const TRANSACTIONS_DRY_RUN: &str = "transactions/dry-run";
const USER_AGENT: &str = "ghostline/0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubmitTransactionRequest {
    transaction: TransactionEnvelope,
}

#[derive(Debug, Clone, Deserialize)]
struct SubmitTransactionResponse {
    transaction_id: TransactionId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EpochStats {
    pub current_epoch: Epoch,
    #[serde(default)]
    pub consensus_epoch: Option<Epoch>,
}

/// REST client for a Tari Ootle indexer. Does not talk to public networks unless you pass their URL.
#[derive(Debug, Clone)]
pub struct IndexerClient {
    http: Client,
    base: Url,
}

impl IndexerClient {
    pub fn connect(endpoint: impl IntoUrl) -> Result<Self, NetError> {
        let mut base = endpoint.into_url()?;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Ok(Self {
            http: Client::builder().user_agent(USER_AGENT).build()?,
            base,
        })
    }

    pub fn endpoint(&self) -> &Url {
        &self.base
    }

    pub async fn wait_until_ready(&self) -> Result<(), NetError> {
        let response = self.http.get(self.join(WAIT_UNTIL_READY)?).send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(NetError::Rejected {
                path: WAIT_UNTIL_READY.to_string(),
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        Ok(())
    }

    pub async fn epoch_stats(&self) -> Result<EpochStats, NetError> {
        self.get_json(EPOCH_STATS).await
    }

    pub async fn submit(&self, transaction: Transaction) -> Result<TransactionId, NetError> {
        self.post_transaction(TRANSACTIONS, transaction).await
    }

    pub async fn dry_run(&self, transaction: Transaction) -> Result<TransactionId, NetError> {
        self.post_transaction(TRANSACTIONS_DRY_RUN, transaction)
            .await
    }

    /// `GET /transactions/{id}/result` (`tari_indexer_client` 0.45 `get_transaction_result`).
    pub async fn transaction_result(
        &self,
        transaction_id: TransactionId,
    ) -> Result<Value, NetError> {
        let path = format!("{TRANSACTIONS}/{transaction_id}/result");
        let mut url = self.join(&path)?;
        url.query_pairs_mut().append_pair("include_proof", "false");
        self.send_json(self.http.get(url), &path).await
    }

    /// `GET /transactions/{id}` — best-effort stored transaction body from the indexer.
    ///
    /// The transaction result endpoint is authoritative for finality, but does not include
    /// the instruction body needed to verify a confidential funding output's recipient.
    pub async fn transaction(&self, transaction_id: TransactionId) -> Result<Value, NetError> {
        let path = format!("{TRANSACTIONS}/{transaction_id}");
        self.get_json(&path).await
    }

    /// `GET /substates/{id}`. `Down` is a 404 whose body says `(vN) is down`.
    pub async fn substate_presence(&self, substate_id: &str) -> Result<SubstatePresence, NetError> {
        let (version, down, _) = self.substate_lookup(substate_id).await?;
        Ok(if down {
            SubstatePresence::Down { version }
        } else {
            SubstatePresence::Up { version }
        })
    }

    /// `GET /substates/{id}` (`tari_indexer_client` 0.45 `get_substate`).
    pub async fn substate(&self, substate_id: &str) -> Result<Value, NetError> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|()| NetError::Decode {
                path: "substates".into(),
                details: "indexer URL cannot take a path".into(),
            })?
            .pop_if_empty()
            .push("substates")
            .push(substate_id);
        self.send_json(self.http.get(url), "substates").await
    }

    /// Fill `version: null` inputs on unsigned JSON (`V1.inputs`) from `GET /substates/{id}`.
    ///
    /// Ghostline-sealed envelopes leave versions unset; walletd `detect_inputs` fills them
    /// (including the fee account's vault). The indexer submit path does not. Official ootle.ts
    /// `resolveInputs` / `getVaultIdsForAccount` does the same before seal.
    /// Spent ("down") substates 404 with `(vN) is down`; N is still the version to pin.
    pub async fn resolve_unsigned_input_versions(
        &self,
        unsigned: &mut Value,
    ) -> Result<ResolvedInputs, NetError> {
        let Some(inputs) = unsigned_inputs_mut(unsigned) else {
            return Ok(ResolvedInputs::default());
        };
        let mut report = ResolvedInputs::default();
        let mut component_bodies = Vec::new();
        for input in inputs.iter_mut() {
            let id = input
                .get("substate_id")
                .and_then(Value::as_str)
                .ok_or_else(|| NetError::Decode {
                    path: "substates".into(),
                    details: "unsigned input missing substate_id".into(),
                })?
                .to_string();
            let needs_version = input.get("version").map(Value::is_null).unwrap_or(true);
            if needs_version || id.starts_with("component_") {
                let (version, down, body) = self.substate_lookup(&id).await?;
                if needs_version {
                    input["version"] = Value::from(version);
                    report.filled += 1;
                    if down {
                        report.down += 1;
                    }
                }
                if id.starts_with("component_")
                    && let Some(body) = body
                {
                    component_bodies.push(body);
                }
            }
        }
        let existing: std::collections::HashSet<String> = inputs
            .iter()
            .filter_map(|i| {
                i.get("substate_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect();
        let mut to_add = Vec::new();
        for body in &component_bodies {
            for vault in vault_ids_from_substate(body) {
                if !existing.contains(&vault) {
                    to_add.push(vault);
                }
            }
        }
        drop(existing);
        for vault in to_add {
            let (version, down, _) = self.substate_lookup(&vault).await?;
            inputs.push(serde_json::json!({
                "is_write": true,
                "substate_id": vault,
                "version": version,
            }));
            report.filled += 1;
            report.added += 1;
            if down {
                report.down += 1;
            }
        }
        Ok(report)
    }

    async fn substate_lookup(
        &self,
        substate_id: &str,
    ) -> Result<(u32, bool, Option<Value>), NetError> {
        match self.substate(substate_id).await {
            Ok(got) => {
                let version =
                    got.get("version")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| NetError::Decode {
                            path: "substates".into(),
                            details: format!("substate response for {substate_id} had no version"),
                        })?;
                let version = u32::try_from(version).map_err(|_| NetError::Decode {
                    path: "substates".into(),
                    details: format!("substate version {version} does not fit u32"),
                })?;
                Ok((version, false, Some(got)))
            }
            Err(NetError::Rejected {
                status: 404, body, ..
            }) => {
                let version = down_version_from_indexer_body(&body).ok_or(NetError::Rejected {
                    path: "substates".into(),
                    status: 404,
                    body,
                })?;
                Ok((version, true, None))
            }
            Err(err) => Err(err),
        }
    }

    async fn post_transaction(
        &self,
        path: &str,
        transaction: Transaction,
    ) -> Result<TransactionId, NetError> {
        let envelope = TransactionEnvelope::encode(transaction).map_err(|_| NetError::Encode)?;
        let resp: SubmitTransactionResponse = self
            .post_json(
                path,
                &SubmitTransactionRequest {
                    transaction: envelope,
                },
            )
            .await?;
        Ok(resp.transaction_id)
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, NetError> {
        self.send_json(self.http.get(self.join(path)?), path).await
    }

    async fn post_json<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, NetError> {
        self.send_json(self.http.post(self.join(path)?).json(body), path)
            .await
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        path: &str,
    ) -> Result<T, NetError> {
        let response = request.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(NetError::Rejected {
                path: path.to_string(),
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        serde_json::from_slice(&bytes).map_err(|err| NetError::Decode {
            path: path.to_string(),
            details: err.to_string(),
        })
    }

    fn join(&self, path: &str) -> Result<Url, NetError> {
        self.base.join(path).map_err(NetError::Url)
    }
}

/// Whether `GET /substates/{id}` still has the object. `Down` means the indexer 404 said `(vN) is down`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "presence", rename_all = "snake_case")]
pub enum SubstatePresence {
    Up { version: u32 },
    Down { version: u32 },
}

/// How many unsigned inputs got a version, and how many of those were already down (spent).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResolvedInputs {
    pub filled: u32,
    pub down: u32,
    /// Vault (and other dependent) inputs appended from component state.
    pub added: u32,
}

fn unsigned_inputs_mut(unsigned: &mut Value) -> Option<&mut Vec<Value>> {
    unsigned.pointer_mut("/V1/inputs")?.as_array_mut()
}

/// Public indexer 404 body: `Input substate {id} (v0) is down`.
/// `tari_template_lib_types` `BinaryTag::VaultId = 132`. Indexer JSON uses `{ "@cbor": "tag", "tag": 132, "value": { "hex": … } }`.
const VAULT_CBOR_TAG: u64 = 132;

fn vault_ids_from_substate(body: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_vault_ids(body, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_vault_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if map.get("@cbor").and_then(Value::as_str) == Some("tag")
                && map.get("tag").and_then(Value::as_u64) == Some(VAULT_CBOR_TAG)
                && let Some(hex) = value
                    .pointer("/value/hex")
                    .and_then(Value::as_str)
                    .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
            {
                out.push(format!("vault_{hex}"));
            }
            for child in map.values() {
                collect_vault_ids(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_vault_ids(child, out);
            }
        }
        _ => {}
    }
}

fn down_version_from_indexer_body(body: &str) -> Option<u32> {
    if !body.contains("is down") {
        return None;
    }
    let i = body.find("(v")?;
    body[i + 2..].split(')').next()?.parse().ok()
}

/// Classify `GET /transactions/{id}/result` without dumping the execution payload.
pub fn indexer_result_status(body: &Value) -> &str {
    let result = body.get("result").unwrap_or(body);
    if result.get("Pending").is_some() || result.as_str() == Some("Pending") {
        return "Pending";
    }
    if result.get("Rejected").is_some() {
        return "Rejected";
    }
    if result.get("Finalized").is_some() {
        let decision = result.pointer("/Finalized/final_decision");
        if decision.and_then(|d| d.get("Abort")).is_some()
            || decision.and_then(|d| d.as_str()) == Some("Abort")
        {
            return "FinalizedAbort";
        }
        return "Finalized";
    }
    "Unknown"
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_core::NONCE_LEN;
    use ghostline_sdk::{
        Channel, ChannelSpec, MaskAndValue, seal_localnet_stealth_transfer, settle,
    };
    use ootle_byte_type::ToByteType;
    use tari_crypto::keys::SecretKey as _;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;
    use tari_template_lib::types::constants::STEALTH_TARI_RESOURCE_ADDRESS;

    fn sample_tx() -> Transaction {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (_service_secret, service_public) = create_key_pair_from_seed(13);
        let (sealer, _) = create_key_pair_from_seed(17);
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            deposit: 100,
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let voucher = ghostline_core::SignedVoucher::sign(
            &payer_secret,
            channel.params(),
            30,
            channel.spec().service,
        )
        .unwrap();
        let funded = MaskAndValue {
            value: 100,
            mask: tari_crypto::ristretto::RistrettoSecretKey::random(&mut rand::rng()),
        };
        let statement = settle(
            &channel,
            funded,
            &voucher,
            service_public.to_byte_type(),
            ghostline_sdk::OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
        )
        .unwrap();
        seal_localnet_stealth_transfer(STEALTH_TARI_RESOURCE_ADDRESS, statement, Epoch(1), &sealer)
    }

    #[test]
    fn submit_body_is_base64_envelope() {
        let tx = sample_tx();
        let envelope = TransactionEnvelope::encode(tx).unwrap();
        let json = serde_json::to_value(&SubmitTransactionRequest {
            transaction: envelope,
        })
        .unwrap();
        assert!(json["transaction"].as_str().is_some());
    }

    #[test]
    fn epoch_stats_parse_current_epoch_number() {
        let stats: EpochStats =
            serde_json::from_str(r#"{"current_epoch": 3, "current_block_height": 9}"#).unwrap();
        assert_eq!(stats.current_epoch, Epoch(3));
        assert_eq!(stats.consensus_epoch, None);
    }

    #[test]
    fn connect_adds_trailing_slash_so_join_does_not_drop_the_port_path() {
        let client = IndexerClient::connect("http://127.0.0.1:18300").unwrap();
        assert_eq!(client.endpoint().as_str(), "http://127.0.0.1:18300/");
        assert_eq!(
            client.join(TRANSACTIONS).unwrap().as_str(),
            "http://127.0.0.1:18300/transactions"
        );
    }

    #[test]
    fn transaction_id_round_trips_as_hex() {
        let id = TransactionId::new([0xab; 32]);
        let json = serde_json::to_string(&id).unwrap();
        let back: TransactionId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
        assert!(json.contains("abab"));
    }

    #[test]
    fn indexer_result_status_reads_pending_and_finalized() {
        assert_eq!(
            indexer_result_status(&serde_json::json!({"result": "Pending"})),
            "Pending"
        );
        assert_eq!(
            indexer_result_status(&serde_json::json!({"result": {"Pending": null}})),
            "Pending"
        );
        assert_eq!(
            indexer_result_status(
                &serde_json::json!({"result": {"Finalized": {"final_decision": "Commit"}}})
            ),
            "Finalized"
        );
        assert_eq!(
            indexer_result_status(
                &serde_json::json!({"result": {"Rejected": {"details": "nope"}}})
            ),
            "Rejected"
        );
    }

    #[test]
    fn vault_ids_from_account_cbor_tag_132() {
        let resource = "01".repeat(32);
        let vault = "ab".repeat(32);
        let body = serde_json::json!({
            "version": 0,
            "substate": {
                "Component": {
                    "body": {
                        "state": [{
                            "@cbor": "map",
                            "entries": [[
                                {"@cbor":"tag","tag":131,"value":{"@cbor":"bytes","hex": resource}},
                                {"@cbor":"tag","tag":132,"value":{"@cbor":"bytes","hex": vault}}
                            ]]
                        }]
                    }
                }
            }
        });
        let ids = vault_ids_from_substate(&body);
        assert_eq!(ids, vec![format!("vault_{vault}")]);
    }

    #[test]
    fn down_version_parses_indexer_404_body() {
        let body = r#"{"error":"Input substate utxo_ab (v0) is down"}"#;
        assert_eq!(down_version_from_indexer_body(body), Some(0));
        assert_eq!(down_version_from_indexer_body("not found"), None);
    }

    #[test]
    fn unsigned_inputs_mut_finds_v1_inputs() {
        let mut v = serde_json::json!({"V1":{"inputs":[{"substate_id":"utxo_ab","version":null}]}});
        let inputs = unsigned_inputs_mut(&mut v).unwrap();
        assert!(inputs[0]["version"].is_null());
        inputs[0]["version"] = serde_json::json!(3);
        assert_eq!(v["V1"]["inputs"][0]["version"], 3);
    }

    #[tokio::test]
    #[ignore = "requires a local indexer; set GHOSTLINE_INDEXER_URL"]
    async fn wait_until_ready_against_env() {
        let url = std::env::var("GHOSTLINE_INDEXER_URL").expect("GHOSTLINE_INDEXER_URL");
        IndexerClient::connect(url)
            .unwrap()
            .wait_until_ready()
            .await
            .unwrap();
    }
}
