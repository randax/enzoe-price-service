# RFC: ENTSOE European Electricity Price Aggregation Service

| Field | Value |
|-------|-------|
| **RFC Number** | RFC-2024-001 |
| **Title** | ENTSOE European Electricity Price Aggregation Service |
| **Author** | Platform Engineering Team |
| **Status** | Draft |
| **Created** | December 2024 |
| **Last Updated** | December 2024 |

---

## Abstract

This RFC proposes the implementation of a service to fetch, store, and serve day-ahead electricity prices from the ENTSOE (European Network of Transmission System Operators for Electricity) Transparency Platform. The service will aggregate prices for all 65+ European bidding zones and expose them via a REST API for internal consumption.

---

## 1. Motivation

### 1.1 Background

European electricity markets operate through a Single Day-Ahead Coupling (SDAC) mechanism where prices for the next day are determined through an auction process. These prices are published daily around 12:55-13:00 CET on the ENTSOE Transparency Platform. Access to this data is valuable for:

1. **Infrastructure cost optimization**: Scheduling compute-intensive workloads in regions with lower electricity prices
2. **Budget forecasting**: Predicting energy costs for European data center operations  
3. **Sustainability reporting**: Tracking energy consumption costs across regions
4. **Operational intelligence**: Making data-driven decisions about workload placement

### 1.2 Current State

Currently, there is no centralized internal service providing this data. Teams requiring electricity price information must either:
- Manually query the ENTSOE web interface
- Build ad-hoc integrations with the ENTSOE API
- Use incomplete or delayed third-party data sources

This leads to duplicated effort, inconsistent data, and missed optimization opportunities.

### 1.3 Proposed Solution

Build a dedicated service (`entsoe-price-fetcher`) that:
- Automatically fetches prices daily from ENTSOE
- Stores historical data in PostgreSQL
- Provides a simple REST API for querying prices
- Handles the complexity of European bidding zone mappings

---

## 2. Proposal

### 2.1 System Overview

The proposed system consists of three main components:

```
┌─────────────────────────────────────────────────────────┐
│                  entsoe-price-fetcher                    │
├─────────────────────────────────────────────────────────┤
│                                                          │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│   │  Scheduler   │  │   Fetcher    │  │   REST API   │  │
│   │  (13:00 CET) │──│   Service    │  │   (axum)     │  │
│   └──────────────┘  └──────┬───────┘  └──────┬───────┘  │
│                            │                  │          │
│                     ┌──────▼──────────────────▼──────┐   │
│                     │        PostgreSQL              │   │
│                     │    (partitioned tables)        │   │
│                     └────────────────────────────────┘   │
│                                                          │
└─────────────────────────────────────────────────────────┘
             │                           ▲
             │ HTTPS                     │ Queries
             ▼                           │
    ┌────────────────┐          ┌────────────────┐
    │   ENTSOE API   │          │  Internal      │
    │                │          │  Consumers     │
    └────────────────┘          └────────────────┘
```

### 2.2 Technology Choices

| Component | Choice | Rationale |
|-----------|--------|-----------|
| **Language** | Rust | Memory safety, excellent async support, single binary deployment |
| **Web Framework** | Axum 0.8 | Modern, Tower-compatible, excellent ergonomics |
| **Database** | PostgreSQL 17 | Mature, native partitioning, BRIN indexes for time-series |
| **DB Client** | sqlx 0.8 | Compile-time query verification, async native |
| **Scheduler** | tokio-cron-scheduler | In-process scheduling, timezone support |

### 2.3 Data Model

#### 2.3.1 Core Price Table

```sql
CREATE TABLE electricity_prices (
    timestamp       TIMESTAMPTZ NOT NULL,
    bidding_zone    VARCHAR(20) NOT NULL,
    price           DECIMAL(10,2) NOT NULL,
    currency        VARCHAR(3) NOT NULL DEFAULT 'EUR',
    resolution      VARCHAR(10) NOT NULL DEFAULT 'PT60M',
    fetched_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (timestamp, bidding_zone)
) PARTITION BY RANGE (timestamp);
```

**Design decisions**:
- Composite primary key on `(timestamp, bidding_zone)` enforces uniqueness and enables efficient upserts
- Monthly partitioning balances query performance with partition management overhead
- BRIN index on timestamp provides efficient range scans with minimal storage
- Prices stored in EUR/MWh (source format) - conversion handled by consumers

#### 2.3.2 Zone Registry

