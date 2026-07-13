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
    /// Effective runtime configuration after managed engine resolution.
    pub config: Arc<Config>,
    /// Environment/CLI-derived configuration before persistent overrides.
    pub base_config: Arc<Config>,
    /// User-configured values after persistent overrides but before managed
    /// engine paths are substituted.
    pub configured_config: Arc<Config>,
    pub repository: Repository,
    pub manager: Arc<JobManager>,
    pub provisioning_cancellation: services::components::ProvisioningCancellation,
    pub component_manifest: Arc<dyn services::components::ManifestProvider>,
    _rqbit_process: Option<services::rqbit_process::RqbitProcessManager>,
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
        let component_manifest = services::components::default_manifest_provider(&config.data_dir)?;
        apply_managed_engine_paths(&mut config).await?;
        let rqbit_process = start_managed_rqbit_if_required(&mut config, &repository).await?;
        let configured_config = Arc::new(config.clone());
        let provisioning_cancellation = services::components::ProvisioningCancellation::new();
        let config = Arc::new(config);
        let manager = Arc::new(JobManager::new(config.clone(), repository.clone()).await?);
        reconcile_interrupted_component_operations(
            configured_config.clone(),
            repository.clone(),
            component_manifest.clone(),
        )
        .await?;

        if configured_config.auto_provision {
            let provision_config = configured_config.clone();
            let provision_repo = repository.clone();
            let registry = provisioning_cancellation.clone();
            let manifest = component_manifest.clone();
            let events = manager.events();
            tokio::spawn(async move {
                if let Err(error) = ensure_provisioned_components(
                    provision_config,
                    provision_repo,
                    registry,
                    manifest,
                    events,
                )
                .await
                {
                    tracing::warn!(%error, "background component provisioning encountered errors");
                }
            });
        }

        Ok(Self {
            config,
            base_config,
            configured_config,
            repository,
            manager,
            provisioning_cancellation,
            component_manifest,
            _rqbit_process: rqbit_process,
        })
    }
}

