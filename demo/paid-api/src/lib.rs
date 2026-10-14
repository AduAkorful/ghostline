// SPDX-License-Identifier: MIT

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ghostline_core::ServiceLedger;
use ghostline_paywall::{PaymentRejection, PaymentRequired, VOUCHER_HEADER, charge};
use serde::Serialize;
use tokio::sync::Mutex;

const PRICE: u64 = 10;

#[derive(Clone)]
pub struct AppState {
    ledger: Arc<Mutex<ServiceLedger>>,
    price: u64,
}

impl AppState {
    pub fn new(ledger: ServiceLedger) -> Self {
        Self {
            ledger: Arc::new(Mutex::new(ledger)),
            price: PRICE,
        }
    }

    pub fn price(&self) -> u64 {
        self.price
    }
}

#[derive(Serialize)]
struct Fortune {
    fortune: &'static str,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/fortune", get(fortune))
        .with_state(state)
}

async fn fortune(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let voucher = headers
        .get(VOUCHER_HEADER)
        .and_then(|value| value.to_str().ok());
    let mut ledger = state.ledger.lock().await;
    match charge(&mut ledger, state.price, voucher) {
        Ok(_) => (
            StatusCode::OK,
            Json(Fortune {
                fortune: "pay once, call many",
            }),
        )
            .into_response(),
        Err(ghostline_paywall::PaywallError::MissingVoucher) => {
            let body = PaymentRequired::new(state.price, &ledger.params().channel_id());
            (StatusCode::PAYMENT_REQUIRED, Json(body)).into_response()
        }
        Err(err) => {
            let status = StatusCode::from_u16(err.status()).unwrap_or(StatusCode::FORBIDDEN);
            (status, Json(PaymentRejection::from_error(&err))).into_response()
        }
    }
}
