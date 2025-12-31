use std::sync::Arc;

use axum::{routing::get, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::storage::PriceRepository;

use super::handlers;
use super::middleware::{CorrelationIdLayer, MetricsLayer};

#[derive(Clone)]
pub struct AppState {
    pub repository: Arc<PriceRepository>,
    pub metrics_handle: PrometheusHandle,
}

async fn metrics_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> String {
    state.metrics_handle.render()
}

pub fn create_router(repository: Arc<PriceRepository>, metrics_handle: PrometheusHandle) -> Router {
    let state = AppState {
        repository,
        metrics_handle,
    };

    let api_routes = Router::new()
        .route("/prices/zone/:zone", get(handlers::get_prices_by_zone))
        .route(
            "/prices/country/:country",
            get(handlers::get_prices_by_country),
        )
        .route("/prices/latest", get(handlers::get_latest_prices))
        .route("/zones", get(handlers::list_zones))
        .route("/countries", get(handlers::list_countries));

    Router::new()
        .route("/health", get(handlers::health_check))
        .route("/ready", get(handlers::ready_check))
        .route("/metrics", get(metrics_handler))
        .nest("/api/v1", api_routes)
        .layer(CorrelationIdLayer)
        .layer(MetricsLayer)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
