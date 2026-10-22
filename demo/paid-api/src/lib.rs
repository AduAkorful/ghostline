// SPDX-License-Identifier: MIT

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::Response,
};
use ghostline_core::{ChannelParams, ServiceLedger, VoucherError};
use ghostline_paywall::{
    CHANNEL_HEADER, ChannelDirectory, ChannelStore, PaymentRejection, PaymentTerms, PriceKind,
    VOUCHER_HEADER, charge, charge_to_output_key, quote,
};
use serde::{Deserialize, Serialize};
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};
use tokio::sync::Mutex;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, ExposeHeaders};

mod metrics;
mod recovery;
mod relay;

use metrics::ServiceMetrics;

pub use recovery::{
    RecoveryError, key_from_hex as recovery_key_from_hex, open as open_recovery_receipt,
    seal as seal_recovery_receipt,
};

const PRICE: u64 = 10;

#[derive(Clone)]
pub struct AppState {
    ledger: Arc<Mutex<ServiceLedger>>,
    store: Option<Arc<Mutex<ChannelStore>>>,
    directory: Option<Arc<Mutex<ChannelDirectory>>>,
    price: u64,
    payment_terms: Option<PaymentTerms>,
    /// Per-channel terms let a relay rotate authorization and payout keys for every demo.
    channel_terms: Arc<Mutex<HashMap<String, PaymentTerms>>>,
    metrics: Arc<ServiceMetrics>,
    configured_channels: Arc<AtomicUsize>,
    demo_usage: Arc<Mutex<DemoUsage>>,
    relay_registry: relay::RelayRegistry,
}

