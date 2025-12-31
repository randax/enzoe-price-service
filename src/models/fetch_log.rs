use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "lowercase")]
pub enum FetchStatus {
    Pending,
    Success,
    NoData,
    Error,
    RateLimited,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FetchLog {
    pub id: i64,
    pub fetch_started_at: DateTime<Utc>,
    pub fetch_completed_at: Option<DateTime<Utc>>,
    pub bidding_zone: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub status: FetchStatus,
    pub records_inserted: Option<i32>,
    pub error_message: Option<String>,
    pub http_status: Option<i32>,
    pub duration_ms: Option<i32>,
}

impl FetchLog {
    pub fn new(
        bidding_zone: Option<String>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Self {
        Self {
            id: 0,
            fetch_started_at: Utc::now(),
            fetch_completed_at: None,
            bidding_zone,
            period_start,
            period_end,
            status: FetchStatus::Pending,
            records_inserted: None,
            error_message: None,
            http_status: None,
            duration_ms: None,
        }
    }
}
