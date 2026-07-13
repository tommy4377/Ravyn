//! Component and feature management API routes.
//!
//! Provides endpoints for listing component status, saving feature selections,
//! installing, updating, verifying, rolling back, removing, and cancelling
//! component operations.

use super::*;

use crate::services::components::{
    ComponentHealth, ComponentId, ComponentManager, ComponentState, ComponentStatus, FeatureId,
    FeatureStatus, InstallComponentRequest, PersistedComponent, SaveFeatureSelections,
    SetupProfile, current_target, effective_feature_set, required_components_for_features,
};

const THROTTLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Serialize)]
pub(super) struct ComponentOverviewResponse {
    setup_profile: SetupProfile,
    features: Vec<FeatureStatus>,
    components: Vec<ComponentStatus>,
    platform: &'static str,
    manifest_provider: &'static str,
}

pub(super) async fn list_components(
    State(s): State<ApiState>,
) -> Result<Json<ComponentOverviewResponse>> {
    let records = s.repository.load_component_records().await?;
    let (profile, features_json) = match s.repository.load_feature_selections().await? {
        Some((profile, features)) => (profile, features),
        None => (SetupProfile::Minimal, Vec::new()),
    };
    let features_set = effective_feature_set(profile, &features_json)?;
    let required = required_components_for_features(&features_set);
    let manager = ComponentManager::new(
        &s.configured_config.data_dir,
        s.component_manifest.clone(),
        tokio_util::sync::CancellationToken::new(),
    );

    let mut components = Vec::with_capacity(ComponentId::ALL.len());
    for &component in ComponentId::ALL {
        let enabled = required.contains(&component);
        let state = manager
            .component_state(
                component,
                &s.configured_config,
                &records,
                s.provisioning_cancellation.is_active(component),
            )
            .await;
        let effective_path = manager
            .effective_path(component, &s.configured_config, &records)
            .await;
        let record = records.get(&component);
        let configured_path = component_config_path(component, &s.configured_config);
        let custom_path = (configured_path
            != std::path::Path::new(component.default_command()))
        .then(|| configured_path.clone());
        let managed_version = manager
            .installed_version(component)
            .await
            .or_else(|| record.and_then(|record| record.managed_version.clone()));

        components.push(ComponentStatus {
            component,
            state,
            enabled,
            managed_version,
            detected_version: record.and_then(|record| record.detected_version.clone()),
            managed_path: record.and_then(|record| record.managed_path.clone()),
            custom_path,
            effective_path,
            available_version: manager.available_version(component)?,
            rollback_available: manager.rollback_available(component).await,
            error_message: record.and_then(|record| record.error_message.clone()),
            last_checked_at: record.and_then(|record| record.last_checked_at),
            verified_at: record.and_then(|record| record.verified_at),
            install_started_at: record.and_then(|record| record.install_started_at),
            install_completed_at: record.and_then(|record| record.install_completed_at),
        });
    }

    let mut features = Vec::with_capacity(FeatureId::ALL.len());
    for &feature in FeatureId::ALL {
        let enabled = features_set.contains(&feature);
        let satisfied = manager
            .feature_satisfied(
                feature,
                enabled,
                &s.configured_config,
                &records,
                &s.provisioning_cancellation,
            )
            .await;
        features.push(FeatureStatus {
            feature,
            enabled,
            satisfied,
            required_components: feature.required_components().to_vec(),
        });
    }

    Ok(Json(ComponentOverviewResponse {
        setup_profile: profile,
        features,
        components,
        platform: current_target(),
        manifest_provider: manager.manifest_provider_name(),
    }))
}

pub(super) async fn save_feature_selections(
    State(s): State<ApiState>,
    Json(request): Json<SaveFeatureSelections>,
) -> Result<(StatusCode, Json<ComponentOverviewResponse>)> {
    let mut seen = std::collections::BTreeSet::new();
    for selection in &request.features {
        if !seen.insert(selection.feature) {
            return Err(crate::error::RavynError::Invalid(format!(
                "feature {:?} was provided more than once",
                selection.feature
            )));
        }
    }

    let mut selected = match request.setup_profile {
        SetupProfile::Custom => request
            .features
            .iter()
            .filter(|selection| selection.enabled)
            .map(|selection| selection.feature)
            .collect::<std::collections::BTreeSet<_>>(),
        profile => profile.default_features(),
    };
    selected.insert(FeatureId::StandardDownloads);
    let values = selected
        .iter()
        .map(serde_json::to_string)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    s.repository
        .save_feature_selections(request.setup_profile, &values)
        .await?;

    Ok((StatusCode::OK, list_components(State(s)).await?))
}

