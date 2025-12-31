# Architecture Requirements Document (ARD)

## ENTSOE European Electricity Price Aggregation Service

| Field | Value |
|-------|-------|
| **Document Version** | 1.0 |
| **Author** | Platform Engineering Team |
| **Date** | December 2024 |
| **Status** | Draft |
| **Related PRD** | PRD-entsoe-price-fetcher v1.0 |

---

## 1. Executive Summary

This Architecture Requirements Document defines the technical architecture for the ENTSOE Price Fetcher service. The service is designed as a single Rust binary that combines scheduled data fetching with a REST API server, backed by PostgreSQL for persistent storage. The architecture prioritizes reliability, maintainability, and operational simplicity.

---

## 2. Architecture Overview

### 2.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        entsoe-price-fetcher                                  │
│                                                                              │
│  ┌────────────────┐    ┌────────────────┐    ┌────────────────────────────┐ │
│  │   Scheduler    │    │   REST API     │    │   Configuration            │ │
│  │ (tokio-cron)   │    │   (axum)       │    │   (config + env)           │ │
│  └───────┬────────┘    └───────┬────────┘    └────────────────────────────┘ │
│          │                     │                                             │
│          ▼                     ▼                                             │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                        Core Services                                    │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                  │ │
│  │  │   Fetcher    │  │   Storage    │  │    Zone      │                  │ │
│  │  │   Service    │  │   Service    │  │   Registry   │                  │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘                  │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                │                                             │
└────────────────────────────────┼─────────────────────────────────────────────┘
                                 │
           ┌─────────────────────┼─────────────────────┐
           │                     │                     │
           ▼                     ▼                     ▼
    ┌─────────────┐      ┌─────────────┐      ┌─────────────┐
    │   ENTSOE    │      │ PostgreSQL  │      │  Prometheus │
    │     API     │      │             │      │  (metrics)  │
    └─────────────┘      └─────────────┘      └─────────────┘
