// SPDX-License-Identifier: MIT

mod channel_file;
mod close_channel;
mod open_channel;

use ghostline_core::{ChannelParams, SignedVoucher, VoucherError, from_hex, to_hex};
use ghostline_net::TransactionEnvelope;
use ghostline_paywall::{
    ChannelStatus, ChannelStore, PaymentRequired, VOUCHER_HEADER, encode_voucher_header, gated_sign,
};
use ghostline_policy::{Policy, PolicyEngine, env_flag};
use ghostline_sdk::{inspect_transaction, unblind_transaction};
use rmcp::{ServerHandler, handler::server::wrapper::Parameters, tool, tool_handler, tool_router};
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

#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct EnvOnlyArgs {}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct StatusArgs {
    /// Channel expiry epoch. Falls back to `GHOSTLINE_EXPIRY_EPOCH`.
    #[serde(default)]
    pub expiry_epoch: Option<u64>,
    /// Read-only indexer URL for `current_epoch`. Falls back to `GHOSTLINE_INDEXER_URL`.
    #[serde(default)]
    pub indexer_url: Option<String>,
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

/// `GHOSTLINE_SESSION`, or the `session` path on `GHOSTLINE_CHANNEL_FILE`.
fn payer_session_path() -> Result<Option<std::path::PathBuf>, String> {
    if let Ok(path) = std::env::var("GHOSTLINE_SESSION")
        && !path.trim().is_empty()
    {
        return Ok(Some(std::path::PathBuf::from(path)));
    }
    let Ok(file) = std::env::var("GHOSTLINE_CHANNEL_FILE") else {
        return Ok(None);
    };
    if file.trim().is_empty() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("channel file: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    Ok(value
        .get("session")
        .and_then(|v| v.as_str())
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from))
}

pub fn sign_voucher_hex(args: &SignArgs) -> Result<String, String> {
    let params = parse_params(&args.channel)?;
    let secret = payer_secret_from_env()?;
    let mut store = session_store(params)?;
    sign_store(&mut store, &secret, args.price, args.attested)
}

/// Sign `quote.price` against the payer session. The quote's channel id must match the store.
pub fn voucher_for_quote(quote: &PaymentRequired, attested: bool) -> Result<String, String> {
    let path =
        payer_session_path()?.ok_or("set GHOSTLINE_SESSION or session on the channel file")?;
    voucher_at(&path, quote, attested)
}

