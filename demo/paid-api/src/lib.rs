// SPDX-License-Identifier: MIT

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ghostline_core::ServiceLedger;
use ghostline_paywall::{
    CHANNEL_HEADER, ChannelDirectory, ChannelStore, PaymentRejection, PaymentTerms, PriceKind,
    VOUCHER_HEADER, charge, charge_to_output_key, quote,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

mod metrics;

use metrics::ServiceMetrics;

const PRICE: u64 = 10;

#[derive(Clone)]
pub struct AppState {
    ledger: Arc<Mutex<ServiceLedger>>,
    store: Option<Arc<Mutex<ChannelStore>>>,
    directory: Option<Arc<Mutex<ChannelDirectory>>>,
    price: u64,
    payment_terms: Option<PaymentTerms>,
    metrics: Arc<ServiceMetrics>,
    configured_channels: usize,
}

impl AppState {
    pub fn new(ledger: ServiceLedger) -> Self {
        Self {
            ledger: Arc::new(Mutex::new(ledger)),
            store: None,
            directory: None,
            price: PRICE,
            payment_terms: None,
            metrics: Arc::new(ServiceMetrics::default()),
            configured_channels: 1,
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
            directory: None,
            price,
            payment_terms: None,
            metrics: Arc::new(ServiceMetrics::default()),
            configured_channels: 1,
        })
    }

    /// Serve every ledger in `directory`. An unpaid request names one channel, or the only channel.
    pub fn with_directory(
        directory: ChannelDirectory,
        price: u64,
    ) -> Result<Self, ghostline_paywall::PaywallError> {
        let params = directory.first_params()?;
        let configured_channels = directory.len();
        Ok(Self {
            ledger: Arc::new(Mutex::new(ServiceLedger::new(params))),
            store: None,
            directory: Some(Arc::new(Mutex::new(directory))),
            price,
            payment_terms: None,
            metrics: Arc::new(ServiceMetrics::default()),
            configured_channels,
        })
    }

    /// Add the service identity and expiry range advertised to agents in HTTP 402 responses.
    pub fn with_payment_terms(mut self, terms: PaymentTerms) -> Self {
        self.payment_terms = Some(terms);
        self
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
    router_inner(state, false)
}

/// Build the reference router with the aggregate Prometheus endpoint enabled.
/// Exported metrics intentionally contain no per-channel labels or payment amounts.
pub fn router_with_metrics(state: AppState) -> Router {
    router_inner(state, true)
}

fn router_inner(state: AppState, expose_metrics: bool) -> Router {
    let router = Router::new()
        .route("/fortune", get(fortune))
        .route("/meter", get(meter));
    let router = if expose_metrics {
        router.route("/metrics", get(metrics))
    } else {
        router
    };
    router.with_state(state)
}

#[derive(Deserialize)]
struct MeterQuery {
    units: u64,
}

#[derive(Serialize)]
struct Metered {
    units: u64,
    charged: u64,
}

async fn fortune(State(state): State<AppState>, headers: HeaderMap) -> Response {
    state.metrics.request("fortune");
    if state.directory.is_some() {
        return serve_directory(
            &state,
            PriceKind::PerRequest { price: state.price },
            headers,
            None,
        )
        .await;
    }
    serve(
        &state,
        PriceKind::PerRequest { price: state.price },
        headers,
        None,
    )
    .await
}

async fn meter(
    State(state): State<AppState>,
    Query(query): Query<MeterQuery>,
    headers: HeaderMap,
) -> Response {
    state.metrics.request("meter");
    if state.directory.is_some() {
        return serve_directory(
            &state,
            PriceKind::Metered {
                unit_price: state.price,
                units: query.units,
            },
            headers,
            Some(query.units),
        )
        .await;
    }
    serve(
        &state,
        PriceKind::Metered {
            unit_price: state.price,
            units: query.units,
        },
        headers,
        Some(query.units),
    )
    .await
}

async fn serve(
    state: &AppState,
    kind: PriceKind,
    headers: HeaderMap,
    units: Option<u64>,
) -> Response {
    let quoted = match quote(kind) {
        Ok(quoted) => quoted,
        Err(err) => {
            state.metrics.rejected();
            tracing::warn!(
                event = "payment_request",
                outcome = "rejected",
                status = err.status()
            );
            let status = StatusCode::from_u16(err.status()).unwrap_or(StatusCode::BAD_REQUEST);
            return (status, Json(PaymentRejection::from_error(&err))).into_response();
        }
    };
    let voucher = headers
        .get(VOUCHER_HEADER)
        .and_then(|value| value.to_str().ok());
    let mut ledger = state.ledger.lock().await;
    let mut next_ledger = ledger.clone();
    let charge_result = match state.payment_terms.as_ref() {
        Some(terms) => match terms.service_output_public_key() {
            Ok(Some(output_key)) => {
                charge_to_output_key(&mut next_ledger, quoted.price, voucher, &output_key)
            }
            Ok(None) => charge(&mut next_ledger, quoted.price, voucher),
            Err(err) => Err(err),
        },
        None => charge(&mut next_ledger, quoted.price, voucher),
    };
    match charge_result {
        Ok(_) => {
            if let Some(store) = &state.store {
                let mut store = store.lock().await;
                if let Some(hex) = voucher {
                    if let Err(err) = store.commit_charge(&next_ledger, quoted.price, hex) {
                        state.metrics.internal_error();
                        tracing::error!(event = "payment_request", outcome = "storage_error");
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(PaymentRejection::from_error(&err)),
                        )
                            .into_response();
                    }
                } else {
                    state.metrics.internal_error();
                    tracing::error!(
                        event = "payment_request",
                        outcome = "missing_voucher_after_accept"
                    );
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(PaymentRejection {
                            scheme: ghostline_paywall::SCHEME.to_string(),
                            status: 500,
                            reason: "accepted voucher missing from request".into(),
                        }),
                    )
                        .into_response();
                }
            }
            *ledger = next_ledger;
            state.metrics.paid();
            tracing::info!(event = "payment_request", outcome = "paid");
            if let Some(units) = units {
                (
                    StatusCode::OK,
                    Json(Metered {
                        units,
                        charged: quoted.price,
                    }),
                )
                    .into_response()
            } else {
                (
                    StatusCode::OK,
                    Json(Fortune {
                        fortune: "pay once, call many",
                    }),
                )
                    .into_response()
            }
        }
        Err(ghostline_paywall::PaywallError::MissingVoucher) => {
            state.metrics.payment_required();
            tracing::debug!(event = "payment_request", outcome = "payment_required");
            let mut body = ghostline_paywall::PaymentRequired::from_quote(
                &quoted,
                &ledger.params().channel_id(),
            );
            if let Some(terms) = &state.payment_terms {
                body = body.with_terms(terms);
            }
            (StatusCode::PAYMENT_REQUIRED, Json(body)).into_response()
        }
        Err(err) => {
            state.metrics.rejected();
            tracing::warn!(
                event = "payment_request",
                outcome = "rejected",
                status = err.status()
            );
            let status = StatusCode::from_u16(err.status()).unwrap_or(StatusCode::FORBIDDEN);
            (status, Json(PaymentRejection::from_error(&err))).into_response()
        }
    }
}

