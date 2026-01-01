use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::models::{BiddingZone, Price};

#[derive(Debug, Serialize)]
pub struct PricePoint {
    pub timestamp: String,
    pub timestamp_utc: DateTime<Utc>,
    pub price: Decimal,
}

impl PricePoint {
    pub fn new(price: &Price, tz: &Tz) -> Self {
        let local_time = price.timestamp.with_timezone(tz);
        Self {
            timestamp: local_time.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
            timestamp_utc: price.timestamp,
            price: price.price_kwh,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ZonePricesResponse {
    pub zone_code: String,
    pub zone_name: String,
    pub country_code: String,
    pub country_name: String,
    pub timezone: String,
    pub currency: String,
    pub unit: String,
    pub prices: Vec<PricePoint>,
    pub fetched_at: DateTime<Utc>,
}

impl ZonePricesResponse {
    pub fn new(zone: &BiddingZone, prices: Vec<Price>, timezone: Option<&str>) -> Self {
        let tz: Tz = timezone
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| zone.timezone.parse().unwrap_or(chrono_tz::UTC));
        
        Self {
            zone_code: zone.zone_code.clone(),
            zone_name: zone.zone_name.clone(),
            country_code: zone.country_code.clone(),
            country_name: zone.country_name.clone(),
            timezone: tz.to_string(),
            currency: "EUR".to_string(),
            unit: "kWh".to_string(),
            prices: prices.iter().map(|p| PricePoint::new(p, &tz)).collect(),
            fetched_at: Utc::now(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ZonePrices {
    pub zone_code: String,
    pub zone_name: String,
    pub timezone: String,
    pub prices: Vec<PricePoint>,
}

#[derive(Debug, Serialize)]
pub struct CountryPricesResponse {
    pub country_code: String,
    pub country_name: String,
    pub currency: String,
    pub unit: String,
    pub zones: Vec<ZonePrices>,
    pub fetched_at: DateTime<Utc>,
}

impl CountryPricesResponse {
    pub fn new(
        country_code: String,
        country_name: String,
        zones: &[BiddingZone],
        prices_by_zone: HashMap<String, Vec<Price>>,
        timezone: Option<&str>,
    ) -> Self {
        let zone_prices: Vec<ZonePrices> = zones
            .iter()
            .filter_map(|zone| {
                let tz: Tz = timezone
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| zone.timezone.parse().unwrap_or(chrono_tz::UTC));
                
                prices_by_zone.get(&zone.zone_code).map(|prices| ZonePrices {
                    zone_code: zone.zone_code.clone(),
                    zone_name: zone.zone_name.clone(),
                    timezone: tz.to_string(),
                    prices: prices.iter().map(|p| PricePoint::new(p, &tz)).collect(),
                })
            })
            .collect();

        Self {
            country_code,
            country_name,
            currency: "EUR".to_string(),
            unit: "kWh".to_string(),
            zones: zone_prices,
            fetched_at: Utc::now(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LatestPriceEntry {
    pub zone_code: String,
    pub zone_name: String,
    pub country_code: String,
    pub timezone: String,
    pub timestamp: String,
    pub timestamp_utc: DateTime<Utc>,
    pub price: Decimal,
}

#[derive(Debug, Serialize)]
pub struct LatestPricesResponse {
    pub prices: Vec<LatestPriceEntry>,
    pub fetched_at: DateTime<Utc>,
}

impl LatestPricesResponse {
    pub fn new(prices: Vec<Price>, zones: &[BiddingZone], timezone: Option<&str>) -> Self {
        let zone_map: HashMap<&str, &BiddingZone> = zones
            .iter()
            .map(|z| (z.zone_code.as_str(), z))
            .collect();

        let entries: Vec<LatestPriceEntry> = prices
            .into_iter()
            .filter_map(|p| {
                zone_map.get(p.bidding_zone.as_str()).map(|zone| {
                    let tz: Tz = timezone
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(|| zone.timezone.parse().unwrap_or(chrono_tz::UTC));
                    let local_time = p.timestamp.with_timezone(&tz);
                    
                    LatestPriceEntry {
                        zone_code: p.bidding_zone,
                        zone_name: zone.zone_name.clone(),
                        country_code: zone.country_code.clone(),
                        timezone: tz.to_string(),
                        timestamp: local_time.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
                        timestamp_utc: p.timestamp,
                        price: p.price_kwh,
                    }
                })
            })
            .collect();

        Self {
            prices: entries,
            fetched_at: Utc::now(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ZoneInfo {
    pub zone_code: String,
    pub zone_name: String,
    pub country_code: String,
    pub country_name: String,
    pub eic_code: String,
    pub timezone: String,
    pub active: bool,
}

impl From<&BiddingZone> for ZoneInfo {
    fn from(z: &BiddingZone) -> Self {
        Self {
            zone_code: z.zone_code.clone(),
            zone_name: z.zone_name.clone(),
            country_code: z.country_code.clone(),
            country_name: z.country_name.clone(),
            eic_code: z.eic_code.clone(),
            timezone: z.timezone.clone(),
            active: z.active,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ZonesResponse {
    pub zones: Vec<ZoneInfo>,
}

#[derive(Debug, Serialize)]
pub struct CountryInfo {
    pub country_code: String,
    pub country_name: String,
}

#[derive(Debug, Serialize)]
pub struct CountriesResponse {
    pub countries: Vec<CountryInfo>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ReadyResponse {
    pub status: String,
    pub database: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct TimezoneQuery {
    pub timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DateRangeQuery {
    pub start: Option<String>,
    pub end: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FetchResponse {
    pub status: String,
    pub succeeded: usize,
    pub failed: usize,
    pub no_data: usize,
    pub total_prices_stored: usize,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct BackfillRequest {
    pub start: String,
    pub end: String,
    pub zones: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct GapInfo {
    pub date: String,
    pub zone: String,
    pub missing_hours: i32,
}

#[derive(Debug, Serialize)]
pub struct BackfillResponse {
    pub status: String,
    pub dates_checked: usize,
    pub dates_with_gaps: usize,
    pub prices_fetched: usize,
    pub prices_stored: usize,
    pub gaps_found: Vec<GapInfo>,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

impl DateRangeQuery {
    pub fn parse(&self) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
        let start = match &self.start {
            Some(s) => DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| format!("Invalid start date format: {}. Use ISO8601/RFC3339.", e))?,
            None => Utc::now() - Duration::days(7),
        };

        let end = match &self.end {
            Some(s) => DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| format!("Invalid end date format: {}. Use ISO8601/RFC3339.", e))?,
            None => {
                let tomorrow = Utc::now().date_naive().succ_opt().unwrap();
                tomorrow
                    .and_hms_opt(23, 59, 59)
                    .unwrap()
                    .and_utc()
            }
        };

        if start >= end {
            return Err("Start date must be before end date".to_string());
        }

        Ok((start, end))
    }
}
