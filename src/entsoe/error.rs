use thiserror::Error;

#[derive(Debug, Error)]
pub enum EntsoeError {
    #[error("No data available for the requested period (code 999)")]
    NoData,

    #[error("Rate limited by ENTSOE API (HTTP 429)")]
    RateLimited,

    #[error("ENTSOE API temporarily unavailable: {0}")]
    TemporaryUnavailable(String),

    #[error("Failed to parse XML response: {0}")]
    XmlParseError(String),

    #[error("Invalid response structure: {0}")]
    InvalidResponse(String),

    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Invalid resolution format: {0}")]
    InvalidResolution(String),

    #[error("Failed to parse timestamp: {0}")]
    TimestampParseError(String),
}

impl EntsoeError {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::RateLimited | Self::TemporaryUnavailable(_))
    }
}
