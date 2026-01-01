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
    // Handle common ENTSOE resolutions directly for reliability
    match resolution {
        "PT15M" => return Ok(Duration::minutes(15)),
        "PT30M" => return Ok(Duration::minutes(30)),
        "PT60M" => return Ok(Duration::minutes(60)),
        "P1D" => return Ok(Duration::days(1)),
        "P7D" => return Ok(Duration::days(7)),
        "P1Y" => return Ok(Duration::days(365)), // Approximate
        _ => {}
    }

    // Fallback to ISO8601 parsing for edge cases
    let parsed = iso8601_duration::Duration::parse(resolution)
        .map_err(|e| EntsoeError::InvalidResolution(format!("{}: {:?}", resolution, e)))?;

    let minutes = parsed.minute as i64 + (parsed.hour as i64 * 60);
    let days = parsed.day as i64;

    if minutes == 0 && days == 0 {
        return Err(EntsoeError::InvalidResolution(format!(
            "Resolution must have non-zero duration: {}",
            resolution
        )));
    }

    if days > 0 {
        Ok(Duration::days(days))
    } else {
        Ok(Duration::minutes(minutes))
    }
}

impl PublicationMarketDocument {
    pub fn extract_prices(&self, bidding_zone: &str) -> Result<Vec<Price>, EntsoeError> {
        use super::validation::validate_and_fill_period;

        let mut prices = Vec::new();

        for time_series in &self.time_series {
            for period in &time_series.periods {
                let period_prices = validate_and_fill_period(period, bidding_zone)?;
                prices.extend(period_prices);
            }
        }

        // Sort by timestamp to handle mixed resolutions (e.g., Austria returns PT15M + PT60M)
        prices.sort_by_key(|p| p.timestamp);

        Ok(prices)
    }
}

pub fn parse_timestamp(timestamp_str: &str) -> Result<DateTime<Utc>, EntsoeError> {
    // Try RFC3339 first (with seconds)
    if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp_str) {
        return Ok(dt.with_timezone(&Utc));
    }
    
    // ENTSO-E sometimes returns timestamps without seconds (e.g., "2025-12-30T23:00Z")
    // Try parsing with custom format
    let normalized = if timestamp_str.ends_with('Z') && !timestamp_str.contains(':') {
        timestamp_str.to_string()
    } else if timestamp_str.len() == 17 && timestamp_str.ends_with('Z') {
        // Format: "2025-12-30T23:00Z" -> add ":00" for seconds
        format!("{}:00Z", &timestamp_str[..16])
    } else {
        timestamp_str.to_string()
    };
    
    DateTime::parse_from_rfc3339(&normalized)
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

    #[test]
    fn test_parse_resolution_30m() {
        let result = parse_resolution("PT30M").unwrap();
        assert_eq!(result, Duration::minutes(30));
    }

    #[test]
    fn test_parse_resolution_p1d() {
        let result = parse_resolution("P1D").unwrap();
        assert_eq!(result, Duration::days(1));
    }

    #[test]
    fn test_parse_resolution_p7d() {
        let result = parse_resolution("P7D").unwrap();
        assert_eq!(result, Duration::days(7));
    }

    #[test]
    fn test_parse_resolution_p1y() {
        let result = parse_resolution("P1Y").unwrap();
        assert_eq!(result, Duration::days(365));
    }
}
