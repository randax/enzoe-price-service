use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::fetcher::FetcherService;
use crate::metrics;

pub struct PriceFetchScheduler {
    scheduler: JobScheduler,
    fetcher: Arc<FetcherService>,
}

impl PriceFetchScheduler {
    pub async fn new(fetcher: Arc<FetcherService>) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self { scheduler, fetcher })
    }

    async fn add_primary_fetch_job(&self) -> Result<()> {
        let fetcher = Arc::clone(&self.fetcher);
        
        let job = Job::new_async_tz("0 0 13 * * *", chrono_tz::Europe::Oslo, move |_uuid, _lock| {
            let fetcher = Arc::clone(&fetcher);
            Box::pin(async move {
                let start = Instant::now();
                let job_name = "primary_fetch_13:00";
                info!("Starting primary daily fetch job (13:00 CET)");
                match fetcher.fetch_all_prices().await {
                    Ok(summary) => {
                        metrics::record_scheduler_job_execution(job_name, "success");
                        metrics::record_scheduler_job_duration(job_name, start.elapsed());
                        info!(
                            succeeded = summary.succeeded,
                            failed = summary.failed,
                            no_data = summary.no_data,
                            total_prices = summary.total_prices_stored,
                            "Primary fetch job completed"
                        );
                    }
                    Err(e) => {
                        metrics::record_scheduler_job_execution(job_name, "failure");
                        metrics::record_scheduler_job_duration(job_name, start.elapsed());
                        error!(error = %e, "Primary fetch job failed");
                    }
                }
            })
        })?;

        self.scheduler.add(job).await?;
        info!("Added primary fetch job at 13:00 CET");
        Ok(())
    }

    async fn add_conditional_fetch_job(&self, cron_expr: &str, job_name: &str) -> Result<()> {
        let fetcher = Arc::clone(&self.fetcher);
        let name = job_name.to_string();

        let job = Job::new_async_tz(cron_expr, chrono_tz::Europe::Oslo, move |_uuid, _lock| {
            let fetcher = Arc::clone(&fetcher);
            let job_name = name.clone();
            Box::pin(async move {
                let start = Instant::now();
                info!(job = %job_name, "Starting conditional fetch job");
                match fetcher.fetch_tomorrow_if_missing().await {
                    Ok(summary) => {
                        metrics::record_scheduler_job_execution(&job_name, "success");
                        metrics::record_scheduler_job_duration(&job_name, start.elapsed());
                        if summary.succeeded == 0 && summary.no_data == 0 && summary.failed == 0 {
                            info!(job = %job_name, "Conditional fetch skipped - data already exists");
                        } else {
                            info!(
                                job = %job_name,
                                succeeded = summary.succeeded,
                                failed = summary.failed,
                                no_data = summary.no_data,
                                total_prices = summary.total_prices_stored,
                                "Conditional fetch job completed"
                            );
                        }
                    }
                    Err(e) => {
                        metrics::record_scheduler_job_execution(&job_name, "failure");
                        metrics::record_scheduler_job_duration(&job_name, start.elapsed());
                        error!(job = %job_name, error = %e, "Conditional fetch job failed");
                    }
                }
            })
        })?;

        self.scheduler.add(job).await?;
        info!(job = %job_name, cron = %cron_expr, "Added conditional fetch job");
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        self.add_primary_fetch_job().await?;
        
        self.add_conditional_fetch_job("0 0 14 * * *", "retry_1_14:00").await?;
        self.add_conditional_fetch_job("0 0 15 * * *", "retry_2_15:00").await?;
        self.add_conditional_fetch_job("0 0 16 * * *", "retry_3_16:00").await?;

        self.scheduler.start().await?;
        info!("Price fetch scheduler started");
        
        Ok(())
    }

    pub async fn shutdown(mut self) -> Result<()> {
        self.scheduler.shutdown().await?;
        info!("Price fetch scheduler stopped");
        Ok(())
    }
}
