use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BiddingZone {
    pub zone_code: String,
    pub zone_name: String,
    pub country_code: String,
    pub country_name: String,
    pub eic_code: String,
    pub timezone: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BiddingZone {
    /// Get timezone as chrono_tz::Tz
    pub fn get_timezone(&self) -> Result<chrono_tz::Tz, String> {
        self.timezone
            .parse::<chrono_tz::Tz>()
            .map_err(|e| format!("Invalid timezone {}: {}", self.timezone, e))
    }
}