pub(super) async fn install_component(
    State(s): State<ApiState>,
    Path(component_id): Path<String>,
    Json(request): Json<InstallComponentRequest>,
) -> Result<StatusCode> {
    start_component_installation(s, parse_component_id(&component_id)?, request.force).await
}

pub(super) async fn update_component(
    State(s): State<ApiState>,
    Path(component_id): Path<String>,
) -> Result<StatusCode> {
    start_component_installation(s, parse_component_id(&component_id)?, true).await
}

async fn start_component_installation(
    s: ApiState,
    component: ComponentId,
    force: bool,
) -> Result<StatusCode> {
    let records = s.repository.load_component_records().await?;
    let manager = ComponentManager::new(
        &s.configured_config.data_dir,
        s.component_manifest.clone(),
        tokio_util::sync::CancellationToken::new(),
    );
    let state = manager
        .component_state(component, &s.configured_config, &records, false)
        .await;
    if state.is_operational() && !force {
        return Ok(StatusCode::NO_CONTENT);
    }
    if matches!(state, ComponentState::CustomPath | ComponentState::CustomPathInvalid) {
        return Err(crate::error::RavynError::Conflict(format!(
            "component {} uses a custom path; reset it to the default command before installing a managed version",
            component.engine_name()
        )));
    }
    let Some(artifact) = manager.manifest_artifact(component)? else {
        let message = format!(
            "no verified {} artifact is available for {}",
            component.engine_name(),
            manager.target()
        );
        let now = chrono::Utc::now();
        let existing = records.get(&component);
        s.repository
            .save_component_record(&PersistedComponent {
                component,
                state: ComponentState::Unsupported,
                managed_version: existing.and_then(|record| record.managed_version.clone()),
                detected_version: existing.and_then(|record| record.detected_version.clone()),
                managed_path: existing.and_then(|record| record.managed_path.clone()),
                custom_path: None,
                error_message: Some(message.clone()),
                last_checked_at: Some(now),
                verified_at: existing.and_then(|record| record.verified_at),
                install_started_at: existing.and_then(|record| record.install_started_at),
                install_completed_at: existing.and_then(|record| record.install_completed_at),
            })
            .await?;
        publish_component_event(
            &s.manager.events(),
            component,
            ComponentState::Unsupported,
            None,
            None,
            None,
            Some(message.clone()),
        );
        return Err(crate::error::RavynError::Unavailable(message));
    };
    let cancellation = s.provisioning_cancellation.begin(component)?;
    let started = chrono::Utc::now();
    let existing = records.get(&component);
    let record = PersistedComponent {
        component,
        state: ComponentState::Queued,
        managed_version: existing.and_then(|record| record.managed_version.clone()),
        detected_version: existing.and_then(|record| record.detected_version.clone()),
        managed_path: existing.and_then(|record| record.managed_path.clone()),
        custom_path: None,
        error_message: None,
        last_checked_at: Some(started),
        verified_at: existing.and_then(|record| record.verified_at),
        install_started_at: Some(started),
        install_completed_at: None,
    };
    if let Err(error) = s.repository.save_component_record(&record).await {
        s.provisioning_cancellation.finish(component);
        return Err(error);
    }
    publish_component_event(
        &s.manager.events(),
        component,
        ComponentState::Queued,
        None,
        None,
        None,
        Some(format!("{} {} queued", component.label(), artifact.version)),
    );

    tokio::spawn(run_component_installation(
        component,
        InstallationContext {
            config: s.configured_config.clone(),
            repository: s.repository.clone(),
            manifest_provider: s.component_manifest.clone(),
            registry: s.provisioning_cancellation.clone(),
            events: s.manager.events(),
        },
        cancellation,
        started,
    ));
    Ok(StatusCode::ACCEPTED)
}

struct InstallationContext {
    config: Arc<crate::config::Config>,
    repository: crate::storage::Repository,
    manifest_provider: Arc<dyn crate::services::components::ManifestProvider>,
    registry: crate::services::components::ProvisioningCancellation,
    events: crate::core::events::EventBus,
}

