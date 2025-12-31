# Product Requirements Document (PRD)

## ENTSOE European Electricity Price Aggregation Service

| Field | Value |
|-------|-------|
| **Document Version** | 1.0 |
| **Author** | Platform Engineering Team |
| **Date** | December 2024 |
| **Status** | Draft |
| **Product Name** | entsoe-price-fetcher |

---

## 1. Executive Summary

This document outlines the requirements for a service that fetches, stores, and serves day-ahead electricity prices from the ENTSOE Transparency Platform for all European bidding zones. The service will provide a REST API for internal consumers to query electricity prices by zone or by country, supporting operational decision-making and cost optimization for infrastructure running across European data centers.

### 1.1 Problem Statement

European electricity markets publish day-ahead prices daily through the ENTSOE Transparency Platform. These prices vary significantly across the 65+ bidding zones and are essential for:
- Optimizing workload placement across European regions
- Cost forecasting and budgeting
- Energy-aware scheduling of batch jobs
- Internal reporting and analytics

Currently, there is no centralized service to aggregate, store, and expose this data in a developer-friendly format.

### 1.2 Proposed Solution

Build a Rust-based service that:
1. Automatically fetches day-ahead prices daily at 13:00 CET
2. Stores prices in PostgreSQL with historical retention
3. Exposes a REST API for querying prices by zone or country
4. Handles retry logic for data availability delays

---

## 2. Objectives and Success Metrics

### 2.1 Business Objectives

| Objective | Description |
|-----------|-------------|
| **Data Availability** | 99.5% availability of price data within 2 hours of ENTSOE publication |
| **API Reliability** | 99.9% uptime for the REST API |
| **Data Freshness** | Tomorrow's prices available by 14:00 CET on normal days |
| **Coverage** | Support all 65+ European bidding zones |

### 2.2 Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Price fetch success rate | ≥99% | Daily monitoring |
| API response time (p95) | <100ms | Application metrics |
| Data completeness | 100% of zones | Daily audit |
| Historical data retention | ≥2 years | Storage audit |

---

## 3. Scope

### 3.1 In Scope

| Category | Items |
|----------|-------|
| **Data Sources** | ENTSOE Transparency Platform REST API |
| **Data Type** | Day-ahead electricity prices (Article 12.1.D, DocumentType A44) |
| **Geographic Coverage** | All European bidding zones (see Appendix A) |
| **API Endpoints** | Price queries by zone, by country, latest prices |
| **Storage** | PostgreSQL with time-series optimized schema |
| **Deployment** | Containerized deployment (Docker) |

### 3.2 Out of Scope (v1.0)

- Intraday prices
- Cross-border transmission capacities
- Generation forecasts
- Load forecasts
- External authentication (internal-only API)
- Rate limiting on consumer API
- Real-time price streaming

---

## 4. User Personas

### 4.1 Internal Platform Services

**Description**: Automated services that consume price data for optimization decisions.

**Needs**:
- Programmatic access via REST API
- JSON response format
- Low-latency responses (<100ms)
- Reliable data availability

### 4.2 Internal Analytics Teams

**Description**: Teams building reports and dashboards on energy costs.

**Needs**:
- Historical price queries
- Aggregation by country
- Data export capabilities
- Consistent data quality

---

## 5. Functional Requirements

### 5.1 Data Fetching

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | System SHALL fetch day-ahead prices from ENTSOE API daily at 13:00 CET | P0 |
| FR-002 | System SHALL fetch prices for today and tomorrow on each run | P0 |
| FR-003 | If tomorrow's prices are not available, system SHALL retry at 14:00, 15:00, and 16:00 CET | P0 |
| FR-004 | System SHALL authenticate using ENTSOE security token | P0 |
| FR-005 | System SHALL respect ENTSOE rate limits (400 requests/minute) | P0 |
| FR-006 | System SHALL fetch all configured bidding zones in parallel (with rate limiting) | P1 |
| FR-007 | System SHALL handle partial failures gracefully (continue fetching other zones) | P1 |
| FR-008 | System SHALL log all fetch attempts with success/failure status | P0 |

### 5.2 Data Storage

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-010 | System SHALL store prices in PostgreSQL database | P0 |
| FR-011 | System SHALL use UTC timestamps for all stored data | P0 |
| FR-012 | System SHALL store price, currency, and bidding zone for each hourly price point | P0 |
| FR-013 | System SHALL support upsert semantics (handle re-fetching same data) | P0 |
| FR-014 | System SHALL retain historical data for minimum 2 years | P1 |
| FR-015 | System SHALL use table partitioning for efficient queries | P1 |

### 5.3 REST API

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | System SHALL expose GET /api/v1/prices/zone/{zone_code} endpoint | P0 |
| FR-021 | System SHALL expose GET /api/v1/prices/country/{country_code} endpoint | P0 |
| FR-022 | System SHALL expose GET /api/v1/prices/latest endpoint | P0 |
| FR-023 | System SHALL support query parameters: start_date, end_date | P0 |
| FR-024 | System SHALL return prices in JSON format | P0 |
| FR-025 | System SHALL return prices in EUR/MWh (source format) | P0 |
| FR-026 | System SHALL expose GET /health endpoint for liveness checks | P0 |
| FR-027 | System SHALL expose GET /ready endpoint for readiness checks | P0 |
| FR-028 | System SHALL expose GET /api/v1/zones endpoint listing available zones | P1 |
| FR-029 | System SHALL expose GET /api/v1/countries endpoint listing country→zone mappings | P1 |

