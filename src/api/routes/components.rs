//! Component and feature management API routes.
//!
//! Provides endpoints for listing component status, saving feature selections,
//! installing, updating, rolling back, removing, and cancelling component
//! operations.

use super::*;

use crate::services::components::{
    ComponentId, ComponentManager, ComponentState, ComponentStatus, FeatureId, FeatureStatus,
    InstallComponentRequest, PersistedComponent, SaveFeatureSelections, SetupProfile,
    current_target, effective_feature_set, required_components_for_features,
};

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
        Some((p, f)) => (p, f),
        None => (SetupProfile::Minimal, vec![]),
    };
    let features_set = effective_feature_set(profile, &features_json).unwrap_or_default();
    let required = required_components_for_features(&features_set);

    let component_manager = ComponentManager::new(
        &s.base_config.data_dir,
        std::sync::Arc::new(crate::services::components::BuiltInManifestProvider::empty()),
        s.provisioning_cancellation.current(),
    );

    let mut components = Vec::new();
    for &component in ComponentId::ALL {
        let enabled = required.contains(&component);
        let state = component_manager
            .component_state(component, &s.base_config, &records)
            .await;
        let effective_path = component_manager
            .effective_path(component, &s.base_config, &records)
            .await;
        let record = records.get(&component);

        components.push(ComponentStatus {
            component,
            state,
            enabled,
            managed_version: record.and_then(|r| r.managed_version.clone()),
            managed_path: record.and_then(|r| r.managed_path.clone()),
            custom_path: record.and_then(|r| r.custom_path.clone()),
            effective_path,
            error_message: record.and_then(|r| r.error_message.clone()),
            last_checked_at: record.and_then(|r| r.last_checked_at),
            install_started_at: record.and_then(|r| r.install_started_at),
            install_completed_at: record.and_then(|r| r.install_completed_at),
        });
    }

    let mut features = Vec::new();
    for &feature in FeatureId::ALL {
        let enabled = features_set.contains(&feature);
        let satisfied = component_manager
            .feature_satisfied(feature, enabled, &s.base_config, &records)
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
        manifest_provider: "built-in",
    }))
}

pub(super) async fn save_feature_selections(
    State(s): State<ApiState>,
    Json(request): Json<SaveFeatureSelections>,
) -> Result<(StatusCode, Json<ComponentOverviewResponse>)> {
    // Validate that all feature IDs are known.
    for selection in &request.features {
        let _ = FeatureId::ALL
            .iter()
            .find(|f| **f == selection.feature)
            .ok_or_else(|| {
                crate::error::RavynError::Invalid(format!(
                    "unknown feature: {:?}",
                    selection.feature
                ))
            })?;
    }

    let features_strs: Vec<String> = request
        .features
        .iter()
        .filter(|f| f.enabled)
        .map(|f| serde_json::to_string(&f.feature).unwrap_or_default())
        .collect();

    s.repository
        .save_feature_selections(request.setup_profile, &features_strs)
        .await?;

    // Return the updated overview.
    let response = list_components(State(s)).await?;
    Ok((StatusCode::OK, response))
}

pub(super) async fn install_component(
    State(s): State<ApiState>,
    axum::extract::Path(component_id): Path<String>,
    Json(request): Json<InstallComponentRequest>,
) -> Result<StatusCode> {
    let component = parse_component_id(&component_id)?;

    let records = s.repository.load_component_records().await?;

    // Check if the component requires installation.
    let component_manager = ComponentManager::new(
        &s.base_config.data_dir,
        std::sync::Arc::new(crate::services::components::BuiltInManifestProvider::empty()),
        s.provisioning_cancellation.current(),
    );

    let state = component_manager
        .component_state(component, &s.base_config, &records)
        .await;
    if state.is_operational() && !request.force {
        return Ok(StatusCode::NO_CONTENT);
    }

    // Check for user-provided custom path.
    let default = std::path::Path::new(component.default_command());
    let config_path = match component {
        ComponentId::Ytdlp => &s.base_config.ytdlp,
        ComponentId::Ffmpeg => &s.base_config.ffmpeg,
        ComponentId::Rqbit => &s.base_config.rqbit,
        ComponentId::SevenZip => &s.base_config.seven_zip,
    };
    if config_path != default {
        return Err(crate::error::RavynError::Conflict(format!(
            "component {} has a custom path configured; remove the custom path to use managed binaries",
            component.engine_name()
        )));
    }

    // Mark as queued.
    let record = PersistedComponent {
        component,
        state: ComponentState::Queued,
        managed_version: None,
        managed_path: None,
        custom_path: None,
        error_message: None,
        last_checked_at: None,
        install_started_at: Some(chrono::Utc::now()),
        install_completed_at: None,
    };
    s.repository.save_component_record(&record).await?;

    let events = s.manager.events();
    publish_component_event(&events, component, ComponentState::Queued, None, None, None);

    // Spawn background installation.
    let repo = s.repository.clone();
    let config = s.base_config.clone();
    let cancellation = s.provisioning_cancellation.current();
    let comp = component;

    tokio::spawn(async move {
        let manager = ComponentManager::new(
            &config.data_dir,
            std::sync::Arc::new(crate::services::components::BuiltInManifestProvider::empty()),
            cancellation,
        );

        // Update state to downloading.
        let mut record = PersistedComponent {
            component: comp,
            state: ComponentState::Downloading,
            managed_version: None,
            managed_path: None,
            custom_path: None,
            error_message: None,
            last_checked_at: None,
            install_started_at: Some(chrono::Utc::now()),
            install_completed_at: None,
        };
        let _ = repo.save_component_record(&record).await;
        publish_component_event(
            &events,
            comp,
            ComponentState::Downloading,
            Some(0),
            None,
            None,
        );

        // Coalesce download progress to roughly 4 events per second so slow
        // consumers and the SSE replay buffer are never flooded.
        let throttle = std::sync::Mutex::new(std::time::Instant::now() - THROTTLE_INTERVAL);
        let progress_events = events.clone();
        let report = move |received: u64, total: u64| {
            let mut last = throttle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let done = received >= total;
            if !done && last.elapsed() < THROTTLE_INTERVAL {
                return;
            }
            *last = std::time::Instant::now();
            let pct = if total > 0 {
                u8::try_from(received.saturating_mul(100) / total).unwrap_or(100)
            } else {
                0
            };
            publish_component_event(
                &progress_events,
                comp,
                ComponentState::Downloading,
                Some(pct),
                Some(received),
                Some(total),
            );
        };

        match manager
            .install_component_with_progress(comp, &config, Some(&report))
            .await
        {
            Ok(path) => {
                record.state = ComponentState::Installed;
                record.managed_path = Some(path);
                record.install_completed_at = Some(chrono::Utc::now());
                let _ = repo.save_component_record(&record).await;
                publish_component_event(
                    &events,
                    comp,
                    ComponentState::Installed,
                    Some(100),
                    None,
                    None,
                );
                tracing::info!(
                    component = comp.engine_name(),
                    "component installed via API"
                );
            }
            Err(error) => {
                record.state = ComponentState::Failed;
                record.error_message = Some(error.to_string());
                record.install_completed_at = Some(chrono::Utc::now());
                let _ = repo.save_component_record(&record).await;
                events.publish(crate::core::events::Event::Component {
                    component: comp,
                    state: ComponentState::Failed,
                    progress_pct: None,
                    bytes_downloaded: None,
                    bytes_total: None,
                    message: Some(error.to_string()),
                });
                tracing::warn!(
                    %error,
                    component = comp.engine_name(),
                    "component installation failed via API"
                );
            }
        }
    });

    Ok(StatusCode::ACCEPTED)
}

