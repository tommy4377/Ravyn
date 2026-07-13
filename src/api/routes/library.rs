//! Persistent library query handlers.

use chrono::{DateTime, Utc};

use super::*;
use crate::{
    services::library::{LibraryCategory, TemplatePreview, TemplatePreviewRequest},
    storage::{LibraryEntry, LibraryEntryState, LibraryListFilter},
};

#[derive(Debug, Default, Deserialize)]
pub(super) struct LibraryQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
    category: Option<LibraryCategory>,
    state: Option<LibraryEntryState>,
    tag: Option<String>,
    mime: Option<String>,
    downloaded_from: Option<DateTime<Utc>>,
    downloaded_to: Option<DateTime<Utc>>,
}

pub(super) async fn list_library(
    State(s): State<ApiState>,
    Query(query): Query<LibraryQuery>,
) -> Result<Json<Page<LibraryEntry>>> {
    let window = PageWindow::from_query(&PageQuery {
        cursor: query.cursor,
        limit: query.limit,
        search: query.q.clone(),
    })?;
    let items = s
        .repository
        .list_library_entries(
            &LibraryListFilter {
                search: query.q,
                category: query.category,
                state: query.state,
                tag: query.tag,
                mime_type: query.mime,
                downloaded_from: query.downloaded_from,
                downloaded_to: query.downloaded_to,
            },
            window.offset,
            window.database_limit(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

pub(super) async fn get_library_entry(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LibraryEntry>> {
    Ok(Json(s.repository.get_library_entry(id).await?))
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct DuplicateCandidateQuery {
    sha256: Option<String>,
    size_bytes: Option<u64>,
    filename: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(super) struct DuplicateCandidate {
    entry: LibraryEntry,
    matches: Vec<&'static str>,
}

pub(super) async fn find_library_duplicates(
    State(s): State<ApiState>,
    Query(query): Query<DuplicateCandidateQuery>,
) -> Result<Json<Vec<DuplicateCandidate>>> {
    let entries = s
        .repository
        .find_library_duplicate_candidates(
            query.sha256.as_deref(),
            query.size_bytes,
            query.filename.as_deref(),
            query.limit.unwrap_or(25),
        )
        .await?;
    let candidates = entries
        .into_iter()
        .map(|entry| {
            let mut matches = Vec::new();
            if query.sha256.as_deref().is_some_and(|value| {
                entry
                    .sha256
                    .as_deref()
                    .is_some_and(|hash| hash.eq_ignore_ascii_case(value))
            }) {
                matches.push("sha256");
            }
            if query
                .size_bytes
                .is_some_and(|size| entry.size_bytes == Some(size))
            {
                matches.push("size_bytes");
            }
            if query
                .filename
                .as_deref()
                .is_some_and(|filename| entry.filename.eq_ignore_ascii_case(filename))
            {
                matches.push("filename");
            }
            DuplicateCandidate { entry, matches }
        })
        .collect();
    Ok(Json(candidates))
}

pub(super) async fn preview_template(
    Json(request): Json<TemplatePreviewRequest>,
) -> Result<Json<TemplatePreview>> {
    Ok(Json(crate::services::library::render_template(
        &request.template,
        &request.variables,
    )?))
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DeleteLibraryMode {
    #[default]
    Trash,
    Purge,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct DeleteLibraryQuery {
    mode: DeleteLibraryMode,
}

#[derive(Debug, Serialize)]
pub(super) struct DeleteLibraryResult {
    purged: bool,
    entry: Option<LibraryEntry>,
}

pub(super) async fn delete_library_entry(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Query(query): Query<DeleteLibraryQuery>,
) -> Result<Json<DeleteLibraryResult>> {
    let resource_id = id.to_string();
    let result = match query.mode {
        DeleteLibraryMode::Trash => {
            let config = s.manager.config();
            crate::services::library::move_to_trash(&config, &s.repository, id)
                .await
                .map(|entry| DeleteLibraryResult {
                    purged: false,
                    entry: Some(entry),
                })
        }
        DeleteLibraryMode::Purge => {
            let config = s.manager.config();
            crate::services::library::purge_entry(&config, &s.repository, id)
                .await
                .map(|()| DeleteLibraryResult {
                    purged: true,
                    entry: None,
                })
        }
    };
    let action = match query.mode {
        DeleteLibraryMode::Trash => "library.trash",
        DeleteLibraryMode::Purge => "library.purge",
    };
    Ok(Json(
        audited(
            &s.repository,
            action,
            "library_entry",
            Some(&resource_id),
            result,
        )
        .await?,
    ))
}

pub(super) async fn restore_library_entry(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LibraryEntry>> {
    let config = s.manager.config();
    let result = crate::services::library::restore_entry(&config, &s.repository, id).await;
    let resource_id = id.to_string();
    Ok(Json(
        audited(
            &s.repository,
            "library.restore",
            "library_entry",
            Some(&resource_id),
            result,
        )
        .await?,
    ))
}

pub(super) async fn start_library_import(
    State(s): State<ApiState>,
    Json(request): Json<crate::services::library::LibraryImportRequest>,
) -> Result<(
    StatusCode,
    Json<crate::services::library::LibraryImportStatus>,
)> {
    let config = s.manager.config();
    let status = s.library_import_status.clone();
    let snapshot = audited(
        &s.repository,
        "library.import.start",
        "library_import",
        None,
        crate::services::library::reserve_import(&config, &request, &status).await,
    )
    .await?;
    let repository = s.repository.clone();
    let task_status = status.clone();
    tokio::spawn(async move {
        let result = crate::services::library::import_directory(
            config,
            repository.clone(),
            request,
            task_status.clone(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
        let snapshot = task_status.read().await.clone();
        let outcome = if result.is_ok() { "success" } else { "failure" };
        let resource_id = snapshot.run_id.map(|id| id.to_string());
        if let Err(error) = repository
            .append_audit_with_metadata(
                "library.import.finish",
                "library_import",
                resource_id.as_deref(),
                outcome,
                serde_json::json!({
                    "scanned": snapshot.scanned,
                    "imported": snapshot.imported,
                    "duplicates": snapshot.duplicates,
                    "skipped": snapshot.skipped,
                }),
            )
            .await
        {
            tracing::warn!(%error, "failed to persist library import audit record");
        }
        if let Err(error) = result {
            tracing::warn!(%error, "library import failed");
        }
    });
    Ok((StatusCode::ACCEPTED, Json(snapshot)))
}

pub(super) async fn library_import_status(
    State(s): State<ApiState>,
) -> Json<crate::services::library::LibraryImportStatus> {
    Json(s.library_import_status.read().await.clone())
}

pub(super) async fn verify_library(
    State(s): State<ApiState>,
) -> Result<Json<crate::services::library::VerifyLibraryReport>> {
    let result = crate::services::library::verify_entries(&s.repository).await;
    Ok(Json(
        audited(&s.repository, "library.verify", "library", None, result).await?,
    ))
}

pub(super) async fn relocate_library(
    State(s): State<ApiState>,
    Json(request): Json<crate::services::library::RelocationRequest>,
) -> Result<Json<crate::services::library::RelocationReport>> {
    let config = s.manager.config();
    let result = crate::services::library::repair_relocations(
        &config,
        &s.repository,
        request,
        &tokio_util::sync::CancellationToken::new(),
    )
    .await;
    Ok(Json(
        audited(&s.repository, "library.relocate", "library", None, result).await?,
    ))
}

pub(super) async fn list_presets(
    State(s): State<ApiState>,
) -> Result<Json<Vec<crate::storage::DownloadPreset>>> {
    Ok(Json(s.repository.list_download_presets().await?))
}

pub(super) async fn create_preset(
    State(s): State<ApiState>,
    Json(input): Json<crate::storage::PutDownloadPreset>,
) -> Result<(StatusCode, Json<crate::storage::DownloadPreset>)> {
    let result = s.repository.create_download_preset(input).await;
    Ok((
        StatusCode::CREATED,
        Json(audited(&s.repository, "preset.create", "preset", None, result).await?),
    ))
}

pub(super) async fn get_preset(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::storage::DownloadPreset>> {
    Ok(Json(s.repository.get_download_preset(id).await?))
}

pub(super) async fn update_preset(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::storage::PutDownloadPreset>,
) -> Result<Json<crate::storage::DownloadPreset>> {
    let result = s.repository.update_download_preset(id, input).await;
    let resource_id = id.to_string();
    Ok(Json(
        audited(
            &s.repository,
            "preset.update",
            "preset",
            Some(&resource_id),
            result,
        )
        .await?,
    ))
}

pub(super) async fn delete_preset(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let result = s.repository.delete_download_preset(id).await;
    let resource_id = id.to_string();
    audited(
        &s.repository,
        "preset.delete",
        "preset",
        Some(&resource_id),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_basket(
    State(s): State<ApiState>,
) -> Result<Json<Vec<crate::storage::BasketItem>>> {
    let mut items = s.repository.list_basket_items().await?;
    for item in &mut items {
        item.request = item.request.clone().redacted();
    }
    Ok(Json(items))
}

pub(super) async fn add_basket_item(
    State(s): State<ApiState>,
    Json(input): Json<crate::storage::PutBasketItem>,
) -> Result<(StatusCode, Json<crate::storage::BasketItem>)> {
    let result = s.repository.add_basket_item(input).await;
    let mut item = audited(&s.repository, "basket.add", "basket_item", None, result).await?;
    item.request = item.request.redacted();
    Ok((StatusCode::CREATED, Json(item)))
}

pub(super) async fn update_basket_item(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::storage::PutBasketItem>,
) -> Result<Json<crate::storage::BasketItem>> {
    let result = s.repository.update_basket_item(id, input).await;
    let resource_id = id.to_string();
    let mut item = audited(
        &s.repository,
        "basket.update",
        "basket_item",
        Some(&resource_id),
        result,
    )
    .await?;
    item.request = item.request.redacted();
    Ok(Json(item))
}

pub(super) async fn delete_basket_item(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let result = s.repository.delete_basket_item(id).await;
    let resource_id = id.to_string();
    audited(
        &s.repository,
        "basket.delete",
        "basket_item",
        Some(&resource_id),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub(super) struct ReorderBasketRequest {
    ids: Vec<Uuid>,
}

pub(super) async fn reorder_basket(
    State(s): State<ApiState>,
    Json(request): Json<ReorderBasketRequest>,
) -> Result<Json<Vec<crate::storage::BasketItem>>> {
    let result = s.repository.reorder_basket(&request.ids).await;
    let mut items = audited(&s.repository, "basket.reorder", "basket", None, result).await?;
    for item in &mut items {
        item.request = item.request.clone().redacted();
    }
    Ok(Json(items))
}

#[derive(Debug, Serialize)]
pub(super) struct BasketStartItemResult {
    basket_item_id: Uuid,
    job: Option<Job>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct BasketStartResult {
    started: usize,
    failed: usize,
    items: Vec<BasketStartItemResult>,
}

pub(super) async fn start_basket(State(s): State<ApiState>) -> Result<Json<BasketStartResult>> {
    let items = s.repository.list_basket_items().await?;
    let mut result = BasketStartResult {
        started: 0,
        failed: 0,
        items: Vec::with_capacity(items.len()),
    };
    for item in items {
        let mut request = item.request;
        request.preset_id = item.preset_id.or(request.preset_id);
        match s.manager.create(request).await {
            Ok(job) => {
                s.repository.delete_basket_item(item.id).await?;
                result.started += 1;
                result.items.push(BasketStartItemResult {
                    basket_item_id: item.id,
                    job: Some(job.redacted()),
                    error: None,
                });
            }
            Err(error) => {
                result.failed += 1;
                result.items.push(BasketStartItemResult {
                    basket_item_id: item.id,
                    job: None,
                    error: Some(error.public_message()),
                });
            }
        }
    }
    let outcome = if result.failed == 0 {
        "success"
    } else {
        "failure"
    };
    if let Err(error) = s
        .repository
        .append_audit_with_metadata(
            "basket.start",
            "basket",
            None,
            outcome,
            serde_json::json!({
                "started": result.started,
                "failed": result.failed,
            }),
        )
        .await
    {
        tracing::warn!(%error, "failed to persist basket start audit record");
    }
    Ok(Json(result))
}

pub(super) async fn clear_basket(State(s): State<ApiState>) -> Result<Json<Value>> {
    let result = s.repository.clear_basket().await;
    let removed = audited(&s.repository, "basket.clear", "basket", None, result).await?;
    Ok(Json(serde_json::json!({"removed": removed})))
}

pub(super) async fn list_profiles(
    State(s): State<ApiState>,
) -> Result<Json<Vec<crate::storage::UserProfile>>> {
    Ok(Json(s.repository.list_user_profiles().await?))
}

pub(super) async fn create_profile(
    State(s): State<ApiState>,
    Json(input): Json<crate::storage::PutUserProfile>,
) -> Result<(StatusCode, Json<crate::storage::UserProfile>)> {
    let result = s.repository.create_user_profile(input).await;
    Ok((
        StatusCode::CREATED,
        Json(audited(&s.repository, "profile.create", "profile", None, result).await?),
    ))
}

pub(super) async fn get_profile(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::storage::UserProfile>> {
    Ok(Json(s.repository.get_user_profile(id).await?))
}

pub(super) async fn update_profile(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::storage::PutUserProfile>,
) -> Result<Json<crate::storage::UserProfile>> {
    let result = s.repository.update_user_profile(id, input).await;
    let resource_id = id.to_string();
    Ok(Json(
        audited(
            &s.repository,
            "profile.update",
            "profile",
            Some(&resource_id),
            result,
        )
        .await?,
    ))
}

pub(super) async fn delete_profile(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let result = s.repository.delete_user_profile(id).await;
    let resource_id = id.to_string();
    audited(
        &s.repository,
        "profile.delete",
        "profile",
        Some(&resource_id),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub(super) struct ActivateProfileResponse {
    profile: crate::storage::UserProfile,
    restart_required: bool,
}

pub(super) async fn activate_profile(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ActivateProfileResponse>> {
    let pending_profile = s.repository.get_user_profile(id).await?;
    // Profiles are deterministic overlays on the startup configuration, not on the
    // previously active profile. This prevents settings from accumulating across switches.
    let mut settings = crate::config::PersistentSettings::from_config(&s.base_config);
    settings.merge(pending_profile.settings_patch.clone());
    let mut candidate = (*s.manager.config()).clone();
    settings.apply_to(&mut candidate)?;
    let profile = s
        .repository
        .activate_user_profile_with_settings(id, &settings)
        .await?;
    s.manager.apply_live_settings(&settings)?;
    s.protection
        .reconfigure(
            settings.api_max_concurrent_requests,
            settings.api_rate_limit_per_minute,
            settings.api_rate_limit_burst,
            Duration::from_secs(settings.api_request_timeout_secs),
        )
        .await;
    let response = ActivateProfileResponse {
        restart_required: settings_patch_requires_restart(&profile.settings_patch),
        profile,
    };
    let resource_id = id.to_string();
    audited(
        &s.repository,
        "profile.activate",
        "profile",
        Some(&resource_id),
        Ok(()),
    )
    .await?;
    Ok(Json(response))
}

pub(super) async fn preview_trust(
    Json(request): Json<crate::services::trust::TrustPreviewRequest>,
) -> Result<Json<crate::services::trust::TrustReport>> {
    Ok(Json(crate::services::trust::evaluate(&request)?))
}

pub(super) async fn job_trust(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::services::trust::TrustReport>> {
    let job = s.repository.get_job(id).await?;
    Ok(Json(
        crate::services::trust::for_job(&s.repository, &job).await?,
    ))
}

pub(super) async fn get_cleanup_policies(
    State(s): State<ApiState>,
) -> Result<Json<crate::services::library::CleanupPolicies>> {
    Ok(Json(s.repository.load_cleanup_policies().await?))
}

pub(super) async fn put_cleanup_policies(
    State(s): State<ApiState>,
    Json(policies): Json<crate::services::library::CleanupPolicies>,
) -> Result<Json<crate::services::library::CleanupPolicies>> {
    let result = s.repository.save_cleanup_policies(&policies).await;
    audited(
        &s.repository,
        "library.cleanup_policies.update",
        "library_settings",
        None,
        result,
    )
    .await?;
    Ok(Json(policies))
}

pub(super) async fn run_library_cleanup(
    State(s): State<ApiState>,
) -> Result<Json<crate::services::library::CleanupReport>> {
    let policies = s.repository.load_cleanup_policies().await?;
    let config = s.manager.config();
    let result = crate::services::library::run_cleanup(&config, &s.repository, &policies).await;
    Ok(Json(
        audited(
            &s.repository,
            "library.cleanup.run",
            "library",
            None,
            result,
        )
        .await?,
    ))
}

pub(super) async fn personal_statistics(
    State(s): State<ApiState>,
) -> Result<Json<crate::services::library::PersonalStatistics>> {
    Ok(Json(s.repository.personal_statistics().await?))
}