async fn serve_directory(
    state: &AppState,
    kind: PriceKind,
    headers: HeaderMap,
    units: Option<u64>,
) -> Response {
    let quoted = match quote(kind) {
        Ok(quoted) => quoted,
        Err(err) => {
            state.metrics.rejected();
            tracing::warn!(
                event = "payment_request",
                outcome = "rejected",
                status = err.status()
            );
            let status = StatusCode::from_u16(err.status()).unwrap_or(StatusCode::BAD_REQUEST);
            return (status, Json(PaymentRejection::from_error(&err))).into_response();
        }
    };
    let Some(directory) = &state.directory else {
        state.metrics.internal_error();
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PaymentRejection {
                scheme: ghostline_paywall::SCHEME.to_string(),
                status: 500,
                reason: "no channel directory".into(),
            }),
        )
            .into_response();
    };
    let mut directory = directory.lock().await;
    let voucher = headers
        .get(VOUCHER_HEADER)
        .and_then(|value| value.to_str().ok());
    let requested = headers
        .get(CHANNEL_HEADER)
        .and_then(|value| value.to_str().ok());
    let charge_result = match state.payment_terms.as_ref() {
        Some(terms) => match terms.service_output_public_key() {
            Ok(Some(output_key)) => {
                directory.charge_to_output_key(quoted.price, voucher, &output_key)
            }
            Ok(None) => directory.charge(quoted.price, voucher),
            Err(err) => Err(err),
        },
        None => directory.charge(quoted.price, voucher),
    };
    match charge_result {
        Ok(_) => {
            state.metrics.paid();
            tracing::info!(event = "payment_request", outcome = "paid");
            paid_body(units, quoted.price)
        }
        Err(ghostline_paywall::PaywallError::MissingVoucher) => {
            match directory.channel_for_quote(requested) {
                Ok(channel_id) => {
                    state.metrics.payment_required();
                    tracing::debug!(event = "payment_request", outcome = "payment_required");
                    let mut body =
                        ghostline_paywall::PaymentRequired::from_quote(&quoted, &channel_id);
                    if let Some(terms) = &state.payment_terms {
                        body = body.with_terms(terms);
                    }
                    (StatusCode::PAYMENT_REQUIRED, Json(body)).into_response()
                }
                Err(err) => {
                    state.metrics.rejected();
                    tracing::warn!(
                        event = "payment_request",
                        outcome = "rejected",
                        status = err.status()
                    );
                    let status =
                        StatusCode::from_u16(err.status()).unwrap_or(StatusCode::BAD_REQUEST);
                    (status, Json(PaymentRejection::from_error(&err))).into_response()
                }
            }
        }
        Err(err) => {
            if matches!(err, ghostline_paywall::PaywallError::Storage(_)) {
                state.metrics.internal_error();
                tracing::error!(event = "payment_request", outcome = "storage_error");
            } else {
                state.metrics.rejected();
                tracing::warn!(
                    event = "payment_request",
                    outcome = "rejected",
                    status = err.status()
                );
            }
            let status = StatusCode::from_u16(err.status()).unwrap_or(StatusCode::FORBIDDEN);
            (status, Json(PaymentRejection::from_error(&err))).into_response()
        }
    }
}

