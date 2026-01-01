use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use chrono::{NaiveDate, Utc};
use futures::stream::{self, StreamExt};
use tracing::{error, info, warn};

use crate::entsoe::{EntsoeClient, EntsoeError};
use crate::metrics;
use crate::models::{BiddingZone, FetchStatus, Price};
use crate::storage::PriceRepository;

#[derive(Debug, Clone, Default)]
pub struct FetchSummary {
    pub succeeded: usize,
    pub failed: usize,
    pub no_data: usize,
    pub total_prices_stored: usize,
    pub errors: Vec<String>,
}

impl FetchSummary {
    pub fn merge(&mut self, other: FetchSummary) {
        self.succeeded += other.succeeded;
        self.failed += other.failed;
        self.no_data += other.no_data;
        self.total_prices_stored += other.total_prices_stored;
        self.errors.extend(other.errors);
    }
}

#[derive(Debug, Clone, Default)]
pub struct BackfillSummary {
    pub dates_checked: usize,
    pub dates_with_gaps: usize,
    pub prices_fetched: usize,
    pub prices_stored: usize,
    pub gaps_found: Vec<(NaiveDate, String, i64)>, // (date, zone, missing_hours)
    pub errors: Vec<String>,
}

pub struct FetcherService {
    client: Arc<EntsoeClient>,
    repository: Arc<PriceRepository>,
}

impl FetcherService {
    pub fn new(client: Arc<EntsoeClient>, repository: Arc<PriceRepository>) -> Self {
        Self { client, repository }
    }

    #[tracing::instrument(skip(self), fields(date = %date))]
    pub async fn fetch_date_all_zones(&self, date: NaiveDate) -> Result<FetchSummary, anyhow::Error> {
        let start = Instant::now();
        
        let zones = self.repository.load_zones().await?;
        info!(zone_count = zones.len(), "Loaded active zones for fetching");

        let results: Vec<(BiddingZone, Result<Vec<Price>, EntsoeError>)> = stream::iter(zones)
            .map(|zone| {
                let client = Arc::clone(&self.client);
                async move {
                    let result = client.fetch_day_ahead_prices_with_retry(&zone, date).await;
                    (zone, result)
                }
            })
            .buffer_unordered(5)
            .collect()
            .await;

        let mut summary = FetchSummary::default();
        let mut all_prices: Vec<Price> = Vec::new();

        for (zone, result) in results {
            match result {
                Ok(prices) if prices.is_empty() => {
                    summary.no_data += 1;
                    warn!(zone_code = %zone.zone_code, "No data available for zone");
                }
                Ok(prices) => {
                    summary.succeeded += 1;
                    info!(zone_code = %zone.zone_code, count = prices.len(), "Fetched prices for zone");
                    all_prices.extend(prices);
                }
                Err(EntsoeError::NoData) => {
                    summary.no_data += 1;
                    warn!(zone_code = %zone.zone_code, "No data available (NoData error)");
                }
                Err(e) => {
                    summary.failed += 1;
                    let error_msg = format!("{}: {}", zone.zone_code, e);
                    error!(zone_code = %zone.zone_code, error = %e, "Failed to fetch prices");
                    summary.errors.push(error_msg);
                }
            }
        }

        if !all_prices.is_empty() {
            let stored = self.repository.upsert_prices(&all_prices).await?;
            summary.total_prices_stored = stored;
            info!(
                count = stored,
                duration_ms = start.elapsed().as_millis(),
                "Batch upserted prices"
            );
        }

        info!(
            succeeded = summary.succeeded,
            failed = summary.failed,
            no_data = summary.no_data,
            total_prices = summary.total_prices_stored,
            duration_ms = start.elapsed().as_millis(),
            "Completed fetch for date"
        );

        Ok(summary)
    }

