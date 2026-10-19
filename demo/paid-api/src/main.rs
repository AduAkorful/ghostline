// SPDX-License-Identifier: MIT

use std::path::{Path, PathBuf};
use std::{
    io::{self, IsTerminal, Write},
    net::SocketAddr,
};

use clap::{Parser, Subcommand};
use ghostline_core::{ChannelParams, NONCE_LEN, from_hex};
use ghostline_paid_api::{AppState, router, router_with_metrics};
use ghostline_paywall::{ChannelStore, PaymentRequired, PaymentTerms, VOUCHER_HEADER};
use ghostline_policy::PolicyEngine;
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};
use tari_utilities::ByteArray;

#[derive(Parser)]
#[command(name = "ghostline-paid-api")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write empty payer and service JSON snapshots for the same channel.
    Init {
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        network_tag: u8,
        #[arg(long)]
        deposit: u64,
        #[arg(long)]
        payer_hex: String,
        #[arg(long)]
        payer_root_hex: String,
        #[arg(long)]
        nonce_hex: String,
    },
    /// Serve `/fortune` behind the paywall.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: SocketAddr,
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long, default_value_t = 10)]
        price: u64,
        /// Public key authorized to settle channels for this service.
        #[arg(long)]
        service_key_hex: String,
        /// Optional fresh one-time public key that receives this channel's settlement output.
        #[arg(long)]
        service_output_key_hex: Option<String>,
        /// Inclusive lower expiry accepted for newly opened channels.
        #[arg(long)]
        min_expiry_epoch: u64,
        /// Inclusive upper expiry accepted for newly opened channels.
        #[arg(long)]
        max_expiry_epoch: u64,
        /// Expose aggregate, privacy-safe Prometheus metrics at GET /metrics.
        #[arg(long)]
        metrics: bool,
    },
    /// Hit a paid URL, signing a voucher on each 402 until `--calls` succeed.
    Call {
        #[arg(long)]
        url: String,
        #[arg(long)]
        session: PathBuf,
        #[arg(long, default_value_t = 1)]
        calls: u32,
        #[arg(long)]
        vendor_hex: String,
        #[arg(long)]
        budget: u64,
        #[arg(long)]
        max_per_call: u64,
    },
}

fn params(
    network_tag: u8,
    deposit: u64,
    payer_hex: &str,
    payer_root_hex: &str,
    nonce_hex: &str,
) -> Result<ChannelParams, String> {
    let nonce_bytes = from_hex(nonce_hex).map_err(|e| e.to_string())?;
    let nonce: [u8; NONCE_LEN] = nonce_bytes
        .try_into()
        .map_err(|_| format!("nonce: expected {NONCE_LEN} bytes"))?;
    Ok(ChannelParams {
        network_tag,
        payer: RistrettoPublicKeyBytes::from_hex(payer_hex).map_err(|e| e.to_string())?,
        deposit,
        payer_root: Hash32::from_hex(payer_root_hex).map_err(|e| e.to_string())?,
        nonce,
    })
}

fn payer_secret() -> Result<RistrettoSecretKey, String> {
    let hex = std::env::var("GHOSTLINE_PAYER_SECRET").map_err(|_| "set GHOSTLINE_PAYER_SECRET")?;
    let bytes = from_hex(hex.trim()).map_err(|e| e.to_string())?;
    RistrettoSecretKey::from_canonical_bytes(&bytes).map_err(|e| e.to_string())
}

fn validate_payment_quote(
    quote: &PaymentRequired,
    channel_id: &[u8; ghostline_core::CHANNEL_ID_LEN],
    vendor: &RistrettoPublicKeyBytes,
) -> Result<(), String> {
    quote
        .validate_for(channel_id, vendor)
        .map_err(|error| error.to_string())
}