### 5.4 API Response Format

**Price Query Response**:
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
    },
    {
      "timestamp": "2024-12-15T01:00:00Z",
      "price": 42.18
    }
  ],
  "fetched_at": "2024-12-14T12:05:23Z"
}
```

**Country Query Response**:
```json
{
  "country_code": "NO",
  "country_name": "Norway",
  "zones": [
    {
      "zone_code": "NO1",
      "zone_name": "Oslo",
      "prices": [...]
    },
    {
      "zone_code": "NO2",
      "zone_name": "Kristiansand",
      "prices": [...]
    }
  ]
}
```

---

## 6. Non-Functional Requirements

### 6.1 Performance

| ID | Requirement | Target |
|----|-------------|--------|
| NFR-001 | API response time (p50) | <20ms |
| NFR-002 | API response time (p95) | <100ms |
| NFR-003 | API response time (p99) | <500ms |
| NFR-004 | Concurrent request handling | ≥100 requests/second |
| NFR-005 | Fetch cycle completion time | <5 minutes for all zones |

### 6.2 Reliability

| ID | Requirement | Target |
|----|-------------|--------|
| NFR-010 | API availability | 99.9% uptime |
| NFR-011 | Data fetch success rate | ≥99% per day |
| NFR-012 | Maximum data staleness | 4 hours after ENTSOE publication |
| NFR-013 | Recovery time objective (RTO) | <15 minutes |

### 6.3 Scalability

| ID | Requirement | Notes |
|----|-------------|-------|
| NFR-020 | Horizontal scaling | Stateless API allows multiple replicas |
| NFR-021 | Database scaling | Partitioned tables, read replicas if needed |
| NFR-022 | Storage growth | ~1.5 MB/day (65 zones × 24 hours × ~100 bytes) |

### 6.4 Security

| ID | Requirement | Priority |
|----|-------------|----------|
| NFR-030 | ENTSOE API token stored as secret (not in code/config) | P0 |
| NFR-031 | Database credentials stored as secrets | P0 |
| NFR-032 | No external authentication required (internal network only) | P0 |
| NFR-033 | Container runs as non-root user | P1 |

### 6.5 Observability

| ID | Requirement | Priority |
|----|-------------|----------|
| NFR-040 | Structured JSON logging | P0 |
| NFR-041 | Prometheus metrics endpoint (/metrics) | P1 |
| NFR-042 | Metrics: fetch_success_total, fetch_errors_total, api_request_duration | P1 |
| NFR-043 | Health check endpoints for Kubernetes | P0 |

---

## 7. ENTSOE API Specification Details

### 7.1 Authentication

- **Token Acquisition**: Register at transparency.entsoe.eu, email transparency@entsoe.eu with subject "Restful API access"
- **Token Usage**: Query parameter `securityToken=` or HTTP header `SECURITY_TOKEN:`
- **Token Format**: Alphanumeric string, does not expire automatically

### 7.2 Day-Ahead Prices Endpoint

| Parameter | Value | Description |
|-----------|-------|-------------|
| Base URL | `https://web-api.tp.entsoe.eu/api` | Production endpoint |
| documentType | `A44` | Price document (Article 12.1.D) |
| processType | `A01` | Day ahead |
| in_Domain | EIC Code | Bidding zone (e.g., `10YNO-1--------2`) |
| out_Domain | EIC Code | Same as in_Domain for prices |
| periodStart | `YYYYMMDDHHMM` | UTC timestamp (12 digits) |
| periodEnd | `YYYYMMDDHHMM` | UTC timestamp (12 digits) |

**Example Request**:
```
GET https://web-api.tp.entsoe.eu/api?
    securityToken={TOKEN}&
    documentType=A44&
    processType=A01&
    in_Domain=10YNO-1--------2&
    out_Domain=10YNO-1--------2&
    periodStart=202412150000&
    periodEnd=202412160000
```

### 7.3 Rate Limits

| Limit | Value | Consequence |
|-------|-------|-------------|
| Requests per minute | 400 | Tracked by IP and token |
| Exceeded limit | 10-minute ban | HTTP 429 returned |

### 7.4 Response Format

- Format: XML only (no JSON option)
- Success: `Publication_MarketDocument` with `TimeSeries`
- No data: `Acknowledgement_MarketDocument` with code `999`
- Prices: EUR/MWh in `price.amount` field
- Resolution: `PT60M` (hourly) or `PT15M` (15-minute)

### 7.5 Data Availability

| Event | Time (CET) |
|-------|------------|
| Market gate closure | 12:00 |
| EUPHEMIA calculation | 12:00-12:45 |
| Results publication | 12:55-12:57 |
| Fallback procedures | May extend to 14:20 |

---

## 8. European Bidding Zones

### 8.1 Single-Zone Countries