```sql
CREATE TABLE bidding_zones (
    zone_code       VARCHAR(20) PRIMARY KEY,
    zone_name       VARCHAR(100) NOT NULL,
    country_code    VARCHAR(2) NOT NULL,
    country_name    VARCHAR(100) NOT NULL,
    eic_code        VARCHAR(20) NOT NULL UNIQUE,
    timezone        VARCHAR(50) NOT NULL,
    active          BOOLEAN NOT NULL DEFAULT TRUE
);
```

This table enables:
- Zone code to EIC code resolution for API requests
- Country to zones mapping for aggregate queries
- Future zone changes without code deployment

### 2.4 API Design

#### 2.4.1 Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/prices/zone/{zone}` | GET | Get prices for a specific bidding zone |
| `/api/v1/prices/country/{country}` | GET | Get prices for all zones in a country |
| `/api/v1/prices/latest` | GET | Get current/next hour prices for all zones |
| `/api/v1/zones` | GET | List all available bidding zones |
| `/api/v1/countries` | GET | List countries with zone mappings |
| `/health` | GET | Liveness probe |
| `/ready` | GET | Readiness probe |
| `/metrics` | GET | Prometheus metrics |

#### 2.4.2 Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `start` | ISO8601 | 7 days ago | Start of date range |
| `end` | ISO8601 | Tomorrow end | End of date range |

#### 2.4.3 Response Format

```json
{
  "zone_code": "NO1",
  "zone_name": "Norway - Oslo",
  "country": "NO",
  "currency": "EUR",
  "unit": "MWh",
  "prices": [
    {
      "timestamp": "2024-12-15T00:00:00Z",
      "price": 45.32
    }
  ],
  "fetched_at": "2024-12-14T12:05:23Z"
}
```

### 2.5 Scheduling Logic

The service implements a multi-attempt fetch strategy to handle publication delays:

```
13:00 CET - Primary fetch
    ├── Fetch today's prices (should always succeed)
    └── Fetch tomorrow's prices (may fail if not yet published)
    
14:00 CET - First retry (conditional)
    └── Only if tomorrow's prices not yet stored
    
15:00 CET - Second retry (conditional)
    └── Only if tomorrow's prices not yet stored
    
16:00 CET - Final retry (conditional)
    └── Only if tomorrow's prices not yet stored
```

**Rationale**: ENTSOE typically publishes prices between 12:55-13:00 CET, but delays can occur due to:
- EUPHEMIA algorithm calculation issues
- Partial decoupling events
- System maintenance

The 16:00 cutoff provides ample margin while avoiding unnecessary API calls.

---

## 3. ENTSOE API Integration

### 3.1 Authentication

ENTSOE requires a security token obtained through:
1. Registration at transparency.entsoe.eu
2. Email to transparency@entsoe.eu requesting API access
3. Token generation in account settings

The token is passed as a query parameter: `securityToken={TOKEN}`

### 3.2 Day-Ahead Prices Request

```
GET https://web-api.tp.entsoe.eu/api
    ?securityToken={TOKEN}
    &documentType=A44
    &processType=A01
    &in_Domain={EIC_CODE}
    &out_Domain={EIC_CODE}
    &periodStart={YYYYMMDDHHMM}
    &periodEnd={YYYYMMDDHHMM}
```

| Parameter | Value | Notes |
|-----------|-------|-------|
| documentType | A44 | Price document (Article 12.1.D) |
| processType | A01 | Day ahead |
| in_Domain | EIC code | e.g., `10YNO-1--------2` for NO1 |
| out_Domain | EIC code | Same as in_Domain for prices |
| periodStart | UTC timestamp | Format: `202412150000` |
| periodEnd | UTC timestamp | Format: `202412160000` |

### 3.3 Response Handling

**Success response** (Publication_MarketDocument):
```xml
<Publication_MarketDocument xmlns="...">
  <TimeSeries>
    <currency_Unit.name>EUR</currency_Unit.name>
    <price_Measure_Unit.name>MWH</price_Measure_Unit.name>
    <Period>
      <timeInterval>
        <start>2024-12-14T23:00Z</start>
        <end>2024-12-15T23:00Z</end>
      </timeInterval>
      <resolution>PT60M</resolution>
      <Point>
        <position>1</position>
        <price.amount>45.32</price.amount>
      </Point>
      <!-- ... more points ... -->
    </Period>
  </TimeSeries>
</Publication_MarketDocument>
```

**No data response** (Acknowledgement_MarketDocument):
```xml
<Acknowledgement_MarketDocument xmlns="...">
  <Reason>
    <code>999</code>
    <text>No matching data found</text>
  </Reason>
</Acknowledgement_MarketDocument>
```

**Critical**: The API returns HTTP 200 for "no data" scenarios - response body must be checked.

