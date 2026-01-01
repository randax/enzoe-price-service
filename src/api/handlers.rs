use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::Utc;

use crate::metrics;

use super::dto::{
    BackfillRequest, BackfillResponse, CountriesResponse, CountryInfo, CountryPricesResponse,
    DateRangeQuery, FetchResponse, GapInfo, HealthResponse, LatestPricesResponse, ReadyResponse,
    TimezoneQuery, ZoneInfo, ZonePricesResponse, ZonesResponse,
};
use super::error::{AppError, AppErrorWithContext};
use super::middleware::CorrelationId;
use super::routes::AppState;

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: Utc::now(),
    })
}

pub async fn ready_check(
    State(state): State<AppState>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<ReadyResponse>, AppErrorWithContext> {
    let start = Instant::now();
    let result = state.repository.health_check().await;
    metrics::record_db_query_duration("health_check", start.elapsed());

    match result {
        Ok(_) => Ok(Json(ReadyResponse {
            status: "ready".to_string(),
            database: "connected".to_string(),
            timestamp: Utc::now(),
        })),
        Err(err) => Err(AppError::DatabaseError(err).with_correlation_id(Some(correlation_id.0))),
    }
}

pub async fn get_prices_by_zone(
    State(state): State<AppState>,
    Path(zone_code): Path<String>,
    Query(query): Query<DateRangeQuery>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<ZonePricesResponse>, AppErrorWithContext> {
    let cid = Some(correlation_id.0.clone());
    let (start, end) = query
        .parse()
        .map_err(|e| AppError::BadRequest(e).with_correlation_id(cid.clone()))?;

    let zone_start = Instant::now();
    let zone = state
        .repository
        .get_zone_by_code(&zone_code)
        .await
        .map_err(|e| AppError::from(e).with_correlation_id(cid.clone()))?;
    metrics::record_db_query_duration("get_zone_by_code", zone_start.elapsed());

    let prices_start = Instant::now();
    let prices = state
        .repository
        .get_prices_by_zone(&zone_code, start, end)
        .await
        .map_err(|e| AppError::from(e).with_correlation_id(cid.clone()))?;
    metrics::record_db_query_duration("get_prices_by_zone", prices_start.elapsed());

    Ok(Json(ZonePricesResponse::new(&zone, prices, query.timezone.as_deref())))
}

pub async fn get_prices_by_country(
    State(state): State<AppState>,
    Path(country_code): Path<String>,
    Query(query): Query<DateRangeQuery>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<CountryPricesResponse>, AppErrorWithContext> {
    let cid = Some(correlation_id.0.clone());
    let (start, end) = query
        .parse()
        .map_err(|e| AppError::BadRequest(e).with_correlation_id(cid.clone()))?;

    let zones_start = Instant::now();
    let zones = state
        .repository
        .get_zones_by_country(&country_code)
        .await
        .map_err(|e| AppError::from(e).with_correlation_id(cid.clone()))?;
    metrics::record_db_query_duration("get_zones_by_country", zones_start.elapsed());

    if zones.is_empty() {
        return Err(AppError::NotFound(format!(
            "Country not found: {}",
            country_code
        ))
        .with_correlation_id(cid));
    }

    let country_name = zones.first().map(|z| z.country_name.clone()).unwrap();
    let prices_start = Instant::now();
    let prices_by_zone = state
        .repository
        .get_prices_by_country(&country_code, start, end)
        .await
        .map_err(|e| AppError::from(e).with_correlation_id(cid.clone()))?;
    metrics::record_db_query_duration("get_prices_by_country", prices_start.elapsed());

    Ok(Json(CountryPricesResponse::new(
        country_code,
        country_name,
        &zones,
        prices_by_zone,
        query.timezone.as_deref(),
    )))
}

pub async fn get_latest_prices(
    State(state): State<AppState>,
    Query(query): Query<TimezoneQuery>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<LatestPricesResponse>, AppErrorWithContext> {
    let cid = Some(correlation_id.0.clone());

    let prices_start = Instant::now();
    let prices = state
        .repository
        .get_latest_prices(Some(24))
        .await
        .map_err(|e| AppError::from(e).with_correlation_id(cid.clone()))?;
    metrics::record_db_query_duration("get_latest_prices", prices_start.elapsed());

    let zones_start = Instant::now();
    let zones = state
        .repository
        .load_zones()
        .await
        .map_err(|e| AppError::from(e).with_correlation_id(cid.clone()))?;
    metrics::record_db_query_duration("load_zones", zones_start.elapsed());

    Ok(Json(LatestPricesResponse::new(prices, &zones, query.timezone.as_deref())))
}

pub async fn list_zones(
    State(state): State<AppState>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<ZonesResponse>, AppErrorWithContext> {
    let cid = Some(correlation_id.0.clone());

    let start = Instant::now();
    let zones = state
        .repository
        .load_zones()
        .await
        .map_err(|e| AppError::from(e).with_correlation_id(cid.clone()))?;
    metrics::record_db_query_duration("load_zones", start.elapsed());

    let zone_infos: Vec<ZoneInfo> = zones.iter().map(ZoneInfo::from).collect();

    Ok(Json(ZonesResponse { zones: zone_infos }))
}

pub async fn list_countries(
    State(state): State<AppState>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<CountriesResponse>, AppErrorWithContext> {
    let cid = Some(correlation_id.0.clone());

    let start = Instant::now();
    let countries = state
        .repository
        .get_countries()
        .await
        .map_err(|e| AppError::from(e).with_correlation_id(cid.clone()))?;
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

pub async fn trigger_fetch(
    State(state): State<AppState>,
    Extension(correlation_id): Extension<CorrelationId>,
) -> Result<Json<FetchResponse>, AppErrorWithContext> {
    let cid = Some(correlation_id.0.clone());

    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("Fetcher not configured".into()).with_correlation_id(cid.clone()))?;

    let start = Instant::now();
    let summary = fetcher
        .fetch_all_prices()
        .await
        .map_err(|e| AppError::InternalError(e.to_string()).with_correlation_id(cid.clone()))?;

    Ok(Json(FetchResponse {
        status: if summary.failed == 0 {
            "success".to_string()
        } else {
            "partial".to_string()
        },
        succeeded: summary.succeeded,
        failed: summary.failed,
        no_data: summary.no_data,
        total_prices_stored: summary.total_prices_stored,
        errors: summary.errors,
        duration_ms: start.elapsed().as_millis() as u64,
    }))
}

pub async fn backfill_prices(
    State(state): State<AppState>,
    Extension(correlation_id): Extension<CorrelationId>,
    Json(request): Json<BackfillRequest>,
) -> Result<Json<BackfillResponse>, AppErrorWithContext> {
    let cid = Some(correlation_id.0.clone());

    let fetcher = state
        .fetcher
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("Fetcher not configured".into()).with_correlation_id(cid.clone()))?;

    // Parse dates
    let start_date = chrono::NaiveDate::parse_from_str(&request.start, "%Y-%m-%d")
        .map_err(|e| AppError::BadRequest(format!("Invalid start date: {}. Use YYYY-MM-DD format.", e)).with_correlation_id(cid.clone()))?;
    
    let end_date = chrono::NaiveDate::parse_from_str(&request.end, "%Y-%m-%d")
        .map_err(|e| AppError::BadRequest(format!("Invalid end date: {}. Use YYYY-MM-DD format.", e)).with_correlation_id(cid.clone()))?;

    if start_date > end_date {
        return Err(AppError::BadRequest("Start date must be before or equal to end date".into()).with_correlation_id(cid));
    }

    let start = Instant::now();
    let summary = fetcher
        .backfill_missing(start_date, end_date, request.zones)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()).with_correlation_id(cid.clone()))?;

    let status = if summary.errors.is_empty() {
        if summary.dates_with_gaps == 0 {
            "no_gaps".to_string()
        } else {
            "success".to_string()
        }
    } else {
        "partial".to_string()
    };

    Ok(Json(BackfillResponse {
        status,
        dates_checked: summary.dates_checked,
        dates_with_gaps: summary.dates_with_gaps,
        prices_fetched: summary.prices_fetched,
        prices_stored: summary.prices_stored,
        gaps_found: summary.gaps_found.into_iter().map(|(date, zone, missing)| GapInfo {
            date: date.to_string(),
            zone,
            missing_hours: missing as i32,
        }).collect(),
        errors: summary.errors,
        duration_ms: start.elapsed().as_millis() as u64,
    }))
}