fn voucher_at(
    path: &std::path::Path,
    quote: &PaymentRequired,
    attested: bool,
) -> Result<String, String> {
    let mut store = ChannelStore::open(path).map_err(|e| e.to_string())?;
    let id = to_hex(&store.channel_id().map_err(|e| e.to_string())?);
    if id != quote.channel_id {
        return Err("payment-required channel_id does not match the payer session".into());
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaidCall {
    pub status: u16,
    pub body: String,
    pub payment_required: Option<PaymentRequired>,
    /// 402 and the channel is not open. A live open happens only when `GHOSTLINE_OPEN_SUBMIT=1`.
    #[serde(default)]
    pub needs_channel: bool,
}

pub async fn call_paid(args: &CallPaidArgs) -> Result<PaidCall, String> {
    let first = get_paid(&args.url, args.voucher_hex.as_deref()).await?;
    if first.status != 402 {
        return Ok(first);
    }
    let Some(quote) = first.payment_required.clone() else {
        return Ok(first);
    };
    let Some(session) = payer_session_for_pay().await? else {
        let mut unpaid = first;
        unpaid.needs_channel = unpaid.status == 402;
        return Ok(unpaid);
    };
    if args.voucher_hex.is_some() {
        return Ok(first);
    }
    let hex = voucher_at(&session, &quote, args.attested)?;
    get_paid(&args.url, Some(&hex)).await
}

/// An existing session, a new one when the file already holds a deposit mask, or a live open
/// when `GHOSTLINE_OPEN_SUBMIT=1` and the file has no mask yet.
async fn payer_session_for_pay() -> Result<Option<std::path::PathBuf>, String> {
    if let Some(path) = payer_session_path()? {
        return Ok(Some(path));
    }
    let Ok(file) = std::env::var("GHOSTLINE_CHANNEL_FILE") else {
        return Ok(None);
    };
    if file.trim().is_empty() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("channel file: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("channel file: {e}"))?;
    let opened = value
        .get("deposit_mask_hex")
        .and_then(|v| v.as_str())
        .is_some_and(|mask| !mask.is_empty());
    if !opened {
        if !env_flag("GHOSTLINE_OPEN_SUBMIT") {
            return Ok(None);
        }
        open_channel::open_channel().await?;
    }
    let outcome = open_channel::ensure_session()?;
    Ok(Some(std::path::PathBuf::from(outcome.session)))
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
        needs_channel: false,
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

pub async fn channel_status(args: &StatusArgs) -> Result<ChannelStatus, String> {
    let session_path = std::env::var("GHOSTLINE_SESSION").map_err(|_| "set GHOSTLINE_SESSION")?;
    let session = ChannelStore::open(&session_path).map_err(|e| e.to_string())?;
    let ledger = match std::env::var("GHOSTLINE_LEDGER") {
        Ok(path) => Some(ChannelStore::open(path).map_err(|e| e.to_string())?),
        Err(_) => None,
    };
    let expiry_epoch = args.expiry_epoch.or_else(|| {
        std::env::var("GHOSTLINE_EXPIRY_EPOCH")
            .ok()
            .and_then(|s| s.parse().ok())
    });
    let indexer_url = args
        .indexer_url
        .clone()
        .or_else(|| std::env::var("GHOSTLINE_INDEXER_URL").ok());
    let current_epoch = match indexer_url.as_deref() {
        Some(url) => {
            let stats = ghostline_net::IndexerClient::connect(url)
                .map_err(|e| e.to_string())?
                .epoch_stats()
                .await
                .map_err(|e| e.to_string())?;
            Some(stats.current_epoch.as_u64())
        }
        None => None,
    };
    match ledger {
        Some(ledger) => {
            ChannelStatus::from_session_and_ledger(&session, &ledger, expiry_epoch, current_epoch)
                .map_err(|e| e.to_string())
        }
        None => ChannelStatus::from_store(&session, expiry_epoch, current_epoch)
            .map_err(|e| e.to_string()),
    }
}

#[derive(Clone)]
pub struct GhostlineMcp {
    #[allow(dead_code)]
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

#[derive(Serialize)]
struct HexOut {
    hex: String,
}

#[derive(Serialize)]
struct OkOut {
    ok: bool,
}

#[tool_router]
impl GhostlineMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Compute the 32-byte Ghostline channel id from open parameters (hex).")]
    fn ghostline_channel_id(
        &self,
        Parameters(args): Parameters<ChannelArgs>,
    ) -> Result<String, String> {
        let hex = channel_id_hex(&args)?;
        serde_json::to_string(&HexOut { hex }).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Sign the next Ghostline voucher for `price`. Uses GHOSTLINE_PAYER_SECRET, GHOSTLINE_SESSION, and policy env vars. Does not take a secret argument."
    )]
    fn ghostline_sign_voucher(
        &self,
        Parameters(args): Parameters<SignArgs>,
    ) -> Result<String, String> {
        let hex = sign_voucher_hex(&args)?;
        serde_json::to_string(&HexOut { hex }).map_err(|e| e.to_string())
    }

    #[tool(description = "Verify a hex-encoded Ghostline voucher against channel parameters.")]
    fn ghostline_verify_voucher(
        &self,
        Parameters(args): Parameters<VerifyArgs>,
    ) -> Result<String, String> {
        verify_voucher(&args)?;
        serde_json::to_string(&OkOut { ok: true }).map_err(|e| e.to_string())
    }

    #[tool(
        description = "GET a paid HTTP resource. On 402, if GHOSTLINE_SESSION is set, signs a voucher from the session and retries once."
    )]
    async fn ghostline_call_paid(
        &self,
        Parameters(args): Parameters<CallPaidArgs>,
    ) -> Result<String, String> {
        let result = call_paid(&args).await?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(
        description = "List stealth outputs in a sealed Ghostline tx hex (resource, tag, commitment, nonce, encrypted data). No secret."
    )]
    fn ghostline_inspect_tx(
        &self,
        Parameters(args): Parameters<TxHexArgs>,
    ) -> Result<String, String> {
        let views = inspect_tx(&args)?;
        serde_json::to_string(&views).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Recover value and mask from stealth outputs in a sealed tx hex encrypted to GHOSTLINE_PAYER_SECRET. Does not take a secret argument."
    )]
    fn ghostline_unblind_tx(
        &self,
        Parameters(args): Parameters<TxHexArgs>,
    ) -> Result<String, String> {
        let found = unblind_tx(&args)?;
        serde_json::to_string(&found).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Off-chain channel totals and observer panel from GHOSTLINE_SESSION (optional GHOSTLINE_LEDGER). Does not take secrets. Not a chain confirmation."
    )]
    async fn ghostline_channel_status(
        &self,
        Parameters(args): Parameters<StatusArgs>,
    ) -> Result<String, String> {
        let view = channel_status(&args).await?;
        serde_json::to_string(&view).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Off-chain totals, expiry window, and channel-output presence for GHOSTLINE_CHANNEL_FILE. No arguments. Does not return the deposit mask, keys, or fee accounts."
    )]
    async fn ghostline_channel_file(
        &self,
        Parameters(_args): Parameters<EnvOnlyArgs>,
    ) -> Result<String, String> {
        let view = channel_file::channel_file_status().await?;
        serde_json::to_string(&view).map_err(|e| e.to_string())
    }

    #[tool(
        description = "One row per channel file in GHOSTLINE_CHANNEL_DIR. No arguments. Skips JSON that is not a channel record. Does not return the deposit mask, keys, or fee accounts."
    )]
    async fn ghostline_channel_list(
        &self,
        Parameters(_args): Parameters<EnvOnlyArgs>,
    ) -> Result<String, String> {
        let views = channel_file::channel_file_list().await?;
        serde_json::to_string(&views).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Accepted-call prices from ledgers under GHOSTLINE_CHANNEL_DIR. No arguments. Does not return voucher hex. Older ledgers are not backfilled."
    )]
    fn ghostline_channel_calls(
        &self,
        Parameters(_args): Parameters<EnvOnlyArgs>,
    ) -> Result<String, String> {
        let rows = channel_file::channel_file_calls()?;
        serde_json::to_string(&rows).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Open the channel in GHOSTLINE_CHANNEL_FILE. No arguments. Dry-run unless GHOSTLINE_OPEN_SUBMIT=1, which broadcasts, stores the deposit mask, and creates the payer session. Refuses when the file already holds a deposit mask."
    )]
    async fn ghostline_open_channel(
        &self,
        Parameters(_args): Parameters<EnvOnlyArgs>,
    ) -> Result<String, String> {
        let result = open_channel::open_channel().await?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Create the payer session for GHOSTLINE_CHANNEL_FILE, or keep one whose channel id matches. No arguments. Does not reset a cumulative total or return the deposit mask."
    )]
    fn ghostline_channel_session(
        &self,
        Parameters(_args): Parameters<EnvOnlyArgs>,
    ) -> Result<String, String> {
        let result = open_channel::ensure_session()?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    #[tool(
        description = "Close GHOSTLINE_CHANNEL_FILE. No arguments. Settles inside the expiry margin and refunds at or after expiry. A down output is output_spent. Broadcasts only when GHOSTLINE_CLOSE_SUBMIT=1. GHOSTLINE_CLOSE_DRY_RUN=1 prices the transaction and does not broadcast. Does not return the deposit mask."
    )]
    async fn ghostline_close_channel(
        &self,
        Parameters(_args): Parameters<EnvOnlyArgs>,
    ) -> Result<String, String> {
        let result = close_channel::close_channel().await?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }
}