### 3.4 Rate Limiting

| Limit | Value | Scope |
|-------|-------|-------|
| Requests per minute | 400 | Per IP + Per token |
| Exceeded limit | 10-minute ban | HTTP 429 returned |

**Mitigation**: 
- Target 300 requests/minute for safety margin
- Exponential backoff on 429 responses
- Parallel fetching with semaphore-based rate limiting

### 3.5 Timezone Complexity

ENTSOE uses UTC for all timestamps, but prices correspond to local market days. For CET-based zones:

| Local Date | UTC Start | UTC End | Note |
|------------|-----------|---------|------|
| 2024-12-15 (Winter) | 2024-12-14T23:00Z | 2024-12-15T23:00Z | CET = UTC+1 |
| 2024-06-15 (Summer) | 2024-06-14T22:00Z | 2024-06-15T22:00Z | CEST = UTC+2 |
| DST transition day | Variable | Variable | 23 or 25 hours |

**Implementation**: Use `chrono-tz` to calculate correct UTC boundaries for each zone's local date.

---

## 4. European Bidding Zones

### 4.1 Zone Structure

Europe's electricity market is divided into bidding zones, which are areas with uniform wholesale electricity prices. Some countries have multiple zones due to transmission constraints.

**Total zones**: ~65 active zones (varies with market changes)

### 4.2 Zone Mapping

#### Single-Zone Countries (Examples)

| Country | Zone | EIC Code |
|---------|------|----------|
| Germany-Luxembourg | DE-LU | `10Y1001A1001A82H` |
| France | FR | `10YFR-RTE------C` |
| Netherlands | NL | `10YNL----------L` |
| Belgium | BE | `10YBE----------2` |
| Spain | ES | `10YES-REE------0` |
| Poland | PL | `10YPL-AREA-----S` |
| Finland | FI | `10YFI-1--------U` |

#### Multi-Zone Countries

**Norway (5 zones)**:
| Zone | Region | EIC Code |
|------|--------|----------|
| NO1 | Oslo | `10YNO-1--------2` |
| NO2 | Kristiansand | `10YNO-2--------T` |
| NO3 | Trondheim | `10YNO-3--------J` |
| NO4 | Tromsø | `10YNO-4--------9` |
| NO5 | Bergen | `10Y1001A1001A48H` |

**Sweden (4 zones)**:
| Zone | Region | EIC Code |
|------|--------|----------|
| SE1 | Luleå | `10Y1001A1001A44P` |
| SE2 | Sundsvall | `10Y1001A1001A45N` |
| SE3 | Stockholm | `10Y1001A1001A46L` |
| SE4 | Malmö | `10Y1001A1001A47J` |

**Denmark (2 zones)**:
| Zone | Region | EIC Code |
|------|--------|----------|
| DK1 | Western (Jutland) | `10YDK-1--------W` |
| DK2 | Eastern (Zealand) | `10YDK-2--------M` |

**Italy (6+ zones)**:
| Zone | Region | EIC Code |
|------|--------|----------|
| IT-North | Northern Italy | `10Y1001A1001A73I` |
| IT-Centre-North | Central-North | `10Y1001A1001A70O` |
| IT-Centre-South | Central-South | `10Y1001A1001A71M` |
| IT-South | Southern Italy | `10Y1001A1001A788` |
| IT-Sardinia | Sardinia | `10Y1001A1001A74G` |
| IT-Sicily | Sicily | `10Y1001A1001A75E` |

### 4.3 Configuration Management

Zone configuration is stored in the database rather than hardcoded, allowing:
- Addition of new zones without deployment
- Deactivation of deprecated zones
- Timezone corrections
- EIC code updates

---

## 5. Implementation Plan

### 5.1 Phase 1: Core Infrastructure (Week 1-2)

**Deliverables**:
- Project scaffolding with Rust workspace
- ENTSOE client with XML parsing
- PostgreSQL schema with migrations
- Basic scheduler (single zone test)

**Acceptance Criteria**:
- [ ] Successfully fetch prices for NO1 zone
- [ ] Parse XML response into typed structures
- [ ] Store prices in PostgreSQL
- [ ] Handle "no data" responses gracefully

### 5.2 Phase 2: Full Fetching (Week 2-3)

**Deliverables**:
- Rate-limited parallel fetching for all zones
- Retry logic with exponential backoff
- Zone registry with full European coverage
- Fetch logging and status tracking

**Acceptance Criteria**:
- [ ] Fetch all 65+ zones within 5 minutes
- [ ] Respect 400 req/min rate limit
- [ ] Log fetch status for each zone
- [ ] Retry failed zones automatically

