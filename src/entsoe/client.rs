use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::config::EntsoeConfig;
use crate::metrics;
use crate::models::{BiddingZone, Price};

use super::error::EntsoeError;
use super::xml::{AcknowledgementMarketDocument, PublicationMarketDocument};

/// Token bucket rate limiter that enforces a per-minute rate limit.
/// Tokens are replenished continuously based on elapsed time.
struct TokenBucketRateLimiter {
    tokens: f64,
    max_tokens: f64,
    refill_rate_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucketRateLimiter {
    fn new(requests_per_minute: u32) -> Self {
        let max_tokens = requests_per_minute as f64;
        let refill_rate_per_sec = max_tokens / 60.0;
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate_per_sec,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate_per_sec).min(self.max_tokens);
        self.last_refill = now;
    }

    /// Attempt to acquire a token. Returns the duration to wait if no token is available.
    fn try_acquire(&mut self) -> Option<Duration> {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            None
        } else {
            let wait_secs = (1.0 - self.tokens) / self.refill_rate_per_sec;
            Some(Duration::from_secs_f64(wait_secs))
        }
    }
}

pub struct EntsoeClient {
    client: Client,
    base_url: String,
    security_token: String,
    rate_limiter: Arc<Mutex<TokenBucketRateLimiter>>,
}

impl EntsoeClient {
    pub fn new(config: &EntsoeConfig) -> Result<Self, EntsoeError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()?;

        let rate_limiter = TokenBucketRateLimiter::new(config.rate_limit_per_minute);

