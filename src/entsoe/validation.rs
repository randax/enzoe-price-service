use std::collections::HashMap;

use chrono::{DateTime, Duration, Timelike, Utc};
use rust_decimal::Decimal;
use tracing::{info, warn};

use crate::metrics;
use crate::models::Price;

use super::error::EntsoeError;
use super::xml::{parse_resolution, parse_timestamp, Period};

/// Calculate expected number of periods for an interval and resolution
pub fn expected_period_count(start: DateTime<Utc>, end: DateTime<Utc>, resolution: Duration) -> usize {
    let interval_duration = end - start;
    (interval_duration.num_seconds() / resolution.num_seconds()) as usize
}

/// Aggregate sub-hourly prices into hourly averages.
/// PT15M: 4 values -> 1 hourly average
/// PT30M: 2 values -> 1 hourly average
/// PT60M and longer: no change
pub fn aggregate_to_hourly(prices: Vec<Price>, bidding_zone: &str) -> Vec<Price> {
    if prices.is_empty() {
        return prices;
    }

    let resolution = &prices[0].resolution;
    
    // If already hourly or longer, return as-is
    if resolution == "PT60M" || resolution == "P1D" || resolution == "P7D" || resolution == "P1Y" {
        return prices;
    }

    let original_count = prices.len();

    // Group prices by hour (truncate timestamp to hour boundary)
    let mut hourly_groups: HashMap<DateTime<Utc>, Vec<&Price>> = HashMap::new();
    
    for price in &prices {
        let hour_start = price
            .timestamp
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();
        hourly_groups.entry(hour_start).or_default().push(price);
    }

    // Calculate hourly averages
    let mut aggregated: Vec<Price> = hourly_groups
        .into_iter()
        .map(|(hour_start, group)| {
            let sum: Decimal = group.iter().map(|p| p.price_kwh).sum();
            let count = Decimal::from(group.len());
            let avg_price = sum / count;

            Price {
                timestamp: hour_start,
                bidding_zone: bidding_zone.to_string(),
                price_kwh: avg_price,
                currency: group[0].currency.clone(),
                resolution: "PT60M".to_string(),
                fetched_at: group[0].fetched_at,
            }
        })
        .collect();

    // Sort by timestamp
    aggregated.sort_by_key(|p| p.timestamp);

    let aggregated_count = aggregated.len();
    
    info!(
        bidding_zone = %bidding_zone,
        original_count = original_count,
        aggregated_count = aggregated_count,
        original_resolution = %resolution,
        "Aggregated sub-hourly prices to hourly averages"
    );
    
    metrics::record_prices_aggregated(bidding_zone, original_count as u64, aggregated_count as u64);

    aggregated
}