### 5.3 Phase 3: REST API (Week 3-4)

**Deliverables**:
- Axum-based REST API
- Zone and country query endpoints
- Health and readiness checks
- OpenAPI documentation

**Acceptance Criteria**:
- [ ] Query prices by zone code
- [ ] Query prices by country code (aggregate)
- [ ] Get latest prices for all zones
- [ ] p95 response time < 100ms

### 5.4 Phase 4: Observability & Production (Week 4-5)

**Deliverables**:
- Prometheus metrics endpoint
- Structured JSON logging
- Dockerfile with multi-stage build
- Kubernetes manifests
- Runbook documentation

**Acceptance Criteria**:
- [ ] Metrics exposed at /metrics
- [ ] Container image < 50MB
- [ ] Health checks work in Kubernetes
- [ ] Alerts configured for fetch failures

---

## 6. Alternatives Considered

### 6.1 Language Choice

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| **Rust** | Type safety, performance, single binary | Steeper learning curve | ✅ Selected |
| Go | Fast compilation, good stdlib | Less type safety, GC pauses | ❌ |
| Python | Rapid development, entsoe-py exists | Runtime, memory usage | ❌ |

**Rationale**: Rust provides the best combination of correctness, performance, and operational simplicity (single static binary).

### 6.2 Database Choice

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| **PostgreSQL 17** | Mature, partitioning, BRIN indexes | Not specialized for time-series | ✅ Selected |
| TimescaleDB | Time-series optimized, compression | Additional complexity | ❌ |
| ClickHouse | Excellent compression, fast aggregates | Overkill for data volume | ❌ |

**Rationale**: Expected data volume (~1.5 MB/day) doesn't justify specialized time-series databases. PostgreSQL with monthly partitions provides adequate performance.

### 6.3 Scheduling Approach

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| **In-process (tokio-cron)** | Shared state, simple deployment | Scheduler coupled to API | ✅ Selected |
| Kubernetes CronJob | Native K8s, isolated failures | Cold starts, no shared state | ❌ |
| External (systemd timer) | OS-native | Not container-friendly | ❌ |

**Rationale**: In-process scheduling allows sharing database connection pools and simplifies deployment. The coupling concern is acceptable given the service's focused scope.

### 6.4 External Services

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| **Direct ENTSOE API** | Authoritative, free, complete | XML-only, complex zones | ✅ Selected |
| ENTSO-E via entsoe-py | Python wrapper exists | Python dependency | ❌ |
| Third-party APIs | JSON, simplified | Cost, incomplete, delayed | ❌ |

**Rationale**: Direct API access provides authoritative data without third-party dependencies or costs.

---

## 7. Risks and Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| ENTSOE API unavailable | Low | High | Retry logic; cached data serves stale; alert on failures |
| Rate limit exceeded | Medium | Medium | Request throttling at 75% limit; exponential backoff |
| XML format changes | Low | High | Defensive parsing; CI tests against fixtures; monitoring |
| Price publication delayed | Medium | Low | Extended retry window (until 16:00 CET) |
| Zone configuration changes | Low | Medium | Database-driven config; monitoring for new zones |
| DST handling bugs | Medium | Medium | Use chrono-tz; explicit tests for transition days |

---

## 8. Security Considerations

### 8.1 Credential Management

| Credential | Storage | Rotation |
|------------|---------|----------|
| ENTSOE API Token | Kubernetes Secret | Manual (on compromise) |
| PostgreSQL Password | Kubernetes Secret | 90 days |

### 8.2 Container Hardening

- **Base image**: `gcr.io/distroless/cc-debian12:nonroot`
- **User**: Non-root (UID 65534)
- **Filesystem**: Read-only root
- **Capabilities**: None required
- **Network**: Outbound HTTPS only

### 8.3 API Security

- Internal-only exposure (no external ingress)
- No authentication required (trusted network)
- Rate limiting deferred (internal clients trusted)

---

## 9. Operational Considerations

### 9.1 Monitoring

**Key metrics**:
| Metric | Alert Threshold | Description |
|--------|-----------------|-------------|
| `entsoe_fetch_errors_total` | > 5 in 1h | Fetch failures |
| `entsoe_zones_with_tomorrow_data` | < 60 after 14:00 | Missing data |
| `http_request_duration_seconds` (p95) | > 500ms | API latency |
| `entsoe_last_fetch_timestamp` | > 25h ago | Stale data |

### 9.2 Runbook Entries

