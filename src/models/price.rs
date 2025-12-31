use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Price {
    pub timestamp: DateTime<Utc>,
    pub bidding_zone: String,
    pub price_kwh: Decimal,
    pub currency: String,
    pub resolution: String,
    pub fetched_at: DateTime<Utc>,
}

impl Price {
    /// Convert price from MWh to kWh (divide by 1000)
    pub fn from_mwh(
        timestamp: DateTime<Utc>,
        bidding_zone: String,
        price_mwh: f64,
        resolution: String,
    ) -> Self {
        let price_kwh = Decimal::from_str(&(price_mwh / 1000.0).to_string())
            .unwrap_or(Decimal::ZERO);

        Self {
            timestamp,
            bidding_zone,
            price_kwh,
            currency: "EUR".to_string(),
            resolution,
            fetched_at: Utc::now(),
        }
    }
}
