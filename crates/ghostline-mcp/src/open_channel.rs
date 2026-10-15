// SPDX-License-Identifier: MIT

//! Wallet open for a channel file that does not yet hold a deposit mask.
//! Broadcasts only when `GHOSTLINE_OPEN_SUBMIT=1`. Otherwise this is a dry-run and stores nothing.

use ghostline_core::{NONCE_LEN, from_hex, to_hex};
use ghostline_net::{
    ChannelOpenParams, StealthTransferRequest, UtxoInputSelection, WalletClient, channel_open_utxo,
    execution_accepted, stealth_utxos_from_wallet_tx,
};
use ghostline_paywall::PaymentRequired;
use ghostline_policy::env_flag;
use ghostline_sdk::{Channel, ChannelSpec, OutputScan, unblind_output};
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{
    EncryptedData, Hash32,
    constants::STEALTH_TARI_RESOURCE_ADDRESS,
    crypto::{PedersenCommitmentBytes, RistrettoPublicKeyBytes, UtxoTag},
    stealth::UnspentOutput,
};
use tari_utilities::ByteArray;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionOutcome {
    pub created: bool,
    pub cumulative: u64,
    pub session: String,
}

/// Create the payer session for `GHOSTLINE_CHANNEL_FILE`, or keep one whose channel id matches.
pub fn ensure_session() -> Result<SessionOutcome, String> {
    let path = std::env::var("GHOSTLINE_CHANNEL_FILE").map_err(|_| "set GHOSTLINE_CHANNEL_FILE")?;
    let path = std::path::PathBuf::from(path);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("channel file: {e}"))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    let channel = channel_from_value(&value)?;
    let params = channel.params().clone();
    let session_path = value
        .get("session")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| default_session_path(&path));
    let created = if session_path.exists() {
        let existing =
            ghostline_paywall::ChannelStore::open(&session_path).map_err(|e| e.to_string())?;
        if existing.channel_id().map_err(|e| e.to_string())? != params.channel_id() {
            return Err("session channel id does not match the channel file".into());
        }
        false
    } else {
        ghostline_paywall::ChannelStore::create(&session_path, params)
            .map_err(|e| e.to_string())?;
        true
    };
    let store = ghostline_paywall::ChannelStore::open(&session_path).map_err(|e| e.to_string())?;
    if value.get("session").and_then(|v| v.as_str()) != Some(session_path.to_str().unwrap_or("")) {
        value["session"] = serde_json::Value::String(session_path.display().to_string());
        ghostline_core::write_private_file(
            &path,
            &serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("channel file: {e}"))?;
    }
    Ok(SessionOutcome {
        created,
        cumulative: store.cumulative(),
        session: session_path.display().to_string(),
    })
}

fn default_session_path(channel_file: &std::path::Path) -> std::path::PathBuf {
    let mut name = channel_file
        .file_stem()
        .unwrap_or(std::ffi::OsStr::new("channel"))
        .to_os_string();
    name.push(".session.json");
    channel_file.with_file_name(name)
}