impl AppState {
    pub fn new(ledger: ServiceLedger) -> Self {
        Self {
            ledger: Arc::new(Mutex::new(ledger)),
            store: None,
            directory: None,
            price: PRICE,
            payment_terms: None,
            channel_terms: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(ServiceMetrics::default()),
            configured_channels: Arc::new(AtomicUsize::new(1)),
            demo_usage: Arc::new(Mutex::new(DemoUsage::default())),
            relay_registry: relay::RelayRegistry::default(),
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
            channel_terms: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(ServiceMetrics::default()),
            configured_channels: Arc::new(AtomicUsize::new(1)),
            demo_usage: Arc::new(Mutex::new(DemoUsage::default())),
            relay_registry: relay::RelayRegistry::default(),
        })
    }

    /// Serve every ledger in `directory`. An unpaid request names one channel, or the only channel.
    pub fn with_directory(
        directory: ChannelDirectory,
        price: u64,
    ) -> Result<Self, ghostline_paywall::PaywallError> {
        let configured_channels = directory.len();
        let params = if directory.is_empty() {
            // The fallback ledger is unused while a directory is configured. It lets a relay
            // start with no channels and register the first one after a successful wallet open.
            ChannelParams {
                network_tag: 0,
                payer: RistrettoPublicKeyBytes::from_bytes(&[1u8; 32]).map_err(|_| {
                    ghostline_paywall::PaywallError::Voucher(VoucherError::Malformed)
                })?,
                deposit: 1,
                payer_root: Hash32::from_array([0; 32]),
                nonce: [0; ghostline_core::NONCE_LEN],
            }
        } else {
            directory.first_params()?
        };
        Ok(Self {
            ledger: Arc::new(Mutex::new(ServiceLedger::new(params))),
            store: None,
            directory: Some(Arc::new(Mutex::new(directory))),
            price,
            payment_terms: None,
            channel_terms: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(ServiceMetrics::default()),
            configured_channels: Arc::new(AtomicUsize::new(configured_channels)),
            demo_usage: Arc::new(Mutex::new(DemoUsage::default())),
            relay_registry: relay::RelayRegistry::default(),
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

    /// Restore relay transaction metadata and per-channel quote keys from a private state file.
    /// The channel directory must be opened first so its persisted ledgers are available.
    pub async fn restore_relay_state(&mut self, path: PathBuf) -> Result<(), String> {
        let registry = relay::RelayRegistry::open(path).await?;
        let terms = registry.payment_terms().await?;
        let mut channel_terms = self.channel_terms.lock().await;
        for (channel, terms) in terms {
            channel_terms.insert(channel, terms);
        }
        drop(channel_terms);
        self.relay_registry = registry;
        Ok(())
    }

    /// Submit the latest accepted voucher for each still-open relay channel during graceful shutdown.
    pub async fn settle_pending_relay_channels(&self) {
        self.relay_registry.settle_pending_on_shutdown().await;
    }

    /// Register a channel created while the API is running.
    pub async fn register_channel(
        &self,
        store: ChannelStore,
    ) -> Result<[u8; ghostline_core::CHANNEL_ID_LEN], ghostline_paywall::PaywallError> {
        let directory = self.directory.as_ref().ok_or_else(|| {
            ghostline_paywall::PaywallError::Storage(
                "dynamic channel registration requires a channel directory".into(),
            )
        })?;
        let channel_id = directory.lock().await.insert(store)?;
        self.configured_channels.fetch_add(1, Ordering::Relaxed);
        Ok(channel_id)
    }

    /// Register a fresh channel and the terms used to quote and validate its vouchers.
    ///
    /// `terms` should contain per-channel authorization and payout public keys. Keeping the
    /// mapping beside the channel directory prevents a rotated key from being applied to a
    /// different payer's channel.
    pub async fn register_channel_with_terms(
        &self,
        store: ChannelStore,
        terms: PaymentTerms,
    ) -> Result<[u8; ghostline_core::CHANNEL_ID_LEN], ghostline_paywall::PaywallError> {
        let channel_id = self.register_channel(store).await?;
        self.channel_terms
            .lock()
            .await
            .insert(ghostline_core::to_hex(&channel_id), terms);
        Ok(channel_id)
    }

    async fn terms_for_channel(&self, channel_id: &str) -> Option<PaymentTerms> {
        self.channel_terms
            .lock()
            .await
            .get(channel_id)
            .cloned()
            .or_else(|| self.payment_terms.clone())
    }
}

#[derive(Default)]
struct DemoUsage {
    day: u64,
    accepted_calls: HashMap<String, u32>,
}

impl DemoUsage {
    fn current_day() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 86_400
    }

    fn roll_day(&mut self) {
        let day = Self::current_day();
        if self.day != day {
            self.day = day;
            self.accepted_calls.clear();
        }
    }

    fn can_accept(&mut self, channel_id: &str) -> bool {
        self.roll_day();
        self.accepted_calls
            .get(channel_id)
            .copied()
            .unwrap_or_default()
            < 100
            && (self.accepted_calls.contains_key(channel_id) || self.accepted_calls.len() < 50)
    }

    fn record(&mut self, channel_id: String) {
        self.roll_day();
        *self.accepted_calls.entry(channel_id).or_default() += 1;
    }
}

fn demo_mode() -> bool {
    std::env::var("GHOSTLINE_DEMO_MODE").is_ok_and(|value| value == "1")
}

fn too_many_demo_calls() -> Response {
    (StatusCode::TOO_MANY_REQUESTS, "demo channel limit reached").into_response()
}

#[derive(Serialize)]
struct Fortune {
    fortune: &'static str,
}

pub fn router(state: AppState) -> Router {
    router_inner(state, false, true)
}

/// Build the reference router with the aggregate Prometheus endpoint enabled.
/// Exported metrics intentionally contain no per-channel labels or payment amounts.
pub fn router_with_metrics(state: AppState) -> Router {
    router_inner(state, true, true)
}

/// Router for the CLI's in-process loopback demo. It is never used by the network service.
pub fn local_demo_router(state: AppState) -> Router {
    router_inner(state, false, false)
}

fn router_inner(state: AppState, expose_metrics: bool, rate_limit: bool) -> Router {
    let router = Router::new()
        .route("/fortune", get(fortune))
        .route("/meter", get(meter))
        .route("/healthz", get(healthz))
        .route("/demo/open", axum::routing::post(relay::open))
        .route("/demo/session", axum::routing::post(relay::create_session))
        .route("/demo/recover", axum::routing::post(relay::recover))
        .route("/demo/settle", axum::routing::post(relay::settle))
        .route("/demo/refund", axum::routing::post(relay::refund))
        .route("/demo/config", get(relay::config))
        .route("/demo/status", get(relay::status));
    let router = if expose_metrics {
        router.route("/metrics", get(metrics))
    } else {
        router
    };
    let cors = cors_from_env();
    let router = router.with_state(state).layer(cors);
    if rate_limit {
        router.layer(middleware::from_fn_with_state(
            RateWindow::default(),
            enforce_rate_limit,
        ))
    } else {
        router
    }
}

#[derive(Clone, Default)]
struct RateWindow(Arc<Mutex<HashMap<String, VecDeque<Instant>>>>);

async fn enforce_rate_limit(
    State(window): State<RateWindow>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if path == "/healthz" || request.method() == axum::http::Method::OPTIONS {
        return next.run(request).await;
    }
    let forwarded = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let key = forwarded.unwrap_or("unknown-client").to_owned();
    let now = Instant::now();
    let mut clients = window.0.lock().await;
    if !clients.contains_key(&key) && clients.len() >= 4096 {
        clients.retain(|_, times| {
            times
                .back()
                .is_some_and(|last| now.duration_since(*last) < Duration::from_secs(60))
        });
        if clients.len() >= 4096 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "service request capacity reached",
            )
                .into_response();
        }
    }
    let times = clients.entry(key).or_default();
    while times
        .front()
        .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
    {
        times.pop_front();
    }
    if times.len() >= 30 {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit: 30 requests per minute",
        )
            .into_response();
    }
    times.push_back(now);
    drop(clients);
    next.run(request).await
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    network: String,
    #[serde(rename = "templateAddress")]
    template_address: String,
    version: &'static str,
    #[serde(rename = "walletReady")]
    wallet_ready: bool,
    #[serde(rename = "uptimeSecs")]
    uptime_secs: u64,
}

