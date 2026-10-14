// SPDX-License-Identifier: MIT

use reqwest::{Client, IntoUrl};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tari_ootle_transaction::{Epoch, Transaction, TransactionEnvelope, TransactionId};
use url::Url;

use crate::NetError;

/// Paths from `tari_indexer_client` README / `IndexerRestApiClient` (0.46 source).
const WAIT_UNTIL_READY: &str = "wait-until-ready";
const EPOCH_STATS: &str = "epoch-manager/stats";
const TRANSACTIONS: &str = "transactions";
const TRANSACTIONS_DRY_RUN: &str = "transactions/dry-run";

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
            http: Client::new(),
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
        let voucher =
            ghostline_core::SignedVoucher::sign(&payer_secret, channel.params(), 30).unwrap();
        let funded = MaskAndValue {
            value: 100,
            mask: tari_crypto::ristretto::RistrettoSecretKey::random(&mut rand::rng()),
        };
        let statement = settle(&channel, funded, &voucher, service_public.to_byte_type()).unwrap();
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