| Country | Zone Code | EIC Code |
|---------|-----------|----------|
| Germany-Luxembourg | DE-LU | `10Y1001A1001A82H` |
| France | FR | `10YFR-RTE------C` |
| Netherlands | NL | `10YNL----------L` |
| Belgium | BE | `10YBE----------2` |
| Austria | AT | `10YAT-APG------L` |
| Spain | ES | `10YES-REE------0` |
| Portugal | PT | `10YPT-REN------W` |
| Poland | PL | `10YPL-AREA-----S` |
| Finland | FI | `10YFI-1--------U` |
| Czech Republic | CZ | `10YCZ-CEPS-----N` |
| Hungary | HU | `10YHU-MAVIR----U` |
| Romania | RO | `10YRO-TEL------P` |
| Bulgaria | BG | `10YCA-BULGARIA-R` |
| Greece | GR | `10YGR-HTSO-----Y` |
| Switzerland | CH | `10YCH-SWISSGRIDZ` |
| Slovenia | SI | `10YSI-ELES-----O` |
| Slovakia | SK | `10YSK-SEPS-----K` |
| Croatia | HR | `10YHR-HEP------M` |
| Serbia | RS | `10YCS-SERBIATSOV` |
| Estonia | EE | `10Y1001A1001A39I` |
| Latvia | LV | `10YLV-1001A00074` |
| Lithuania | LT | `10YLT-1001A0008Q` |
| Ireland | IE | `10Y1001A1001A59C` |

### 8.2 Multi-Zone Countries

#### Norway (5 zones)
| Zone | Name | EIC Code |
|------|------|----------|
| NO1 | Oslo | `10YNO-1--------2` |
| NO2 | Kristiansand | `10YNO-2--------T` |
| NO3 | Trondheim | `10YNO-3--------J` |
| NO4 | Tromsø | `10YNO-4--------9` |
| NO5 | Bergen | `10Y1001A1001A48H` |

#### Sweden (4 zones)
| Zone | Name | EIC Code |
|------|------|----------|
| SE1 | Luleå | `10Y1001A1001A44P` |
| SE2 | Sundsvall | `10Y1001A1001A45N` |
| SE3 | Stockholm | `10Y1001A1001A46L` |
| SE4 | Malmö | `10Y1001A1001A47J` |

#### Denmark (2 zones)
| Zone | Name | EIC Code |
|------|------|----------|
| DK1 | West (Jutland) | `10YDK-1--------W` |
| DK2 | East (Zealand) | `10YDK-2--------M` |

#### Italy (6+ zones)
| Zone | Name | EIC Code |
|------|------|----------|
| IT-North | Northern Italy | `10Y1001A1001A73I` |
| IT-Centre-North | Central-North | `10Y1001A1001A70O` |
| IT-Centre-South | Central-South | `10Y1001A1001A71M` |
| IT-South | Southern Italy | `10Y1001A1001A788` |
| IT-Sardinia | Sardinia | `10Y1001A1001A74G` |
| IT-Sicily | Sicily | `10Y1001A1001A75E` |

---

## 9. Dependencies

### 9.1 External Dependencies

| Dependency | Type | Criticality | Mitigation |
|------------|------|-------------|------------|
| ENTSOE API | External service | Critical | Retry logic, caching |
| PostgreSQL | Database | Critical | Connection pooling, health checks |

### 9.2 Infrastructure Requirements

| Component | Specification |
|-----------|--------------|
| Container Runtime | Docker / containerd |
| Orchestration | Kubernetes (optional) |
| Database | PostgreSQL 17 |
| Network | Outbound HTTPS to web-api.tp.entsoe.eu |

---

## 10. Risks and Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| ENTSOE API unavailable | Low | High | Retry logic, cached data serves stale |
| API rate limit exceeded | Medium | Medium | Request throttling, exponential backoff |
| Data format changes | Low | High | Defensive XML parsing, versioned schemas |
| Price publication delayed | Medium | Low | Extended retry window (until 16:00 CET) |
| Zone configuration changes | Low | Medium | External configuration, easy updates |

---

## 11. Timeline and Milestones

| Phase | Duration | Deliverables |
|-------|----------|--------------|
| Phase 1: Core Fetcher | 2 weeks | ENTSOE client, scheduler, basic storage |
| Phase 2: REST API | 1 week | Zone/country endpoints, health checks |
| Phase 3: Observability | 1 week | Metrics, structured logging, alerts |
| Phase 4: Production | 1 week | Container optimization, deployment |

---

## 12. Appendices

### Appendix A: Complete Zone List

See Section 8 for complete bidding zone reference.

### Appendix B: Glossary

| Term | Definition |
|------|------------|
| Bidding Zone | Geographic area with uniform wholesale electricity price |
| Day-Ahead Market | Electricity market where power is traded for delivery the next day |
| ENTSOE | European Network of Transmission System Operators for Electricity |
| EIC Code | Energy Identification Code - 16-character unique identifier |
| SDAC | Single Day-Ahead Coupling - pan-European market coupling mechanism |
| EUPHEMIA | Algorithm used to calculate day-ahead prices across coupled markets |

---

*Document End*