        Ok(Self {
            client,
            base_url: config.base_url.clone(),
            security_token: config.security_token.clone(),
            rate_limiter: Arc::new(Mutex::new(rate_limiter)),
        })
    }

    async fn acquire_rate_limit_permit(&self) {
        loop {
            let wait_duration = {
                let mut limiter = self.rate_limiter.lock().await;
                limiter.try_acquire()
            };
            match wait_duration {
                None => break,
                Some(duration) => {
                    metrics::record_rate_limit_wait();
                    debug!(wait_ms = duration.as_millis(), "Rate limit reached, waiting");
                    tokio::time::sleep(duration).await;
                }
            }
        }
    }

    fn build_url(&self, eic_code: &str, period_start: &str, period_end: &str) -> String {
        format!(
            "{}?securityToken={}&documentType=A44&processType=A01&in_Domain={}&out_Domain={}&periodStart={}&periodEnd={}",
            self.base_url,
            self.security_token,
            eic_code,
            eic_code,
            period_start,
            period_end
        )
    }

    fn calculate_utc_bounds(date: NaiveDate, timezone: &Tz) -> (DateTime<Utc>, DateTime<Utc>) {
        let start_local = timezone
            .from_local_datetime(&date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()))
            .single()
            .expect("Ambiguous or invalid local time");

        let end_local = timezone
            .from_local_datetime(
                &date
                    .succ_opt()
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
            )
            .single()
            .expect("Ambiguous or invalid local time");

        (start_local.with_timezone(&Utc), end_local.with_timezone(&Utc))
    }

    fn format_period(dt: &DateTime<Utc>) -> String {
        dt.format("%Y%m%d%H%M").to_string()
    }

    #[tracing::instrument(skip(self), fields(zone_code = %zone.zone_code, date = %date))]
    pub async fn fetch_day_ahead_prices(
        &self,
        zone: &BiddingZone,
        date: NaiveDate,
    ) -> Result<Vec<Price>, EntsoeError> {
        let start_time = Instant::now();
        metrics::record_fetch_attempt(&zone.zone_code, "started");

        self.acquire_rate_limit_permit().await;

        let timezone = zone
            .get_timezone()
            .map_err(EntsoeError::InvalidResponse)?;

        let (start_utc, end_utc) = Self::calculate_utc_bounds(date, &timezone);
        let period_start = Self::format_period(&start_utc);
        let period_end = Self::format_period(&end_utc);

        let url = self.build_url(&zone.eic_code, &period_start, &period_end);
        debug!(url = %url, "Fetching day-ahead prices");

        let response = self.client.get(&url).send().await?;
        let status = response.status();

        let result = match status.as_u16() {
            200 => {
                let body = response.text().await?;
                let prices = self.parse_response(&body, &zone.zone_code)?;
                info!(count = prices.len(), "Successfully fetched prices");
                Ok(prices)
            }
            429 => {
                warn!("Rate limited by ENTSOE API");
                Err(EntsoeError::RateLimited)
            }
            500..=599 => {
                let body = response.text().await.unwrap_or_default();
                error!(status = %status, body = %body, "ENTSOE API server error");
                Err(EntsoeError::TemporaryUnavailable(format!(
                    "HTTP {}: {}",
                    status, body
                )))
            }
            _ => {
                let body = response.text().await.unwrap_or_default();
                error!(status = %status, body = %body, "ENTSOE API request failed");
                Err(EntsoeError::InvalidResponse(format!(
                    "Unexpected HTTP status {}: {}",
                    status, body
                )))
            }
        };

        let duration = start_time.elapsed();
        metrics::record_fetch_duration(&zone.zone_code, duration);

        match &result {
            Ok(_) => {
                metrics::record_fetch_attempt(&zone.zone_code, "success");
            }
            Err(e) => {
                let error_type = match e {
                    EntsoeError::RateLimited => "rate_limited",
                    EntsoeError::TemporaryUnavailable(_) => "temporary",
                    EntsoeError::InvalidResponse(_) => "invalid_response",
                    EntsoeError::XmlParseError(_) => "parse_error",
                    EntsoeError::NoData => "no_data",
                    EntsoeError::HttpError(_) => "http_error",
                    EntsoeError::InvalidResolution(_) => "invalid_resolution",
                    EntsoeError::TimestampParseError(_) => "timestamp_parse_error",
                };
                metrics::record_fetch_error(&zone.zone_code, error_type);
            }
        }

        result
    }

    fn parse_response(&self, body: &str, zone_code: &str) -> Result<Vec<Price>, EntsoeError> {
        if let Ok(doc) = quick_xml::de::from_str::<PublicationMarketDocument>(body) {
            return doc.extract_prices(zone_code);
        }

        if let Ok(ack) = quick_xml::de::from_str::<AcknowledgementMarketDocument>(body) {
            for reason in &ack.reasons {
                if reason.code == "999" {
                    warn!(reason = %reason.text, "No data available for requested period");
                    return Ok(Vec::new());
                }
            }
            return Err(EntsoeError::InvalidResponse(format!(
                "ENTSOE returned acknowledgement: {:?}",
                ack.reasons
            )));
        }

        Err(EntsoeError::XmlParseError(format!(
            "Failed to parse response as either Publication or Acknowledgement document. Body starts with: {}",
            &body.chars().take(200).collect::<String>()
        )))
    }

    fn compute_backoff_with_jitter(attempt: u32, base_delay_ms: u64) -> Duration {
        let exp_delay = base_delay_ms * 2u64.saturating_pow(attempt);
        let capped_delay = exp_delay.min(60_000);
        let jitter = (capped_delay as f64 * 0.2 * rand_jitter()) as u64;
        Duration::from_millis(capped_delay + jitter)
    }

    #[tracing::instrument(skip(self), fields(zone_code = %zone.zone_code, date = %date))]
    pub async fn fetch_day_ahead_prices_with_retry(
        &self,
        zone: &BiddingZone,
        date: NaiveDate,
    ) -> Result<Vec<Price>, EntsoeError> {
        const MAX_ATTEMPTS: u32 = 4;
        const BASE_DELAY_MS: u64 = 1000;

        let mut last_error = None;

        for attempt in 0..MAX_ATTEMPTS {
            match self.fetch_day_ahead_prices(zone, date).await {
                Ok(prices) => return Ok(prices),
                Err(e) if e.is_transient() => {
                    last_error = Some(e);
                    if attempt + 1 < MAX_ATTEMPTS {
                        let backoff = Self::compute_backoff_with_jitter(attempt, BASE_DELAY_MS);
                        warn!(
                            error = %last_error.as_ref().unwrap(),
                            attempt = attempt + 1,
                            max_attempts = MAX_ATTEMPTS,
                            backoff_ms = backoff.as_millis(),
                            "Transient error, retrying with exponential backoff"
                        );
                        tokio::time::sleep(backoff).await;
                    }
                }
                Err(e) => {
                    error!(error = %e, "Permanent error, not retrying");
                    return Err(e);
                }
            }
        }

        error!(
            error = %last_error.as_ref().unwrap(),
            attempts = MAX_ATTEMPTS,
            "All retry attempts exhausted"
        );
        Err(last_error.unwrap())
    }
}

fn rand_jitter() -> f64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64);
    (hasher.finish() % 1000) as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn test_format_period() {
        let dt = Utc.with_ymd_and_hms(2025, 12, 31, 23, 0, 0).unwrap();
        assert_eq!(EntsoeClient::format_period(&dt), "202512312300");
    }

    #[test]
    fn test_calculate_utc_bounds_cet() {
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let tz: Tz = "Europe/Berlin".parse().unwrap();
        let (start, end) = EntsoeClient::calculate_utc_bounds(date, &tz);
        
        assert_eq!(start.hour(), 23);
        assert_eq!(start.day(), 14);
        assert_eq!(end.hour(), 23);
        assert_eq!(end.day(), 15);
    }

    #[test]
    fn test_calculate_utc_bounds_cest() {
        let date = NaiveDate::from_ymd_opt(2025, 7, 15).unwrap();
        let tz: Tz = "Europe/Berlin".parse().unwrap();
        let (start, end) = EntsoeClient::calculate_utc_bounds(date, &tz);
        
        assert_eq!(start.hour(), 22);
        assert_eq!(start.day(), 14);
        assert_eq!(end.hour(), 22);
        assert_eq!(end.day(), 15);
    }
}