#[tool_handler(
    name = "ghostline",
    version = "0.1.0",
    instructions = "Ghostline payment-channel tools. Payer secret comes from GHOSTLINE_PAYER_SECRET, never from a tool argument."
)]
impl ServerHandler for GhostlineMcp {}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_core::NONCE_LEN;
    use ootle_byte_type::ToByteType;
    use std::sync::Mutex;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;
    use tari_utilities::ByteArray;

    static ENV: Mutex<()> = Mutex::new(());

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

        let _env = ENV.lock().unwrap();
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
        let _env = ENV.lock().unwrap();
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

    #[tokio::test]
    async fn mcp_protocol_lists_tools_and_retries_402() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        use rmcp::{ServiceExt, model::CallToolRequestParams};

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
            "ghostline-mcp-proto-{}-{}",
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
                    ("200 OK", "{\"fortune\":\"mcp\"}")
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

        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::set_var("GHOSTLINE_SESSION", &path);
            std::env::set_var("GHOSTLINE_PAYER_SECRET", to_hex(secret.as_bytes()));
            std::env::set_var("GHOSTLINE_POLICY_UNSAFE", "1");
        }

        let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
        let server_handle = tokio::spawn(async move {
            GhostlineMcp::new()
                .serve(server_transport)
                .await
                .map_err(|e| e.to_string())?
                .waiting()
                .await
                .map_err(|e| e.to_string())?;
            Ok::<(), String>(())
        });

        let client = ().serve(client_transport).await.unwrap();
        let tools = client.list_all_tools().await.unwrap();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
        for expected in [
            "ghostline_channel_id",
            "ghostline_sign_voucher",
            "ghostline_verify_voucher",
            "ghostline_call_paid",
            "ghostline_inspect_tx",
            "ghostline_unblind_tx",
            "ghostline_channel_status",
            "ghostline_channel_file",
            "ghostline_channel_list",
            "ghostline_channel_calls",
            "ghostline_open_channel",
            "ghostline_channel_session",
            "ghostline_close_channel",
        ] {
            assert!(names.contains(&expected), "missing {expected} in {names:?}");
        }

        let mut arguments = serde_json::Map::new();
        arguments.insert(
            "url".into(),
            serde_json::Value::String(format!("http://{addr}/fortune")),
        );
        let result = client
            .call_tool(CallToolRequestParams::new("ghostline_call_paid").with_arguments(arguments))
            .await
            .unwrap();
        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .expect("tool text");
        let paid: PaidCall = serde_json::from_str(text).unwrap();
        assert_eq!(paid.status, 200);
        assert!(!paid.needs_channel);
        assert!(paid.body.contains("fortune"));

        client.cancel().await.unwrap();
        let _ = server_handle.await;
        unsafe {
            std::env::remove_var("GHOSTLINE_SESSION");
            std::env::remove_var("GHOSTLINE_PAYER_SECRET");
            std::env::remove_var("GHOSTLINE_POLICY_UNSAFE");
        }
        let loaded = ChannelStore::open(&path).unwrap();
        assert_eq!(loaded.session().unwrap().cumulative(), 10);
    }

    #[tokio::test]
    async fn call_paid_without_a_session_needs_a_channel() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body =
                "{\"scheme\":\"ghostline.channel.voucher.v1\",\"price\":10,\"channel_id\":\"aa\"}";
            let resp = format!(
                "HTTP/1.1 402 Payment Required\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        });
        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_SESSION");
            std::env::remove_var("GHOSTLINE_CHANNEL_FILE");
        }
        let paid = call_paid(&CallPaidArgs {
            url: format!("http://{addr}/fortune"),
            voucher_hex: None,
            attested: false,
        })
        .await
        .unwrap();
        assert_eq!(paid.status, 402);
        assert!(paid.needs_channel);
        assert!(paid.payment_required.is_some());
    }

    #[tokio::test]
    async fn call_paid_uses_the_channel_file_session() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let (secret, public) = create_key_pair_from_seed(11);
        let params = ChannelParams {
            network_tag: 1,
            payer: public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [4u8; NONCE_LEN],
        };
        let dir = std::env::temp_dir().join(format!(
            "ghostline-mcp-sess-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let session_path = dir.join("session.json");
        ChannelStore::create(&session_path, params.clone()).unwrap();
        let file_path = dir.join("channel.json");
        std::fs::write(
            &file_path,
            serde_json::json!({
                "session": session_path,
                "deposit_mask_hex": "55".repeat(32),
            })
            .to_string(),
        )
        .unwrap();
        let quote = serde_json::to_string(&PaymentRequired::new(10, &params.channel_id())).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let quote_for_thread = quote.clone();
        std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let paid = req.to_ascii_lowercase().contains("ghostline-voucher:");
                let (status, body) = if paid {
                    ("200 OK", "{\"fortune\":\"from-file\"}")
                } else {
                    ("402 Payment Required", quote_for_thread.as_str())
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });
        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_SESSION");
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &file_path);
            std::env::set_var("GHOSTLINE_PAYER_SECRET", to_hex(secret.as_bytes()));
            std::env::set_var("GHOSTLINE_POLICY_UNSAFE", "1");
        }
        let paid = call_paid(&CallPaidArgs {
            url: format!("http://{addr}/fortune"),
            voucher_hex: None,
            attested: false,
        })
        .await
        .unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_CHANNEL_FILE");
            std::env::remove_var("GHOSTLINE_PAYER_SECRET");
            std::env::remove_var("GHOSTLINE_POLICY_UNSAFE");
        }
        assert_eq!(paid.status, 200);
        assert!(!paid.needs_channel);
        assert!(paid.body.contains("from-file"));
        let loaded = ChannelStore::open(&session_path).unwrap();
        assert_eq!(loaded.session().unwrap().cumulative(), 10);
        let text = std::fs::read_to_string(&file_path).unwrap();
        assert!(text.contains("deposit_mask_hex"));
    }

    #[tokio::test]
    async fn call_paid_creates_a_session_when_the_mask_is_set() {
        use ghostline_sdk::{Channel, ChannelSpec};
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let (secret, public) = create_key_pair_from_seed(21);
        let (_, service) = create_key_pair_from_seed(22);
        let dir = std::env::temp_dir().join(format!(
            "ghostline-mcp-paymask-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("channel.json");
        let mask = "66".repeat(32);
        let payer_hex = to_hex(public.to_byte_type().as_bytes());
        let service_hex = to_hex(service.to_byte_type().as_bytes());
        let nonce_hex = to_hex(&[6u8; NONCE_LEN]);
        let template_hex = to_hex(&[9u8; 32]);
        std::fs::write(
            &file_path,
            serde_json::json!({
                "network_tag": 1,
                "deposit": 100,
                "payer_hex": payer_hex,
                "service_hex": service_hex,
                "expiry_epoch": 50,
                "nonce_hex": nonce_hex,
                "template_hex": template_hex,
                "opener_account": "ghostline",
                "opener_wallet_url": "http://127.0.0.1:5100",
                "deposit_mask_hex": mask,
            })
            .to_string(),
        )
        .unwrap();
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer: public.to_byte_type(),
            service: service.to_byte_type(),
            deposit: 100,
            expiry_epoch: 50,
            nonce: [6u8; NONCE_LEN],
            channel_template: Hash32::from_hex(&template_hex).unwrap(),
        })
        .unwrap();
        let quote =
            serde_json::to_string(&PaymentRequired::new(10, &channel.params().channel_id()))
                .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let paid = req.to_ascii_lowercase().contains("ghostline-voucher:");
                let (status, body) = if paid {
                    ("200 OK", "{\"fortune\":\"from-mask\"}")
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
        let bare = dir.join("unopened.json");
        std::fs::write(&bare, "{\"opener_account\":\"ghostline\"}").unwrap();
        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_SESSION");
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &bare);
        }
        let unpaid = call_paid(&CallPaidArgs {
            url: format!("http://{addr}/fortune"),
            voucher_hex: None,
            attested: false,
        })
        .await
        .unwrap();
        assert_eq!(unpaid.status, 402);
        assert!(unpaid.needs_channel);
        assert!(!dir.join("unopened.session.json").exists());
        unsafe {
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &file_path);
            std::env::set_var("GHOSTLINE_PAYER_SECRET", to_hex(secret.as_bytes()));
            std::env::set_var("GHOSTLINE_POLICY_UNSAFE", "1");
        }
        let paid = call_paid(&CallPaidArgs {
            url: format!("http://{addr}/fortune"),
            voucher_hex: None,
            attested: false,
        })
        .await
        .unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_CHANNEL_FILE");
            std::env::remove_var("GHOSTLINE_PAYER_SECRET");
            std::env::remove_var("GHOSTLINE_POLICY_UNSAFE");
        }
        assert_eq!(paid.status, 200);
        assert!(!paid.needs_channel);
        assert!(paid.body.contains("from-mask"));
        let text = std::fs::read_to_string(&file_path).unwrap();
        assert!(text.contains(&mask));
        assert!(text.contains("session"));
        let saved: serde_json::Value = serde_json::from_str(&text).unwrap();
        let loaded = ChannelStore::open(saved["session"].as_str().unwrap()).unwrap();
        assert_eq!(loaded.session().unwrap().cumulative(), 10);
    }

    #[test]
    fn channel_session_keeps_cumulative_and_the_mask() {
        use tari_crypto::ristretto::RistrettoSecretKey;
        use tari_utilities::ByteArray;
        let (secret, public) = create_key_pair_from_seed(11);
        let (_, service) = create_key_pair_from_seed(12);
        let dir = std::env::temp_dir().join(format!(
            "ghostline-mcp-ensure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("channel.json");
        let mask = "55".repeat(32);
        std::fs::write(
            &file_path,
            serde_json::json!({
                "network_tag": 1,
                "deposit": 100,
                "payer_hex": to_hex(public.to_byte_type().as_bytes()),
                "service_hex": to_hex(service.to_byte_type().as_bytes()),
                "expiry_epoch": 50,
                "nonce_hex": to_hex(&[3u8; NONCE_LEN]),
                "template_hex": to_hex(&[9u8; 32]),
                "opener_account": "ghostline",
                "opener_wallet_url": "http://127.0.0.1:5100",
                "deposit_mask_hex": mask,
            })
            .to_string(),
        )
        .unwrap();
        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &file_path);
        }
        let first = open_channel::ensure_session().unwrap();
        assert!(first.created);
        assert_eq!(first.cumulative, 0);
        let secret = RistrettoSecretKey::from_canonical_bytes(secret.as_bytes()).unwrap();
        let mut store = ChannelStore::open(&first.session).unwrap();
        let mut session = store.session().unwrap();
        session.sign_next(&secret, 10).unwrap();
        store.save_session(&session).unwrap();
        let second = open_channel::ensure_session().unwrap();
        assert!(!second.created);
        assert_eq!(second.cumulative, 10);
        let text = std::fs::read_to_string(&file_path).unwrap();
        assert!(text.contains(&mask));
        let mut other: serde_json::Value = serde_json::from_str(&text).unwrap();
        other["nonce_hex"] = serde_json::Value::String(to_hex(&[4u8; NONCE_LEN]));
        let other_path = dir.join("other.json");
        std::fs::write(&other_path, other.to_string()).unwrap();
        unsafe {
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &other_path);
        }
        let err = open_channel::ensure_session().unwrap_err();
        assert!(err.contains("does not match"));
        unsafe {
            std::env::remove_var("GHOSTLINE_CHANNEL_FILE");
        }
    }

    #[tokio::test]
    async fn open_channel_refuses_a_mask_and_submit_needs_a_channel_file() {
        let dir = std::env::temp_dir().join(format!(
            "ghostline-mcp-open-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let masked = dir.join("masked.json");
        std::fs::write(
            &masked,
            serde_json::json!({ "deposit_mask_hex": "55".repeat(32) }).to_string(),
        )
        .unwrap();
        let bare = dir.join("bare.json");
        std::fs::write(&bare, "{}").unwrap();
        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &masked);
            std::env::remove_var("GHOSTLINE_OPEN_SUBMIT");
        }
        let err = open_channel::open_channel().await.unwrap_err();
        assert!(err.contains("deposit mask"));
        unsafe {
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &bare);
            std::env::set_var("GHOSTLINE_OPEN_SUBMIT", "1");
        }
        let err = open_channel::open_channel().await.unwrap_err();
        assert!(
            !err.contains("does not broadcast"),
            "submit is a live open, not a refusal: {err}"
        );
        assert!(err.contains("channel file"), "{err}");
        unsafe {
            std::env::remove_var("GHOSTLINE_CHANNEL_FILE");
            std::env::remove_var("GHOSTLINE_OPEN_SUBMIT");
        }
    }

    #[tokio::test]
    async fn channel_status_reads_session_without_indexer() {
        let (_, public) = create_key_pair_from_seed(11);
        let args = ChannelArgs {
            network_tag: 1,
            payer_hex: to_hex(public.to_byte_type().as_bytes()),
            deposit: 100,
            payer_root_hex: to_hex(Hash32::from_array([9u8; 32]).as_slice()),
            nonce_hex: to_hex(&[3u8; NONCE_LEN]),
        };
        let params = parse_params(&args).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "ghostline-mcp-status-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        ChannelStore::create(&path, params).unwrap();
        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::set_var("GHOSTLINE_SESSION", &path);
            std::env::remove_var("GHOSTLINE_LEDGER");
            std::env::remove_var("GHOSTLINE_INDEXER_URL");
        }
        let view = channel_status(&StatusArgs {
            expiry_epoch: Some(50),
            indexer_url: None,
        })
        .await
        .unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_SESSION");
        }
        assert_eq!(view.deposit, 100);
        assert_eq!(view.cumulative, 0);
        assert_eq!(view.settle_window, ghostline_paywall::SettleWindow::Unknown);
        assert!(!view.observer.at_settle.is_empty());
    }

    #[tokio::test]
    async fn channel_file_tools_omit_the_mask() {
        use ghostline_core::{PaymentSession, ServiceLedger};
        let (secret, public) = create_key_pair_from_seed(11);
        let params = ChannelParams {
            network_tag: 1,
            payer: public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let dir = std::env::temp_dir().join(format!(
            "ghostline-mcp-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ledger_path = dir.join("ledger.json");
        let mut session = PaymentSession::new(params.clone());
        let voucher = session.sign_next(&secret, 10).unwrap();
        let mut ledger = ServiceLedger::new(params.clone());
        assert_eq!(ledger.accept(&voucher).unwrap(), 10);
        let mut store = ChannelStore::create(&ledger_path, params).unwrap();
        store.save_ledger(&ledger).unwrap();
        store.record_call(10).unwrap();
        let mask = "55".repeat(32);
        let file_path = dir.join("due.file.json");
        std::fs::write(
            &file_path,
            serde_json::json!({
                "network_tag": 1,
                "deposit": 100,
                "payer_hex": "11".repeat(32),
                "service_hex": "22".repeat(32),
                "expiry_epoch": 50,
                "nonce_hex": "33".repeat(32),
                "template_hex": "44".repeat(32),
                "indexer_url": "",
                "ootle_network": "esmeralda",
                "max_epoch": 10,
                "opener_account": "ghostline",
                "opener_wallet_url": "http://127.0.0.1:5100",
                "fee_account": "component_secret",
                "ledger": ledger_path,
                "deposit_mask_hex": mask,
            })
            .to_string(),
        )
        .unwrap();
        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &file_path);
            std::env::set_var("GHOSTLINE_CHANNEL_DIR", &dir);
        }
        let view = channel_file::channel_file_status().await.unwrap();
        let listed = channel_file::channel_file_list().await.unwrap();
        let calls = channel_file::channel_file_calls().unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_CHANNEL_FILE");
            std::env::remove_var("GHOSTLINE_CHANNEL_DIR");
        }
        let text = serde_json::to_string(&view).unwrap();
        assert!(!text.contains(&mask));
        assert!(!text.contains("component_secret"));
        assert!(!text.contains(&"11".repeat(32)));
        assert_eq!(view.status.accepted, 10);
        assert_eq!(view.status.remainder_at_accepted, 90);
        assert_eq!(view.output_presence, "not_checked");
        assert_eq!(listed.len(), 1);
        assert_eq!(
            calls,
            vec![channel_file::ChannelCallView {
                file: "due.file.json".into(),
                price: Some(10),
                accepted: Some(10),
                note: String::new(),
            }]
        );
    }

    #[tokio::test]
    #[ignore = "reads a local Esmeralda channel file from GHOSTLINE_CHANNEL_FILE and GHOSTLINE_CHANNEL_DIR"]
    async fn live_channel_file_omits_mask() {
        let file = std::env::var("GHOSTLINE_CHANNEL_FILE").expect("GHOSTLINE_CHANNEL_FILE");
        let raw = std::fs::read_to_string(&file).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let mask = parsed["deposit_mask_hex"]
            .as_str()
            .expect("mask")
            .to_string();
        let fee = parsed["fee_account"].as_str().unwrap_or("").to_string();
        let view = channel_file::channel_file_status().await.unwrap();
        let calls = channel_file::channel_file_calls().unwrap();
        let text = format!(
            "{}{}",
            serde_json::to_string(&view).unwrap(),
            serde_json::to_string(&calls).unwrap()
        );
        assert!(!text.contains(&mask));
        assert!(!fee.is_empty());
        assert!(!text.contains(&fee));
        assert_eq!(view.status.accepted, 20);
        assert_eq!(view.output_presence, "down");
        assert_eq!(view.output_version, Some(0));
        assert!(calls.iter().any(|row| row.note.contains("predates")));
        println!(
            "accepted={} remainder={} window={:?} presence={} calls={}",
            view.status.accepted,
            view.status.remainder_at_accepted,
            view.status.settle_window,
            view.output_presence,
            calls.len()
        );
    }

    #[tokio::test]
    #[ignore = "dry-runs a wallet open from GHOSTLINE_CHANNEL_FILE; does not broadcast"]
    async fn live_open_channel_dry_run_does_not_store_a_mask() {
        let source = std::env::var("GHOSTLINE_CHANNEL_FILE").expect("GHOSTLINE_CHANNEL_FILE");
        let mut parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&source).unwrap()).unwrap();
        let obj = parsed.as_object_mut().unwrap();
        obj.remove("deposit_mask_hex");
        obj.remove("utxo_json");
        let path = std::env::temp_dir().join(format!(
            "ghostline-mcp-open-dry-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, serde_json::to_string(&parsed).unwrap()).unwrap();
        let _env = ENV.lock().unwrap();
        unsafe {
            std::env::set_var("GHOSTLINE_CHANNEL_FILE", &path);
            std::env::remove_var("GHOSTLINE_OPEN_SUBMIT");
        }
        let outcome = open_channel::open_channel().await.unwrap();
        unsafe {
            std::env::remove_var("GHOSTLINE_CHANNEL_FILE");
        }
        let after = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(outcome.dry_run);
        assert!(!outcome.transaction_id.is_empty());
        assert!(!after.contains("deposit_mask"));
        println!("dry_run=true");
    }
}