async fn metrics(State(state): State<AppState>) -> Response {
    let ledgers_with_accepted_vouchers = if let Some(directory) = &state.directory {
        directory.lock().await.ledgers_with_accepted_vouchers()
    } else {
        usize::from(state.ledger.lock().await.accepted() > 0)
    };
    let body = state
        .metrics
        .exposition(state.configured_channels, ledgers_with_accepted_vouchers);
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

fn paid_body(units: Option<u64>, charged: u64) -> Response {
    if let Some(units) = units {
        (StatusCode::OK, Json(Metered { units, charged })).into_response()
    } else {
        (
            StatusCode::OK,
            Json(Fortune {
                fortune: "pay once, call many",
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use ghostline_core::{ChannelParams, NONCE_LEN, PaymentSession, ServiceLedger};
    use ghostline_paywall::{AgentPayer, PaymentRequired, PaymentTerms, Policy, VOUCHER_HEADER};
    use http_body_util::BodyExt;
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn opt_in_metrics_count_outcomes_without_channel_data() {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let payer = payer_public.to_byte_type();
        let (_, service_public) = create_key_pair_from_seed(12);
        let service = service_public.to_byte_type();
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let channel_id = ghostline_core::to_hex(&params.channel_id());
        let state = AppState::new(ServiceLedger::new(params.clone()));
        let default_app = router(state.clone());
        let hidden = default_app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(hidden.status(), StatusCode::NOT_FOUND);

        let app = router_with_metrics(state);
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
        assert_eq!(quote_response.status(), StatusCode::PAYMENT_REQUIRED);
        let quote: PaymentRequired = serde_json::from_slice(
            &quote_response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        let mut agent = AgentPayer::new(
            PaymentSession::new(params),
            Policy::new(100, PRICE),
            service,
        );
        let voucher = agent.voucher_header(&quote, &payer_secret).unwrap();

        let rejected = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(VOUCHER_HEADER, "00")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

        let paid = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(VOUCHER_HEADER, voucher)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(paid.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_TYPE],
            "text/plain; version=0.0.4; charset=utf-8"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("ghostline_http_requests_total{route=\"fortune\"} 3"));
        assert!(body.contains("ghostline_payment_required_total 1"));
        assert!(body.contains("ghostline_paid_requests_total 1"));
        assert!(body.contains("ghostline_payment_rejections_total 1"));
        assert!(body.contains("ghostline_channels_configured 1"));
        assert!(body.contains("ghostline_ledgers_with_accepted_vouchers 1"));
        assert!(!body.contains(&channel_id));
        assert!(!body.contains("100"));
    }

    #[tokio::test]
    async fn metrics_count_durable_storage_failure_without_accepting_payment() {
        let (payer_secret, payer_public) = create_key_pair_from_seed(21);
        let payer = payer_public.to_byte_type();
        let (_, service_public) = create_key_pair_from_seed(22);
        let service = service_public.to_byte_type();
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 100,
            payer_root: Hash32::from_array([8u8; 32]),
            nonce: [4u8; NONCE_LEN],
        };
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("ghostline-metrics-{}-{unique}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let store = ChannelStore::create(dir.join("ledger.json"), params.clone()).unwrap();
        let state = AppState::with_store(store, PRICE)
            .unwrap()
            .with_payment_terms(PaymentTerms::new(service, 20, 100).unwrap());
        let app = router_with_metrics(state);
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
        let quote: PaymentRequired = serde_json::from_slice(
            &quote_response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        let mut agent = AgentPayer::new(
            PaymentSession::new(params),
            Policy::new(100, PRICE),
            service,
        );
        let voucher = agent.voucher_header(&quote, &payer_secret).unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::write(&dir, b"not a directory").unwrap();
        let paid = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(VOUCHER_HEADER, voucher)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(paid.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("ghostline_payment_required_total 1"));
        assert!(body.contains("ghostline_paid_requests_total 0"));
        assert!(body.contains("ghostline_internal_errors_total 1"));
        assert!(body.contains("ghostline_ledgers_with_accepted_vouchers 0"));
        std::fs::remove_file(dir).unwrap();
    }

    #[tokio::test]
    async fn thousand_paid_calls_preserve_cumulative_accounting() {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let payer = payer_public.to_byte_type();
        let (_, service_public) = create_key_pair_from_seed(12);
        let service = service_public.to_byte_type();
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 10_001,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        let terms = PaymentTerms::new(service, 20, 100).unwrap();
        let state = AppState::new(ServiceLedger::new(params.clone())).with_payment_terms(terms);
        let observer = state.clone();
        let app = router(state);
        let mut agent = AgentPayer::new(
            PaymentSession::new(params),
            Policy::new(10_000, PRICE),
            service,
        );
        let started = Instant::now();

        for expected_total in (PRICE..=10_000).step_by(PRICE as usize) {
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
            assert_eq!(quote_response.status(), StatusCode::PAYMENT_REQUIRED);
            let quote_bytes = quote_response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes();
            let quote: PaymentRequired = serde_json::from_slice(&quote_bytes).unwrap();
            let voucher = agent.voucher_header(&quote, &payer_secret).unwrap();

            let paid = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/fortune")
                        .header(VOUCHER_HEADER, voucher)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(paid.status(), StatusCode::OK);
            let ledger = observer.ledger.lock().await;
            assert_eq!(ledger.accepted(), expected_total);
            assert_eq!(agent.session().cumulative(), expected_total);
        }

        let elapsed = started.elapsed();
        eprintln!(
            "1,000 sequential paid calls completed in {:.2}s ({:.1} paid calls/s)",
            elapsed.as_secs_f64(),
            1_000.0 / elapsed.as_secs_f64()
        );
        assert_eq!(observer.ledger.lock().await.remaining(), 0);
    }
}