#[tokio::main]
async fn main() {
    init_logging();
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

async fn run() -> Result<(), String> {
    match Cli::parse().command {
        Command::Init {
            ledger,
            session,
            network_tag,
            deposit,
            payer_hex,
            payer_root_hex,
            nonce_hex,
        } => {
            let params = params(
                network_tag,
                deposit,
                &payer_hex,
                &payer_root_hex,
                &nonce_hex,
            )?;
            ensure_parent(&ledger)?;
            ensure_parent(&session)?;
            ChannelStore::create(&ledger, params.clone()).map_err(|e| e.to_string())?;
            ChannelStore::create(&session, params).map_err(|e| e.to_string())?;
            println!("ledger={}", ledger.display());
            println!("session={}", session.display());
        }
        Command::Serve {
            bind,
            ledger,
            price,
            service_key_hex,
            service_output_key_hex,
            min_expiry_epoch,
            max_expiry_epoch,
            metrics,
        } => {
            let store = ChannelStore::open(&ledger).map_err(|e| e.to_string())?;
            let service_key = RistrettoPublicKeyBytes::from_hex(service_key_hex.trim())
                .map_err(|e| format!("--service-key-hex: {e}"))?;
            let mut terms = PaymentTerms::new(service_key, min_expiry_epoch, max_expiry_epoch)
                .map_err(|e| e.to_string())?;
            if let Some(output_key_hex) = service_output_key_hex {
                let output_key = RistrettoPublicKeyBytes::from_hex(output_key_hex.trim())
                    .map_err(|e| format!("--service-output-key-hex: {e}"))?;
                terms = terms.with_service_output_key(output_key);
            }
            let state = AppState::with_store(store, price)
                .map_err(|e| e.to_string())?
                .with_payment_terms(terms);
            let listener = tokio::net::TcpListener::bind(bind)
                .await
                .map_err(|e| e.to_string())?;
            eprintln!(
                "ghostline-paid-api listening on {bind}  GET /fortune price={price}  GET /meter?units= unit_price={price}"
            );
            let app = if metrics {
                router_with_metrics(state)
            } else {
                router(state)
            };
            axum::serve(listener, app)
                .await
                .map_err(|e| e.to_string())?;
        }
        Command::Call {
            url,
            session,
            calls,
            vendor_hex,
            budget,
            max_per_call,
        } => {
            let secret = payer_secret()?;
            let vendor =
                RistrettoPublicKeyBytes::from_hex(vendor_hex.trim()).map_err(|e| e.to_string())?;
            let mut store = ChannelStore::open(&session).map_err(|e| e.to_string())?;
            let mut engine = PolicyEngine::new(
                ghostline_policy::Policy::new(budget, max_per_call).allow_vendor(vendor),
            )
            .with_audit_file_from_env()
            .map_err(|e| e.to_string())?
            .with_human_rules_from_env()
            .map_err(|e| e.to_string())?;
            let client = reqwest::Client::new();
            let mut ok = 0u32;
            for i in 1..=calls {
                let unpaid = client.get(&url).send().await.map_err(|e| e.to_string())?;
                if unpaid.status() != reqwest::StatusCode::PAYMENT_REQUIRED {
                    return Err(format!("call {i}: expected 402, got {}", unpaid.status()));
                }
                let quote: PaymentRequired = unpaid.json().await.map_err(|e| e.to_string())?;
                let session_state = store.session().map_err(|e| e.to_string())?;
                validate_payment_quote(&quote, &session_state.params().channel_id(), &vendor)
                    .map_err(|err| format!("call {i}: invalid payment quote: {err}"))?;
                let attested = if engine.requires_human_approval(&vendor, quote.price) {
                    confirm_human_approval(
                        quote.price,
                        session_state.cumulative(),
                        &session_state.params().channel_id(),
                    )?
                } else {
                    false
                };
                let (voucher, next) = ghostline_paywall::gated_sign_quote(
                    &mut store, engine, vendor, &quote, &secret, attested,
                )
                .map_err(|e| e.to_string())?;
                engine = next;
                let paid = client
                    .get(&url)
                    .header(
                        VOUCHER_HEADER,
                        ghostline_paywall::encode_voucher_header(&voucher),
                    )
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if !paid.status().is_success() {
                    return Err(format!(
                        "call {i}: expected 200, got {} {}",
                        paid.status(),
                        paid.text().await.unwrap_or_default()
                    ));
                }
                ok += 1;
            }
            let session = store.session().map_err(|e| e.to_string())?;
            let cumulative = session.cumulative();
            let remaining = session.params().max_spend().saturating_sub(cumulative);
            println!("ok_calls={ok}");
            println!("cumulative={cumulative}");
            println!("unspent_on_channel={remaining}");
            println!("chain_would_see=1_settlement exact_service_amount=hidden");
        }
    }
    Ok(())
}

fn confirm_human_approval(
    price: u64,
    cumulative: u64,
    channel_id: &[u8; 32],
) -> Result<bool, String> {
    if !io::stdin().is_terminal() {
        return Err("human approval requires an interactive terminal".into());
    }
    eprint!(
        "Approve {price} µT for channel {} (current cumulative: {cumulative} µT)? [y/N] ",
        ghostline_core::to_hex(channel_id)
    );
    io::stderr().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    Ok(answer.trim().eq_ignore_ascii_case("y") || answer.trim().eq_ignore_ascii_case("yes"))
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payer_rejects_payment_quotes_for_another_channel_or_recipient() {
        let channel_id = [1u8; ghostline_core::CHANNEL_ID_LEN];
        let other_channel_id = [2u8; ghostline_core::CHANNEL_ID_LEN];
        let vendor = RistrettoPublicKeyBytes::from_bytes(&[3u8; 32]).unwrap();
        let other_vendor = RistrettoPublicKeyBytes::from_bytes(&[4u8; 32]).unwrap();
        let terms = PaymentTerms::new(vendor, 10, 20).unwrap();
        let quote = PaymentRequired::new(10, &channel_id).with_terms(&terms);

        assert!(validate_payment_quote(&quote, &channel_id, &vendor).is_ok());
        assert!(validate_payment_quote(&quote, &other_channel_id, &vendor).is_err());
        assert!(validate_payment_quote(&quote, &channel_id, &other_vendor).is_err());
    }
}
