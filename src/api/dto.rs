use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::models::{BiddingZone, Price};

#[derive(Debug, Serialize)]
pub struct PricePoint {
    pub timestamp: DateTime<Utc>,
    pub price: Decimal,
}

impl From<&Price> for PricePoint {
    fn from(p: &Price) -> Self {
        Self {
            timestamp: p.timestamp,
            price: p.price_kwh,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ZonePricesResponse {
    pub zone_code: String,
    pub zone_name: String,
    pub country_code: String,
    pub country_name: String,
    pub currency: String,
    pub unit: String,
    pub prices: Vec<PricePoint>,
    pub fetched_at: DateTime<Utc>,
}

impl ZonePricesResponse {
    pub fn new(zone: &BiddingZone, prices: Vec<Price>) -> Self {
        Self {
            zone_code: zone.zone_code.clone(),
            zone_name: zone.zone_name.clone(),
            country_code: zone.country_code.clone(),
            country_name: zone.country_name.clone(),
            currency: "EUR".to_string(),
            unit: "kWh".to_string(),
            prices: prices.iter().map(PricePoint::from).collect(),
            fetched_at: Utc::now(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ZonePrices {
    pub zone_code: String,
    pub zone_name: String,
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
    ) -> Self {
        let zone_prices: Vec<ZonePrices> = zones
            .iter()
            .filter_map(|zone| {
                prices_by_zone.get(&zone.zone_code).map(|prices| ZonePrices {
                    zone_code: zone.zone_code.clone(),
                    zone_name: zone.zone_name.clone(),
                    prices: prices.iter().map(PricePoint::from).collect(),
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
    pub timestamp: DateTime<Utc>,
    pub price: Decimal,
}

#[derive(Debug, Serialize)]
pub struct LatestPricesResponse {
    pub prices: Vec<LatestPriceEntry>,
    pub fetched_at: DateTime<Utc>,
}

impl LatestPricesResponse {
    pub fn new(prices: Vec<Price>, zones: &[BiddingZone]) -> Self {
        let zone_map: HashMap<&str, &BiddingZone> = zones
            .iter()
            .map(|z| (z.zone_code.as_str(), z))
            .collect();

        let entries: Vec<LatestPriceEntry> = prices
            .into_iter()
            .filter_map(|p| {
                zone_map.get(p.bidding_zone.as_str()).map(|zone| LatestPriceEntry {
                    zone_code: p.bidding_zone,
                    zone_name: zone.zone_name.clone(),
                    country_code: zone.country_code.clone(),
                    timestamp: p.timestamp,
                    price: p.price_kwh,
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
pub struct DateRangeQuery {
    pub start: Option<String>,
    pub end: Option<String>,
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
