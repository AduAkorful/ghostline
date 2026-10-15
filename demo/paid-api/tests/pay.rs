// SPDX-License-Identifier: MIT

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ghostline_core::{ChannelParams, NONCE_LEN, PaymentSession, ServiceLedger, to_hex};
use ghostline_paid_api::{AppState, router};
use ghostline_paywall::{
    AgentPayer, CHANNEL_HEADER, ChannelDirectory, ChannelStore, PaymentRequired, VOUCHER_HEADER,
    encode_voucher_header, gated_sign,
};
use ghostline_policy::{Policy, PolicyEngine};
use http_body_util::BodyExt;
use ootle_byte_type::ToByteType;
use tari_ootle_common_types::crypto::create_key_pair_from_seed;
use tari_template_lib::types::Hash32;
use tower::ServiceExt;

fn fixtures() -> (
    AppState,
    AgentPayer,
    tari_crypto::ristretto::RistrettoSecretKey,
) {
    let (secret, public) = create_key_pair_from_seed(11);
    let payer = public.to_byte_type();
    let params = ChannelParams {
        network_tag: 1,
        payer,
        deposit: 100,
        payer_root: Hash32::from_array([9u8; 32]),
        nonce: [3u8; NONCE_LEN],
    };
    let state = AppState::new(ServiceLedger::new(params.clone()));
    let agent = AgentPayer::new(PaymentSession::new(params), Policy::new(100, 10), payer);
    (state, agent, secret)
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn unpaid_request_returns_402_quote() {
    let (state, _agent, _secret) = fixtures();
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/fortune")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
    let body = json_body(response).await;
    assert_eq!(body["scheme"], "ghostline.channel.voucher.v1");
    assert_eq!(body["price"], 10);
    assert_eq!(body["basis"], "per_request");
}

#[tokio::test]
async fn metered_call_charges_unit_price_times_units() {
    let (secret, public) = create_key_pair_from_seed(11);
    let payer = public.to_byte_type();
    let params = ChannelParams {
        network_tag: 1,
        payer,
        deposit: 100,
        payer_root: Hash32::from_array([9u8; 32]),
        nonce: [3u8; NONCE_LEN],
    };
    let state = AppState::new(ServiceLedger::new(params.clone()));
    let mut agent = AgentPayer::new(PaymentSession::new(params), Policy::new(100, 40), payer);
    let app = router(state);
    let quote_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/meter?units=3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(quote_response.status(), StatusCode::PAYMENT_REQUIRED);
    let raw = json_body(quote_response).await;
    assert_eq!(raw["basis"], "metered");
    assert_eq!(raw["unit_price"], 10);
    assert_eq!(raw["units"], 3);
    assert_eq!(raw["price"], 30);
    let quote: PaymentRequired = serde_json::from_value(raw).unwrap();
    let header = agent.voucher_header(&quote, &secret).unwrap();
    let paid = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/meter?units=3")
                .header(VOUCHER_HEADER, header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paid.status(), StatusCode::OK);
    let body = json_body(paid).await;
    assert_eq!(body["units"], 3);
    assert_eq!(body["charged"], 30);

    let zero = app
        .oneshot(
            Request::builder()
                .uri("/meter?units=0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(zero.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn agent_pays_and_receives_the_fortune() {
    let (state, mut agent, secret) = fixtures();
    let app = router(state);

    let quote_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/fortune")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let quote: PaymentRequired = serde_json::from_value(json_body(quote_response).await).unwrap();
    let header = agent.voucher_header(&quote, &secret).unwrap();

    let paid = app
        .oneshot(
            Request::builder()
                .uri("/fortune")
                .header(VOUCHER_HEADER, header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paid.status(), StatusCode::OK);
    let body = json_body(paid).await;
    assert_eq!(body["fortune"], "pay once, call many");
}

#[tokio::test]
async fn ten_paid_calls_leave_one_settle_total() {
    let (state, mut agent, secret) = fixtures();
    let app = router(state);
    let mut last_amount = 0u64;
    for _ in 0..10 {
        let quote: PaymentRequired = serde_json::from_value(
            json_body(
                app.clone()
                    .oneshot(
                        Request::builder()
                            .uri("/fortune")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap(),
            )
            .await,
        )
        .unwrap();
        let header = agent.voucher_header(&quote, &secret).unwrap();
        let paid = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(VOUCHER_HEADER, header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(paid.status(), StatusCode::OK);
        last_amount += 10;
    }
    assert_eq!(agent.session().cumulative(), last_amount);
    assert_eq!(last_amount, 100);
}

#[tokio::test]
async fn ledger_file_keeps_accepted_after_reopen() {
    let (secret, public) = create_key_pair_from_seed(11);
    let payer = public.to_byte_type();
    let params = ChannelParams {
        network_tag: 1,
        payer,
        deposit: 100,
        payer_root: Hash32::from_array([9u8; 32]),
        nonce: [3u8; NONCE_LEN],
    };
    let dir = std::env::temp_dir().join(format!(
        "ghostline-paid-persist-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let ledger_path = dir.join("ledger.json");
    let session_path = dir.join("session.json");
    ChannelStore::create(&ledger_path, params.clone()).unwrap();
    ChannelStore::create(&session_path, params.clone()).unwrap();

    let state = AppState::with_store(ChannelStore::open(&ledger_path).unwrap(), 10).unwrap();
    let app = router(state);
    let mut payer_store = ChannelStore::open(&session_path).unwrap();
    let mut engine = PolicyEngine::new(Policy::new(100, 10).allow_vendor(payer));
    for _ in 0..3 {
        let quote: PaymentRequired = serde_json::from_value(
            json_body(
                app.clone()
                    .oneshot(
                        Request::builder()
                            .uri("/fortune")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap(),
            )
            .await,
        )
        .unwrap();
        let (voucher, next) =
            gated_sign(&mut payer_store, engine, payer, quote.price, &secret, false).unwrap();
        engine = next;
        let paid = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(VOUCHER_HEADER, encode_voucher_header(&voucher))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(paid.status(), StatusCode::OK);
    }
    drop(app);
    let ledger = ChannelStore::open(&ledger_path).unwrap();
    let session = ChannelStore::open(&session_path).unwrap();
    assert_eq!(ledger.ledger().unwrap().accepted(), 30);
    assert_eq!(session.session().unwrap().cumulative(), 30);
    assert_eq!(
        ledger
            .calls()
            .iter()
            .map(|call| (call.price, call.accepted))
            .collect::<Vec<_>>(),
        vec![(10, 10), (10, 20), (10, 30)]
    );
    assert!(session.calls().is_empty());
}

#[tokio::test]
async fn retry_of_the_same_voucher_is_served() {
    let (state, mut agent, secret) = fixtures();
    let app = router(state);
    let quote: PaymentRequired = serde_json::from_value(
        json_body(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri("/fortune")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await,
    )
    .unwrap();
    let header = agent.voucher_header(&quote, &secret).unwrap();
    for _ in 0..2 {
        let paid = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(VOUCHER_HEADER, header.clone())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(paid.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn two_channels_charge_only_the_named_one() {
    let (secret, public) = create_key_pair_from_seed(11);
    let payer = public.to_byte_type();
    let first = ChannelParams {
        network_tag: 1,
        payer,
        deposit: 100,
        payer_root: Hash32::from_array([9u8; 32]),
        nonce: [3u8; NONCE_LEN],
    };
    let mut second = first.clone();
    second.nonce = [4u8; NONCE_LEN];
    let dir = std::env::temp_dir().join(format!(
        "ghostline-paid-dir-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    ChannelStore::create(dir.join("a.json"), first.clone()).unwrap();
    ChannelStore::create(dir.join("b.json"), second.clone()).unwrap();
    let state = AppState::with_directory(ChannelDirectory::open(&dir).unwrap(), 10).unwrap();
    let app = router(state);
    let ambiguous = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/fortune")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ambiguous.status(), StatusCode::BAD_REQUEST);
    let quote_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/fortune")
                .header(CHANNEL_HEADER, to_hex(&second.channel_id()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(quote_response.status(), StatusCode::PAYMENT_REQUIRED);
    let quote: PaymentRequired = serde_json::from_value(json_body(quote_response).await).unwrap();
    assert_eq!(quote.channel_id, to_hex(&second.channel_id()));
    let mut agent = AgentPayer::new(
        PaymentSession::new(second.clone()),
        Policy::new(100, 10),
        payer,
    );
    let header = agent.voucher_header(&quote, &secret).unwrap();
    let paid = app
        .oneshot(
            Request::builder()
                .uri("/fortune")
                .header(VOUCHER_HEADER, header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paid.status(), StatusCode::OK);
    let directory = ChannelDirectory::open(&dir).unwrap();
    assert_eq!(directory.accepted(&first.channel_id()).unwrap(), 0);
    assert_eq!(directory.accepted(&second.channel_id()).unwrap(), 10);
}
