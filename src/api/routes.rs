use super::pagination::{Page, PageQuery, PageWindow};
use crate::{
    adapters::{
        media::{DependencyStatus, MediaProbe, MediaProbeRequest},
        torrent::{
            TorrentDependencyStatus, TorrentDetails, TorrentEngineList, TorrentGlobalStats,
            TorrentPeerStats, TorrentProbe, TorrentProbeRequest, TorrentSnapshot,
        },
    },
    config::{PersistentSettings, PersistentSettingsPatch},
    core::{
        manager::JobManager,
        models::{
            CreateJob, DownloadOptions, DuplicatePolicy, Job, JobKind, JobOutput, JobStatus,
            UpdateJob,
        },
    },
    error::Result,
    services::{
        browser::{BrowserTokenRecord, CreateBrowserToken, IssuedBrowserToken},
        imports::{ImportDefaults, ImportResult, ImportTextRequest},
        schedules::ScheduleInput,
        sniffer::{ResourceKind, SniffRequest, SniffResult},
    },
    storage::{
        AuditRecord, JobActionRecord, JobListFilter, JobLogRecord, MediaArchiveRecord,
        MediaItemOutputRecord, MediaItemRecord, PageRecord, PageResourceRecord, Repository,
        RuleInput, Schedule, ScheduleExecutionRecord, SecretReference, TagRecord, TorrentRecord,
        host_profiles::HostProfile,
    },
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    pub repository: Repository,
    pub manager: Arc<JobManager>,
    pub base_config: Arc<crate::config::Config>,
}

async fn audited<T>(
    repository: &Repository,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    result: Result<T>,
) -> Result<T> {
    let outcome = if result.is_ok() { "success" } else { "failure" };
    if let Err(error) = repository
        .append_audit(action, resource_type, resource_id, outcome)
        .await
    {
        tracing::warn!(%error, action, resource_type, "failed to persist audit record");
    }
    result
}
async fn audited_import(
    repository: &Repository,
    action: &str,
    result: Result<ImportResult>,
) -> Result<ImportResult> {
    let (outcome, metadata) = match result.as_ref() {
        Ok(summary) => (
            if summary.rejected == 0 && !summary.truncated {
                "success"
            } else {
                "failure"
            },
            serde_json::json!({
                "accepted": summary.accepted,
                "rejected": summary.rejected,
                "duplicates": summary.duplicate_lines,
                "truncated": summary.truncated,
            }),
        ),
        Err(_) => ("failure", serde_json::json!({})),
    };
    if let Err(error) = repository
        .append_audit_with_metadata(action, "job_import", None, outcome, metadata)
        .await
    {
        tracing::warn!(%error, action, "failed to persist import audit record");
    }
    result
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/live", get(health))
        .route("/health/ready", get(readiness))
        .route("/metrics", get(metrics))
        .route("/openapi.json", get(openapi))
        .route("/v1/jobs", get(list_jobs).post(create_job))
        .route("/v1/jobs/batch", post(create_batch))
        .route("/v1/jobs/actions", post(apply_job_action))
        .route("/v1/jobs/import-text", post(import_text))
        .route(
            "/v1/jobs/{id}",
            get(get_job).patch(update_job).delete(delete_job),
        )
        .route("/v1/jobs/{id}/outputs", get(list_job_outputs))
        .route("/v1/jobs/{id}/media-items", get(list_media_items))
        .route(
            "/v1/jobs/{id}/media-items/{item_id}/outputs",
            get(list_media_item_outputs),
        )
        .route("/v1/jobs/{id}/media-summary", get(media_item_summary))
        .route(
            "/v1/jobs/{id}/media-items/{item_id}/retry",
            post(retry_media_item),
        )
        .route(
            "/v1/jobs/{id}/media-items/retry-failed",
            post(retry_failed_media_items),
        )
        .route("/v1/jobs/{id}/segments", get(list_job_segments))
        .route("/v1/jobs/{id}/actions", get(list_job_actions))
        .route("/v1/jobs/{id}/logs", get(list_job_logs))
        .route("/v1/jobs/{id}/pause", post(pause))
        .route("/v1/jobs/{id}/resume", post(resume))
        .route("/v1/jobs/{id}/cancel", post(cancel))
        .route("/v1/jobs/{id}/retry", post(retry))
        .route("/v1/media/probe", post(probe_media))
        .route(
            "/v1/media/archive",
            get(list_media_archive).delete(remove_media_archive),
        )
        .route("/v1/system/dependencies", get(dependencies))
        .route("/v1/system/capabilities", get(system_capabilities))
        .route("/v1/settings", get(get_settings).patch(patch_settings))
        .route("/v1/settings/reset", post(reset_settings))
        .route("/v1/system/database", get(database_status))
        .route("/v1/system/database/backup", post(backup_database))
        .route("/v1/system/database/backups", get(list_backups))
        .route(
            "/v1/system/database/backups/{name}/verify",
            post(verify_backup),
        )
        .route(
            "/v1/system/database/backups/{name}/restore",
            post(schedule_database_restore),
        )
        .route(
            "/v1/system/database/restore",
            get(database_restore_status).delete(cancel_database_restore),
        )
        .route("/v1/system/maintenance", post(run_maintenance))
        .route("/v1/audit", get(list_audit))
        .route("/v1/secrets", get(list_secrets).post(put_secret))
        .route("/v1/secrets/{id}", axum::routing::delete(delete_secret))
        .route("/v1/system/hosts", get(list_host_profiles))
        .route("/v1/system/hosts/reset", post(reset_host_profiles))
        .route("/v1/torrents/probe", post(probe_torrent))
        .route("/v1/torrents", get(managed_torrents))
        .route("/v1/torrents/engine", get(list_engine_torrents))
        .route("/v1/torrents/engine/stats", get(torrent_engine_stats))
        .route("/v1/torrents/dht/stats", get(torrent_dht_stats))
        .route("/v1/torrents/dht/table", get(torrent_dht_table))
        .route("/v1/torrents/{id}", get(torrent_details))
        .route("/v1/torrents/{id}/stats", get(torrent_stats))
        .route(
            "/v1/torrents/{id}/peers",
            get(torrent_peers).post(add_torrent_peers),
        )
        .route("/v1/torrents/{id}/files", post(update_torrent_files))
        .route("/v1/torrents/{id}/seeding", get(torrent_seeding_state))
        .route("/v1/torrents/{id}/remove", post(remove_torrent))
        .route("/v1/rules", get(list_rules).post(create_rule))
        .route(
            "/v1/rules/{id}",
            get(get_rule).put(update_rule).delete(delete_rule),
        )
        .route("/v1/tags", get(list_tags))
        .route("/v1/tags/{id}", axum::routing::delete(delete_tag))
        .route(
            "/v1/jobs/{id}/tags",
            get(list_job_tags).put(replace_job_tags),
        )
        .route("/v1/pages", get(list_pages))
        .route("/v1/pages/resources", post(list_page_resources))
        .route("/v1/pages/history/clear", post(clear_page_history))
        .route("/v1/schedules", get(list_schedules).post(create_schedule))
        .route(
            "/v1/schedules/{id}",
            get(get_schedule)
                .put(update_schedule)
                .delete(delete_schedule),
        )
        .route(
            "/v1/schedules/{id}/executions",
            get(list_schedule_executions),
        )
        .route("/v1/schedules/{id}/run-now", post(run_schedule_now))
        .route("/v1/schedules/{id}/enable", post(enable_schedule))
        .route("/v1/schedules/{id}/disable", post(disable_schedule))
        .route("/v1/schedule-executions/{id}", get(get_schedule_execution))
        .route(
            "/v1/schedule-executions/{id}/cancel",
            post(cancel_schedule_execution),
        )
        .route(
            "/v1/browser/tokens",
            get(list_browser_tokens).post(create_browser_token),
        )
        .route(
            "/v1/browser/tokens/{id}",
            axum::routing::delete(revoke_browser_token),
        )
        .route("/v1/browser/sniff", post(sniff_page))
        .route("/v1/browser/import", post(import_browser_resources))
        .route("/v1/events", get(events))
        .with_state(state)
}
async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn openapi() -> Json<Value> {
    Json(super::openapi::document())
}

