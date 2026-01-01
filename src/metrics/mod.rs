use std::time::Duration;

use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

// ENTSOE fetch metrics
pub const ENTSOE_FETCH_ATTEMPTS_TOTAL: &str = "entsoe_fetch_attempts_total";
pub const ENTSOE_FETCH_ERRORS_TOTAL: &str = "entsoe_fetch_errors_total";
pub const ENTSOE_FETCH_DURATION_SECONDS: &str = "entsoe_fetch_duration_seconds";
pub const ENTSOE_ZONES_WITH_TOMORROW_DATA: &str = "entsoe_zones_with_tomorrow_data";
pub const ENTSOE_RATE_LIMIT_WAITS_TOTAL: &str = "entsoe_rate_limit_waits_total";
pub const ENTSOE_GAPS_FILLED_TOTAL: &str = "entsoe_gaps_filled_total";
pub const ENTSOE_PRICES_AGGREGATED_TOTAL: &str = "entsoe_prices_aggregated_total";

// HTTP request metrics
pub const HTTP_REQUEST_DURATION_SECONDS: &str = "http_request_duration_seconds";
pub const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";

// Database metrics
pub const DATABASE_QUERY_DURATION_SECONDS: &str = "database_query_duration_seconds";

// Scheduler metrics
pub const SCHEDULER_JOB_EXECUTIONS_TOTAL: &str = "scheduler_job_executions_total";
pub const SCHEDULER_JOB_DURATION_SECONDS: &str = "scheduler_job_duration_seconds";

pub fn init_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Suffix(ENTSOE_FETCH_DURATION_SECONDS.to_string()),
            &[0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0],
        )
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Suffix(HTTP_REQUEST_DURATION_SECONDS.to_string()),
            &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
        )
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Suffix(DATABASE_QUERY_DURATION_SECONDS.to_string()),
            &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0],
        )
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Suffix(SCHEDULER_JOB_DURATION_SECONDS.to_string()),
            &[1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0],
        )
        .unwrap()
        .install_recorder()
        .expect("Failed to install Prometheus recorder")
}

pub fn record_fetch_attempt(zone_code: &str, status: &str) {
    counter!(ENTSOE_FETCH_ATTEMPTS_TOTAL, "zone_code" => zone_code.to_string(), "status" => status.to_string())
        .increment(1);
}

pub fn record_fetch_error(zone_code: &str, error_type: &str) {
    counter!(ENTSOE_FETCH_ERRORS_TOTAL, "zone_code" => zone_code.to_string(), "error_type" => error_type.to_string())
        .increment(1);
}

pub fn record_fetch_duration(zone_code: &str, duration: Duration) {
    histogram!(ENTSOE_FETCH_DURATION_SECONDS, "zone_code" => zone_code.to_string())
        .record(duration.as_secs_f64());
}

pub fn record_http_request(method: &str, path: &str, status: u16, duration: Duration) {
    let status_str = status.to_string();
    counter!(HTTP_REQUESTS_TOTAL, "method" => method.to_string(), "path" => path.to_string(), "status" => status_str.clone())
        .increment(1);
    histogram!(HTTP_REQUEST_DURATION_SECONDS, "method" => method.to_string(), "path" => path.to_string(), "status" => status_str)
        .record(duration.as_secs_f64());
}

pub fn update_zones_with_tomorrow_data(count: u64) {
    gauge!(ENTSOE_ZONES_WITH_TOMORROW_DATA).set(count as f64);
}

pub fn record_rate_limit_wait() {
    counter!(ENTSOE_RATE_LIMIT_WAITS_TOTAL).increment(1);
}

pub fn record_gaps_filled(zone_code: &str, count: u64) {
    counter!(ENTSOE_GAPS_FILLED_TOTAL, "zone_code" => zone_code.to_string()).increment(count);
}

pub fn record_prices_aggregated(zone_code: &str, original_count: u64, aggregated_count: u64) {
    counter!(
        ENTSOE_PRICES_AGGREGATED_TOTAL,
        "zone_code" => zone_code.to_string(),
        "original" => original_count.to_string(),
        "aggregated" => aggregated_count.to_string()
    )
    .increment(1);
}

pub fn record_db_query_duration(operation: &str, duration: Duration) {
    histogram!(DATABASE_QUERY_DURATION_SECONDS, "operation" => operation.to_string())
        .record(duration.as_secs_f64());
}

pub fn record_scheduler_job_execution(job_name: &str, status: &str) {
    counter!(SCHEDULER_JOB_EXECUTIONS_TOTAL, "job_name" => job_name.to_string(), "status" => status.to_string())
        .increment(1);
}

pub fn record_scheduler_job_duration(job_name: &str, duration: Duration) {
    histogram!(SCHEDULER_JOB_DURATION_SECONDS, "job_name" => job_name.to_string())
        .record(duration.as_secs_f64());
}