pub(super) async fn rollback_component(
    State(s): State<ApiState>,
    axum::extract::Path(component_id): Path<String>,
) -> Result<StatusCode> {
    let component = parse_component_id(&component_id)?;

    let component_manager = ComponentManager::new(
        &s.base_config.data_dir,
        std::sync::Arc::new(crate::services::components::BuiltInManifestProvider::empty()),
        s.provisioning_cancellation.current(),
    );

    let path = component_manager.rollback_component(component).await?;

    let record = PersistedComponent {
        component,
        state: ComponentState::Installed,
        managed_version: None,
        managed_path: Some(path),
        custom_path: None,
        error_message: None,
        last_checked_at: Some(chrono::Utc::now()),
        install_started_at: None,
        install_completed_at: Some(chrono::Utc::now()),
    };
    s.repository.save_component_record(&record).await?;

    Ok(StatusCode::OK)
}

pub(super) async fn remove_component(
    State(s): State<ApiState>,
    axum::extract::Path(component_id): Path<String>,
) -> Result<StatusCode> {
    let component = parse_component_id(&component_id)?;

    let component_manager = ComponentManager::new(
        &s.base_config.data_dir,
        std::sync::Arc::new(crate::services::components::BuiltInManifestProvider::empty()),
        s.provisioning_cancellation.current(),
    );

    component_manager.remove_component(component).await?;
    s.repository.delete_component_record(component).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn cancel_installation(
    State(s): State<ApiState>,
    axum::extract::Path(component_id): Path<String>,
) -> Result<StatusCode> {
    let component = parse_component_id(&component_id)?;

    let records = s.repository.load_component_records().await?;
    if let Some(record) = records.get(&component) {
        if record.state.is_busy() {
            // Cancel the provisioning token (cancels ALL active installations).
            s.provisioning_cancellation.cancel_and_reset();
            // Reset state.
            let updated = PersistedComponent {
                component,
                state: ComponentState::NotInstalled,
                managed_version: None,
                managed_path: None,
                custom_path: None,
                error_message: Some("installation cancelled by user".into()),
                last_checked_at: Some(chrono::Utc::now()),
                install_started_at: None,
                install_completed_at: None,
            };
            s.repository.save_component_record(&updated).await?;
            s.manager
                .events()
                .publish(crate::core::events::Event::Component {
                    component,
                    state: ComponentState::NotInstalled,
                    progress_pct: None,
                    bytes_downloaded: None,
                    bytes_total: None,
                    message: Some("installation cancelled by user".into()),
                });
            return Ok(StatusCode::OK);
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Minimum interval between coalesced component progress events.
const THROTTLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// Publish a component provisioning event on the shared event bus.
fn publish_component_event(
    events: &crate::core::events::EventBus,
    component: ComponentId,
    state: ComponentState,
    progress_pct: Option<u8>,
    bytes_downloaded: Option<u64>,
    bytes_total: Option<u64>,
) {
    events.publish(crate::core::events::Event::Component {
        component,
        state,
        progress_pct,
        bytes_downloaded,
        bytes_total,
        message: None,
    });
}

fn parse_component_id(s: &str) -> Result<ComponentId> {
    match s {
        "yt-dlp" | "ytdlp" => Ok(ComponentId::Ytdlp),
        "ffmpeg" => Ok(ComponentId::Ffmpeg),
        "rqbit" => Ok(ComponentId::Rqbit),
        "7zip" | "7z" => Ok(ComponentId::SevenZip),
        _ => Err(crate::error::RavynError::NotFound(format!(
            "unknown component: {s}"
        ))),
    }
}
