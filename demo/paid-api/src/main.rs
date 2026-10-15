// SPDX-License-Identifier: MIT

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ghostline_core::{ChannelParams, NONCE_LEN, from_hex};
use ghostline_paid_api::{AppState, router};
use ghostline_paywall::{ChannelStore, PaymentRequired, VOUCHER_HEADER, gated_sign};
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
        #[arg(long)]
        attest: bool,
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

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
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
        } => {
            let store = ChannelStore::open(&ledger).map_err(|e| e.to_string())?;
            let state = AppState::with_store(store, price).map_err(|e| e.to_string())?;
            let listener = tokio::net::TcpListener::bind(bind)
                .await
                .map_err(|e| e.to_string())?;
            eprintln!("ghostline-paid-api listening on {bind}  GET /fortune  price={price}");
            axum::serve(listener, router(state))
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
            attest,
        } => {
            let secret = payer_secret()?;
            let vendor =
                RistrettoPublicKeyBytes::from_hex(vendor_hex.trim()).map_err(|e| e.to_string())?;
            let mut store = ChannelStore::open(&session).map_err(|e| e.to_string())?;
            let mut engine = PolicyEngine::new(
                ghostline_policy::Policy::new(budget, max_per_call).allow_vendor(vendor),
            );
            let client = reqwest::Client::new();
            let mut ok = 0u32;
            for i in 1..=calls {
                let unpaid = client.get(&url).send().await.map_err(|e| e.to_string())?;
                if unpaid.status() != reqwest::StatusCode::PAYMENT_REQUIRED {
                    return Err(format!("call {i}: expected 402, got {}", unpaid.status()));
                }
                let quote: PaymentRequired = unpaid.json().await.map_err(|e| e.to_string())?;
                let (voucher, next) =
                    gated_sign(&mut store, engine, vendor, quote.price, &secret, attest)
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
            let remaining = session.params().deposit.saturating_sub(cumulative);
            println!("ok_calls={ok}");
            println!("cumulative={cumulative}");
            println!("unspent_on_channel={remaining}");
            println!("chain_would_see=1_settlement amount={cumulative}");
        }
    }
    Ok(())
}

fn ensure_parent(path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}
