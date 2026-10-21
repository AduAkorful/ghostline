// SPDX-License-Identifier: MIT

use ghostline_core::{ChannelParams, PaymentSession, ServiceLedger};
use ghostline_paid_api::{AppState, local_demo_router};
use ghostline_paywall::{AgentPayer, PaymentRequired, PaymentTerms, Policy, encode_voucher_header};
use tari_crypto::{
    keys::PublicKey as _,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};
use tari_utilities::ByteArray;

const LOCAL_EVIDENCE: &str = include_str!("../../../demo/ui/evidence.json");

pub async fn run_demo(calls: u32, show_chain_evidence: bool) -> Result<(), String> {
    if !(1..=50).contains(&calls) {
        return Err("--calls must be between 1 and 50".into());
    }

    let (payer_secret, payer_public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
    let (_, service_public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
    let payer = RistrettoPublicKeyBytes::from_bytes(payer_public.as_bytes())
        .map_err(|error| format!("payer public key: {error}"))?;
    let service = RistrettoPublicKeyBytes::from_bytes(service_public.as_bytes())
        .map_err(|error| format!("service public key: {error}"))?;
    let deposit = 100_000u64;
    let params = ChannelParams {
        network_tag: 1,
        payer,
        deposit,
        payer_root: Hash32::from_array([0; 32]),
        nonce: rand::random(),
    };

    let terms = PaymentTerms::new(service, 1, u64::MAX)
        .map_err(|error| format!("create local service terms: {error}"))?;
    let state = AppState::new(ServiceLedger::new(params.clone())).with_payment_terms(terms);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("start local demo listener: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("read local demo address: {error}"))?;
    let app = local_demo_router(state);
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let result = run_calls(
        calls,
        &format!("http://{address}/fortune"),
        params,
        payer_secret,
        service,
    )
    .await;
    server.abort();
    result?;

    if show_chain_evidence {
        print_recorded_chain_evidence()?;
    }
    Ok(())
}

async fn run_calls(
    calls: u32,
    url: &str,
    params: ChannelParams,
    payer_secret: RistrettoSecretKey,
    service: RistrettoPublicKeyBytes,
) -> Result<(), String> {
    let budget = u64::from(calls)
        .checked_mul(10)
        .ok_or_else(|| "call total overflow".to_string())?;
    let policy = Policy::new(budget, 10).allow_vendor(service);
    let mut payer = AgentPayer::new(PaymentSession::new(params), policy, service);
    let client = reqwest::Client::new();
    for call in 1..=calls {
        let quote_response = client
            .get(url)
            .send()
            .await
            .map_err(|error| format!("call {call}: get quote: {error}"))?;
        if quote_response.status() != reqwest::StatusCode::PAYMENT_REQUIRED {
            return Err(format!(
                "call {call}: expected HTTP 402, received {}",
                quote_response.status()
            ));
        }
        let quote: PaymentRequired = quote_response
            .json()
            .await
            .map_err(|error| format!("call {call}: parse quote: {error}"))?;
        let voucher = payer
            .pay(&quote, &payer_secret)
            .map_err(|error| format!("call {call}: validate quote and sign: {error}"))?;
        let paid = client
            .get(url)
            .header(
                ghostline_paywall::VOUCHER_HEADER,
                encode_voucher_header(&voucher),
            )
            .send()
            .await
            .map_err(|error| format!("call {call}: submit local voucher: {error}"))?;
        if !paid.status().is_success() {
            return Err(format!(
                "call {call}: paid request returned {}",
                paid.status()
            ));
        }
        println!(
            "👻 toll {call}/{calls} paid · cumulative {} µT",
            payer.session().cumulative()
        );
    }

    println!("run_mode=local_simulation");
    println!("chain_transactions=0");
    println!(
        "quickstart smoke passed: quote validated, {calls} paid calls, cumulative {} µT",
        payer.session().cumulative()
    );
    Ok(())
}

fn print_recorded_chain_evidence() -> Result<(), String> {
    let evidence: serde_json::Value = serde_json::from_str(LOCAL_EVIDENCE)
        .map_err(|error| format!("parse bundled sanitized evidence: {error}"))?;
    println!("recorded_esmeralda_evidence=separate_historical_run");
    for path in [
        &["chainRecord", "openTransactionUrl"][..],
        &["chainRecord", "settlementTransactionUrl"][..],
    ] {
        let url = path
            .iter()
            .try_fold(&evidence, |value, field| value.get(*field))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("bundled evidence is missing {}", path.join(".")))?;
        println!("{}={url}", path.last().unwrap_or(&"transaction"));
    }
    Ok(())
}
