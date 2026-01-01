use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::Duration as StdDuration;

use crate::config::DatabaseConfig;
use crate::models::{BiddingZone, FetchLog, FetchStatus, Price};

use super::error::StorageError;

pub struct PoolStatus {
    pub active_connections: u32,
    pub idle_connections: u32,
    pub max_connections: u32,
}

pub struct PriceRepository {
    pool: PgPool,
}

impl PriceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn from_config(config: &DatabaseConfig) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(StdDuration::from_secs(config.connect_timeout_seconds))
            .connect(&config.url)
            .await?;

        Ok(Self { pool })
    }

    pub async fn health_check(&self) -> Result<(), StorageError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn pool_status(&self) -> PoolStatus {
        PoolStatus {
            active_connections: self.pool.size(),
            idle_connections: self.pool.num_idle() as u32,
            max_connections: self.pool.options().get_max_connections(),
        }
    }

    pub async fn begin_transaction(
        &self,
    ) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, StorageError> {
        self.pool.begin().await.map_err(StorageError::from)
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Price Operations
    // ─────────────────────────────────────────────────────────────────────────────

    pub async fn upsert_prices(&self, prices: &[Price]) -> Result<usize, StorageError> {
        if prices.is_empty() {
            return Ok(0);
        }

        let mut timestamps: Vec<DateTime<Utc>> = Vec::with_capacity(prices.len());
        let mut bidding_zones: Vec<String> = Vec::with_capacity(prices.len());
        let mut prices_kwh: Vec<rust_decimal::Decimal> = Vec::with_capacity(prices.len());
        let mut currencies: Vec<String> = Vec::with_capacity(prices.len());
        let mut resolutions: Vec<String> = Vec::with_capacity(prices.len());
        let mut fetched_ats: Vec<DateTime<Utc>> = Vec::with_capacity(prices.len());

        for price in prices {
            timestamps.push(price.timestamp);
            bidding_zones.push(price.bidding_zone.clone());
            prices_kwh.push(price.price_kwh);
            currencies.push(price.currency.clone());
            resolutions.push(price.resolution.clone());
            fetched_ats.push(price.fetched_at);
        }

        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(
            r#"
            INSERT INTO electricity_prices (timestamp, bidding_zone, price_kwh, currency, resolution, fetched_at)
            SELECT * FROM UNNEST($1::timestamptz[], $2::varchar[], $3::numeric[], $4::varchar[], $5::varchar[], $6::timestamptz[])
            ON CONFLICT (timestamp, bidding_zone)
            DO UPDATE SET
                price_kwh = EXCLUDED.price_kwh,
                currency = EXCLUDED.currency,
                resolution = EXCLUDED.resolution,
                fetched_at = EXCLUDED.fetched_at
            "#,
        )
        .bind(&timestamps)
        .bind(&bidding_zones)
        .bind(&prices_kwh)
        .bind(&currencies)
        .bind(&resolutions)
        .bind(&fetched_ats)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(result.rows_affected() as usize)
    }

    pub async fn get_prices_by_zone(
        &self,
        zone_code: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Price>, StorageError> {
        let prices = sqlx::query_as::<_, Price>(
            r#"
            SELECT timestamp, bidding_zone, price_kwh, currency, resolution, fetched_at
            FROM electricity_prices
            WHERE bidding_zone = $1 AND timestamp >= $2 AND timestamp < $3
            ORDER BY timestamp ASC
            "#,
        )
        .bind(zone_code)
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;

        Ok(prices)
    }

    pub async fn get_prices_by_country(
        &self,
        country_code: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<HashMap<String, Vec<Price>>, StorageError> {
        let rows = sqlx::query_as::<_, Price>(
            r#"
            SELECT ep.timestamp, ep.bidding_zone, ep.price_kwh, ep.currency, ep.resolution, ep.fetched_at
            FROM electricity_prices ep
            JOIN bidding_zones bz ON ep.bidding_zone = bz.zone_code
            WHERE bz.country_code = $1
              AND bz.active = TRUE
              AND ep.timestamp >= $2 AND ep.timestamp < $3
            ORDER BY ep.bidding_zone, ep.timestamp ASC
            "#,
        )
        .bind(country_code)
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;

        let mut grouped: HashMap<String, Vec<Price>> = HashMap::new();
        for price in rows {
            grouped
                .entry(price.bidding_zone.clone())
                .or_default()
                .push(price);
        }

        Ok(grouped)
    }

    pub async fn get_latest_prices(
        &self,
        max_age_hours: Option<i32>,
    ) -> Result<Vec<Price>, StorageError> {
        let prices = match max_age_hours {
            Some(hours) => {
                sqlx::query_as::<_, Price>(
                    r#"
                    SELECT DISTINCT ON (bidding_zone) timestamp, bidding_zone, price_kwh, currency, resolution, fetched_at
                    FROM electricity_prices
                    WHERE timestamp >= NOW() - make_interval(hours => $1)
                    ORDER BY bidding_zone, timestamp DESC
                    "#,
                )
                .bind(hours)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Price>(
                    r#"
                    SELECT DISTINCT ON (bidding_zone) timestamp, bidding_zone, price_kwh, currency, resolution, fetched_at
                    FROM electricity_prices
                    ORDER BY bidding_zone, timestamp DESC
                    "#,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(prices)
    }

    pub async fn delete_old_prices(&self, older_than: DateTime<Utc>) -> Result<u64, StorageError> {
        let result = sqlx::query("DELETE FROM electricity_prices WHERE timestamp < $1")
            .bind(older_than)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Zone Registry Operations
    // ─────────────────────────────────────────────────────────────────────────────

    pub async fn load_zones(&self) -> Result<Vec<BiddingZone>, StorageError> {
        let zones = sqlx::query_as::<_, BiddingZone>(
            r#"
            SELECT zone_code, zone_name, country_code, country_name, eic_code, timezone, active, created_at, updated_at
            FROM bidding_zones
            WHERE active = TRUE
            ORDER BY country_code, zone_code
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(zones)
    }

    pub async fn get_zone_by_code(&self, zone_code: &str) -> Result<BiddingZone, StorageError> {
        sqlx::query_as::<_, BiddingZone>(
            r#"
            SELECT zone_code, zone_name, country_code, country_name, eic_code, timezone, active, created_at, updated_at
            FROM bidding_zones
            WHERE zone_code = $1
            "#,
        )
        .bind(zone_code)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("Zone not found: {}", zone_code)))
    }

    pub async fn get_zone_by_eic(&self, eic_code: &str) -> Result<BiddingZone, StorageError> {
        sqlx::query_as::<_, BiddingZone>(
            r#"
            SELECT zone_code, zone_name, country_code, country_name, eic_code, timezone, active, created_at, updated_at
            FROM bidding_zones
            WHERE eic_code = $1
            "#,
        )
        .bind(eic_code)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("Zone not found for EIC: {}", eic_code)))
    }

    pub async fn get_zones_by_country(
        &self,
        country_code: &str,
    ) -> Result<Vec<BiddingZone>, StorageError> {
        let zones = sqlx::query_as::<_, BiddingZone>(
            r#"
            SELECT zone_code, zone_name, country_code, country_name, eic_code, timezone, active, created_at, updated_at
            FROM bidding_zones
            WHERE country_code = $1 AND active = TRUE
            ORDER BY zone_code
            "#,
        )
        .bind(country_code)
        .fetch_all(&self.pool)
        .await?;

        Ok(zones)
    }

    pub async fn get_countries(&self) -> Result<Vec<(String, String)>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT country_code, country_name
            FROM bidding_zones
            WHERE active = TRUE
            ORDER BY country_code
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let countries = rows
            .iter()
            .map(|row| {
                let code: String = row.get("country_code");
                let name: String = row.get("country_name");
                (code, name)
            })
            .collect();

        Ok(countries)
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Fetch Log Operations
    // ─────────────────────────────────────────────────────────────────────────────

    pub async fn log_fetch_start(
        &self,
        zone_code: Option<String>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<i64, StorageError> {
        let row = sqlx::query(
            r#"
            INSERT INTO fetch_log (fetch_started_at, bidding_zone, period_start, period_end, status)
            VALUES (NOW(), $1, $2, $3, 'pending')
            RETURNING id
            "#,
        )
        .bind(&zone_code)
        .bind(period_start)
        .bind(period_end)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("id"))
    }

    pub async fn log_fetch_complete(
        &self,
        fetch_id: i64,
        status: FetchStatus,
        records_inserted: i32,
        error_message: Option<String>,
        http_status: Option<i32>,
        duration_ms: i32,
    ) -> Result<(), StorageError> {
        let status_str = match status {
            FetchStatus::Pending => "pending",
            FetchStatus::Success => "success",
            FetchStatus::NoData => "nodata",
            FetchStatus::Error => "error",
            FetchStatus::RateLimited => "ratelimited",
        };

        let result = sqlx::query(
            r#"
            UPDATE fetch_log
            SET fetch_completed_at = NOW(),
                status = $1::text,
                records_inserted = $2,
                error_message = $3,
                http_status = $4,
                duration_ms = $5
            WHERE id = $6
            "#,
        )
        .bind(status_str)
        .bind(records_inserted)
        .bind(&error_message)
        .bind(http_status)
        .bind(duration_ms)
        .bind(fetch_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!(
                "Fetch log not found: {}",
                fetch_id
            )));
        }

        Ok(())
    }

    pub async fn get_recent_fetch_logs(&self, limit: i64) -> Result<Vec<FetchLog>, StorageError> {
        let logs = sqlx::query_as::<_, FetchLog>(
            r#"
            SELECT id, fetch_started_at, fetch_completed_at, bidding_zone, period_start, period_end,
                   status, records_inserted, error_message, http_status, duration_ms
            FROM fetch_log
            ORDER BY fetch_started_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(logs)
    }

    pub async fn get_fetch_logs_by_zone(
        &self,
        zone_code: &str,
        limit: i64,
    ) -> Result<Vec<FetchLog>, StorageError> {
        let logs = sqlx::query_as::<_, FetchLog>(
            r#"
            SELECT id, fetch_started_at, fetch_completed_at, bidding_zone, period_start, period_end,
                   status, records_inserted, error_message, http_status, duration_ms
            FROM fetch_log
            WHERE bidding_zone = $1
            ORDER BY fetch_started_at DESC
            LIMIT $2
            "#,
        )
        .bind(zone_code)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(logs)
    }

    pub async fn has_tomorrow_data(&self, zone_code: &str) -> Result<bool, StorageError> {
        let tomorrow_start = Utc::now().date_naive().succ_opt().unwrap();
        let tomorrow_end = tomorrow_start.succ_opt().unwrap();

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM electricity_prices
            WHERE bidding_zone = $1
              AND timestamp >= $2::date
              AND timestamp < $3::date
            "#,
        )
        .bind(zone_code)
        .bind(tomorrow_start)
        .bind(tomorrow_end)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    /// Find dates with missing hourly prices for given zones in date range
    /// Returns list of (date, zone_code, existing_count) where existing_count < 24
    pub async fn find_gaps(
        &self,
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
        zone_codes: &[String],
    ) -> Result<Vec<(chrono::NaiveDate, String, i64)>, StorageError> {
        let rows = sqlx::query(
            r#"
            WITH date_range AS (
                SELECT generate_series($1::date, $2::date, '1 day'::interval)::date AS date
            ),
            zones AS (
                SELECT unnest($3::varchar[]) AS zone_code
            ),
            date_zone_pairs AS (
                SELECT d.date, z.zone_code
                FROM date_range d
                CROSS JOIN zones z
            ),
            price_counts AS (
                SELECT 
                    date(timestamp AT TIME ZONE 'UTC') AS price_date,
                    bidding_zone,
                    COUNT(*) AS hour_count
                FROM electricity_prices
                WHERE timestamp >= $1::date
                  AND timestamp < ($2::date + interval '1 day')
                  AND bidding_zone = ANY($3::varchar[])
                GROUP BY date(timestamp AT TIME ZONE 'UTC'), bidding_zone
            )
            SELECT 
                dzp.date,
                dzp.zone_code,
                COALESCE(pc.hour_count, 0) AS existing_count
            FROM date_zone_pairs dzp
            LEFT JOIN price_counts pc 
                ON dzp.date = pc.price_date 
                AND dzp.zone_code = pc.bidding_zone
            WHERE COALESCE(pc.hour_count, 0) < 24
            ORDER BY dzp.date, dzp.zone_code
            "#,
        )
        .bind(start_date)
        .bind(end_date)
        .bind(zone_codes)
        .fetch_all(&self.pool)
        .await?;

        let gaps = rows
            .iter()
            .map(|row| {
                let date: chrono::NaiveDate = row.get("date");
                let zone_code: String = row.get("zone_code");
                let existing_count: i64 = row.get("existing_count");
                (date, zone_code, existing_count)
            })
            .collect();

        Ok(gaps)
    }
}
