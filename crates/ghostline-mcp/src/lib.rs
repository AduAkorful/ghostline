// SPDX-License-Identifier: MIT

use ghostline_core::{ChannelParams, SignedVoucher, VoucherError, from_hex, to_hex};
use ghostline_net::TransactionEnvelope;
use ghostline_paywall::{
    ChannelStore, PaymentRequired, VOUCHER_HEADER, encode_voucher_header, gated_sign,
};
use ghostline_sdk::{inspect_transaction, unblind_transaction};
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
    #[serde(default)]
    pub attested: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct TxHexArgs {
    /// BOR-encoded sealed transaction hex from `ghostline seal-open` / `seal-settle`.
    pub tx_hex: String,
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
    sign_store(&mut store, &secret, args.price, args.attested)
}

/// Sign `quote.price` against `GHOSTLINE_SESSION`. The quote's channel id must match the store.
pub fn voucher_for_quote(quote: &PaymentRequired, attested: bool) -> Result<String, String> {
    let path = std::env::var("GHOSTLINE_SESSION").map_err(|_| "set GHOSTLINE_SESSION")?;
    let mut store = ChannelStore::open(path).map_err(|e| e.to_string())?;
    let id = to_hex(&store.channel_id().map_err(|e| e.to_string())?);
    if id != quote.channel_id {
        return Err("payment-required channel_id does not match GHOSTLINE_SESSION".into());
    }
    let secret = payer_secret_from_env()?;
    sign_store(&mut store, &secret, quote.price, attested)
}