#[derive(Serialize)]
struct Readiness {
    ready: bool,
    database_writable: bool,
    download_root_writable: bool,
    progress_writer_running: bool,
    accepting_tasks: bool,
}

async fn readiness(State(s): State<ApiState>) -> impl IntoResponse {
    let database_writable = s.repository.health_check().await.is_ok();
    let config = s.manager.config();
    let probe = config
        .effective_download_dir()
        .join(format!(".ravyn-readiness-{}", Uuid::new_v4()));
    let download_root_writable = match tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .await
    {
        Ok(file) => {
            drop(file);
            tokio::fs::remove_file(&probe).await.is_ok()
        }
        Err(_) => false,
    };
    let progress_writer_running = s.manager.progress_writer_is_running().await;
    let accepting_tasks = s.manager.is_accepting_tasks();
    let ready =
        database_writable && download_root_writable && progress_writer_running && accepting_tasks;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(Readiness {
            ready,
            database_writable,
            download_root_writable,
            progress_writer_running,
            accepting_tasks,
        }),
    )
}

async fn metrics(State(s): State<ApiState>) -> Result<impl IntoResponse> {
    let counts = s.repository.job_status_counts().await?;
    let active = s.manager.active_job_count().await;
    let mut body =
        String::from("# HELP ravyn_jobs Number of jobs by state.\n# TYPE ravyn_jobs gauge\n");
    for (status, count) in counts {
        body.push_str(&format!("ravyn_jobs{{status=\"{status}\"}} {count}\n"));
    }
    body.push_str("# HELP ravyn_active_jobs Currently executing jobs.\n");
    body.push_str("# TYPE ravyn_active_jobs gauge\n");
    body.push_str(&format!("ravyn_active_jobs {active}\n"));
    let (queue_depth, bytes_transferred, output_count, failure_count) =
        s.repository.operational_metrics().await?;
    body.push_str(
        "# HELP ravyn_queue_depth Jobs waiting in the queue.\n# TYPE ravyn_queue_depth gauge\n",
    );
    body.push_str(&format!("ravyn_queue_depth {queue_depth}\n"));
    body.push_str("# HELP ravyn_bytes_transferred Persisted downloaded bytes across jobs.\n# TYPE ravyn_bytes_transferred counter\n");
    body.push_str(&format!("ravyn_bytes_transferred {bytes_transferred}\n"));
    body.push_str(
        "# HELP ravyn_outputs Ready registered output artifacts.\n# TYPE ravyn_outputs gauge\n",
    );
    body.push_str(&format!("ravyn_outputs {output_count}\n"));
    body.push_str("# HELP ravyn_failures Failed jobs.\n# TYPE ravyn_failures gauge\n");
    body.push_str(&format!("ravyn_failures {failure_count}\n"));
    body.push_str(&s.manager.metrics().encode_openmetrics());
    let event_stats = s.manager.events().stats();
    body.push_str(
        "# HELP ravyn_sse_receivers Active SSE receivers.\n# TYPE ravyn_sse_receivers gauge\n",
    );
    body.push_str(&format!(
        "ravyn_sse_receivers {}\n",
        event_stats.receiver_count
    ));
    body.push_str("# HELP ravyn_sse_replay_buffer_events Events retained for SSE replay.\n# TYPE ravyn_sse_replay_buffer_events gauge\n");
    body.push_str(&format!(
        "ravyn_sse_replay_buffer_events {}\n",
        event_stats.replay_buffer_events
    ));
    body.push_str("# HELP ravyn_sse_sequence_span Current retained SSE sequence span.\n# TYPE ravyn_sse_sequence_span gauge\n");
    body.push_str(&format!(
        "ravyn_sse_sequence_span {}\n",
        event_stats.sequence_span
    ));
    body.push_str("# HELP ravyn_sse_replayed_events_total Events supplied from the replay buffer.\n# TYPE ravyn_sse_replayed_events_total counter\n");
    body.push_str(&format!(
        "ravyn_sse_replayed_events_total {}\n",
        event_stats.replayed_events_total
    ));
    body.push_str("# HELP ravyn_sse_resync_required_total Replay requests too old for the retained buffer.\n# TYPE ravyn_sse_resync_required_total counter\n");
    body.push_str(&format!(
        "ravyn_sse_resync_required_total {}\n",
        event_stats.resync_required_total
    ));
    body.push_str("# EOF\n");
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        body,
    ))
}

