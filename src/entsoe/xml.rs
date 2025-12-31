use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use crate::models::Price;
use super::error::EntsoeError;

#[derive(Debug, Deserialize)]
#[serde(rename = "Publication_MarketDocument")]
pub struct PublicationMarketDocument {
    #[serde(rename = "@xmlns", default)]
    #[allow(dead_code)]
    pub xmlns: String,
    #[serde(rename = "mRID", default)]
    #[allow(dead_code)]
    pub m_rid: String,
    #[serde(rename = "TimeSeries", default)]
    pub time_series: Vec<TimeSeries>,
}

#[derive(Debug, Deserialize)]
pub struct TimeSeries {
    #[serde(rename = "currency_Unit.name", default)]
    #[allow(dead_code)]
    pub currency_unit_name: String,
    #[serde(rename = "price_Measure_Unit.name", default)]
    #[allow(dead_code)]
    pub price_measure_unit_name: String,
    #[serde(rename = "Period", default)]
    pub periods: Vec<Period>,
}

#[derive(Debug, Deserialize)]
pub struct Period {
    #[serde(rename = "timeInterval")]
    pub time_interval: TimeInterval,
    pub resolution: String,
    #[serde(rename = "Point", default)]
    pub points: Vec<Point>,
}

#[derive(Debug, Deserialize)]
pub struct TimeInterval {
    pub start: String,
    #[allow(dead_code)]
    pub end: String,
}

#[derive(Debug, Deserialize)]
pub struct Point {
    pub position: u32,
    #[serde(rename = "price.amount")]
    pub price_amount: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "Acknowledgement_MarketDocument")]
pub struct AcknowledgementMarketDocument {
    #[serde(rename = "@xmlns", default)]
    #[allow(dead_code)]
    pub xmlns: String,
    #[serde(rename = "Reason", default)]
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Deserialize)]
pub struct Reason {
    pub code: String,
    pub text: String,
}

pub fn parse_resolution(resolution: &str) -> Result<Duration, EntsoeError> {
    let parsed = iso8601_duration::Duration::parse(resolution)
        .map_err(|e| EntsoeError::InvalidResolution(format!("{}: {:?}", resolution, e)))?;
    
    let minutes = parsed.minute as i64 + (parsed.hour as i64 * 60);
    
    if minutes == 0 {
        return Err(EntsoeError::InvalidResolution(format!(
            "Resolution must have non-zero duration: {}",
            resolution
        )));
    }
    
    Ok(Duration::minutes(minutes))
}

impl PublicationMarketDocument {
    pub fn extract_prices(&self, bidding_zone: &str) -> Result<Vec<Price>, EntsoeError> {
        let mut prices = Vec::new();

        for time_series in &self.time_series {
            for period in &time_series.periods {
                let start_time = parse_timestamp(&period.time_interval.start)?;
                let resolution = parse_resolution(&period.resolution)?;

                for point in &period.points {
                    let position_offset = (point.position - 1) as i64;
                    let timestamp = start_time + resolution * position_offset as i32;

                    let price = Price::from_mwh(
                        timestamp,
                        bidding_zone.to_string(),
                        point.price_amount,
                        period.resolution.clone(),
                    );
                    prices.push(price);
                }
            }
        }

        Ok(prices)
    }
}

fn parse_timestamp(timestamp_str: &str) -> Result<DateTime<Utc>, EntsoeError> {
    DateTime::parse_from_rfc3339(timestamp_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| EntsoeError::TimestampParseError(format!("{}: {}", timestamp_str, e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resolution_15m() {
        let result = parse_resolution("PT15M").unwrap();
        assert_eq!(result, Duration::minutes(15));
    }

    #[test]
    fn test_parse_resolution_60m() {
        let result = parse_resolution("PT60M").unwrap();
        assert_eq!(result, Duration::minutes(60));
    }

    #[test]
    fn test_parse_resolution_1h() {
        let result = parse_resolution("PT1H").unwrap();
        assert_eq!(result, Duration::minutes(60));
    }

    #[test]
    fn test_parse_resolution_invalid() {
        let result = parse_resolution("invalid");
        assert!(result.is_err());
    }
}