fn sign_store(
    store: &mut ChannelStore,
    secret: &RistrettoSecretKey,
    price: u64,
    attested: bool,
) -> Result<String, String> {
    if env_flag("GHOSTLINE_POLICY_UNSAFE") {
        let mut session = store.session().map_err(|e| e.to_string())?;
        let voucher = session
            .sign_next(secret, price)
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
    let (voucher, _) =
        gated_sign(store, engine, vendor, price, secret, attested).map_err(|e| e.to_string())?;
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
    let first = get_paid(&args.url, args.voucher_hex.as_deref()).await?;
    if first.status != 402 {
        return Ok(first);
    }
    let Some(quote) = first.payment_required.clone() else {
        return Ok(first);
    };
    if args.voucher_hex.is_some() || std::env::var("GHOSTLINE_SESSION").is_err() {
        return Ok(first);
    }
    let hex = voucher_for_quote(&quote, args.attested)?;
    get_paid(&args.url, Some(&hex)).await
}

async fn get_paid(url: &str, voucher_hex: Option<&str>) -> Result<PaidCall, String> {
    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if let Some(voucher) = voucher_hex {
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

#[derive(Debug, Clone, Serialize)]
pub struct InspectedOutput {
    pub tag: u32,
    pub resource: String,
    pub commitment_hex: String,
    pub nonce_hex: String,
    pub encrypted_hex: String,
    pub promise: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnblindedOutput {
    pub value: u64,
    pub tag: u32,
    pub resource: String,
    pub mask_hex: String,
}

fn decode_tx(tx_hex: &str) -> Result<ghostline_net::Transaction, String> {
    let bytes = from_hex(tx_hex).map_err(|_| "tx: expected even-length hex".to_string())?;
    TransactionEnvelope::from_raw(bytes.into_boxed_slice())
        .decode()
        .map_err(|e| format!("tx envelope: {e}"))
}

/// List stealth outputs in a sealed tx. No secret.
pub fn inspect_tx(args: &TxHexArgs) -> Result<Vec<InspectedOutput>, String> {
    let tx = decode_tx(&args.tx_hex)?;
    Ok(inspect_transaction(&tx)
        .into_iter()
        .map(|view| InspectedOutput {
            tag: view.tag.value(),
            resource: view.resource.to_string(),
            commitment_hex: to_hex(view.output.commitment.as_bytes()),
            nonce_hex: to_hex(view.output.sender_public_nonce.as_bytes()),
            encrypted_hex: to_hex(view.output.encrypted_data.as_bytes()),
            promise: view.output.minimum_value_promise,
        })
        .collect())
}

/// Recover value and mask for outputs encrypted to `GHOSTLINE_PAYER_SECRET`.
pub fn unblind_tx(args: &TxHexArgs) -> Result<Vec<UnblindedOutput>, String> {
    let secret = payer_secret_from_env()?;
    let tx = decode_tx(&args.tx_hex)?;
    let found = unblind_transaction(&tx, &secret).map_err(|e| e.to_string())?;
    Ok(found
        .into_iter()
        .map(|out| UnblindedOutput {
            value: out.funds.value,
            tag: out.tag.value(),
            resource: out.resource.to_string(),
            mask_hex: to_hex(out.funds.mask.as_bytes()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_core::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;
    use tari_utilities::ByteArray;

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

    #[test]
    fn quote_must_match_session_channel() {
        let (_, public) = create_key_pair_from_seed(11);
        let args = ChannelArgs {
            network_tag: 1,
            payer_hex: to_hex(public.to_byte_type().as_bytes()),
            deposit: 100,
            payer_root_hex: to_hex(Hash32::from_array([9u8; 32]).as_slice()),
            nonce_hex: to_hex(&[3u8; NONCE_LEN]),
        };
        let params = parse_params(&args).unwrap();
        let dir = std::env::temp_dir().join(format!("ghostline-mcp-q-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("session.json");
        let _ = std::fs::remove_file(&path);
        let store = ChannelStore::create(&path, params.clone()).unwrap();
        let quote = PaymentRequired::new(10, &[0u8; 32]);
        let id = to_hex(&store.channel_id().unwrap());
        assert_ne!(id, quote.channel_id);
        assert_eq!(id, to_hex(&params.channel_id()));
    }

    #[tokio::test]
    async fn call_paid_retries_once_after_402() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let (secret, public) = create_key_pair_from_seed(11);
        let args = ChannelArgs {
            network_tag: 1,
            payer_hex: to_hex(public.to_byte_type().as_bytes()),
            deposit: 100,
            payer_root_hex: to_hex(Hash32::from_array([9u8; 32]).as_slice()),
            nonce_hex: to_hex(&[3u8; NONCE_LEN]),
        };
        let params = parse_params(&args).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "ghostline-mcp-402-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        ChannelStore::create(&path, params.clone()).unwrap();
        let quote = serde_json::to_string(&PaymentRequired::new(10, &params.channel_id())).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
                let paid = req.contains("ghostline-voucher:");
                let (status, body) = if paid {
                    ("200 OK", "{\"fortune\":\"ok\"}")
                } else {
                    ("402 Payment Required", quote.as_str())
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });

        unsafe {
            std::env::set_var("GHOSTLINE_SESSION", &path);
            std::env::set_var("GHOSTLINE_PAYER_SECRET", to_hex(secret.as_bytes()));
            std::env::set_var("GHOSTLINE_POLICY_UNSAFE", "1");
        }
        let result = call_paid(&CallPaidArgs {
            url: format!("http://{addr}/fortune"),
            voucher_hex: None,
            attested: false,
        })
        .await
        .unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_SESSION");
            std::env::remove_var("GHOSTLINE_PAYER_SECRET");
            std::env::remove_var("GHOSTLINE_POLICY_UNSAFE");
        }
        assert_eq!(result.status, 200);
        assert!(result.body.contains("fortune"));
        let loaded = ChannelStore::open(&path).unwrap();
        assert_eq!(loaded.session().unwrap().cumulative(), 10);
    }

    #[test]
    fn inspect_and_unblind_a_sealed_open() {
        use ghostline_sdk::{
            Channel, ChannelSpec, MaskAndValue, OutputScan, fund_open, seal_localnet_hex,
        };
        use tari_template_lib::types::constants::STEALTH_TARI_RESOURCE_ADDRESS;

        let (secret, public) = create_key_pair_from_seed(11);
        let (_, service) = create_key_pair_from_seed(13);
        let (input_mask, _) = create_key_pair_from_seed(99);
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer: public.to_byte_type(),
            service: service.to_byte_type(),
            deposit: 100,
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let scan = OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS);
        let opened = fund_open(
            &channel,
            [MaskAndValue {
                value: 100,
                mask: input_mask,
            }],
            channel.spec().payer,
            scan,
        )
        .unwrap();
        let hex = seal_localnet_hex(scan.resource, opened.statement, 1, &secret).unwrap();
        let views = inspect_tx(&TxHexArgs {
            tx_hex: hex.clone(),
        })
        .unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].promise, 100);
        unsafe {
            std::env::set_var("GHOSTLINE_PAYER_SECRET", to_hex(secret.as_bytes()));
        }
        let found = unblind_tx(&TxHexArgs { tx_hex: hex }).unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_PAYER_SECRET");
        }
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].value, 100);
        assert_eq!(found[0].mask_hex, to_hex(opened.deposit.mask.as_bytes()));
    }
}
