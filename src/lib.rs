#![recursion_limit = "256"]

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
        config.prepare_bootstrap_directories().await?;
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
        }
        config.prepare_directories().await?;
        apply_managed_engine_paths(&mut config).await?;
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

/// Prefer a verified managed binary only when the operator left the matching
/// executable at its built-in command-name default. Explicit CLI, environment,
/// or persistent paths always win.
async fn apply_managed_engine_paths(config: &mut Config) -> Result<()> {
    let manager = services::engines::EngineManager::new(&config.data_dir);
    for (engine, configured, default) in [
        ("yt-dlp", &mut config.ytdlp, "yt-dlp"),
        ("ffmpeg", &mut config.ffmpeg, "ffmpeg"),
        ("7zip", &mut config.seven_zip, "7z"),
        ("rqbit", &mut config.rqbit, "rqbit"),
    ] {
        if configured == std::path::Path::new(default) {
            if let Some(active) = manager.active_path(engine).await? {
                *configured = active;
            }
        }
    }
    Ok(())
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

#[cfg(test)]
mod managed_engine_tests {
    use super::*;
    use clap::Parser;
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn startup_selects_managed_defaults_but_preserves_explicit_paths() {
        let temporary = tempfile::tempdir().unwrap();
        let mut config = Config::parse_from([
            "ravyn",
            "--data-dir",
            temporary.path().to_str().unwrap(),
            "--ffmpeg",
            "custom-ffmpeg",
        ]);
        let bytes = b"managed executable";
        let artifact = services::engines::EngineArtifact {
            engine: "yt-dlp".into(),
            version: "1.0.0".into(),
            target: "test-target".into(),
            url: "https://example.test/yt-dlp".into(),
            sha256: hex::encode(Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
            filename: "yt-dlp.exe".into(),
            capabilities: Vec::new(),
        };
        let installed = services::engines::EngineManager::new(temporary.path())
            .install_verified(&artifact, bytes)
            .await
            .unwrap();

        apply_managed_engine_paths(&mut config).await.unwrap();

        assert_eq!(config.ytdlp, installed);
        assert_eq!(config.ffmpeg, std::path::Path::new("custom-ffmpeg"));
    }
}