/// Starts rqbit only when a verified Ravyn-managed binary is active and the
/// persisted feature selection requires torrent support. Custom rqbit paths
/// and remote endpoints remain operator-owned and are never spawned here.
async fn start_managed_rqbit_if_required(
    config: &mut Config,
    repository: &Repository,
) -> Result<Option<services::rqbit_process::RqbitProcessManager>> {
    use services::components::{FeatureId, effective_feature_set};

    let Some((profile, selections)) = repository.load_feature_selections().await? else {
        return Ok(None);
    };
    if !effective_feature_set(profile, &selections)?.contains(&FeatureId::TorrentSupport) {
        return Ok(None);
    }
    let engines = services::engines::EngineManager::new(&config.data_dir);
    let Some(managed) = engines.active_path("rqbit").await? else {
        return Ok(None);
    };
    if managed != config.rqbit {
        return Ok(None);
    }

    let process = services::rqbit_process::RqbitProcessManager::new(&config.data_dir);
    process.start(&managed, config).await?;
    Ok(Some(process))
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

/// Reconcile provisioning states left behind by an interrupted process.
///
/// Lifecycle states are durable for frontend resynchronization while an
/// operation is active, but they must not remain busy forever after a crash.
async fn reconcile_interrupted_component_operations(
    config: Arc<Config>,
    repository: Repository,
    manifest_provider: Arc<dyn services::components::ManifestProvider>,
) -> Result<()> {
    use services::components::{ComponentId, ComponentManager, PersistedComponent};

    let records = repository.load_component_records().await?;
    let manager = ComponentManager::new(
        &config.data_dir,
        manifest_provider,
        tokio_util::sync::CancellationToken::new(),
    );
    for &component in ComponentId::ALL {
        let Some(previous) = records.get(&component) else {
            continue;
        };
        if !previous.state.is_busy() {
            continue;
        }
        let state = manager
            .component_state(component, &config, &records, false)
            .await;
        let active = manager.active_managed_component(component).await.ok().flatten();
        let custom_path = component_config_path(component, &config);
        let custom = custom_path != std::path::Path::new(component.default_command());
        let now = chrono::Utc::now();
        repository
            .save_component_record(&PersistedComponent {
                component,
                state,
                managed_version: active
                    .as_ref()
                    .map(|installed| installed.version.clone()),
                detected_version: previous.detected_version.clone(),
                managed_path: active
                    .as_ref()
                    .map(|installed| installed.path.clone()),
                custom_path: custom.then(|| custom_path.clone()),
                error_message: Some("component operation was interrupted by a previous shutdown".into()),
                last_checked_at: Some(now),
                verified_at: previous.verified_at,
                install_started_at: previous.install_started_at,
                install_completed_at: Some(now),
            })
            .await?;
    }
    Ok(())
}

/// Provision all components required by the user's feature selections.
///
/// The startup worker runs at most two independent component operations in
/// parallel. Failures are persisted and published without preventing Ravyn or
/// unrelated components from starting.
async fn ensure_provisioned_components(
    config: Arc<Config>,
    repository: Repository,
    registry: services::components::ProvisioningCancellation,
    manifest_provider: Arc<dyn services::components::ManifestProvider>,
    events: core::events::EventBus,
) -> Result<()> {
    use futures_util::{StreamExt, stream};
    use services::components::{effective_feature_set, required_components_for_features};

    let (profile, features_json) = match repository.load_feature_selections().await? {
        Some((profile, features)) => (profile, features),
        None => return Ok(()),
    };
    let features = effective_feature_set(profile, &features_json)?;
    let required = required_components_for_features(&features);

    stream::iter(required)
        .map(|component| {
            let config = config.clone();
            let repository = repository.clone();
            let registry = registry.clone();
            let manifest_provider = manifest_provider.clone();
            let events = events.clone();
            async move {
                if let Err(error) = provision_component(
                    component,
                    config,
                    repository,
                    registry,
                    manifest_provider,
                    events,
                )
                .await
                {
                    tracing::warn!(%error, component = component.engine_name(), "startup provisioning failed");
                }
            }
        })
        .buffer_unordered(2)
        .collect::<Vec<_>>()
        .await;
    Ok(())
}

async fn provision_component(
    component: services::components::ComponentId,
    config: Arc<Config>,
    repository: Repository,
    registry: services::components::ProvisioningCancellation,
    manifest_provider: Arc<dyn services::components::ManifestProvider>,
    events: core::events::EventBus,
) -> Result<()> {
    use services::components::{ComponentManager, ComponentState, PersistedComponent};

    let token = registry.begin(component)?;
    let result = async {
        let _permit = registry.acquire(&token).await?;
        let manager = ComponentManager::new(&config.data_dir, manifest_provider, token);
        let records = repository.load_component_records().await?;
        let state = manager.component_state(component, &config, &records, false).await;
        if matches!(state, ComponentState::Installed | ComponentState::CustomPath) {
            let health = manager.health_check(component, &config, &records).await;
            let checked = chrono::Utc::now();
            let healthy_state = if state == ComponentState::CustomPath {
                ComponentState::CustomPath
            } else {
                ComponentState::Installed
            };
            let unhealthy_state = if state == ComponentState::CustomPath {
                ComponentState::CustomPathInvalid
            } else {
                ComponentState::Failed
            };
            let active = manager.active_managed_component(component).await?;
            let record = PersistedComponent {
                component,
                state: if health.healthy { healthy_state } else { unhealthy_state },
                managed_version: active.as_ref().map(|installed| installed.version.clone()),
                detected_version: health.version.clone(),
                managed_path: active.as_ref().map(|installed| installed.path.clone()),
                custom_path: (state == ComponentState::CustomPath)
                    .then(|| component_config_path(component, &config).clone()),
                error_message: health.message.clone(),
                last_checked_at: Some(checked),
                verified_at: if health.healthy {
                    Some(checked)
                } else {
                    records.get(&component).and_then(|record| record.verified_at)
                },
                install_started_at: records
                    .get(&component)
                    .and_then(|record| record.install_started_at),
                install_completed_at: records
                    .get(&component)
                    .and_then(|record| record.install_completed_at),
            };
            repository.save_component_record(&record).await?;
            publish_component_event(
                &events,
                component,
                record.state,
                None,
                None,
                None,
                record.error_message,
            );
            return Ok(());
        }
        if state == ComponentState::CustomPathInvalid {
            let record = PersistedComponent {
                component,
                state,
                managed_version: None,
                detected_version: None,
                managed_path: None,
                custom_path: Some(component_config_path(component, &config).clone()),
                error_message: Some("configured executable cannot be resolved".into()),
                last_checked_at: Some(chrono::Utc::now()),
                verified_at: None,
                install_started_at: None,
                install_completed_at: None,
            };
            repository.save_component_record(&record).await?;
            publish_component_event(&events, component, state, None, None, None, record.error_message);
            return Ok(());
        }
        if manager.manifest_artifact(component)?.is_none() {
            let message = format!(
                "no verified {} artifact is available for {}",
                component.engine_name(),
                manager.target()
            );
            let record = PersistedComponent {
                component,
                state: ComponentState::Unsupported,
                managed_version: None,
                detected_version: None,
                managed_path: None,
                custom_path: None,
                error_message: Some(message.clone()),
                last_checked_at: Some(chrono::Utc::now()),
                verified_at: None,
                install_started_at: None,
                install_completed_at: None,
            };
            repository.save_component_record(&record).await?;
            publish_component_event(
                &events,
                component,
                ComponentState::Unsupported,
                None,
                None,
                None,
                Some(message),
            );
            return Ok(());
        }

        let started = chrono::Utc::now();
        let record = PersistedComponent {
            component,
            state: ComponentState::Queued,
            managed_version: records.get(&component).and_then(|record| record.managed_version.clone()),
            detected_version: records.get(&component).and_then(|record| record.detected_version.clone()),
            managed_path: records.get(&component).and_then(|record| record.managed_path.clone()),
            custom_path: None,
            error_message: None,
            last_checked_at: Some(started),
            verified_at: records.get(&component).and_then(|record| record.verified_at),
            install_started_at: Some(started),
            install_completed_at: None,
        };
        repository.save_component_record(&record).await?;
        publish_component_event(&events, component, ComponentState::Queued, None, None, None, None);

        let progress_events = events.clone();
        let throttle = std::sync::Mutex::new(
            std::time::Instant::now() - std::time::Duration::from_millis(250),
        );
        let report = move |received: u64, total: u64| {
            let mut last = throttle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let done = total > 0 && received >= total;
            if !done && last.elapsed() < std::time::Duration::from_millis(250) {
                return;
            }
            *last = std::time::Instant::now();
            let pct = (total > 0).then(|| {
                u8::try_from(received.saturating_mul(100) / total).unwrap_or(100)
            });
            publish_component_event(
                &progress_events,
                component,
                ComponentState::Downloading,
                pct,
                Some(received),
                Some(total),
                None,
            );
        };

        let (stage_tx, mut stage_rx) = tokio::sync::mpsc::unbounded_channel();
        let stage_repo = repository.clone();
        let stage_events = events.clone();
        let stage_writer = tokio::spawn(async move {
            while let Some(stage) = stage_rx.recv().await {
                if let Err(error) = stage_repo
                    .update_component_lifecycle(component, stage, None)
                    .await
                {
                    tracing::warn!(%error, component = component.engine_name(), "failed to persist component stage");
                }
                publish_component_event(&stage_events, component, stage, None, None, None, None);
            }
        });
        let stage = move |state: ComponentState| {
            let _ = stage_tx.send(state);
        };

        let installation = manager
            .install_component_with_progress(component, &config, Some(&report), Some(&stage))
            .await;
        drop(stage);
        if let Err(error) = stage_writer.await {
            tracing::warn!(%error, component = component.engine_name(), "component stage writer failed");
        }

        match installation {
            Ok(installed) => {
                let completed = chrono::Utc::now();
                let record = PersistedComponent {
                    component,
                    state: ComponentState::Installed,
                    managed_version: Some(installed.version.clone()),
                    detected_version: installed.detected_version,
                    managed_path: Some(installed.path),
                    custom_path: None,
                    error_message: None,
                    last_checked_at: Some(completed),
                    verified_at: Some(completed),
                    install_started_at: Some(started),
                    install_completed_at: Some(completed),
                };
                repository.save_component_record(&record).await?;
                publish_component_event(
                    &events,
                    component,
                    ComponentState::Installed,
                    Some(100),
                    None,
                    None,
                    None,
                );
            }
            Err(crate::error::RavynError::Cancelled) => {
                let message = "installation cancelled by user".to_owned();
                let completed = chrono::Utc::now();
                let (state, active) = manager
                    .state_after_unsuccessful_operation(component, ComponentState::Cancelled)
                    .await?;
                let record = PersistedComponent {
                    component,
                    state,
                    managed_version: active.as_ref().map(|installed| installed.version.clone()),
                    detected_version: active.as_ref().map(|installed| installed.version.clone()),
                    managed_path: active.as_ref().map(|installed| installed.path.clone()),
                    custom_path: None,
                    error_message: Some(message.clone()),
                    last_checked_at: Some(completed),
                    verified_at: records.get(&component).and_then(|record| record.verified_at),
                    install_started_at: Some(started),
                    install_completed_at: Some(completed),
                };
                repository.save_component_record(&record).await?;
                publish_component_event(
                    &events,
                    component,
                    state,
                    None,
                    None,
                    None,
                    Some(message),
                );
            }
            Err(error) => {
                let message = error.to_string();
                let completed = chrono::Utc::now();
                let (state, active) = manager
                    .state_after_unsuccessful_operation(component, ComponentState::Failed)
                    .await?;
                let record = PersistedComponent {
                    component,
                    state,
                    managed_version: active.as_ref().map(|installed| installed.version.clone()),
                    detected_version: active.as_ref().map(|installed| installed.version.clone()),
                    managed_path: active.as_ref().map(|installed| installed.path.clone()),
                    custom_path: None,
                    error_message: Some(message.clone()),
                    last_checked_at: Some(completed),
                    verified_at: records.get(&component).and_then(|record| record.verified_at),
                    install_started_at: Some(started),
                    install_completed_at: Some(completed),
                };
                repository.save_component_record(&record).await?;
                publish_component_event(
                    &events,
                    component,
                    state,
                    None,
                    None,
                    None,
                    Some(message),
                );
            }
        }
        Ok(())
    }
    .await;
    registry.finish(component);
    result
}

fn component_config_path(component: services::components::ComponentId, config: &Config) -> &std::path::PathBuf {
    match component {
        services::components::ComponentId::Ytdlp => &config.ytdlp,
        services::components::ComponentId::Ffmpeg => &config.ffmpeg,
        services::components::ComponentId::Rqbit => &config.rqbit,
        services::components::ComponentId::SevenZip => &config.seven_zip,
    }
}

fn publish_component_event(
    events: &core::events::EventBus,
    component: services::components::ComponentId,
    state: services::components::ComponentState,
    progress_pct: Option<u8>,
    bytes_downloaded: Option<u64>,
    bytes_total: Option<u64>,
    message: Option<String>,
) {
    events.publish(core::events::Event::Component {
        component,
        state,
        progress_pct,
        bytes_downloaded,
        bytes_total,
        message,
    });
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
            archive_member: None,
            member_sha256: None,
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
