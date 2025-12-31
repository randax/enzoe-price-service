-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS pg_partman;

-- Bidding zones registry table
CREATE TABLE bidding_zones (
    zone_code       VARCHAR(20) PRIMARY KEY,
    zone_name       VARCHAR(100) NOT NULL,
    country_code    VARCHAR(2) NOT NULL,
    country_name    VARCHAR(100) NOT NULL,
    eic_code        VARCHAR(20) NOT NULL UNIQUE,
    timezone        VARCHAR(50) NOT NULL DEFAULT 'Europe/Oslo',
    active          BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for country-based queries
CREATE INDEX idx_bidding_zones_country ON bidding_zones(country_code) WHERE active = TRUE;

-- Electricity prices table (partitioned by month)
CREATE TABLE electricity_prices (
    timestamp       TIMESTAMPTZ NOT NULL,
    bidding_zone    VARCHAR(20) NOT NULL REFERENCES bidding_zones(zone_code),
    price_kwh       NUMERIC(12,6) NOT NULL,  -- EUR per kWh (converted from MWh)
    currency        VARCHAR(3) NOT NULL DEFAULT 'EUR',
    resolution      VARCHAR(10) NOT NULL DEFAULT 'PT60M',
    fetched_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (timestamp, bidding_zone)
) PARTITION BY RANGE (timestamp);

-- Create initial partitions for current and next 3 months
SELECT partman.create_parent(
    p_parent_table := 'public.electricity_prices',
    p_control := 'timestamp',
    p_type := 'native',
    p_interval := '1 month',
    p_premake := 3
);

-- BRIN index on timestamp for efficient range scans
CREATE INDEX idx_electricity_prices_timestamp 
    ON electricity_prices USING BRIN (timestamp) 
    WITH (pages_per_range = 128);

-- B-tree index on bidding_zone for zone-specific queries
CREATE INDEX idx_electricity_prices_zone 
    ON electricity_prices (bidding_zone, timestamp DESC);

-- Fetch log table for tracking API calls
CREATE TABLE fetch_log (
    id              BIGSERIAL PRIMARY KEY,
    fetch_started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    fetch_completed_at TIMESTAMPTZ,
    bidding_zone    VARCHAR(20) REFERENCES bidding_zones(zone_code),
    period_start    TIMESTAMPTZ NOT NULL,
    period_end      TIMESTAMPTZ NOT NULL,
    status          VARCHAR(20) NOT NULL CHECK (status IN ('pending', 'success', 'nodata', 'error', 'ratelimited')),
    records_inserted INTEGER DEFAULT 0,
    error_message   TEXT,
    http_status     INTEGER,
    duration_ms     INTEGER
);

-- Index for monitoring recent fetches
CREATE INDEX idx_fetch_log_recent 
    ON fetch_log (fetch_started_at DESC, status);

-- Index for zone-specific fetch history
CREATE INDEX idx_fetch_log_zone 
    ON fetch_log (bidding_zone, fetch_started_at DESC);

-- Seed Norwegian bidding zones
INSERT INTO bidding_zones (zone_code, zone_name, country_code, country_name, eic_code, timezone) VALUES
    ('NO1', 'Oslo', 'NO', 'Norway', '10YNO-1--------2', 'Europe/Oslo'),
    ('NO2', 'Kristiansand', 'NO', 'Norway', '10YNO-2--------T', 'Europe/Oslo'),
    ('NO3', 'Trondheim', 'NO', 'Norway', '10YNO-3--------J', 'Europe/Oslo'),
    ('NO4', 'Tromsø', 'NO', 'Norway', '10YNO-4--------9', 'Europe/Oslo'),
    ('NO5', 'Bergen', 'NO', 'Norway', '10Y1001A1001A48H', 'Europe/Oslo');

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for bidding_zones
CREATE TRIGGER update_bidding_zones_updated_at
    BEFORE UPDATE ON bidding_zones
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
