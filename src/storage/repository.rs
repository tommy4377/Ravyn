//! The shared SQLite handle. Domain-specific queries live in the sibling
//! modules (`jobs`, `outputs`, `schedules`, `audit`, `secrets`, `settings`,
//! `backup`, `media`, `automation`, `torrent_policy`, `pagination`), each
//! contributing its own `impl Repository` block.

use std::str::FromStr;

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::error::Result;

#[derive(Clone)]
pub struct Repository {
    pool: SqlitePool,
    /// Shared across every clone; installed once by the manager so hot-path
    /// query latency can be observed without threading a handle everywhere.
    metrics: std::sync::Arc<std::sync::OnceLock<crate::core::metrics::Metrics>>,
}

impl Repository {
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Installs the shared metrics registry. Later calls are ignored, and all
    /// existing clones observe the installed registry.
    pub fn attach_metrics(&self, metrics: crate::core::metrics::Metrics) {
        let _ = self.metrics.set(metrics);
    }

    pub(crate) fn metrics_handle(&self) -> Option<&crate::core::metrics::Metrics> {
        self.metrics.get()
    }

    pub(crate) fn observe_query(&self, operation: &'static str, started: std::time::Instant) {
        if let Some(metrics) = self.metrics.get() {
            metrics.db_query(operation, started.elapsed());
        }
    }

    pub async fn connect(url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self {
            pool,
            metrics: std::sync::Arc::new(std::sync::OnceLock::new()),
        })
    }
}