/// Validate and fill gaps in a period's points using forward-fill strategy.
/// Returns prices for all expected positions in the interval.
pub fn validate_and_fill_period(
    period: &Period,
    bidding_zone: &str,
) -> Result<Vec<Price>, EntsoeError> {
    let start_time = parse_timestamp(&period.time_interval.start)?;
    let end_time = parse_timestamp(&period.time_interval.end)?;
    let resolution = parse_resolution(&period.resolution)?;

    let expected_count = expected_period_count(start_time, end_time, resolution);
    if expected_count == 0 {
        return Ok(Vec::new());
    }

    // Build a map of position -> price_amount for quick lookup
    let point_map: HashMap<u32, f64> = period
        .points
        .iter()
        .map(|p| (p.position, p.price_amount))
        .collect();

    let mut prices = Vec::with_capacity(expected_count);
    let mut previous_price: Option<f64> = None;
    let mut gaps_filled: u64 = 0;

    for position in 1..=(expected_count as u32) {
        let price_amount = if let Some(&amount) = point_map.get(&position) {
            previous_price = Some(amount);
            amount
        } else {
            // Gap detected - use forward-fill
            match previous_price {
                Some(prev) => {
                    gaps_filled += 1;
                    warn!(
                        bidding_zone = %bidding_zone,
                        position = position,
                        resolution = %period.resolution,
                        "Gap detected at position {}, forward-filling with previous value",
                        position
                    );
                    prev
                }
                None => {
                    // First position is missing - cannot forward-fill
                    return Err(EntsoeError::MissingFirstPeriod);
                }
            }
        };

        let position_offset = (position - 1) as i64;
        let timestamp = start_time + resolution * position_offset as i32;

        let price = Price::from_mwh(
            timestamp,
            bidding_zone.to_string(),
            price_amount,
            period.resolution.clone(),
        );
        prices.push(price);
    }

    if gaps_filled > 0 {
        metrics::record_gaps_filled(bidding_zone, gaps_filled);
    }

    // Aggregate sub-hourly prices to hourly averages
    let prices = aggregate_to_hourly(prices, bidding_zone);

    Ok(prices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entsoe::xml::{Point, TimeInterval};

    fn create_period(
        start: &str,
        end: &str,
        resolution: &str,
        points: Vec<(u32, f64)>,
    ) -> Period {
        Period {
            time_interval: TimeInterval {
                start: start.to_string(),
                end: end.to_string(),
            },
            resolution: resolution.to_string(),
            points: points
                .into_iter()
                .map(|(pos, price)| Point {
                    position: pos,
                    price_amount: price,
                })
                .collect(),
        }
    }

    #[test]
    fn test_expected_period_count_pt60m() {
        let start = DateTime::parse_from_rfc3339("2025-12-30T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2025-12-31T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let resolution = Duration::minutes(60);

        assert_eq!(expected_period_count(start, end, resolution), 24);
    }

    #[test]
    fn test_expected_period_count_pt15m() {
        let start = DateTime::parse_from_rfc3339("2025-12-30T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2025-12-31T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let resolution = Duration::minutes(15);

        assert_eq!(expected_period_count(start, end, resolution), 96);
    }

    #[test]
    fn test_expected_period_count_pt30m() {
        let start = DateTime::parse_from_rfc3339("2025-12-30T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2025-12-31T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let resolution = Duration::minutes(30);

        assert_eq!(expected_period_count(start, end, resolution), 48);
    }

    #[test]
    fn test_validate_complete_period() {
        let points: Vec<(u32, f64)> = (1..=24).map(|i| (i, 50.0 + i as f64)).collect();
        let period = create_period(
            "2025-12-30T23:00:00Z",
            "2025-12-31T23:00:00Z",
            "PT60M",
            points,
        );

        let prices = validate_and_fill_period(&period, "DE-LU").unwrap();
        assert_eq!(prices.len(), 24);
        assert_eq!(prices[0].price_kwh.to_string(), "0.051"); // 51.0 / 1000
        assert_eq!(prices[23].price_kwh.to_string(), "0.074"); // 74.0 / 1000
    }

    #[test]
    fn test_validate_period_with_gap_forward_fill() {
        // Missing position 3, should be filled with position 2's value
        let points = vec![
            (1, 50.0),
            (2, 55.0),
            // position 3 missing
            (4, 60.0),
            (5, 65.0),
        ];
        let period = create_period(
            "2025-12-31T00:00:00Z",
            "2025-12-31T05:00:00Z",
            "PT60M",
            points,
        );

        let prices = validate_and_fill_period(&period, "DE-LU").unwrap();
        assert_eq!(prices.len(), 5);

        // Position 3 should have position 2's value (55.0 / 1000 = 0.055)
        assert_eq!(prices[2].price_kwh.to_string(), "0.055");
    }

    #[test]
    fn test_validate_period_with_multiple_gaps() {
        // Missing positions 2, 3, 5
        let points = vec![
            (1, 50.0),
            // position 2 missing
            // position 3 missing
            (4, 60.0),
            // position 5 missing
            (6, 70.0),
        ];
        let period = create_period(
            "2025-12-31T00:00:00Z",
            "2025-12-31T06:00:00Z",
            "PT60M",
            points,
        );

        let prices = validate_and_fill_period(&period, "DE-LU").unwrap();
        assert_eq!(prices.len(), 6);

        // Position 2 and 3 filled with position 1's value
        assert_eq!(prices[1].price_kwh.to_string(), "0.05");
        assert_eq!(prices[2].price_kwh.to_string(), "0.05");
        // Position 5 filled with position 4's value
        assert_eq!(prices[4].price_kwh.to_string(), "0.06");
    }

    #[test]
    fn test_validate_period_missing_first_position_error() {
        // Missing position 1 - cannot forward-fill
        let points = vec![(2, 55.0), (3, 60.0)];
        let period = create_period(
            "2025-12-31T00:00:00Z",
            "2025-12-31T03:00:00Z",
            "PT60M",
            points,
        );

        let result = validate_and_fill_period(&period, "DE-LU");
        assert!(matches!(result, Err(EntsoeError::MissingFirstPeriod)));
    }

    #[test]
    fn test_validate_period_pt15m_aggregated_to_hourly() {
        // 4 hours = 16 periods at 15-minute resolution, aggregated to 4 hourly values
        // Hour 0: positions 1-4, prices 41,42,43,44 -> avg 42.5 EUR/MWh = 0.0425 EUR/kWh
        // Hour 1: positions 5-8, prices 45,46,47,48 -> avg 46.5 EUR/MWh = 0.0465 EUR/kWh
        // Hour 2: positions 9-12, prices 49,50,51,52 -> avg 50.5 EUR/MWh = 0.0505 EUR/kWh
        // Hour 3: positions 13-16, prices 53,54,55,56 -> avg 54.5 EUR/MWh = 0.0545 EUR/kWh
        let points: Vec<(u32, f64)> = (1..=16).map(|i| (i, 40.0 + i as f64)).collect();
        let period = create_period(
            "2025-12-31T00:00:00Z",
            "2025-12-31T04:00:00Z",
            "PT15M",
            points,
        );

        let prices = validate_and_fill_period(&period, "AT").unwrap();
        
        // Should be aggregated to 4 hourly values
        assert_eq!(prices.len(), 4);
        assert_eq!(prices[0].resolution, "PT60M");
        
        // Verify hourly timestamps
        assert_eq!(prices[0].timestamp.hour(), 0);
        assert_eq!(prices[1].timestamp.hour(), 1);
        assert_eq!(prices[2].timestamp.hour(), 2);
        assert_eq!(prices[3].timestamp.hour(), 3);
        
        // Verify averages (41+42+43+44)/4 = 42.5 -> 0.0425 kWh
        // Note: Decimal division may add trailing zeros
        assert!(prices[0].price_kwh.to_string().starts_with("0.0425"));
        assert!(prices[1].price_kwh.to_string().starts_with("0.0465"));
        assert!(prices[2].price_kwh.to_string().starts_with("0.0505"));
        assert!(prices[3].price_kwh.to_string().starts_with("0.0545"));
    }

    #[test]
    fn test_validate_period_pt30m_aggregated_to_hourly() {
        // 4 hours = 8 periods at 30-minute resolution, aggregated to 4 hourly values
        // Hour 0: positions 1-2, prices 31,32 -> avg 31.5 EUR/MWh = 0.0315 EUR/kWh
        // Hour 1: positions 3-4, prices 33,34 -> avg 33.5 EUR/MWh = 0.0335 EUR/kWh
        let points: Vec<(u32, f64)> = (1..=8).map(|i| (i, 30.0 + i as f64)).collect();
        let period = create_period(
            "2025-12-31T00:00:00Z",
            "2025-12-31T04:00:00Z",
            "PT30M",
            points,
        );

        let prices = validate_and_fill_period(&period, "NL").unwrap();
        
        // Should be aggregated to 4 hourly values
        assert_eq!(prices.len(), 4);
        assert_eq!(prices[0].resolution, "PT60M");
        
        // Verify averages (31+32)/2 = 31.5 -> 0.0315 kWh
        // Note: Decimal division may add trailing zeros
        assert!(prices[0].price_kwh.to_string().starts_with("0.0315"));
        assert!(prices[1].price_kwh.to_string().starts_with("0.0335"));
    }

    #[test]
    fn test_aggregate_to_hourly_pt60m_passthrough() {
        // PT60M should pass through unchanged
        let prices = vec![
            Price::from_mwh(
                DateTime::parse_from_rfc3339("2025-12-31T00:00:00Z").unwrap().with_timezone(&Utc),
                "DE-LU".to_string(),
                50.0,
                "PT60M".to_string(),
            ),
            Price::from_mwh(
                DateTime::parse_from_rfc3339("2025-12-31T01:00:00Z").unwrap().with_timezone(&Utc),
                "DE-LU".to_string(),
                55.0,
                "PT60M".to_string(),
            ),
        ];

        let result = aggregate_to_hourly(prices.clone(), "DE-LU");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].price_kwh, prices[0].price_kwh);
        assert_eq!(result[1].price_kwh, prices[1].price_kwh);
    }

    #[test]
    fn test_aggregate_to_hourly_empty() {
        let prices: Vec<Price> = vec![];
        let result = aggregate_to_hourly(prices, "DE-LU");
        assert!(result.is_empty());
    }

    #[test]
    fn test_aggregate_to_hourly_pt15m_single_hour() {
        // 4 x 15-minute values for a single hour: 50, 52, 48, 54 EUR/MWh
        // Average = 51 EUR/MWh = 0.051 EUR/kWh
        let prices = vec![
            Price::from_mwh(
                DateTime::parse_from_rfc3339("2025-12-31T00:00:00Z").unwrap().with_timezone(&Utc),
                "AT".to_string(),
                50.0,
                "PT15M".to_string(),
            ),
            Price::from_mwh(
                DateTime::parse_from_rfc3339("2025-12-31T00:15:00Z").unwrap().with_timezone(&Utc),
                "AT".to_string(),
                52.0,
                "PT15M".to_string(),
            ),
            Price::from_mwh(
                DateTime::parse_from_rfc3339("2025-12-31T00:30:00Z").unwrap().with_timezone(&Utc),
                "AT".to_string(),
                48.0,
                "PT15M".to_string(),
            ),
            Price::from_mwh(
                DateTime::parse_from_rfc3339("2025-12-31T00:45:00Z").unwrap().with_timezone(&Utc),
                "AT".to_string(),
                54.0,
                "PT15M".to_string(),
            ),
        ];

        let result = aggregate_to_hourly(prices, "AT");
        
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].resolution, "PT60M");
        assert_eq!(result[0].timestamp.hour(), 0);
        assert_eq!(result[0].timestamp.minute(), 0);
        // (50+52+48+54)/4 = 51 EUR/MWh = 0.051 EUR/kWh
        assert_eq!(result[0].price_kwh.to_string(), "0.051");
    }
}
