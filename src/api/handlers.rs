use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::Utc;

use crate::metrics;

use super::dto::{
    CountriesResponse, CountryInfo, CountryPricesResponse, DateRangeQuery, HealthResponse,
    LatestPricesResponse, ReadyResponse, ZoneInfo, ZonePricesResponse, ZonesResponse,
};
use super::error::AppError;
use super::middleware::CorrelationId;
use super::routes::AppState;

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: Utc::now(),
    })
}

pub async fn ready_check(State(state): State<AppState>) -> Result<Json<ReadyResponse>, AppError> {
    let start = Instant::now();
    let result = state.repository.health_check().await;
    metrics::record_db_query_duration("health_check", start.elapsed());

    match result {
        Ok(_) => Ok(Json(ReadyResponse {
            status: "ready".to_string(),
            database: "connected".to_string(),
            timestamp: Utc::now(),
        })),
        Err(err) => Err(AppError::DatabaseError(err)),
    }
}

pub async fn get_prices_by_zone(
    State(state): State<AppState>,
    Path(zone_code): Path<String>,
    Query(query): Query<DateRangeQuery>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<ZonePricesResponse>, AppError> {
    let _ = correlation_id;
    let (start, end) = query.parse().map_err(AppError::BadRequest)?;

    let zone_start = Instant::now();
    let zone = state.repository.get_zone_by_code(&zone_code).await?;
    metrics::record_db_query_duration("get_zone_by_code", zone_start.elapsed());

    let prices_start = Instant::now();
    let prices = state
        .repository
        .get_prices_by_zone(&zone_code, start, end)
        .await?;
    metrics::record_db_query_duration("get_prices_by_zone", prices_start.elapsed());

    Ok(Json(ZonePricesResponse::new(&zone, prices)))
}

pub async fn get_prices_by_country(
    State(state): State<AppState>,
    Path(country_code): Path<String>,
    Query(query): Query<DateRangeQuery>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<CountryPricesResponse>, AppError> {
    let _ = correlation_id;
    let (start, end) = query.parse().map_err(AppError::BadRequest)?;

    let zones_start = Instant::now();
    let zones = state
        .repository
        .get_zones_by_country(&country_code)
        .await?;
    metrics::record_db_query_duration("get_zones_by_country", zones_start.elapsed());

    if zones.is_empty() {
        return Err(AppError::NotFound(format!(
            "Country not found: {}",
            country_code
        )));
    }

    let country_name = zones.first().map(|z| z.country_name.clone()).unwrap();
    let prices_start = Instant::now();
    let prices_by_zone = state
        .repository
        .get_prices_by_country(&country_code, start, end)
        .await?;
    metrics::record_db_query_duration("get_prices_by_country", prices_start.elapsed());

    Ok(Json(CountryPricesResponse::new(
        country_code,
        country_name,
        &zones,
        prices_by_zone,
    )))
}

pub async fn get_latest_prices(
    State(state): State<AppState>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<LatestPricesResponse>, AppError> {
    let _ = correlation_id;

    let prices_start = Instant::now();
    let prices = state.repository.get_latest_prices(Some(24)).await?;
    metrics::record_db_query_duration("get_latest_prices", prices_start.elapsed());

    let zones_start = Instant::now();
    let zones = state.repository.load_zones().await?;
    metrics::record_db_query_duration("load_zones", zones_start.elapsed());

    Ok(Json(LatestPricesResponse::new(prices, &zones)))
}

pub async fn list_zones(
    State(state): State<AppState>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<ZonesResponse>, AppError> {
    let _ = correlation_id;

    let start = Instant::now();
    let zones = state.repository.load_zones().await?;
    metrics::record_db_query_duration("load_zones", start.elapsed());

    let zone_infos: Vec<ZoneInfo> = zones.iter().map(ZoneInfo::from).collect();

    Ok(Json(ZonesResponse { zones: zone_infos }))
}

pub async fn list_countries(
    State(state): State<AppState>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<CountriesResponse>, AppError> {
    let _ = correlation_id;

    let start = Instant::now();
    let countries = state.repository.get_countries().await?;
    metrics::record_db_query_duration("get_countries", start.elapsed());

    let country_infos: Vec<CountryInfo> = countries
        .into_iter()
        .map(|(code, name)| CountryInfo {
            country_code: code,
            country_name: name,
        })
        .collect();

    Ok(Json(CountriesResponse {
        countries: country_infos,
    }))
}
