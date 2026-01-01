use std::sync::Arc;

use axum::{routing::{get, post}, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::fetcher::FetcherService;
use crate::storage::PriceRepository;

use super::handlers;
use super::middleware::{CorrelationIdLayer, MetricsLayer};

#[derive(Clone)]
pub struct AppState {
    pub repository: Arc<PriceRepository>,
    pub metrics_handle: PrometheusHandle,
    pub fetcher: Option<Arc<FetcherService>>,
}

async fn metrics_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> String {
    state.metrics_handle.render()
}

pub fn create_router(
    repository: Arc<PriceRepository>,
    metrics_handle: PrometheusHandle,
    fetcher: Option<Arc<FetcherService>>,
) -> Router {
    let state = AppState {
        repository,
        metrics_handle,
        fetcher,
    };

    let api_routes = Router::new()
        .route("/prices/zone/{zone}", get(handlers::get_prices_by_zone))
        .route(
            "/prices/country/{country}",
            get(handlers::get_prices_by_country),
        )
        .route("/prices/latest", get(handlers::get_latest_prices))
        .route("/zones", get(handlers::list_zones))
        .route("/countries", get(handlers::list_countries));

    let admin_routes = Router::new()
        .route("/fetch", post(handlers::trigger_fetch));

    let cors = if std::env::var("APP_ENV").as_deref() == Ok("development") {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
            .allow_headers([axum::http::header::CONTENT_TYPE])
            .allow_origin(["https://your-ui.example.com".parse().unwrap()])
    };

    Router::new()
        .route("/health", get(handlers::health_check))
        .route("/ready", get(handlers::ready_check))
        .route("/metrics", get(metrics_handler))
        .nest("/api/v1", api_routes)
        .nest("/api/v1/admin", admin_routes)
        .layer(CorrelationIdLayer)
        .layer(MetricsLayer)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