**Fetch Failure Alert**:
1. Check ENTSOE API status at transparency.entsoe.eu
2. Verify network connectivity to `web-api.tp.entsoe.eu`
3. Check for rate limiting (429 responses in logs)
4. Verify API token validity
5. Manual trigger: `curl -X POST http://service/api/v1/admin/fetch`

**Missing Tomorrow's Data**:
1. Check if ENTSOE has published data (web interface)
2. Review fetch logs for the day
3. If data exists on ENTSOE but not locally, trigger manual fetch
4. If data doesn't exist on ENTSOE, wait (market coupling delay)

### 9.3 Backup and Recovery

| Data | Backup Strategy | RTO | RPO |
|------|-----------------|-----|-----|
| Prices | PostgreSQL streaming replication | 15 min | 1 min |
| Zone config | Git (database seed scripts) | 30 min | 0 |

**Recovery procedure**:
1. Deploy new PostgreSQL instance from replica
2. Update connection string
3. Restart service
4. Verify data freshness

---

## 10. Testing Strategy

### 10.1 Test Categories

| Category | Scope | Framework |
|----------|-------|-----------|
| Unit tests | Parsers, zone logic | `cargo test` |
| Integration tests | API + DB | testcontainers-rs |
| Contract tests | ENTSOE response parsing | Recorded fixtures |

### 10.2 Test Fixtures

Maintain XML fixtures for:
- Successful price response
- No data response (code 999)
- Rate limit response
- Malformed response
- DST transition day (23/25 hours)

### 10.3 CI Pipeline

```yaml
stages:
  - lint (clippy, rustfmt)
  - test (unit + integration)
  - build (release binary)
  - container (Docker build + scan)
  - deploy (staging → production)
```

---

## 11. Future Considerations

### 11.1 Potential Extensions (Not in Scope)

| Feature | Effort | Value | Notes |
|---------|--------|-------|-------|
| Intraday prices | Medium | Medium | Different API endpoint |
| Price forecasting | High | High | ML model integration |
| Currency conversion | Low | Medium | External exchange rate API |
| GraphQL API | Medium | Low | REST sufficient for current needs |
| Real-time streaming | High | Medium | WebSocket or SSE |

### 11.2 Scalability Path

Current design supports ~100 QPS with single instance. If needed:
1. Add read replicas for PostgreSQL
2. Deploy multiple API instances (stateless)
3. Add caching layer (Redis) for latest prices
4. Consider TimescaleDB for multi-year retention with compression

---

## 12. Open Questions

| Question | Status | Resolution |
|----------|--------|------------|
| Should we store prices in local currency? | Decided | No - store EUR, convert client-side |
| How long to retain historical data? | Decided | 2 years, then archive |
| Should we support 15-minute resolution? | Deferred | Start with hourly, add if needed |
| External API authentication? | Decided | Not needed (internal only) |

---

## 13. Appendices

### Appendix A: ENTSOE Document Type Codes

| Code | Description |
|------|-------------|
| A44 | Price document |
| A65 | System total load |
| A68 | Installed generation per type |
| A69 | Wind and solar forecast |
| A71 | Generation forecast |
| A73 | Actual generation |

### Appendix B: ENTSOE Process Type Codes

| Code | Description |
|------|-------------|
| A01 | Day ahead |
| A02 | Intra day incremental |
| A16 | Realised |
| A18 | Intraday total |
| A31 | Week ahead |
| A32 | Month ahead |
| A33 | Year ahead |

### Appendix C: Glossary

| Term | Definition |
|------|------------|
| **Bidding Zone** | Geographic area with uniform wholesale electricity price |
| **Day-Ahead Market** | Electricity market where power is traded for delivery the next day |
| **EIC Code** | Energy Identification Code - 16-character unique identifier |
| **ENTSOE** | European Network of Transmission System Operators for Electricity |
| **EUPHEMIA** | Algorithm used for pan-European day-ahead market coupling |
| **SDAC** | Single Day-Ahead Coupling - pan-European market coupling |

---

## 14. References

1. ENTSOE Transparency Platform: https://transparency.entsoe.eu/
2. ENTSOE REST API Documentation: https://transparency.entsoe.eu/content/static_content/Static%20content/web%20api/Guide.html
3. EIC Code Registry: https://www.entsoe.eu/data/energy-identification-codes-eic/
4. Single Day-Ahead Coupling (SDAC): https://www.entsoe.eu/network_codes/cacm/implementation/sdac/
5. entsoe-py (Python reference): https://github.com/EnergieID/entsoe-py

---

## 15. Approval

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Author | | | |
| Technical Lead | | | |
| Platform Lead | | | |
| Security Review | | | |

---

*Document End*