#[derive(Serialize)]
struct DatabaseStatus {
    integrity: String,
}

async fn database_status(State(s): State<ApiState>) -> Result<Json<DatabaseStatus>> {
    Ok(Json(DatabaseStatus {
        integrity: s.repository.integrity_check().await?,
    }))
}

async fn backup_database(State(s): State<ApiState>) -> Result<(StatusCode, Json<Value>)> {
    let result = s.manager.backup_database().await;
    let path = audited(
        &s.repository,
        "backup.create",
        "database_backup",
        None,
        result,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "path": path })),
    ))
}
async fn list_backups(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<Value>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .manager
        .list_backups()
        .await?
        .into_iter()
        .skip(window.offset_usize()?)
        .take(window.database_limit())
        .collect();
    Ok(Json(Page::from_extra_item(items, window)))
}
async fn verify_backup(State(s): State<ApiState>, Path(name): Path<String>) -> Result<Json<Value>> {
    let result = s.manager.verify_backup(&name).await;
    let integrity = audited(
        &s.repository,
        "backup.verify",
        "database_backup",
        Some(&name),
        result,
    )
    .await?;
    Ok(Json(serde_json::json!({"name":name,"integrity":integrity})))
}

async fn schedule_database_restore(
    State(s): State<ApiState>,
    Path(name): Path<String>,
) -> Result<(StatusCode, Json<crate::storage::recovery::RestoreStatus>)> {
    let result = s.manager.schedule_database_restore(&name).await;
    let result = audited(
        &s.repository,
        "backup.restore.schedule",
        "database_backup",
        Some(&name),
        result,
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}

async fn database_restore_status(
    State(s): State<ApiState>,
) -> Result<Json<crate::storage::recovery::RestoreStatus>> {
    Ok(Json(s.manager.database_restore_status().await?))
}

async fn cancel_database_restore(
    State(s): State<ApiState>,
) -> Result<Json<crate::storage::recovery::RestoreStatus>> {
    let result = s.manager.cancel_database_restore().await;
    let result = audited(
        &s.repository,
        "backup.restore.cancel",
        "database_restore",
        Some("pending"),
        result,
    )
    .await?;
    Ok(Json(result))
}
#[derive(Debug, Default, Deserialize)]
struct JobListQuery {
    cursor: Option<Uuid>,
    status: Option<JobStatus>,
    kind: Option<JobKind>,
    search: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct JobPage {
    items: Vec<Job>,
    next_cursor: Option<Uuid>,
}

async fn list_jobs(
    State(s): State<ApiState>,
    Query(query): Query<JobListQuery>,
) -> Result<Json<JobPage>> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let mut jobs = s
        .repository
        .list_jobs_page(JobListFilter {
            cursor: query.cursor,
            status: query.status,
            kind: query.kind,
            search: query.search,
            limit,
        })
        .await?;
    let has_more = jobs.len() > limit;
    jobs.truncate(limit);
    let next_cursor = has_more.then(|| jobs.last().map(|job| job.id)).flatten();
    Ok(Json(JobPage {
        items: jobs.into_iter().map(Job::redacted).collect(),
        next_cursor,
    }))
}
async fn get_job(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<Json<Job>> {
    Ok(Json(s.repository.get_job(id).await?.redacted()))
}
async fn update_job(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateJob>,
) -> Result<Json<Job>> {
    let result = s.manager.update_job(id, request).await;
    let job = audited(
        &s.repository,
        "job.update",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(Json(job.redacted()))
}
async fn list_job_outputs(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<JobOutput>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_job_outputs_page(id, window.offset, window.database_limit())
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}
async fn media_item_summary(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::storage::MediaItemSummary>> {
    Ok(Json(s.repository.media_item_summary(id).await?))
}

async fn list_media_items(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<MediaItemRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_media_items_page(
            id,
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn list_media_item_outputs(
    State(s): State<ApiState>,
    Path((job_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<MediaItemOutputRecord>>> {
    Ok(Json(
        s.repository
            .list_media_item_outputs(job_id, item_id)
            .await?,
    ))
}

async fn retry_media_item(
    State(s): State<ApiState>,
    Path((job_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Job>)> {
    let result = s.manager.retry_media_item(job_id, item_id).await;
    let job = audited(
        &s.repository,
        "media_item.retry",
        "media_item",
        Some(&item_id.to_string()),
        result,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(job.redacted())))
}

#[derive(Debug, Deserialize, Default)]
struct RetryFailedMediaItemsRequest {
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct MediaItemRetryResult {
    item_id: Uuid,
    job: Option<Job>,
    error_code: Option<&'static str>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct RetryFailedMediaItemsResponse {
    attempted: usize,
    accepted: usize,
    failed: usize,
    results: Vec<MediaItemRetryResult>,
}

async fn retry_failed_media_items(
    State(s): State<ApiState>,
    Path(job_id): Path<Uuid>,
    Json(request): Json<RetryFailedMediaItemsRequest>,
) -> Result<Json<RetryFailedMediaItemsResponse>> {
    let parent = s.repository.get_job(job_id).await?;
    if parent.kind != JobKind::Media {
        return Err(crate::error::RavynError::Conflict(
            "failed media items can be retried only for media jobs".into(),
        ));
    }
    let limit = request.limit.unwrap_or(100);
    if !(1..=500).contains(&limit) {
        return Err(crate::error::RavynError::Invalid(
            "media retry limit must be between 1 and 500".into(),
        ));
    }
    let items = s.repository.list_failed_media_items(job_id, limit).await?;
    let mut results = Vec::with_capacity(items.len());
    let mut accepted = 0usize;
    for item in items {
        match s.manager.retry_media_item(job_id, item.id).await {
            Ok(job) => {
                accepted = accepted.saturating_add(1);
                results.push(MediaItemRetryResult {
                    item_id: item.id,
                    job: Some(job.redacted()),
                    error_code: None,
                    error: None,
                });
            }
            Err(error) => results.push(MediaItemRetryResult {
                item_id: item.id,
                job: None,
                error_code: Some(error.api_code()),
                error: Some(error.public_message()),
            }),
        }
    }
    let attempted = results.len();
    let failed = attempted.saturating_sub(accepted);
    if let Err(error) = s
        .repository
        .append_audit_with_metadata(
            "media_items.retry_failed",
            "job",
            Some(&job_id.to_string()),
            if failed == 0 { "success" } else { "failure" },
            serde_json::json!({
                "attempted": attempted,
                "accepted": accepted,
                "failed": failed,
            }),
        )
        .await
    {
        tracing::warn!(%error, job_id = %job_id, "failed to persist media retry audit record");
    }
    Ok(Json(RetryFailedMediaItemsResponse {
        attempted,
        accepted,
        failed,
        results,
    }))
}

async fn list_media_archive(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<MediaArchiveRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_media_archive_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

#[derive(Deserialize)]
struct RemoveMediaArchiveRequest {
    extractor: String,
    media_id: String,
}

async fn remove_media_archive(
    State(s): State<ApiState>,
    Json(request): Json<RemoveMediaArchiveRequest>,
) -> Result<StatusCode> {
    let target = format!("{}:{}", request.extractor, request.media_id);
    let result = s
        .repository
        .remove_media_archive_entry(&request.extractor, &request.media_id)
        .await;
    audited(
        &s.repository,
        "media_archive.delete",
        "media_archive",
        Some(&target),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_job_segments(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<crate::storage::segments::SegmentRecord>>> {
    s.repository.get_job(id).await?;
    let window = PageWindow::from_query(&query)?;
    let items = crate::storage::segments::list_page(
        s.repository.pool(),
        id,
        window.offset,
        window.database_limit(),
    )
    .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn list_job_actions(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<JobActionRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_job_actions_page(id, window.offset, window.database_limit())
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}
async fn list_job_logs(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<JobLogRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_job_logs_page(
            id,
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn list_audit(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<AuditRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_audit_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

#[derive(Deserialize)]
struct PutSecretRequest {
    name: String,
    secret_type: String,
    secret: String,
}

async fn list_secrets(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<SecretReference>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_secret_references_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn put_secret(
    State(s): State<ApiState>,
    Json(request): Json<PutSecretRequest>,
) -> Result<(StatusCode, Json<SecretReference>)> {
    let result = s
        .repository
        .put_secret_reference(&request.name, &request.secret_type, request.secret)
        .await;
    let reference = audited(
        &s.repository,
        "secret.store",
        "secret_reference",
        None,
        result,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(reference)))
}

async fn delete_secret(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let result = s.repository.delete_secret_reference(id).await;
    audited(
        &s.repository,
        "secret.delete",
        "secret_reference",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MaintenanceRequest {
    retention_days: u32,
}

async fn run_maintenance(
    State(s): State<ApiState>,
    Json(request): Json<MaintenanceRequest>,
) -> Result<Json<Value>> {
    let result: Result<Value> = async {
        if !(1..=3650).contains(&request.retention_days) {
            return Err(crate::error::RavynError::Invalid(
                "retention_days must be between 1 and 3650".into(),
            ));
        }
        let cutoff = chrono::Utc::now() - chrono::Duration::days(i64::from(request.retention_days));
        s.repository.run_retention(cutoff).await
    }
    .await;
    Ok(Json(
        audited(&s.repository, "retention.run", "database", None, result).await?,
    ))
}

async fn list_schedule_executions(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<ScheduleExecutionRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_schedule_executions_page(id, window.offset, window.database_limit())
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn get_schedule_execution(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ScheduleExecutionRecord>> {
    Ok(Json(s.repository.get_schedule_execution(id).await?))
}
async fn run_schedule_now(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<ScheduleExecutionRecord>)> {
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok());
    let result = s.manager.run_schedule_now(id, key).await;
    let execution = audited(
        &s.repository,
        "schedule.run_now",
        "schedule",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(execution)))
}
async fn enable_schedule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Schedule>> {
    let result = s.repository.set_schedule_enabled(id, true).await;
    Ok(Json(
        audited(
            &s.repository,
            "schedule.enable",
            "schedule",
            Some(&id.to_string()),
            result,
        )
        .await?,
    ))
}
async fn disable_schedule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Schedule>> {
    let result = s.repository.set_schedule_enabled(id, false).await;
    Ok(Json(
        audited(
            &s.repository,
            "schedule.disable",
            "schedule",
            Some(&id.to_string()),
            result,
        )
        .await?,
    ))
}
async fn cancel_schedule_execution(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ScheduleExecutionRecord>> {
    let result = s.repository.cancel_schedule_execution(id).await;
    Ok(Json(
        audited(
            &s.repository,
            "schedule_execution.cancel",
            "schedule_execution",
            Some(&id.to_string()),
            result,
        )
        .await?,
    ))
}
async fn create_job(
    State(s): State<ApiState>,
    headers: HeaderMap,
    Json(r): Json<CreateJob>,
) -> Result<(StatusCode, Json<Job>)> {
    let result = if let Some(key) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
    {
        s.manager.create_idempotent(r, key).await
    } else {
        s.manager.create(r).await
    };
    let job = audited(&s.repository, "job.create", "job", None, result).await?;
    Ok((StatusCode::CREATED, Json(job.redacted())))
}
async fn create_batch(
    State(s): State<ApiState>,
    Json(requests): Json<Vec<CreateJob>>,
) -> Result<(StatusCode, Json<ImportResult>)> {
    let result = audited_import(
        &s.repository,
        "job.batch_create",
        s.manager.create_batch(requests).await,
    )
    .await?
    .redact_sensitive();
    Ok((import_status(&result), Json(result)))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BulkJobAction {
    Pause,
    Resume,
    Cancel,
    Retry,
    Delete,
}

#[derive(Debug, Deserialize)]
struct BulkJobActionRequest {
    /// An empty list applies the action to all jobs.
    #[serde(default)]
    ids: Vec<Uuid>,
    action: BulkJobAction,
}

#[derive(Serialize)]
struct BulkJobActionResult {
    id: Uuid,
    success: bool,
    error: Option<String>,
}

async fn apply_job_action(
    State(s): State<ApiState>,
    Json(mut request): Json<BulkJobActionRequest>,
) -> Result<Json<Vec<BulkJobActionResult>>> {
    if request.ids.is_empty() {
        request.ids = s
            .repository
            .list_jobs()
            .await?
            .into_iter()
            .map(|job| job.id)
            .collect();
    }
    request.ids.sort_unstable();
    request.ids.dedup();
    if request.ids.len() > 1_000 {
        return Err(crate::error::RavynError::Invalid(
            "bulk actions may target at most 1000 jobs".into(),
        ));
    }
    let action_name = match request.action {
        BulkJobAction::Pause => "pause",
        BulkJobAction::Resume => "resume",
        BulkJobAction::Cancel => "cancel",
        BulkJobAction::Retry => "retry",
        BulkJobAction::Delete => "delete",
    };
    let mut results = Vec::with_capacity(request.ids.len());
    for id in request.ids {
        let result = match request.action {
            BulkJobAction::Pause => s.manager.pause(id).await,
            BulkJobAction::Resume => s.manager.resume(id).await,
            BulkJobAction::Cancel => s.manager.cancel(id).await,
            BulkJobAction::Retry => s.manager.retry(id).await,
            BulkJobAction::Delete => s.manager.delete(id).await,
        };
        results.push(BulkJobActionResult {
            id,
            success: result.is_ok(),
            error: result.err().map(|error| error.to_string()),
        });
    }
    let succeeded = results.iter().filter(|item| item.success).count();
    let failed = results.len().saturating_sub(succeeded);
    if let Err(error) = s
        .repository
        .append_audit_with_metadata(
            "job.bulk_action",
            "job",
            None,
            if failed == 0 { "success" } else { "failure" },
            serde_json::json!({
                "action": action_name,
                "total": results.len(),
                "succeeded": succeeded,
                "failed": failed,
            }),
        )
        .await
    {
        tracing::warn!(%error, "failed to persist bulk job audit record");
    }
    Ok(Json(results))
}
async fn pause(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let result = s.manager.pause(id).await;
    audited(
        &s.repository,
        "job.pause",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn resume(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let result = s.manager.resume(id).await;
    audited(
        &s.repository,
        "job.resume",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn cancel(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let result = s.manager.cancel(id).await;
    audited(
        &s.repository,
        "job.cancel",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn retry(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let result = s.manager.retry(id).await;
    audited(
        &s.repository,
        "job.retry",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn delete_job(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let result = s.manager.delete(id).await;
    audited(
        &s.repository,
        "job.delete",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn probe_media(
    State(s): State<ApiState>,
    Json(request): Json<MediaProbeRequest>,
) -> Result<Json<MediaProbe>> {
    Ok(Json(s.manager.probe_media(&request).await?))
}

#[derive(serde::Serialize)]
struct Dependencies {
    media: Vec<DependencyStatus>,
    torrent: TorrentDependencyStatus,
}

async fn dependencies(State(s): State<ApiState>) -> Json<Dependencies> {
    let (media, torrent) = tokio::join!(
        s.manager.media_dependencies(),
        s.manager.torrent_dependencies()
    );
    Json(Dependencies { media, torrent })
}

#[derive(Serialize)]
struct SystemCapabilities {
    backend_version: &'static str,
    api_version: &'static str,
    database_version: i64,
    supported_job_kinds: [&'static str; 3],
    external_tools: Dependencies,
    available_features: Vec<&'static str>,
    disabled_features: Vec<&'static str>,
    platform: &'static str,
    authentication_modes: Vec<&'static str>,
}

async fn system_capabilities(State(s): State<ApiState>) -> Result<Json<SystemCapabilities>> {
    let (media, torrent) = tokio::join!(
        s.manager.media_dependencies(),
        s.manager.torrent_dependencies()
    );
    let mut authentication_modes = vec!["loopback_unauthenticated", "bearer_token"];
    if !s.manager.config().listen.ip().is_loopback() {
        authentication_modes.retain(|mode| *mode != "loopback_unauthenticated");
    }
    Ok(Json(SystemCapabilities {
        backend_version: env!("CARGO_PKG_VERSION"),
        api_version: "v1",
        database_version: s.repository.database_version().await?,
        supported_job_kinds: ["http", "media", "torrent"],
        external_tools: Dependencies { media, torrent },
        available_features: vec![
            "segmented_http",
            "resume",
            "mirrors",
            "media",
            "torrent",
            "scheduler",
            "rules",
            "browser_bridge",
            "checksums",
            "post_processing",
            "job_outputs",
            "database_backup",
            "sse_replay",
            "platform_secret_store",
            "output_lineage",
            "media_item_tracking",
            "media_archive",
            "torrent_seeding_policies",
            "api_backpressure",
            "per_token_rate_limiting",
            "media_auxiliary_outputs",
            "ytdlp_capability_probe",
            "typed_rqbit_contracts",
            "scheduler_overlap_policies",
            "scheduler_missed_run_policies",
            "fixed_offset_schedule_timezones",
            "live_global_bandwidth_limit",
        ],
        disabled_features: vec!["native_tls", "metalink", "http3"],
        platform: std::env::consts::OS,
        authentication_modes,
    }))
}

#[derive(Serialize)]
struct SettingsResponse {
    values: PersistentSettings,
    application: std::collections::BTreeMap<&'static str, &'static str>,
    restart_required: bool,
}

fn settings_response(values: PersistentSettings, restart_required: bool) -> SettingsResponse {
    let mut application = std::collections::BTreeMap::new();
    for key in [
        "download_dir",
        "max_active",
        "max_segments",
        "segment_threshold_mib",
        "max_connections_per_host",
        "global_speed_limit_bps",
        "ytdlp",
        "ffmpeg",
        "rqbit_api",
        "rqbit_credentials_secret_id",
        "seven_zip",
        "max_extract_mib",
        "max_extract_files",
        "max_extract_depth",
        "max_extract_ratio",
        "max_retries",
        "host_circuit_threshold",
        "host_circuit_cooldown_secs",
        "max_torrent_mib",
        "max_html_mib",
        "max_sniff_resources",
        "max_batch_urls",
        "connect_timeout_secs",
        "read_timeout_secs",
        "media_probe_timeout_secs",
        "media_probe_max_mib",
        "rqbit_timeout_secs",
        "rqbit_stats_timeout_secs",
        "torrent_refresh_concurrency",
        "image_converter",
        "avif_quality",
        "cookie_dir",
        "api_request_timeout_secs",
        "api_max_concurrent_requests",
        "api_rate_limit_per_minute",
        "api_rate_limit_burst",
    ] {
        application.insert(
            key,
            if key == "global_speed_limit_bps" {
                "live"
            } else {
                "backend_restart"
            },
        );
    }
    SettingsResponse {
        values,
        application,
        restart_required,
    }
}

async fn get_settings(State(s): State<ApiState>) -> Result<Json<SettingsResponse>> {
    let values = s
        .repository
        .load_persistent_settings()
        .await?
        .unwrap_or_else(|| PersistentSettings::from_config(&s.manager.config()));
    Ok(Json(settings_response(values, false)))
}

fn settings_patch_requires_restart(patch: &PersistentSettingsPatch) -> bool {
    patch.download_dir.is_some()
        || patch.max_active.is_some()
        || patch.max_segments.is_some()
        || patch.segment_threshold_mib.is_some()
        || patch.max_connections_per_host.is_some()
        || patch.ytdlp.is_some()
        || patch.ffmpeg.is_some()
        || patch.rqbit_api.is_some()
        || patch.rqbit_credentials_secret_id.is_some()
        || patch.seven_zip.is_some()
        || patch.max_extract_mib.is_some()
        || patch.max_extract_files.is_some()
        || patch.max_extract_depth.is_some()
        || patch.max_extract_ratio.is_some()
        || patch.max_retries.is_some()
        || patch.host_circuit_threshold.is_some()
        || patch.host_circuit_cooldown_secs.is_some()
        || patch.max_torrent_mib.is_some()
        || patch.max_html_mib.is_some()
        || patch.max_sniff_resources.is_some()
        || patch.max_batch_urls.is_some()
        || patch.connect_timeout_secs.is_some()
        || patch.read_timeout_secs.is_some()
        || patch.media_probe_timeout_secs.is_some()
        || patch.media_probe_max_mib.is_some()
        || patch.rqbit_timeout_secs.is_some()
        || patch.rqbit_stats_timeout_secs.is_some()
        || patch.torrent_refresh_concurrency.is_some()
        || patch.image_converter.is_some()
        || patch.avif_quality.is_some()
        || patch.cookie_dir.is_some()
        || patch.api_request_timeout_secs.is_some()
        || patch.api_max_concurrent_requests.is_some()
        || patch.api_rate_limit_per_minute.is_some()
        || patch.api_rate_limit_burst.is_some()
}

async fn patch_settings(
    State(s): State<ApiState>,
    Json(patch): Json<PersistentSettingsPatch>,
) -> Result<Json<SettingsResponse>> {
    let result: Result<SettingsResponse> = async {
        let restart_required = settings_patch_requires_restart(&patch);
        let mut values = s
            .repository
            .load_persistent_settings()
            .await?
            .unwrap_or_else(|| PersistentSettings::from_config(&s.manager.config()));
        values.merge(patch);
        let mut candidate = (*s.manager.config()).clone();
        values.apply_to(&mut candidate)?;
        s.repository.save_persistent_settings(&values).await?;
        s.manager.apply_live_settings(&values);
        Ok(settings_response(values, restart_required))
    }
    .await;
    Ok(Json(
        audited(
            &s.repository,
            "settings.update",
            "settings",
            Some("runtime"),
            result,
        )
        .await?,
    ))
}

async fn reset_settings(State(s): State<ApiState>) -> Result<Json<SettingsResponse>> {
    let result: Result<SettingsResponse> = async {
        s.repository.reset_persistent_settings().await?;
        let values = PersistentSettings::from_config(&s.base_config);
        s.manager.apply_live_settings(&values);
        Ok(settings_response(values, true))
    }
    .await;
    Ok(Json(
        audited(
            &s.repository,
            "settings.reset",
            "settings",
            Some("runtime"),
            result,
        )
        .await?,
    ))
}

async fn list_host_profiles(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<HostProfile>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_host_profiles_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn reset_host_profiles(State(s): State<ApiState>) -> Result<Json<serde_json::Value>> {
    let result = crate::storage::host_profiles::clear(s.repository.pool()).await;
    let deleted = audited(
        &s.repository,
        "host_profiles.reset",
        "host_profile",
        None,
        result,
    )
    .await?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

async fn probe_torrent(
    State(s): State<ApiState>,
    Json(request): Json<TorrentProbeRequest>,
) -> Result<Json<TorrentProbe>> {
    Ok(Json(s.manager.probe_torrent(&request).await?))
}

async fn managed_torrents(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<TorrentRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_torrent_records_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn list_engine_torrents(State(s): State<ApiState>) -> Result<Json<TorrentEngineList>> {
    Ok(Json(s.manager.list_engine_torrents().await?))
}

async fn torrent_engine_stats(State(s): State<ApiState>) -> Result<Json<TorrentGlobalStats>> {
    Ok(Json(s.manager.torrent_engine_stats().await?))
}

async fn torrent_dht_stats(State(s): State<ApiState>) -> Result<Json<Value>> {
    Ok(Json(s.manager.torrent_dht_stats().await?))
}

async fn torrent_dht_table(State(s): State<ApiState>) -> Result<Json<Value>> {
    Ok(Json(s.manager.torrent_dht_table().await?))
}

async fn torrent_details(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TorrentDetails>> {
    Ok(Json(s.manager.torrent_details(id).await?))
}

async fn torrent_stats(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TorrentSnapshot>> {
    Ok(Json(s.manager.torrent_stats(id).await?))
}

async fn torrent_seeding_state(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Option<crate::storage::TorrentSeedingState>>> {
    s.repository.get_job(id).await?;
    Ok(Json(s.repository.get_torrent_seeding_state(id).await?))
}

async fn torrent_peers(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TorrentPeerStats>> {
    Ok(Json(s.manager.torrent_peers(id).await?))
}

#[derive(Deserialize)]
struct AddTorrentPeers {
    peers: Vec<String>,
}

async fn add_torrent_peers(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(request): Json<AddTorrentPeers>,
) -> Result<StatusCode> {
    let result = s.manager.add_torrent_peers(id, &request.peers).await;
    audited(
        &s.repository,
        "torrent.peers.add",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct UpdateTorrentFiles {
    files: Vec<usize>,
}

async fn update_torrent_files(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateTorrentFiles>,
) -> Result<StatusCode> {
    let result = s.manager.update_torrent_files(id, &request.files).await;
    audited(
        &s.repository,
        "torrent.files.update",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct RemoveTorrent {
    #[serde(default)]
    delete_files: bool,
}

async fn remove_torrent(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(request): Json<RemoveTorrent>,
) -> Result<StatusCode> {
    let result = s.manager.remove_torrent(id, request.delete_files).await;
    audited(
        &s.repository,
        "torrent.remove",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn import_text(
    State(s): State<ApiState>,
    Json(request): Json<ImportTextRequest>,
) -> Result<(StatusCode, Json<ImportResult>)> {
    let result = audited_import(
        &s.repository,
        "job.text_import",
        s.manager.import_text(request).await,
    )
    .await?
    .redact_sensitive();
    Ok((import_status(&result), Json(result)))
}

async fn list_rules(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<crate::services::rules::Rule>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_rules_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn get_rule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::services::rules::Rule>> {
    Ok(Json(s.repository.get_rule(id).await?))
}

async fn create_rule(
    State(s): State<ApiState>,
    Json(input): Json<RuleInput>,
) -> Result<(StatusCode, Json<crate::services::rules::Rule>)> {
    let result = s.manager.create_rule(input).await;
    let rule = audited(&s.repository, "rule.create", "rule", None, result).await?;
    Ok((StatusCode::CREATED, Json(rule)))
}

async fn update_rule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(input): Json<RuleInput>,
) -> Result<Json<crate::services::rules::Rule>> {
    let result = s.manager.update_rule(id, input).await;
    Ok(Json(
        audited(
            &s.repository,
            "rule.update",
            "rule",
            Some(&id.to_string()),
            result,
        )
        .await?,
    ))
}

async fn delete_rule(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let result = s.repository.delete_rule(id).await;
    audited(
        &s.repository,
        "rule.delete",
        "rule",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_tags(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<TagRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_tags_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn delete_tag(State(s): State<ApiState>, Path(id): Path<i64>) -> Result<StatusCode> {
    let result = s.repository.delete_tag(id).await;
    audited(
        &s.repository,
        "tag.delete",
        "tag",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_job_tags(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<String>>> {
    Ok(Json(s.repository.list_job_tags(id).await?))
}

#[derive(Deserialize)]
struct ReplaceTags {
    tags: Vec<String>,
}

async fn replace_job_tags(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ReplaceTags>,
) -> Result<Json<Vec<String>>> {
    let result = s.repository.replace_job_tags(id, &request.tags).await;
    Ok(Json(
        audited(
            &s.repository,
            "job.tags.replace",
            "job",
            Some(&id.to_string()),
            result,
        )
        .await?,
    ))
}

#[derive(Deserialize)]
struct PageUrlRequest {
    page_url: String,
    cursor: Option<String>,
    limit: Option<usize>,
    search: Option<String>,
}

#[derive(Deserialize)]
struct ClearPageHistoryRequest {
    page_url: Option<String>,
}

async fn list_pages(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<PageRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_pages_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn list_page_resources(
    State(s): State<ApiState>,
    Json(request): Json<PageUrlRequest>,
) -> Result<Json<Page<PageResourceRecord>>> {
    let config = s.manager.config();
    crate::services::security::validate_network_source(&config, &request.page_url)?;
    let query = PageQuery {
        cursor: request.cursor,
        limit: request.limit,
        search: request.search,
    };
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_page_resources_page(
            &request.page_url,
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn clear_page_history(
    State(s): State<ApiState>,
    Json(request): Json<ClearPageHistoryRequest>,
) -> Result<Json<serde_json::Value>> {
    if let Some(page_url) = request.page_url.as_deref() {
        let config = s.manager.config();
        crate::services::security::validate_network_source(&config, page_url)?;
    }
    let resource_id = request.page_url.as_deref();
    let result = s.repository.clear_page_history(resource_id).await;
    let deleted = audited(
        &s.repository,
        "page_history.clear",
        "page_history",
        resource_id,
        result,
    )
    .await?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

async fn list_schedules(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<Schedule>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_schedules_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn get_schedule(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<Json<Schedule>> {
    Ok(Json(s.repository.get_schedule(id).await?))
}

async fn create_schedule(
    State(s): State<ApiState>,
    Json(input): Json<ScheduleInput>,
) -> Result<(StatusCode, Json<Schedule>)> {
    let result = s.manager.create_schedule(input).await;
    let schedule = audited(&s.repository, "schedule.create", "schedule", None, result).await?;
    Ok((StatusCode::CREATED, Json(schedule)))
}

async fn update_schedule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(input): Json<ScheduleInput>,
) -> Result<Json<Schedule>> {
    let result = s.manager.update_schedule(id, input).await;
    Ok(Json(
        audited(
            &s.repository,
            "schedule.update",
            "schedule",
            Some(&id.to_string()),
            result,
        )
        .await?,
    ))
}

async fn delete_schedule(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let result = s.repository.delete_schedule(id).await;
    audited(
        &s.repository,
        "schedule.delete",
        "schedule",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_browser_token(
    State(s): State<ApiState>,
    Json(request): Json<CreateBrowserToken>,
) -> Result<(StatusCode, Json<IssuedBrowserToken>)> {
    let result = s.manager.issue_browser_token(request).await;
    let token = audited(
        &s.repository,
        "browser_token.create",
        "browser_token",
        None,
        result,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(token)))
}

async fn list_browser_tokens(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<BrowserTokenRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_browser_tokens_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

async fn revoke_browser_token(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let result = s.manager.revoke_browser_token(id).await;
    audited(
        &s.repository,
        "browser_token.revoke",
        "browser_token",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sniff_page(
    State(s): State<ApiState>,
    Json(request): Json<SniffRequest>,
) -> Result<Json<SniffResult>> {
    Ok(Json(s.manager.sniff_page(&request).await?))
}

#[derive(Deserialize)]
struct BrowserImportItem {
    url: String,
    kind: ResourceKind,
}

#[derive(Deserialize)]
struct BrowserImportRequest {
    page_url: Option<String>,
    resources: Vec<BrowserImportItem>,
    #[serde(default)]
    defaults: BrowserImportDefaults,
}

/// Deliberately capability-reduced defaults accepted from browser extensions.
/// Browser callers cannot choose local paths, proxies, cookies, arbitrary
/// headers, media/torrent options, or post-processing commands.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BrowserImportDefaults {
    priority: i32,
    speed_limit_bps: Option<u64>,
    duplicate_policy: DuplicatePolicy,
    segments: Option<usize>,
    overwrite: bool,
    tags: Vec<String>,
}

impl From<BrowserImportDefaults> for ImportDefaults {
    fn from(value: BrowserImportDefaults) -> Self {
        Self {
            kind: JobKind::Http,
            destination: None,
            priority: value.priority,
            speed_limit_bps: value.speed_limit_bps,
            duplicate_policy: value.duplicate_policy,
            options: DownloadOptions {
                segments: value.segments,
                overwrite: value.overwrite,
                tags: value.tags,
                ..DownloadOptions::default()
            },
        }
    }
}

async fn import_browser_resources(
    State(s): State<ApiState>,
    Json(request): Json<BrowserImportRequest>,
) -> Result<(StatusCode, Json<ImportResult>)> {
    let result: Result<ImportResult> = async {
        if request.resources.is_empty() {
            return Err(crate::error::RavynError::Invalid(
                "browser import may not be empty".into(),
            ));
        }
        if let Some(page_url) = request.page_url.as_deref() {
            let config = s.manager.config();
            crate::services::security::validate_network_source_resolved(&config, page_url).await?;
        }
        let sources = request
            .resources
            .iter()
            .map(|resource| resource.url.clone())
            .collect::<Vec<_>>();
        let result = s
            .manager
            .import_urls(sources, request.defaults.into(), 0)
            .await?;
        if let Some(page_url) = request.page_url.as_deref() {
            let remembered = request
                .resources
                .iter()
                .filter(|item| {
                    result
                        .items
                        .iter()
                        .any(|result| result.source == item.url && result.job.is_some())
                })
                .map(|item| (item.url.clone(), item.kind.as_str().to_owned(), true))
                .collect::<Vec<_>>();
            s.repository
                .remember_page_resources(page_url, &remembered)
                .await?;
        }
        Ok(result)
    }
    .await;
    let result = audited_import(&s.repository, "browser.import", result)
        .await?
        .redact_sensitive();
    Ok((import_status(&result), Json(result)))
}

fn import_status(result: &ImportResult) -> StatusCode {
    if result.rejected > 0 || result.truncated {
        StatusCode::MULTI_STATUS
    } else {
        StatusCode::CREATED
    }
}

async fn events(
    State(s): State<ApiState>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = std::result::Result<SseEvent, Infallible>>> {
    let last_sequence = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let subscription = s.manager.events().subscribe_from(last_sequence);
    let replay = futures_util::stream::iter(
        subscription
            .replay
            .into_iter()
            .map(|event| Ok(to_sse_event(event))),
    );
    let live = BroadcastStream::new(subscription.receiver).filter_map(|item| async move {
        Some(Ok(match item {
            Ok(event) => to_sse_event(event),
            Err(_) => SseEvent::default()
                .event("resync_required")
                .data(r#"{"reason":"subscriber_lagged"}"#),
        }))
    });
    let stream = replay.chain(live);
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

fn to_sse_event(event: crate::core::events::SequencedEvent) -> SseEvent {
    SseEvent::default()
        .id(event.sequence.to_string())
        .json_data(event)
        .unwrap_or_else(|_| SseEvent::default().data("{}"))
}
