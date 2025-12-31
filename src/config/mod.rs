use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub entsoe: EntsoeConfig,
    pub scheduler: SchedulerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntsoeConfig {
    pub security_token: String,
    pub base_url: String,
    pub rate_limit_per_minute: u32,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub fetch_times_cet: Vec<String>,
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        let config_dir =
            std::env::var("CONFIG_DIR").unwrap_or_else(|_| "config".to_string());

        let builder = config::Config::builder()
            .add_source(config::File::from(
                PathBuf::from(&config_dir).join("default.toml"),
            ))
            .add_source(
                config::File::from(PathBuf::from(&config_dir).join("local.toml"))
                    .required(false),
            )
            .add_source(config::Environment::with_prefix("APP").separator("__"));

        builder.build()?.try_deserialize()
    }
}
