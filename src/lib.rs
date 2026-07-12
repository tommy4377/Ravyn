pub mod adapters;
pub mod api;
pub mod config;
pub mod core;
pub mod download;
pub mod error;
pub mod postprocess;
pub mod services;
pub mod storage;

use std::sync::Arc;

use config::Config;
use core::manager::JobManager;
use error::Result;
use storage::Repository;

/// Fully initialized backend services shared by the API and background workers.
pub struct Ravyn {
    /// Effective configuration after persistent settings are applied.
    pub config: Arc<Config>,
    /// Environment/CLI-derived configuration before persistent overrides.
    pub base_config: Arc<Config>,
    pub repository: Repository,
    pub manager: Arc<JobManager>,
}

impl Ravyn {
    /// Opens persistent storage and creates all long-lived backend services.
    pub async fn bootstrap(mut config: Config) -> Result<Self> {
        install_tls_provider()?;
        config.validate()?;
        config.prepare_directories().await?;
        let applied_restore = storage::recovery::apply_pending(&config.data_dir).await?;
        let repository = match Repository::connect(&config.database_url()).await {
            Ok(repository) => {
                if let Some(applied) = applied_restore {
                    if let Err(error) = storage::recovery::finalize(&config.data_dir, applied).await
                    {
                        tracing::warn!(%error, "database restore succeeded but rollback archival failed");
                    }
                }
                repository
            }
            Err(error) => {
                if let Some(applied) = applied_restore {
                    storage::recovery::rollback_after_open_failure(&config.data_dir, applied)
                        .await?;
                    tracing::error!(%error, "database restore failed; previous database was restored");
                    Repository::connect(&config.database_url()).await?
                } else {
                    return Err(error);
                }
            }
        };
        let base_config = Arc::new(config.clone());
        if let Some(settings) = repository.load_persistent_settings().await? {
            settings.apply_to(&mut config)?;
            config.prepare_directories().await?;
        }
        let config = Arc::new(config);
        let manager = Arc::new(JobManager::new(config.clone(), repository.clone()).await?);
        Ok(Self {
            config,
            base_config,
            repository,
            manager,
        })
    }
}

fn install_tls_provider() -> Result<()> {
    match rustls::crypto::ring::default_provider().install_default() {
        Ok(()) => Ok(()),
        Err(_) if rustls::crypto::CryptoProvider::get_default().is_some() => Ok(()),
        Err(_) => Err(error::RavynError::Internal(
            "failed to install the rustls Ring crypto provider".into(),
        )),
    }
}