    #[tracing::instrument(skip(self))]
    pub async fn fetch_all_prices(&self) -> Result<FetchSummary, anyhow::Error> {
        let start = Instant::now();
        let today = Utc::now().date_naive();
        let tomorrow = today.succ_opt().unwrap();

        info!(today = %today, tomorrow = %tomorrow, "Starting fetch for today and tomorrow");

        let period_start = Utc::now();
        let period_end = Utc::now() + chrono::Duration::days(2);
        let fetch_id = self.repository.log_fetch_start(None, period_start, period_end).await?;

        let mut combined_summary = FetchSummary::default();

        match self.fetch_date_all_zones(today).await {
            Ok(summary) => combined_summary.merge(summary),
            Err(e) => {
                error!(error = %e, "Failed to fetch today's prices");
                combined_summary.errors.push(format!("Today fetch failed: {}", e));
            }
        }

        match self.fetch_date_all_zones(tomorrow).await {
            Ok(summary) => combined_summary.merge(summary),
            Err(e) => {
                error!(error = %e, "Failed to fetch tomorrow's prices");
                combined_summary.errors.push(format!("Tomorrow fetch failed: {}", e));
            }
        }

        let duration_ms = start.elapsed().as_millis() as i32;
        let status = if combined_summary.failed > 0 {
            FetchStatus::Error
        } else if combined_summary.succeeded == 0 && combined_summary.no_data > 0 {
            FetchStatus::NoData
        } else {
            FetchStatus::Success
        };

        let error_message = if combined_summary.errors.is_empty() {
            None
        } else {
            Some(combined_summary.errors.join("; "))
        };

        self.repository
            .log_fetch_complete(
                fetch_id,
                status,
                combined_summary.total_prices_stored as i32,
                error_message,
                None,
                duration_ms,
            )
            .await?;

        info!(
            succeeded = combined_summary.succeeded,
            failed = combined_summary.failed,
            no_data = combined_summary.no_data,
            total_prices = combined_summary.total_prices_stored,
            duration_ms = duration_ms,
            "Completed full fetch operation"
        );

        Ok(combined_summary)
    }

    #[tracing::instrument(skip(self))]
    pub async fn should_fetch_tomorrow(&self) -> Result<bool, anyhow::Error> {
        let zones = self.repository.load_zones().await?;
        let mut zones_with_data = 0;
        let mut zones_missing_data = 0;

        for zone in &zones {
            if self.repository.has_tomorrow_data(&zone.zone_code).await? {
                zones_with_data += 1;
            } else {
                zones_missing_data += 1;
            }
        }

        metrics::update_zones_with_tomorrow_data(zones_with_data as u64);

        info!(
            zones_with_data = zones_with_data,
            zones_missing_data = zones_missing_data,
            "Checked tomorrow data availability"
        );

        Ok(zones_missing_data > 0)
    }

    #[tracing::instrument(skip(self))]
    pub async fn fetch_tomorrow_if_missing(&self) -> Result<FetchSummary, anyhow::Error> {
        if !self.should_fetch_tomorrow().await? {
            info!("Tomorrow's data already exists for all zones, skipping fetch");
            return Ok(FetchSummary::default());
        }

        let start = Instant::now();
        let tomorrow = Utc::now().date_naive().succ_opt().unwrap();
        
        info!(date = %tomorrow, "Fetching tomorrow's prices for zones missing data");

        let zones = self.repository.load_zones().await?;
        let mut zones_to_fetch = Vec::new();

        for zone in zones {
            if !self.repository.has_tomorrow_data(&zone.zone_code).await? {
                zones_to_fetch.push(zone);
            }
        }

        if zones_to_fetch.is_empty() {
            info!("No zones need fetching");
            return Ok(FetchSummary::default());
        }

        info!(zone_count = zones_to_fetch.len(), "Zones needing tomorrow's data");

        let tomorrow_start = tomorrow.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let tomorrow_end = tomorrow.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();
        let fetch_id = self.repository.log_fetch_start(None, tomorrow_start, tomorrow_end).await?;

        let results: Vec<(BiddingZone, Result<Vec<Price>, EntsoeError>)> = stream::iter(zones_to_fetch)
            .map(|zone| {
                let client = Arc::clone(&self.client);
                async move {
                    let result = client.fetch_day_ahead_prices_with_retry(&zone, tomorrow).await;
                    (zone, result)
                }
            })
            .buffer_unordered(5)
            .collect()
            .await;

        let mut summary = FetchSummary::default();
        let mut all_prices: Vec<Price> = Vec::new();

        for (zone, result) in results {
            match result {
                Ok(prices) if prices.is_empty() => {
                    summary.no_data += 1;
                    warn!(zone_code = %zone.zone_code, "No data available for zone");
                }
                Ok(prices) => {
                    summary.succeeded += 1;
                    info!(zone_code = %zone.zone_code, count = prices.len(), "Fetched prices for zone");
                    all_prices.extend(prices);
                }
                Err(EntsoeError::NoData) => {
                    summary.no_data += 1;
                    warn!(zone_code = %zone.zone_code, "No data available (NoData error)");
                }
                Err(e) => {
                    summary.failed += 1;
                    let error_msg = format!("{}: {}", zone.zone_code, e);
                    error!(zone_code = %zone.zone_code, error = %e, "Failed to fetch prices");
                    summary.errors.push(error_msg);
                }
            }
        }

        if !all_prices.is_empty() {
            let stored = self.repository.upsert_prices(&all_prices).await?;
            summary.total_prices_stored = stored;
            info!(count = stored, "Batch upserted tomorrow's prices");
        }

        let duration_ms = start.elapsed().as_millis() as i32;
        let status = if summary.failed > 0 {
            FetchStatus::Error
        } else if summary.succeeded == 0 && summary.no_data > 0 {
            FetchStatus::NoData
        } else {
            FetchStatus::Success
        };

        let error_message = if summary.errors.is_empty() {
            None
        } else {
            Some(summary.errors.join("; "))
        };

        self.repository
            .log_fetch_complete(
                fetch_id,
                status,
                summary.total_prices_stored as i32,
                error_message,
                None,
                duration_ms,
            )
            .await?;

        info!(
            succeeded = summary.succeeded,
            failed = summary.failed,
            no_data = summary.no_data,
            total_prices = summary.total_prices_stored,
            duration_ms = duration_ms,
            "Completed conditional tomorrow fetch"
        );

        Ok(summary)
    }

