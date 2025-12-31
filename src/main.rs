use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use entsoe_price_fetcher::{
    create_router, init_metrics, AppConfig, EntsoeClient, FetcherService, PriceFetchScheduler,
    PriceRepository,
};

#[tokio::main]
async fn main() -> Result<()> {
    let metrics_handle = init_metrics();

    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "entsoe_price_fetcher=debug,tower_http=debug".into());

    if log_format == "json" {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    let config = AppConfig::load()?;
    info!("Configuration loaded successfully");

    let repository = Arc::new(PriceRepository::from_config(&config.database).await?);
    info!("Database connection pool initialized");

    let client = Arc::new(EntsoeClient::new(&config.entsoe)?);
    info!("ENTSOE client initialized");

    let fetcher = Arc::new(FetcherService::new(Arc::clone(&client), Arc::clone(&repository)));
    
    let scheduler = if config.scheduler.enabled {
        let scheduler = PriceFetchScheduler::new(Arc::clone(&fetcher)).await?;
        scheduler.start().await?;
        info!("Scheduler started with fetch times at 13:00, 14:00, 15:00, 16:00 CET");
        Some(scheduler)
    } else {
        info!("Scheduler disabled in configuration");
        None
    };

    let router = create_router(Arc::clone(&repository), metrics_handle);
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(host = %config.server.host, port = %config.server.port, "API server listening");

    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            error!(error = %e, "API server error");
        }
    });

    signal::ctrl_c().await?;
    info!("Shutdown signal received");

    server_handle.abort();

    if let Some(scheduler) = scheduler {
        if let Err(e) = scheduler.shutdown().await {
            error!(error = %e, "Error shutting down scheduler");
        }
    }

    info!("Application stopped");
    Ok(())
}
