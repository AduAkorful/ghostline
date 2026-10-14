// SPDX-License-Identifier: MIT

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ghostline_core::{ChannelParams, NONCE_LEN, PaymentSession, ServiceLedger};
use ghostline_paid_api::{AppState, router};
use ghostline_paywall::{AgentPayer, PaymentRequired, VOUCHER_HEADER};
use ghostline_policy::Policy;
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