    #[tracing::instrument(skip(self), fields(start = %start_date, end = %end_date))]
    pub async fn backfill_missing(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
        zone_filter: Option<Vec<String>>,
    ) -> Result<BackfillSummary, anyhow::Error> {
        let start = Instant::now();
        
        // Get zones to check
        let all_zones = self.repository.load_zones().await?;
        let zone_codes: Vec<String> = match &zone_filter {
            Some(filter) => {
                let filter_set: HashSet<&str> = filter.iter().map(|s| s.as_str()).collect();
                all_zones
                    .iter()
                    .filter(|z| filter_set.contains(z.zone_code.as_str()))
                    .map(|z| z.zone_code.clone())
                    .collect()
            }
            None => all_zones.iter().map(|z| z.zone_code.clone()).collect(),
        };

        if zone_codes.is_empty() {
            return Err(anyhow::anyhow!("No valid zones found"));
        }

        info!(zones = ?zone_codes, "Checking for gaps in price data");

        // Calculate dates to check
        let mut current = start_date;
        let mut dates_checked = 0;
        while current <= end_date {
            dates_checked += 1;
            current = current.succ_opt().unwrap();
        }

        // Find gaps in database
        let gaps = self.repository.find_gaps(start_date, end_date, &zone_codes).await?;
        
        let mut summary = BackfillSummary {
            dates_checked,
            dates_with_gaps: 0,
            prices_fetched: 0,
            prices_stored: 0,
            gaps_found: gaps.iter().map(|(d, z, c)| (*d, z.clone(), 24 - c)).collect(),
            errors: Vec::new(),
        };

        if gaps.is_empty() {
            info!("No gaps found in date range");
            return Ok(summary);
        }

        // Collect unique date-zone pairs to fetch
        let dates_to_fetch: HashSet<(NaiveDate, String)> = gaps
            .iter()
            .map(|(date, zone, _)| (*date, zone.clone()))
            .collect();

        summary.dates_with_gaps = dates_to_fetch.iter().map(|(d, _)| d).collect::<HashSet<_>>().len();

        info!(
            gaps_count = gaps.len(),
            unique_date_zones = dates_to_fetch.len(),
            "Found gaps, starting backfill"
        );

        // Create zone lookup map
        let zone_map: std::collections::HashMap<String, BiddingZone> = all_zones
            .into_iter()
            .map(|z| (z.zone_code.clone(), z))
            .collect();

        // Fetch missing data
        let mut all_prices: Vec<Price> = Vec::new();

        for (date, zone_code) in dates_to_fetch {
            let Some(zone) = zone_map.get(&zone_code) else {
                summary.errors.push(format!("Zone {} not found", zone_code));
                continue;
            };

            match self.client.fetch_day_ahead_prices_with_retry(zone, date).await {
                Ok(prices) => {
                    info!(zone = %zone_code, date = %date, count = prices.len(), "Fetched prices");
                    summary.prices_fetched += prices.len();
                    all_prices.extend(prices);
                }
                Err(EntsoeError::NoData) => {
                    warn!(zone = %zone_code, date = %date, "No data available from ENTSO-E");
                }
                Err(e) => {
                    let msg = format!("{} on {}: {}", zone_code, date, e);
                    error!(zone = %zone_code, date = %date, error = %e, "Failed to fetch");
                    summary.errors.push(msg);
                }
            }
        }

        // Store fetched prices
        if !all_prices.is_empty() {
            let stored = self.repository.upsert_prices(&all_prices).await?;
            summary.prices_stored = stored;
            info!(count = stored, "Stored backfilled prices");
        }

        info!(
            dates_checked = summary.dates_checked,
            dates_with_gaps = summary.dates_with_gaps,
            prices_fetched = summary.prices_fetched,
            prices_stored = summary.prices_stored,
            errors = summary.errors.len(),
            duration_ms = start.elapsed().as_millis(),
            "Completed backfill operation"
        );

        Ok(summary)
    }
}