fn channel_from_value(value: &serde_json::Value) -> Result<Channel, String> {
    let record: OpenRecord =
        serde_json::from_value(value.clone()).map_err(|e| format!("channel file: {e}"))?;
    let nonce_bytes = from_hex(&record.nonce_hex).map_err(|e| format!("nonce: {e}"))?;
    let nonce: [u8; NONCE_LEN] = nonce_bytes
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

/// Check a remote quote against the channel file before an opt-in live open can spend funds.
pub fn validate_quote_config(quote: &PaymentRequired) -> Result<(), String> {
    let path = std::env::var("GHOSTLINE_CHANNEL_FILE").map_err(|_| "set GHOSTLINE_CHANNEL_FILE")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("channel file: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    let record: OpenRecord =
        serde_json::from_value(value.clone()).map_err(|e| format!("channel file: {e}"))?;
    validate_quote_for_channel(quote, &channel_from_value(&value)?, record.expiry_epoch)
}

fn validate_quote_for_channel(
    quote: &PaymentRequired,
    channel: &Channel,
    expiry_epoch: u64,
) -> Result<(), String> {
    quote
        .validate_for(&channel.params().channel_id(), &channel.spec().service)
        .map_err(|e| e.to_string())?;
    if let (Some(min), Some(max)) = (quote.min_expiry_epoch, quote.max_expiry_epoch)
        && !(min..=max).contains(&expiry_epoch)
    {
        return Err(format!(
            "configured channel expiry {expiry_epoch} is outside the service range {min}..={max}"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenOutcome {
    pub dry_run: bool,
    pub transaction_id: String,
    pub max_fee: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_fee: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_fee: Option<u64>,
    pub mask_stored: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_created: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cumulative: Option<u64>,
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
    #[serde(default)]
    ootle_network: String,
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
    let submit = env_flag("GHOSTLINE_OPEN_SUBMIT");
    let channel = channel_from_value(&value)?;
    let record: OpenRecord =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    if submit {
        wallet_claim_secret()?;
    }
    let max_fee = open_max_fee(
        submit,
        std::env::var("GHOSTLINE_OPEN_MAX_FEE").ok().as_deref(),
    )?;
    let pay_to = serde_json::json!({ "Conditions": channel.leaves_vec() });
    let mut wallet = WalletClient::connect(&record.opener_wallet_url).map_err(|e| e.to_string())?;
    wallet.auth_none_admin().await.map_err(|e| e.to_string())?;
    let destination = wallet
        .default_account_address()
        .await
        .map_err(|e| e.to_string())?;
    let request = StealthTransferRequest::channel_open(ChannelOpenParams {
        account_name: record.opener_account.clone(),
        destination_address: destination,
        resource_address: STEALTH_TARI_RESOURCE_ADDRESS.to_string(),
        deposit: record.deposit,
        pay_to,
        input_selection: "PreferConfidential".parse::<UtxoInputSelection>()?,
        fee_input_selection: "PreferRevealed".parse::<UtxoInputSelection>()?,
        max_fee,
        dry_run: !submit,
    });
    let submitted = wallet
        .stealth_transfer(&request)
        .await
        .map_err(|e| e.to_string())?;
    if !submit {
        let transaction = wallet
            .transaction_get(submitted.transaction_id)
            .await
            .map_err(|e| format!("open fee quote: {e}"))?;
        let (estimated_fee, required_fee) = open_fee_quote(&transaction)
            .ok_or("wallet dry-run fee quote is missing final_fee or total_fees_required")?;
        return Ok(OpenOutcome {
            dry_run: true,
            transaction_id: submitted.transaction_id.to_string(),
            max_fee,
            estimated_fee: Some(estimated_fee),
            required_fee: Some(required_fee),
            mask_stored: false,
            session: None,
            session_created: None,
            cumulative: None,
        });
    }
    let timeout = std::env::var("GHOSTLINE_OPEN_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let waited = wallet
        .wait_result(submitted.transaction_id, Some(timeout))
        .await
        .map_err(|e| e.to_string())?;
    let result = waited.get("result").unwrap_or(&waited);
    if !(execution_accepted(result) || execution_accepted(&waited)) {
        return Err(format!("wait did not Accept: {waited}"));
    }
    let got = wallet
        .transaction_get(submitted.transaction_id)
        .await
        .map_err(|e| e.to_string())?;
    let utxos = stealth_utxos_from_wallet_tx(&got);
    let channel_utxo = channel_open_utxo(&utxos)
        .ok_or("no body stealth output to write as utxo-json")?
        .clone();
    let file_path = std::path::PathBuf::from(&path);
    let utxo_path = file_path.with_extension("utxo.json");
    ghostline_core::write_private_file(
        &utxo_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&channel_utxo.unblind_json()).map_err(|e| e.to_string())?
        )
        .as_bytes(),
    )
    .map_err(|e| format!("utxo-json: {e}"))?;
    let recovered = recover_deposit(&channel_utxo, &record)?;
    if recovered.value != record.deposit {
        return Err(format!(
            "unblinded value {} does not match deposit {}",
            recovered.value, record.deposit
        ));
    }
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    value["utxo_json"] = serde_json::Value::String(utxo_path.display().to_string());
    value["deposit_mask_hex"] = serde_json::Value::String(to_hex(recovered.mask.as_bytes()));
    ghostline_core::write_private_file(
        &file_path,
        &serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("channel file: {e}"))?;
    let session = ensure_session()?;
    Ok(OpenOutcome {
        dry_run: false,
        transaction_id: submitted.transaction_id.to_string(),
        max_fee,
        estimated_fee: None,
        required_fee: None,
        mask_stored: true,
        session: Some(session.session),
        session_created: Some(session.created),
        cumulative: Some(session.cumulative),
    })
}

fn open_max_fee(submit: bool, configured: Option<&str>) -> Result<u64, String> {
    match configured {
        Some(value) => {
            let fee = value
                .parse::<u64>()
                .map_err(|_| "GHOSTLINE_OPEN_MAX_FEE must be an integer number of µT".to_string())?;
            if fee == 0 {
                return Err("GHOSTLINE_OPEN_MAX_FEE must be greater than zero".into());
            }
            Ok(fee)
        }
        None if submit => Err(
            "set GHOSTLINE_OPEN_MAX_FEE after reviewing the ghostline_open_channel dry-run fee quote".into(),
        ),
        None => Ok(100_000),
    }
}

fn open_fee_quote(transaction: &serde_json::Value) -> Option<(u64, u64)> {
    Some((
        transaction.get("final_fee")?.as_u64()?,
        transaction
            .get("result")?
            .get("total_fees_required")?
            .as_u64()?,
    ))
}

fn recover_deposit(
    utxo: &ghostline_net::WalletStealthUtxo,
    record: &OpenRecord,
) -> Result<ghostline_sdk::MaskAndValue, String> {
    let encrypted_bytes = from_hex(&utxo.encrypted_hex)
        .map_err(|_| "encrypted: expected even-length hex".to_string())?;
    let encrypted = EncryptedData::try_from(encrypted_bytes)
        .map_err(|len| format!("encrypted: invalid length {len}"))?;
    let output = UnspentOutput {
        commitment: PedersenCommitmentBytes::from_hex(&utxo.commitment)
            .map_err(|e| format!("commitment: {e}"))?,
        sender_public_nonce: RistrettoPublicKeyBytes::from_hex(&utxo.nonce)
            .map_err(|e| format!("nonce: {e}"))?,
        encrypted_data: encrypted,
        minimum_value_promise: 0,
        viewable_balance_proof: None,
    };
    let secret = claim_secret()?;
    let scan = match record.ootle_network.as_str() {
        "localnet" => OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
        "esmeralda" => OutputScan::esmeralda(STEALTH_TARI_RESOURCE_ADDRESS),
        other => {
            return Err(format!(
                "ootle_network: expected localnet or esmeralda, got {other}"
            ));
        }
    };
    let tag = u32::try_from(utxo.tag).map_err(|_| format!("tag: {}", utxo.tag))?;
    unblind_output(&output, UtxoTag::new(tag), &secret, scan).map_err(|e| e.to_string())
}

fn claim_secret() -> Result<RistrettoSecretKey, String> {
    let hex = match std::env::var("GHOSTLINE_CLAIM_SECRET") {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => std::env::var("GHOSTLINE_PAYER_SECRET")
            .map_err(|_| "set GHOSTLINE_CLAIM_SECRET or GHOSTLINE_PAYER_SECRET")?,
    };
    let bytes = from_hex(hex.trim()).map_err(|_| "claim secret: expected even-length hex")?;
    RistrettoSecretKey::from_canonical_bytes(&bytes).map_err(|e| format!("claim secret: {e}"))
}

fn wallet_claim_secret() -> Result<RistrettoSecretKey, String> {
    let hex = std::env::var("GHOSTLINE_CLAIM_SECRET")
        .map_err(|_| "set GHOSTLINE_CLAIM_SECRET to the opener wallet view_only_key")?;
    let bytes = from_hex(hex.trim()).map_err(|_| "claim secret: expected even-length hex")?;
    RistrettoSecretKey::from_canonical_bytes(&bytes).map_err(|e| format!("claim secret: {e}"))
}

#[cfg(test)]
mod fee_tests {
    use super::{open_fee_quote, open_max_fee, validate_quote_for_channel};
    use ghostline_core::NONCE_LEN;
    use ghostline_paywall::{PaymentRequired, PaymentTerms};
    use ghostline_sdk::{Channel, ChannelSpec};
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;

    #[test]
    fn live_open_requires_explicit_fee_cap() {
        assert!(
            open_max_fee(true, None)
                .unwrap_err()
                .contains("GHOSTLINE_OPEN_MAX_FEE")
        );
        assert_eq!(open_max_fee(false, None).unwrap(), 100_000);
        assert_eq!(open_max_fee(true, Some("15217")).unwrap(), 15_217);
        assert!(open_max_fee(true, Some("0")).is_err());
        assert!(open_max_fee(true, Some("not-a-number")).is_err());
    }

    #[test]
    fn extracts_only_fee_values_from_dry_run_transaction() {
        let tx = serde_json::json!({
            "final_fee": 15217,
            "result": { "total_fees_required": 15199 }
        });
        assert_eq!(open_fee_quote(&tx), Some((15_217, 15_199)));
        assert_eq!(open_fee_quote(&serde_json::json!({})), None);
    }

    #[test]
    fn auto_open_quote_must_match_channel_identity_and_expiry() {
        let (_, payer) = create_key_pair_from_seed(11);
        let (_, service) = create_key_pair_from_seed(12);
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer: payer.to_byte_type(),
            service: service.to_byte_type(),
            deposit: 100,
            expiry_epoch: 50,
            nonce: [3u8; NONCE_LEN],
            channel_template: Hash32::from_array([9u8; 32]),
        })
        .unwrap();
        let terms = PaymentTerms::new(service.to_byte_type(), 40, 60).unwrap();
        let quote = PaymentRequired::new(10, &channel.params().channel_id()).with_terms(&terms);
        assert!(validate_quote_for_channel(&quote, &channel, 50).is_ok());
        assert!(validate_quote_for_channel(&quote, &channel, 61).is_err());

        let wrong_channel = PaymentRequired::new(10, &[0xabu8; 32]).with_terms(&terms);
        assert!(validate_quote_for_channel(&wrong_channel, &channel, 50).is_err());
    }
}
