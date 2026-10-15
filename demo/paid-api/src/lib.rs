// SPDX-License-Identifier: MIT

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ghostline_core::ServiceLedger;
use ghostline_paywall::{ChannelStore, PaymentRejection, PaymentRequired, VOUCHER_HEADER, charge};
use serde::Serialize;
use tokio::sync::Mutex;

const PRICE: u64 = 10;

#[derive(Clone)]
pub struct AppState {
    ledger: Arc<Mutex<ServiceLedger>>,
    store: Option<Arc<Mutex<ChannelStore>>>,
    price: u64,
}

impl AppState {
    pub fn new(ledger: ServiceLedger) -> Self {
        Self {
            ledger: Arc::new(Mutex::new(ledger)),
            store: None,
            price: PRICE,
        }
    }

    pub fn with_store(
        store: ChannelStore,
        price: u64,
    ) -> Result<Self, ghostline_paywall::PaywallError> {
        let ledger = store.ledger()?;
        Ok(Self {
            ledger: Arc::new(Mutex::new(ledger)),
            store: Some(Arc::new(Mutex::new(store))),
            price,
        })
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
        Ok(delta) => {
            if let Some(store) = &state.store {
                let mut store = store.lock().await;
                if let Err(err) = store.save_ledger(&ledger) {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(PaymentRejection::from_error(&err)),
                    )
                        .into_response();
                }
                if let Err(err) = store.record_call(delta) {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(PaymentRejection::from_error(&err)),
                    )
                        .into_response();
                }
                if let Some(hex) = voucher
                    && let Err(err) = store.remember_voucher(hex)
                {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(PaymentRejection::from_error(&err)),
                    )
                        .into_response();
                }
            }
            (
                StatusCode::OK,
                Json(Fortune {
                    fortune: "pay once, call many",
                }),
            )
                .into_response()
        }
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
