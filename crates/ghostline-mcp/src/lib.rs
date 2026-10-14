// SPDX-License-Identifier: MIT

use ghostline_core::{ChannelParams, SignedVoucher, VoucherError, from_hex, to_hex};
use ghostline_paywall::{
    ChannelStore, PaymentRequired, VOUCHER_HEADER, encode_voucher_header, gated_sign,
};
use ghostline_policy::{Policy, PolicyEngine, env_flag};
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};
use tari_utilities::ByteArray;

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ChannelArgs {
    pub network_tag: u8,
    pub payer_hex: String,
    pub deposit: u64,
    pub payer_root_hex: String,
    pub nonce_hex: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SignArgs {
    #[serde(flatten)]
    pub channel: ChannelArgs,
    /// Per-call increment, not the cumulative total.
    pub price: u64,
    /// Required when `GHOSTLINE_POLICY_REQUIRE_HUMAN` is set.
    #[serde(default)]
    pub attested: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct VerifyArgs {
    #[serde(flatten)]
    pub channel: ChannelArgs,
    pub voucher_hex: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CallPaidArgs {
    pub url: String,
    pub voucher_hex: Option<String>,
}

pub fn parse_params(args: &ChannelArgs) -> Result<ChannelParams, String> {
    let payer = RistrettoPublicKeyBytes::from_hex(&args.payer_hex).map_err(|e| e.to_string())?;
    let payer_root = Hash32::from_hex(&args.payer_root_hex).map_err(|e| e.to_string())?;
    let nonce_bytes = from_hex(&args.nonce_hex).map_err(|e| e.to_string())?;
    let nonce = nonce_bytes
        .try_into()
        .map_err(|_| "nonce must be 32 bytes".to_string())?;
    Ok(ChannelParams {
        network_tag: args.network_tag,
        payer,
        deposit: args.deposit,
        payer_root,
        nonce,
    })
}

pub fn channel_id_hex(args: &ChannelArgs) -> Result<String, String> {
    Ok(to_hex(&parse_params(args)?.channel_id()))
}

pub fn payer_secret_from_env() -> Result<RistrettoSecretKey, String> {
    let hex = std::env::var("GHOSTLINE_PAYER_SECRET")
        .map_err(|_| "set GHOSTLINE_PAYER_SECRET; the tool does not accept a secret argument")?;
    let bytes = from_hex(hex.trim()).map_err(|e| e.to_string())?;
    RistrettoSecretKey::from_canonical_bytes(&bytes).map_err(|e| e.to_string())
}

fn vendor_from_env() -> Result<RistrettoPublicKeyBytes, String> {
    let hex = std::env::var("GHOSTLINE_POLICY_VENDOR_HEX")
        .map_err(|_| "set GHOSTLINE_POLICY_VENDOR_HEX")?;
    RistrettoPublicKeyBytes::from_hex(hex.trim()).map_err(|e| e.to_string())
}

fn session_store(params: ChannelParams) -> Result<ChannelStore, String> {
    let path = std::env::var("GHOSTLINE_SESSION").map_err(|_| "set GHOSTLINE_SESSION")?;
    ChannelStore::open_or_create(path, params).map_err(|e| e.to_string())
}

pub fn sign_voucher_hex(args: &SignArgs) -> Result<String, String> {
    let params = parse_params(&args.channel)?;
    let secret = payer_secret_from_env()?;
    let mut store = session_store(params)?;
    if env_flag("GHOSTLINE_POLICY_UNSAFE") {
        let mut session = store.session().map_err(|e| e.to_string())?;
        let voucher = session
            .sign_next(&secret, args.price)
            .map_err(|e: VoucherError| e.to_string())?;
        store.save_session(&session).map_err(|e| e.to_string())?;
        return Ok(encode_voucher_header(&voucher));
    }
    let policy = Policy::from_env()?
        .ok_or("set GHOSTLINE_POLICY_BUDGET and GHOSTLINE_POLICY_MAX_PER_CALL")?;
    let vendor = vendor_from_env()?;
    let mut engine = PolicyEngine::new(policy);
    if env_flag("GHOSTLINE_POLICY_REQUIRE_HUMAN") {
        engine = engine.require_human();
    }
    let (voucher, _) = gated_sign(
        &mut store,
        engine,
        vendor,
        args.price,
        &secret,
        args.attested,
    )
    .map_err(|e| e.to_string())?;
    Ok(encode_voucher_header(&voucher))
}

pub fn verify_voucher(args: &VerifyArgs) -> Result<(), String> {
    let params = parse_params(&args.channel)?;
    let bytes = from_hex(&args.voucher_hex).map_err(|e| e.to_string())?;
    let voucher = SignedVoucher::from_bytes(&bytes).map_err(|e| e.to_string())?;
    voucher.verify(&params).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct PaidCall {
    pub status: u16,
    pub body: String,
    pub payment_required: Option<PaymentRequired>,
}

pub async fn call_paid(args: &CallPaidArgs) -> Result<PaidCall, String> {
    let client = reqwest::Client::new();
    let mut req = client.get(&args.url);
    if let Some(voucher) = &args.voucher_hex {
        req = req.header(VOUCHER_HEADER, voucher);
    }
    let response = req.send().await.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|e| e.to_string())?;
    let payment_required = if status == 402 {
        serde_json::from_str(&body).ok()
    } else {
        None
    };
    Ok(PaidCall {
        status,
        body,
        payment_required,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_core::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;

    #[test]
    fn sign_and_verify_round_trip() {
        let (secret, public) = create_key_pair_from_seed(11);
        let args = ChannelArgs {
            network_tag: 1,
            payer_hex: to_hex(public.to_byte_type().as_bytes()),
            deposit: 100,
            payer_root_hex: to_hex(Hash32::from_array([9u8; 32]).as_slice()),
            nonce_hex: to_hex(&[3u8; NONCE_LEN]),
        };
        let dir = std::env::temp_dir().join(format!("ghostline-mcp-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("session.json");
        let _ = std::fs::remove_file(&path);
        let mut store = ChannelStore::create(&path, parse_params(&args).unwrap()).unwrap();
        let engine = PolicyEngine::new(Policy::new(100, 50).allow_vendor(public.to_byte_type()));
        let (voucher, _) = gated_sign(
            &mut store,
            engine,
            public.to_byte_type(),
            30,
            &secret,
            false,
        )
        .unwrap();
        let voucher_hex = encode_voucher_header(&voucher);
        verify_voucher(&VerifyArgs {
            channel: args,
            voucher_hex,
        })
        .unwrap();
    }
}