```

### 2.2 Component Responsibilities

| Component | Responsibility |
|-----------|----------------|
| **Scheduler** | Triggers price fetching at configured times (13:00 CET, retries at 14:00, 15:00, 16:00) |
| **Fetcher Service** | Communicates with ENTSOE API, parses XML responses, handles retries |
| **Storage Service** | Manages database connections, executes queries, handles transactions |
| **Zone Registry** | Maintains bidding zone configuration, EIC code mappings |
| **REST API** | Serves price queries, health checks, zone listings |
| **Configuration** | Loads settings from files and environment variables |

---

## 3. Technology Stack

### 3.1 Core Technologies

| Layer | Technology | Version | Justification |
|-------|------------|---------|---------------|
| **Language** | Rust | 1.75+ | Memory safety, performance, strong typing |
| **Runtime** | Tokio | 1.x | Async runtime, excellent ecosystem |
| **Web Framework** | Axum | 0.8 | Tower-compatible, ergonomic, fast |
| **HTTP Client** | reqwest | 0.12 | De facto standard, async, connection pooling |
| **XML Parser** | quick-xml | 0.38 | High performance, serde integration |
| **Database** | PostgreSQL | 17 | Mature, partitioning support, BRIN indexes |
| **DB Client** | sqlx | 0.8 | Compile-time checked queries, async |
| **Scheduler** | tokio-cron-scheduler | 0.13 | Timezone-aware, async native |
| **Serialization** | serde | 1.x | Industry standard |
| **Date/Time** | chrono + chrono-tz | 0.4 / 0.10 | Timezone handling |

### 3.2 Supporting Libraries

| Purpose | Library | Notes |
|---------|---------|-------|
| Configuration | config | Layered config (file → env → defaults) |
| Error Handling | thiserror, anyhow | Domain errors + application boundaries |
| Retry Logic | backoff | Exponential backoff with jitter |
| Logging | tracing, tracing-subscriber | Structured, async-aware |
| Metrics | metrics, metrics-exporter-prometheus | Prometheus-compatible |
| CLI | clap | Argument parsing |

### 3.3 Cargo.toml Dependencies

```toml
[package]
name = "entsoe-price-fetcher"
version = "0.1.0"
edition = "2021"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Web framework
axum = { version = "0.8", features = ["json"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors"] }

# HTTP client
reqwest = { version = "0.12", features = ["rustls-tls", "gzip"] }

# XML parsing
quick-xml = { version = "0.38", features = ["serialize"] }

# Database
sqlx = { version = "0.8", features = [
    "runtime-tokio",
    "postgres",
    "chrono",
    "bigdecimal"
]}

# Scheduling
tokio-cron-scheduler = "0.13"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Date/time
chrono = { version = "0.4", features = ["serde"] }
chrono-tz = "0.10"

# Configuration
config = "0.15"

# Error handling
thiserror = "2"
anyhow = "1"

# Retry logic
backoff = { version = "0.4", features = ["tokio"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }

# Metrics
metrics = "0.24"
metrics-exporter-prometheus = "0.16"

# Numeric precision
rust_decimal = { version = "1", features = ["db-postgres"] }
```

---

## 4. Module Architecture

### 4.1 Project Structure

```
entsoe-price-fetcher/
├── Cargo.toml
├── Cargo.lock
├── config/
│   ├── default.toml           # Default configuration
│   └── production.toml        # Production overrides
├── migrations/
│   ├── 001_initial_schema.sql
│   └── 002_add_indexes.sql
├── src/
│   ├── main.rs                # Entry point, initialization
│   ├── lib.rs                 # Library root, re-exports
│   ├── config.rs              # Configuration loading
│   ├── error.rs               # Error types
│   ├── api/
│   │   ├── mod.rs             # API module
│   │   ├── routes.rs          # Route definitions
│   │   ├── handlers.rs        # Request handlers
│   │   └── responses.rs       # Response types
│   ├── entsoe/
│   │   ├── mod.rs             # ENTSOE module
│   │   ├── client.rs          # HTTP client for ENTSOE
│   │   ├── parser.rs          # XML response parser
│   │   ├── types.rs           # ENTSOE data types
│   │   └── zones.rs           # Bidding zone definitions
│   ├── storage/
│   │   ├── mod.rs             # Storage module
│   │   ├── repository.rs      # Database operations
│   │   └── models.rs          # Database models
│   ├── scheduler/
│   │   ├── mod.rs             # Scheduler module
│   │   └── jobs.rs            # Scheduled job definitions
│   └── metrics.rs             # Metrics definitions
├── tests/
│   ├── integration/
│   │   ├── api_tests.rs
│   │   └── fetch_tests.rs
│   └── fixtures/
│       └── sample_response.xml
├── Dockerfile
└── docker-compose.yml
```

### 4.2 Module Responsibilities

#### 4.2.1 `entsoe` Module

```rust
// src/entsoe/client.rs
pub struct EntsoeClient {
    http_client: reqwest::Client,
    base_url: String,
    security_token: String,
    rate_limiter: RateLimiter,
}

impl EntsoeClient {
    pub async fn fetch_day_ahead_prices(
        &self,
        zone: &BiddingZone,
        date: NaiveDate,
    ) -> Result<Vec<HourlyPrice>, EntsoeError>;
    
    pub async fn fetch_all_zones(
        &self,
        date: NaiveDate,
    ) -> Vec<ZoneFetchResult>;
}
```

#### 4.2.2 `storage` Module

```rust
// src/storage/repository.rs
pub struct PriceRepository {
    pool: PgPool,
}

impl PriceRepository {
    pub async fn upsert_prices(&self, prices: &[PriceRecord]) -> Result<usize>;
    pub async fn get_prices_by_zone(
        &self,
        zone: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<PriceRecord>>;
    pub async fn get_prices_by_country(
        &self,
        country: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<HashMap<String, Vec<PriceRecord>>>;
    pub async fn get_latest_prices(&self) -> Result<Vec<LatestPrice>>;
    pub async fn get_fetch_status(&self) -> Result<FetchStatus>;
}
```

#### 4.2.3 `api` Module

```rust
// src/api/routes.rs
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/prices/zone/:zone", get(handlers::get_zone_prices))
        .route("/api/v1/prices/country/:country", get(handlers::get_country_prices))
        .route("/api/v1/prices/latest", get(handlers::get_latest_prices))
        .route("/api/v1/zones", get(handlers::list_zones))
        .route("/api/v1/countries", get(handlers::list_countries))
        .route("/health", get(handlers::health_check))
        .route("/ready", get(handlers::readiness_check))
        .route("/metrics", get(handlers::metrics))
        .with_state(state)
}
```

---

## 5. Database Architecture

### 5.1 Schema Design

```sql
-- Main prices table with range partitioning
CREATE TABLE electricity_prices (
    id              BIGSERIAL,
    timestamp       TIMESTAMPTZ NOT NULL,
    bidding_zone    VARCHAR(20) NOT NULL,
    price           DECIMAL(10,2) NOT NULL,
    currency        VARCHAR(3) NOT NULL DEFAULT 'EUR',
    resolution      VARCHAR(10) NOT NULL DEFAULT 'PT60M',
    data_source     VARCHAR(50) NOT NULL DEFAULT 'entsoe',
    fetched_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (timestamp, bidding_zone),
    
    CONSTRAINT valid_price CHECK (price >= -500 AND price <= 10000),
    CONSTRAINT valid_resolution CHECK (resolution IN ('PT60M', 'PT30M', 'PT15M'))
) PARTITION BY RANGE (timestamp);

-- Create monthly partitions
CREATE TABLE electricity_prices_2024_12 PARTITION OF electricity_prices
    FOR VALUES FROM ('2024-12-01') TO ('2025-01-01');

CREATE TABLE electricity_prices_2025_01 PARTITION OF electricity_prices
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');
-- ... additional partitions created by pg_partman

-- Bidding zones reference table
CREATE TABLE bidding_zones (
    zone_code       VARCHAR(20) PRIMARY KEY,
    zone_name       VARCHAR(100) NOT NULL,
    country_code    VARCHAR(2) NOT NULL,
    country_name    VARCHAR(100) NOT NULL,
    eic_code        VARCHAR(20) NOT NULL UNIQUE,
    timezone        VARCHAR(50) NOT NULL DEFAULT 'Europe/Berlin',
    active          BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Fetch log for observability
CREATE TABLE fetch_log (
    id              BIGSERIAL PRIMARY KEY,
    fetch_started   TIMESTAMPTZ NOT NULL,
    fetch_completed TIMESTAMPTZ,
    target_date     DATE NOT NULL,
    zones_attempted INTEGER NOT NULL,
    zones_succeeded INTEGER NOT NULL DEFAULT 0,
    zones_failed    INTEGER NOT NULL DEFAULT 0,
    error_details   JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index on fetch_log for status queries
CREATE INDEX idx_fetch_log_date ON fetch_log (target_date DESC, fetch_started DESC);
```

### 5.2 Indexing Strategy

```sql
-- BRIN index for timestamp range queries (very small, efficient for time-series)
CREATE INDEX idx_prices_timestamp_brin ON electricity_prices 
    USING BRIN (timestamp) WITH (pages_per_range = 32);

-- Composite B-tree for zone + time queries
CREATE INDEX idx_prices_zone_timestamp ON electricity_prices 
    (bidding_zone, timestamp DESC);

-- Partial index for recent data (hot data optimization)
CREATE INDEX idx_prices_recent ON electricity_prices 
    (timestamp, bidding_zone)
    WHERE timestamp > CURRENT_TIMESTAMP - INTERVAL '7 days';

-- Zone lookup index
CREATE INDEX idx_zones_country ON bidding_zones (country_code);
```

### 5.3 Partition Management

Using `pg_partman` for automatic partition management:

```sql
-- Install extension
CREATE EXTENSION pg_partman;

-- Configure automatic partitioning
SELECT partman.create_parent(
    p_parent_table := 'public.electricity_prices',
    p_control := 'timestamp',
    p_interval := '1 month',
    p_premake := 3,          -- Create 3 months ahead
    p_start_partition := '2024-12-01'
);

-- Schedule partition maintenance (run daily)
SELECT partman.run_maintenance('public.electricity_prices');
```

---

## 6. ENTSOE API Integration

### 6.1 Request Construction

```rust
// Request parameters for day-ahead prices
pub struct DayAheadPriceRequest {
    pub document_type: &'static str,    // "A44"
    pub process_type: &'static str,     // "A01"
    pub in_domain: String,              // EIC code
    pub out_domain: String,             // EIC code (same as in_domain)
    pub period_start: String,           // YYYYMMDDHHMM (UTC)
    pub period_end: String,             // YYYYMMDDHHMM (UTC)
}

impl EntsoeClient {
    fn build_url(&self, request: &DayAheadPriceRequest) -> String {
        format!(
            "{}?securityToken={}&documentType={}&processType={}&\
             in_Domain={}&out_Domain={}&periodStart={}&periodEnd={}",
            self.base_url,
            self.security_token,
            request.document_type,
            request.process_type,
            request.in_domain,
            request.out_domain,
            request.period_start,
            request.period_end,
        )
    }
}
```

### 6.2 XML Response Parsing

```rust
// Expected XML structure
#[derive(Debug, Deserialize)]
#[serde(rename = "Publication_MarketDocument")]
pub struct PublicationMarketDocument {
    #[serde(rename = "mRID")]
    pub mrid: String,
    #[serde(rename = "TimeSeries")]
    pub time_series: Vec<TimeSeries>,
}

#[derive(Debug, Deserialize)]
pub struct TimeSeries {
    #[serde(rename = "currency_Unit.name")]
    pub currency: String,
    #[serde(rename = "price_Measure_Unit.name")]
    pub price_unit: String,
    #[serde(rename = "Period")]
    pub periods: Vec<Period>,
}

#[derive(Debug, Deserialize)]
pub struct Period {
    #[serde(rename = "timeInterval")]
    pub time_interval: TimeInterval,
    pub resolution: String,
    #[serde(rename = "Point")]
    pub points: Vec<Point>,
}

#[derive(Debug, Deserialize)]
pub struct Point {
    pub position: u8,
    #[serde(rename = "price.amount")]
    pub price_amount: Decimal,
}

// Error response structure
#[derive(Debug, Deserialize)]
#[serde(rename = "Acknowledgement_MarketDocument")]
pub struct AcknowledgementDocument {
    #[serde(rename = "Reason")]
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Deserialize)]
pub struct Reason {
    pub code: String,
    pub text: String,
}
```

### 6.3 Rate Limiting

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{Duration, sleep};

pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    requests_per_minute: u32,
}

impl RateLimiter {
    pub fn new(requests_per_minute: u32) -> Self {
        // Allow burst of up to 50 concurrent requests
        let semaphore = Arc::new(Semaphore::new(50));
        Self { semaphore, requests_per_minute }
    }
    
    pub async fn acquire(&self) -> RateLimitGuard {
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        // Minimum delay between requests: 60s / 400 = 150ms
        // Use 200ms for safety margin
        RateLimitGuard { permit, delay: Duration::from_millis(200) }
    }
}

pub struct RateLimitGuard {
    permit: tokio::sync::OwnedSemaphorePermit,
    delay: Duration,
}

impl Drop for RateLimitGuard {
    fn drop(&mut self) {
        // Release permit after delay (done in background)
        let delay = self.delay;
        tokio::spawn(async move {
            sleep(delay).await;
        });
    }
}
```

### 6.4 Retry Strategy

```rust
use backoff::{ExponentialBackoff, Error as BackoffError};

pub async fn fetch_with_retry<F, Fut, T>(
    operation: F,
) -> Result<T, EntsoeError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, EntsoeError>>,
{
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_secs(5),
        max_interval: Duration::from_secs(60),
        max_elapsed_time: Some(Duration::from_secs(300)),
        multiplier: 2.0,
        randomization_factor: 0.3,
        ..Default::default()
    };
    
    backoff::future::retry(backoff, || async {
        match operation().await {
            Ok(value) => Ok(value),
            Err(EntsoeError::RateLimited) => {
                Err(BackoffError::transient(EntsoeError::RateLimited))
            }
            Err(EntsoeError::TemporaryUnavailable) => {
                Err(BackoffError::transient(EntsoeError::TemporaryUnavailable))
            }
            Err(e) => Err(BackoffError::permanent(e)),
        }
    })
    .await
}
```

---

## 7. Scheduling Architecture

### 7.1 Scheduler Design

```rust
use tokio_cron_scheduler::{Job, JobScheduler};
use chrono_tz::Europe::Oslo;

pub struct PriceFetchScheduler {
    scheduler: JobScheduler,
    fetcher: Arc<FetcherService>,
}

impl PriceFetchScheduler {
    pub async fn new(fetcher: Arc<FetcherService>) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self { scheduler, fetcher })
    }
    
    pub async fn start(&self) -> Result<()> {
        // Primary fetch at 13:00 CET
        self.add_fetch_job("0 0 13 * * *", "primary").await?;
        
        // Retry at 14:00 CET if tomorrow's prices missing
        self.add_conditional_fetch_job("0 0 14 * * *", "retry_1").await?;
        
        // Retry at 15:00 CET
        self.add_conditional_fetch_job("0 0 15 * * *", "retry_2").await?;
        
        // Final retry at 16:00 CET
        self.add_conditional_fetch_job("0 0 16 * * *", "retry_3").await?;
        
        self.scheduler.start().await?;
        Ok(())
    }
    
    async fn add_fetch_job(&self, cron: &str, name: &str) -> Result<()> {
        let fetcher = self.fetcher.clone();
        let job_name = name.to_string();
        
        let job = Job::new_async_tz(cron, Oslo, move |_uuid, _lock| {
            let fetcher = fetcher.clone();
            let name = job_name.clone();
            Box::pin(async move {
                tracing::info!(job = %name, "Starting scheduled price fetch");
                if let Err(e) = fetcher.fetch_all_prices().await {
                    tracing::error!(job = %name, error = %e, "Fetch failed");
                }
            })
        })?;
        
        self.scheduler.add(job).await?;
        Ok(())
    }
}
```

### 7.2 Fetch Logic

```rust
pub struct FetcherService {
    client: EntsoeClient,
    repository: PriceRepository,
    zones: ZoneRegistry,
}

impl FetcherService {
    pub async fn fetch_all_prices(&self) -> Result<FetchSummary> {
        let today = Utc::now().date_naive();
        let tomorrow = today + Duration::days(1);
        
        let mut summary = FetchSummary::new();
        
        // Fetch today's prices (should always be available)
        summary.merge(self.fetch_date_all_zones(today).await);
        
        // Fetch tomorrow's prices (may not be available yet)
        summary.merge(self.fetch_date_all_zones(tomorrow).await);
        
        // Log fetch summary
        self.repository.log_fetch(&summary).await?;
        
        Ok(summary)
    }
    
    async fn fetch_date_all_zones(&self, date: NaiveDate) -> FetchSummary {
        let zones = self.zones.all_active();
        
        // Fetch in parallel with rate limiting
        let results = futures::stream::iter(zones)
            .map(|zone| {
                let client = &self.client;
                async move {
                    let result = client.fetch_day_ahead_prices(&zone, date).await;
                    (zone, result)
                }
            })
            .buffer_unordered(10)  // Max 10 concurrent requests
            .collect::<Vec<_>>()
            .await;
        
        // Process results
        let mut summary = FetchSummary::new();
        let mut prices_to_store = Vec::new();
        
        for (zone, result) in results {
            match result {
                Ok(prices) => {
                    summary.succeeded += 1;
                    prices_to_store.extend(prices);
                }
                Err(EntsoeError::NoData) => {
                    // Expected for future dates
                    summary.no_data += 1;
                }
                Err(e) => {
                    summary.failed += 1;
                    summary.errors.push((zone.code.clone(), e.to_string()));
                }
            }
        }
        
        // Batch upsert
        if !prices_to_store.is_empty() {
            if let Err(e) = self.repository.upsert_prices(&prices_to_store).await {
                tracing::error!(error = %e, "Failed to store prices");
            }
        }
        
        summary
    }
}
```

---

## 8. API Design

### 8.1 Endpoint Specifications

| Method | Path | Description | Query Params |
|--------|------|-------------|--------------|
| GET | `/api/v1/prices/zone/{zone}` | Prices for specific zone | `start`, `end` |
| GET | `/api/v1/prices/country/{country}` | Prices for all zones in country | `start`, `end` |
| GET | `/api/v1/prices/latest` | Latest prices for all zones | - |
| GET | `/api/v1/zones` | List all bidding zones | - |
| GET | `/api/v1/countries` | List countries with zone mappings | - |
| GET | `/health` | Liveness check | - |
| GET | `/ready` | Readiness check | - |
| GET | `/metrics` | Prometheus metrics | - |

### 8.2 Response Types

```rust
// src/api/responses.rs

#[derive(Serialize)]
pub struct ZonePricesResponse {
    pub zone_code: String,
    pub zone_name: String,
    pub country: String,
    pub currency: String,
    pub unit: String,
    pub prices: Vec<PricePoint>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct PricePoint {
    pub timestamp: DateTime<Utc>,
    pub price: Decimal,
}

#[derive(Serialize)]
pub struct CountryPricesResponse {
    pub country_code: String,
    pub country_name: String,
    pub zones: Vec<ZonePricesResponse>,
}

#[derive(Serialize)]
pub struct LatestPricesResponse {
    pub generated_at: DateTime<Utc>,
    pub prices: Vec<LatestZonePrice>,
}

#[derive(Serialize)]
pub struct LatestZonePrice {
    pub zone_code: String,
    pub zone_name: String,
    pub country: String,
    pub current_price: Decimal,
    pub current_hour: DateTime<Utc>,
    pub next_price: Option<Decimal>,
    pub next_hour: Option<DateTime<Utc>>,
    pub currency: String,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub version: String,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub checks: ReadinessChecks,
}

#[derive(Serialize)]
pub struct ReadinessChecks {
    pub database: bool,
    pub recent_fetch: bool,
    pub last_fetch_at: Option<DateTime<Utc>>,
}
```

### 8.3 Error Handling

```rust
// src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Zone not found: {0}")]
    ZoneNotFound(String),
    
    #[error("Country not found: {0}")]
    CountryNotFound(String),
    
    #[error("Invalid date range")]
    InvalidDateRange,
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::ZoneNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::CountryNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::InvalidDateRange => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            ),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal error".to_string(),
            ),
        };
        
        let body = Json(json!({
            "error": message,
            "status": status.as_u16(),
        }));
        
        (status, body).into_response()
    }
}
```

---

## 9. Configuration Management

### 9.1 Configuration Structure

```rust
// src/config.rs

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub entsoe: EntsoeConfig,
    pub scheduler: SchedulerConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

#[derive(Debug, Deserialize)]
pub struct EntsoeConfig {
    pub base_url: String,
    pub security_token: String,  // From environment
    pub rate_limit_per_minute: u32,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Deserialize)]
pub struct SchedulerConfig {
    pub timezone: String,
    pub primary_fetch_cron: String,
    pub retry_fetch_crons: Vec<String>,
}
```

### 9.2 Configuration Files

```toml
# config/default.toml

[server]
host = "0.0.0.0"
port = 8080

[database]
max_connections = 10
min_connections = 2

[entsoe]
base_url = "https://web-api.tp.entsoe.eu/api"
rate_limit_per_minute = 350  # Safety margin below 400
request_timeout_secs = 30

[scheduler]
timezone = "Europe/Oslo"
primary_fetch_cron = "0 0 13 * * *"
retry_fetch_crons = ["0 0 14 * * *", "0 0 15 * * *", "0 0 16 * * *"]

[logging]
level = "info"
format = "json"
```

### 9.3 Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `DATABASE_URL` | PostgreSQL connection string | Yes |
| `ENTSOE_SECURITY_TOKEN` | ENTSOE API token | Yes |
| `SERVER_PORT` | Override server port | No |
| `RUST_LOG` | Log level filter | No |

---

## 10. Observability

### 10.1 Metrics

```rust
// src/metrics.rs

use metrics::{counter, gauge, histogram};

// Fetch metrics
pub fn record_fetch_attempt(zone: &str, success: bool) {
    let status = if success { "success" } else { "failure" };
    counter!("entsoe_fetch_total", "zone" => zone.to_string(), "status" => status).increment(1);
}

pub fn record_fetch_duration(zone: &str, duration: Duration) {
    histogram!("entsoe_fetch_duration_seconds", "zone" => zone.to_string())
        .record(duration.as_secs_f64());
}

// API metrics
pub fn record_api_request(endpoint: &str, status: u16, duration: Duration) {
    histogram!("http_request_duration_seconds", 
        "endpoint" => endpoint.to_string(),
        "status" => status.to_string()
    ).record(duration.as_secs_f64());
    
    counter!("http_requests_total",
        "endpoint" => endpoint.to_string(),
        "status" => status.to_string()
    ).increment(1);
}

// Data freshness metrics
pub fn set_last_fetch_timestamp(timestamp: i64) {
    gauge!("entsoe_last_fetch_timestamp").set(timestamp as f64);
}

pub fn set_zones_with_tomorrow_data(count: usize) {
    gauge!("entsoe_zones_with_tomorrow_data").set(count as f64);
}
```

### 10.2 Structured Logging

```rust
// Logging configuration
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_logging(config: &LoggingConfig) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));
    
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}

// Example log entries
tracing::info!(
    zone = %zone.code,
    date = %date,
    prices_count = prices.len(),
    "Successfully fetched day-ahead prices"
);

tracing::error!(
    zone = %zone.code,
    error = %e,
    "Failed to fetch prices from ENTSOE"
);
```

---

## 11. Deployment Architecture

### 11.1 Dockerfile

```dockerfile
# Build stage using cargo-chef for caching
FROM lukemathwalker/cargo-chef:latest-rust-1.75 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin entsoe-price-fetcher

# Runtime stage
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app

# Copy binary
COPY --from=builder /app/target/release/entsoe-price-fetcher /app/

# Copy config
COPY config/ /app/config/

USER nonroot:nonroot

EXPOSE 8080

ENTRYPOINT ["/app/entsoe-price-fetcher"]
```

### 11.2 Docker Compose (Development)

```yaml
version: "3.8"

services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      - DATABASE_URL=postgres://postgres:postgres@db:5432/entsoe
      - ENTSOE_SECURITY_TOKEN=${ENTSOE_SECURITY_TOKEN}
      - RUST_LOG=info
    depends_on:
      db:
        condition: service_healthy
    restart: unless-stopped

  db:
    image: postgres:17
    environment:
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=postgres
      - POSTGRES_DB=entsoe
    volumes:
      - pgdata:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  pgdata:
```

### 11.3 Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: entsoe-price-fetcher
  labels:
    app: entsoe-price-fetcher
spec:
  replicas: 2
  selector:
    matchLabels:
      app: entsoe-price-fetcher
  template:
    metadata:
      labels:
        app: entsoe-price-fetcher
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      containers:
      - name: app
        image: entsoe-price-fetcher:latest
        ports:
        - containerPort: 8080
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: entsoe-secrets
              key: database-url
        - name: ENTSOE_SECURITY_TOKEN
          valueFrom:
            secretKeyRef:
              name: entsoe-secrets
              key: entsoe-token
        resources:
          requests:
            memory: "64Mi"
            cpu: "100m"
          limits:
            memory: "256Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
      securityContext:
        runAsNonRoot: true
        runAsUser: 65534
---
apiVersion: v1
kind: Service
metadata:
  name: entsoe-price-fetcher
spec:
  selector:
    app: entsoe-price-fetcher
  ports:
  - port: 80
    targetPort: 8080
```

---

## 12. Security Considerations

### 12.1 Secret Management

| Secret | Storage Method | Rotation |
|--------|---------------|----------|
| ENTSOE API Token | Kubernetes Secret / Vault | Manual (on compromise) |
| Database Password | Kubernetes Secret / Vault | 90 days |

### 12.2 Container Security

- Base image: `distroless/cc-debian12:nonroot`
- Non-root user (UID 65534)
- Read-only root filesystem
- No shell access
- Minimal attack surface

### 12.3 Network Security

- Internal-only service (no ingress)
- TLS for database connections (if remote)
- Outbound HTTPS only to ENTSOE API

---

## 13. Testing Strategy

### 13.1 Test Categories

| Category | Scope | Tools |
|----------|-------|-------|
| Unit Tests | Individual functions, parsers | `cargo test` |
| Integration Tests | API endpoints, database | testcontainers-rs |
| Contract Tests | ENTSOE response parsing | Recorded fixtures |

### 13.2 Test Fixtures

Store sample ENTSOE XML responses in `tests/fixtures/` for reliable parsing tests:

```
tests/fixtures/
├── day_ahead_prices_success.xml
├── day_ahead_prices_no_data.xml
├── acknowledgement_error_999.xml
└── rate_limit_error.xml
```

---

## 14. Decision Log

| Decision | Options Considered | Chosen | Rationale |
|----------|-------------------|--------|-----------|
| Language | Rust, Go, Python | Rust | Type safety, performance, memory efficiency |
| Web Framework | axum, actix-web, warp | axum | Ergonomics, Tower ecosystem |
| XML Parser | quick-xml, roxmltree, serde-xml-rs | quick-xml | Performance, serde integration |
| Database | PostgreSQL, TimescaleDB | PostgreSQL 17 | Sufficient for volume, native partitioning |
| Scheduler | tokio-cron, external cron | tokio-cron-scheduler | In-process, shares state |
| Container Base | scratch, distroless, alpine | distroless | CA certs, glibc, minimal |

---

*Document End*