async fn run_component_installation(
    component: ComponentId,
    context: InstallationContext,
    cancellation: tokio_util::sync::CancellationToken,
    started: chrono::DateTime<chrono::Utc>,
) {
    let InstallationContext {
        config,
        repository,
        manifest_provider,
        registry,
        events,
    } = context;
    let result = async {
        let _permit = registry.acquire(&cancellation).await?;
        let mut prior_records = repository.load_component_records().await?;
        let prior_record = prior_records.remove(&component);
        let manager = ComponentManager::new(&config.data_dir, manifest_provider, cancellation);
        let progress_events = events.clone();
        let throttle = std::sync::Mutex::new(std::time::Instant::now() - THROTTLE_INTERVAL);
        let report = move |received: u64, total: u64| {
            let mut last = throttle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let done = total > 0 && received >= total;
            if !done && last.elapsed() < THROTTLE_INTERVAL {
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
        let stage_repository = repository.clone();
        let stage_events = events.clone();
        let stage_writer = tokio::spawn(async move {
            while let Some(stage) = stage_rx.recv().await {
                if let Err(error) = stage_repository
                    .update_component_lifecycle(component, stage, None)
                    .await
                {
                    tracing::warn!(%error, component = component.engine_name(), "failed to persist component stage");
                }
                publish_component_event(
                    &stage_events,
                    component,
                    stage,
                    None,
                    None,
                    None,
                    None,
                );
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

        let completed = chrono::Utc::now();
        match installation {
            Ok(installed) => {
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
                    verified_at: prior_record.as_ref().and_then(|record| record.verified_at),
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
                    verified_at: prior_record.as_ref().and_then(|record| record.verified_at),
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
        Ok::<(), crate::error::RavynError>(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, component = component.engine_name(), "component operation could not persist its final state");
    }
    registry.finish(component);
}

pub(super) async fn verify_component(
    State(s): State<ApiState>,
    Path(component_id): Path<String>,
) -> Result<Json<ComponentHealth>> {
    let component = parse_component_id(&component_id)?;
    if s.provisioning_cancellation.is_active(component) {
        return Err(crate::error::RavynError::Conflict(
            "component verification cannot run during provisioning".into(),
        ));
    }
    let records = s.repository.load_component_records().await?;
    let manager = ComponentManager::new(
        &s.configured_config.data_dir,
        s.component_manifest.clone(),
        tokio_util::sync::CancellationToken::new(),
    );
    let health = manager
        .health_check(component, &s.configured_config, &records)
        .await;
    let now = chrono::Utc::now();
    let configured_path = component_config_path(component, &s.configured_config);
    let custom = configured_path != std::path::Path::new(component.default_command());
    let reconciled_state = if health.healthy {
        if custom {
            ComponentState::CustomPath
        } else if manager
            .available_version(component)?
            .zip(manager.installed_version(component).await)
            .is_some_and(|(available, installed)| available != installed)
        {
            ComponentState::UpdateAvailable
        } else {
            ComponentState::Installed
        }
    } else if custom {
        ComponentState::CustomPathInvalid
    } else {
        ComponentState::Failed
    };
    let previous = records.get(&component);
    let record = PersistedComponent {
        component,
        state: reconciled_state,
        managed_version: manager
            .installed_version(component)
            .await
            .or_else(|| previous.and_then(|record| record.managed_version.clone())),
        detected_version: health.version.clone(),
        managed_path: manager
            .effective_path(component, &s.configured_config, &records)
            .await
            .filter(|_| !custom),
        custom_path: custom.then(|| configured_path.clone()),
        error_message: health.message.clone(),
        last_checked_at: Some(now),
        verified_at: if health.healthy {
            Some(now)
        } else {
            previous.and_then(|record| record.verified_at)
        },
        install_started_at: previous.and_then(|record| record.install_started_at),
        install_completed_at: previous.and_then(|record| record.install_completed_at),
    };
    s.repository.save_component_record(&record).await?;
    Ok(Json(health))
}

pub(super) async fn rollback_component(
    State(s): State<ApiState>,
    Path(component_id): Path<String>,
) -> Result<StatusCode> {
    let component = parse_component_id(&component_id)?;
    let token = s.provisioning_cancellation.begin(component)?;
    let manager = ComponentManager::new(
        &s.configured_config.data_dir,
        s.component_manifest.clone(),
        token,
    );
    let result = async {
        let installed = manager
            .rollback_component(component, &s.configured_config)
            .await?;
        let now = chrono::Utc::now();
        s.repository
            .save_component_record(&PersistedComponent {
                component,
                state: ComponentState::Installed,
                managed_version: installed.detected_version.clone(),
                detected_version: installed.detected_version,
                managed_path: Some(installed.path),
                custom_path: None,
                error_message: None,
                last_checked_at: Some(now),
                verified_at: Some(now),
                install_started_at: None,
                install_completed_at: Some(now),
            })
            .await?;
        publish_component_event(
            &s.manager.events(),
            component,
            ComponentState::Installed,
            None,
            None,
            None,
            Some("rolled back to the previous verified version".into()),
        );
        Ok(StatusCode::OK)
    }
    .await;
    s.provisioning_cancellation.finish(component);
    result
}

pub(super) async fn remove_component(
    State(s): State<ApiState>,
    Path(component_id): Path<String>,
) -> Result<StatusCode> {
    let component = parse_component_id(&component_id)?;
    if s.provisioning_cancellation.is_active(component) {
        return Err(crate::error::RavynError::Conflict(
            "cancel the active component operation before removal".into(),
        ));
    }
    let manager = ComponentManager::new(
        &s.configured_config.data_dir,
        s.component_manifest.clone(),
        tokio_util::sync::CancellationToken::new(),
    );
    manager.remove_component(component).await?;
    s.repository.delete_component_record(component).await?;
    publish_component_event(
        &s.manager.events(),
        component,
        ComponentState::NotInstalled,
        None,
        None,
        None,
        Some("managed component removed".into()),
    );
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn cleanup_component(
    State(s): State<ApiState>,
    Path(component_id): Path<String>,
) -> Result<Json<crate::services::engines::EngineCleanupReport>> {
    let component = parse_component_id(&component_id)?;
    if s.provisioning_cancellation.is_active(component) {
        return Err(crate::error::RavynError::Conflict(
            "cancel the active component operation before cleanup".into(),
        ));
    }
    let manager = ComponentManager::new(
        &s.configured_config.data_dir,
        s.component_manifest.clone(),
        tokio_util::sync::CancellationToken::new(),
    );
    let report = manager.cleanup_component(component).await?;
    Ok(Json(report))
}

pub(super) async fn cancel_installation(
    State(s): State<ApiState>,
    Path(component_id): Path<String>,
) -> Result<StatusCode> {
    let component = parse_component_id(&component_id)?;
    if s.provisioning_cancellation.cancel(component) {
        publish_component_event(
            &s.manager.events(),
            component,
            ComponentState::Downloading,
            None,
            None,
            None,
            Some("cancellation requested".into()),
        );
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

fn publish_component_event(
    events: &crate::core::events::EventBus,
    component: ComponentId,
    state: ComponentState,
    progress_pct: Option<u8>,
    bytes_downloaded: Option<u64>,
    bytes_total: Option<u64>,
    message: Option<String>,
) {
    events.publish(crate::core::events::Event::Component {
        component,
        state,
        progress_pct,
        bytes_downloaded,
        bytes_total,
        message,
    });
}

fn component_config_path(
    component: ComponentId,
    config: &crate::config::Config,
) -> &std::path::PathBuf {
    match component {
        ComponentId::Ytdlp => &config.ytdlp,
        ComponentId::Ffmpeg => &config.ffmpeg,
        ComponentId::Rqbit => &config.rqbit,
        ComponentId::SevenZip => &config.seven_zip,
    }
}

fn parse_component_id(value: &str) -> Result<ComponentId> {
    match value {
        "yt-dlp" | "ytdlp" => Ok(ComponentId::Ytdlp),
        "ffmpeg" => Ok(ComponentId::Ffmpeg),
        "rqbit" => Ok(ComponentId::Rqbit),
        "7zip" | "7z" | "seven_zip" => Ok(ComponentId::SevenZip),
        _ => Err(crate::error::RavynError::NotFound(format!(
            "unknown component: {value}"
        ))),
    }
}