async fn healthz() -> impl IntoResponse {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    let started = STARTED.get_or_init(Instant::now);
    let wallet_ready = demo_setup_ready() && wallet_daemon_reachable().await;
    let status = if wallet_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(HealthResponse {
            ok: wallet_ready,
            network: std::env::var("GHOSTLINE_NETWORK").unwrap_or_else(|_| "unconfigured".into()),
            template_address: std::env::var("GHOSTLINE_TEMPLATE")
                .unwrap_or_else(|_| "unconfigured".into()),
            version: env!("CARGO_PKG_VERSION"),
            wallet_ready,
            uptime_secs: started.elapsed().as_secs(),
        }),
    )
}

fn demo_setup_ready() -> bool {
    match std::env::var_os("GHOSTLINE_DEMO_READY_FILE") {
        Some(marker) => setup_marker_exists(Some(Path::new(&marker))),
        None => setup_marker_exists(None),
    }
}

fn setup_marker_exists(marker: Option<&Path>) -> bool {
    match marker {
        Some(path) => path.is_file(),
        None => true,
    }
}

async fn wallet_daemon_reachable() -> bool {
    let Ok(url) = std::env::var("GHOSTLINE_WALLET_URL") else {
        return false;
    };
    let authority = url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        return false;
    }
    tokio::time::timeout(
        Duration::from_millis(250),
        tokio::net::TcpStream::connect(authority),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

fn cors_from_env() -> CorsLayer {
    let origins = std::env::var("GHOSTLINE_CORS_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .filter_map(|origin| origin.parse().ok())
        .collect::<Vec<_>>();
    cors_for_origins(origins)
}

fn cors_for_origins(origins: Vec<axum::http::HeaderValue>) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(AllowMethods::list([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderName::from_static("ghostline-voucher"),
            axum::http::HeaderName::from_static("ghostline-channel"),
            axum::http::HeaderName::from_static("ghostline-recovery"),
        ]))
        .expose_headers(ExposeHeaders::list([
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderName::from_static("ghostline-recovery"),
        ]))
        .max_age(Duration::from_secs(600))
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
    let channel_id = ghostline_core::to_hex(&ledger.params().channel_id());
    if voucher.is_some() && demo_mode() && !state.demo_usage.lock().await.can_accept(&channel_id) {
        return too_many_demo_calls();
    }
    let mut next_ledger = ledger.clone();
    let terms = state.terms_for_channel(&channel_id).await;
    let charge_result = match terms.as_ref() {
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
            if voucher.is_some() && demo_mode() {
                state.demo_usage.lock().await.record(channel_id.clone());
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
    let has_voucher = headers.get(VOUCHER_HEADER).is_some();
    if demo_mode()
        && has_voucher
        && let Some(receipt) = headers
            .get("ghostline-recovery")
            .and_then(|value| value.to_str().ok())
        && let Err(error) = relay::restore_from_receipt(state, receipt).await
    {
        tracing::warn!(event = "relay_recovery", outcome = "failed", error = %error);
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PaymentRejection {
                scheme: ghostline_paywall::SCHEME.to_string(),
                status: 422,
                reason: "relay state could not be restored from the saved recovery receipt; preserve it and retry /demo/recover".into(),
            }),
        )
            .into_response();
    }
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
    let channel_id = match directory.channel_for_quote(requested) {
        Ok(id) => ghostline_core::to_hex(&id),
        Err(err) if voucher.is_none() => {
            state.metrics.rejected();
            return (
                StatusCode::from_u16(err.status()).unwrap_or(StatusCode::BAD_REQUEST),
                Json(PaymentRejection::from_error(&err)),
            )
                .into_response();
        }
        Err(_) => String::new(),
    };
    if voucher.is_some() && demo_mode() && !state.demo_usage.lock().await.can_accept(&channel_id) {
        return too_many_demo_calls();
    }
    let terms = state.terms_for_channel(&channel_id).await;
    let charge_result = match terms.as_ref() {
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
            if voucher.is_some() && demo_mode() {
                state.demo_usage.lock().await.record(channel_id.clone());
            }
            state.metrics.paid();
            tracing::info!(event = "payment_request", outcome = "paid");
            drop(directory);
            let mut response = paid_body(units, quoted.price);
            if voucher.is_some() && demo_mode() {
                match state.mint_recovery_receipt(&channel_id).await {
                    Ok(receipt) => {
                        if let Ok(value) = axum::http::HeaderValue::from_str(&receipt) {
                            response.headers_mut().insert("ghostline-recovery", value);
                        } else {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(PaymentRejection {
                                    scheme: ghostline_paywall::SCHEME.to_string(),
                                    status: 500,
                                    reason: "could not encode the recovery receipt; retry the same voucher".into(),
                                }),
                            )
                                .into_response();
                        }
                    }
                    Err(error) => {
                        tracing::error!(event = "recovery_receipt", outcome = "failed", error = %error);
                        return (
                            StatusCode::SERVICE_UNAVAILABLE,
                            Json(PaymentRejection {
                                scheme: ghostline_paywall::SCHEME.to_string(),
                                status: 503,
                                reason: "payment accepted; retry the same voucher to retrieve its recovery receipt".into(),
                            }),
                        )
                            .into_response();
                    }
                }
            }
            response
        }
        Err(ghostline_paywall::PaywallError::MissingVoucher) => {
            match directory.channel_for_quote(requested) {
                Ok(channel_id) => {
                    state.metrics.payment_required();
                    tracing::debug!(event = "payment_request", outcome = "payment_required");
                    let mut body =
                        ghostline_paywall::PaymentRequired::from_quote(&quoted, &channel_id);
                    if let Some(terms) = &terms {
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
    let body = state.metrics.exposition(
        state.configured_channels.load(Ordering::Relaxed),
        ledgers_with_accepted_vouchers,
    );
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
    use tari_utilities::ByteArray;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn demo_health_waits_for_the_startup_ready_marker_when_configured() {
        let marker = std::env::temp_dir().join(format!(
            "ghostline-demo-ready-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(!setup_marker_exists(Some(&marker)));
        std::fs::write(&marker, b"").unwrap();
        assert!(setup_marker_exists(Some(&marker)));
        std::fs::remove_file(marker).unwrap();
        assert!(setup_marker_exists(None));
    }

    #[tokio::test]
    async fn running_api_accepts_a_new_relay_channel() {
        let dir = std::env::temp_dir().join(format!(
            "ghostline-runtime-directory-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let directory = ChannelDirectory::open(&dir).unwrap();
        let state = AppState::with_directory(directory, PRICE).unwrap();
        let (_, payer_public) = create_key_pair_from_seed(80);
        let params = ChannelParams {
            network_tag: 38,
            payer: payer_public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([0u8; 32]),
            nonce: [5u8; NONCE_LEN],
        };
        let store = ChannelStore::create(dir.join("relay.json"), params.clone()).unwrap();

        assert_eq!(
            state.register_channel(store).await.unwrap(),
            params.channel_id()
        );
        let directory = state.directory.as_ref().unwrap().lock().await;
        assert_eq!(directory.len(), 1);
        assert_eq!(
            directory.channel_for_quote(None).unwrap(),
            params.channel_id()
        );
        assert_eq!(state.configured_channels.load(Ordering::Relaxed), 1);
        drop(directory);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn dynamic_channels_use_their_own_rotated_payment_terms() {
        let dir = std::env::temp_dir().join(format!(
            "ghostline-channel-terms-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let directory = ChannelDirectory::open(&dir).unwrap();
        let state = AppState::with_directory(directory, PRICE).unwrap();

        let (payer_secret, payer_public) = create_key_pair_from_seed(82);
        let (_, service_public) = create_key_pair_from_seed(83);
        let (_, payout_public) = create_key_pair_from_seed(84);
        let params = ChannelParams {
            network_tag: 38,
            payer: payer_public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([8u8; 32]),
            nonce: [6u8; NONCE_LEN],
        };
        let store = ChannelStore::create(dir.join("channel.json"), params.clone()).unwrap();
        let terms = PaymentTerms::new(service_public.to_byte_type(), 20, 100)
            .unwrap()
            .with_service_output_key(payout_public.to_byte_type());
        let channel_id = state
            .register_channel_with_terms(store, terms)
            .await
            .unwrap();
        let channel_hex = ghostline_core::to_hex(&channel_id);
        let app = router(state);

        let quote_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(CHANNEL_HEADER, &channel_hex)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(quote_response.status(), StatusCode::PAYMENT_REQUIRED);
        let quote_body: serde_json::Value = serde_json::from_slice(
            &quote_response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(
            quote_body["recipient_key"],
            ghostline_core::to_hex(service_public.as_bytes())
        );
        assert_eq!(
            quote_body["service_output_key"],
            ghostline_core::to_hex(payout_public.as_bytes())
        );

        let voucher = ghostline_core::SignedVoucher::sign(
            &payer_secret,
            &params,
            PRICE,
            payout_public.to_byte_type(),
        )
        .unwrap();
        let (_, wrong_payout_public) = create_key_pair_from_seed(85);
        let wrong_voucher = ghostline_core::SignedVoucher::sign(
            &payer_secret,
            &params,
            PRICE,
            wrong_payout_public.to_byte_type(),
        )
        .unwrap();
        let rejected = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(CHANNEL_HEADER, &channel_hex)
                    .header(
                        VOUCHER_HEADER,
                        ghostline_paywall::encode_voucher_header(&wrong_voucher),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

        let paid_response = app
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header(CHANNEL_HEADER, channel_hex)
                    .header(
                        VOUCHER_HEADER,
                        ghostline_paywall::encode_voucher_header(&voucher),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(paid_response.status(), StatusCode::OK);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn demo_usage_caps_calls_per_channel_and_new_channels_per_day() {
        let mut usage = DemoUsage {
            day: DemoUsage::current_day(),
            accepted_calls: HashMap::new(),
        };
        assert!(usage.can_accept("channel-0"));
        for _ in 0..100 {
            usage.record("channel-0".into());
        }
        assert!(!usage.can_accept("channel-0"));
        for index in 1..50 {
            let id = format!("channel-{index}");
            assert!(usage.can_accept(&id));
            usage.record(id);
        }
        assert!(!usage.can_accept("channel-50"));
        assert!(usage.can_accept("channel-1"));
    }

    #[tokio::test]
    async fn health_endpoint_is_readable_without_a_wallet_or_chain_call() {
        let (_, payer_public) = create_key_pair_from_seed(81);
        let params = ChannelParams {
            network_tag: 38,
            payer: payer_public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([0u8; 32]),
            nonce: [4u8; NONCE_LEN],
        };
        let response = router(AppState::new(ServiceLedger::new(params)))
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::OK | StatusCode::SERVICE_UNAVAILABLE
        ));
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let health: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(health["ok"], health["walletReady"]);
        assert_eq!(
            status,
            if health["walletReady"] == true {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        );
        assert!(health.get("network").is_some());
        assert!(health.get("templateAddress").is_some());
        assert!(health.get("version").is_some());
        assert!(health.get("walletReady").is_some());
        assert!(health.get("uptimeSecs").is_some());
    }

    #[tokio::test]
    async fn requests_are_limited_per_forwarded_ip_but_health_is_exempt() {
        let (_, payer_public) = create_key_pair_from_seed(82);
        let params = ChannelParams {
            network_tag: 38,
            payer: payer_public.to_byte_type(),
            deposit: 100,
            payer_root: Hash32::from_array([0u8; 32]),
            nonce: [5u8; NONCE_LEN],
        };
        let app = router(AppState::new(ServiceLedger::new(params)));
        for index in 0..30 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/fortune")
                        .header("x-forwarded-for", format!("203.0.113.10, 10.0.0.{index}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
        }
        let limited = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/fortune")
                    .header("x-forwarded-for", "203.0.113.10, 10.0.0.31")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);

        let health = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .header("x-forwarded-for", "203.0.113.10, 10.0.0.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(matches!(
            health.status(),
            StatusCode::OK | StatusCode::SERVICE_UNAVAILABLE
        ));
    }

    #[tokio::test]
    async fn cors_accepts_only_the_configured_exact_origin() {
        let app = Router::new()
            .route("/fortune", get(|| async { StatusCode::OK }))
            .layer(cors_for_origins(vec![
                "https://ghostline-ui.vercel.app".parse().unwrap(),
            ]));
        let allowed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(axum::http::Method::OPTIONS)
                    .uri("/fortune")
                    .header("origin", "https://ghostline-ui.vercel.app")
                    .header("access-control-request-method", "POST")
                    .header(
                        "access-control-request-headers",
                        "content-type,ghostline-voucher",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
        assert_eq!(
            allowed.headers()[axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://ghostline-ui.vercel.app"
        );

        let rejected = app
            .oneshot(
                Request::builder()
                    .method(axum::http::Method::OPTIONS)
                    .uri("/fortune")
                    .header("origin", "https://evil.example")
                    .header("access-control-request-method", "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            !rejected
                .headers()
                .contains_key(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
        );
    }

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

        for (index, expected_total) in (PRICE..=10_000).step_by(PRICE as usize).enumerate() {
            let quote_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/fortune")
                        .header("x-forwarded-for", format!("198.51.100.{index}"))
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
                        .header("x-forwarded-for", format!("198.51.100.{index}"))
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
