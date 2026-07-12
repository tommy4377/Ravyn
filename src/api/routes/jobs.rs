//! Job collection and lifecycle handlers.

use super::*;

#[derive(Debug, Default, Deserialize)]
pub(super) struct JobListQuery {
    cursor: Option<Uuid>,
    status: Option<JobStatus>,
    kind: Option<JobKind>,
    search: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
pub(super) struct JobPage {
    items: Vec<Job>,
    next_cursor: Option<Uuid>,
}

pub(super) async fn list_jobs(
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
pub(super) async fn get_job(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<Json<Job>> {
    Ok(Json(s.repository.get_job(id).await?.redacted()))
}
pub(super) async fn update_job(
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
pub(super) async fn list_job_outputs(
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
pub(super) async fn list_job_segments(
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

pub(super) async fn list_job_actions(
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
pub(super) async fn list_job_logs(
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
pub(super) async fn create_job(
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

#[derive(Debug, Deserialize)]
pub(super) struct MetalinkJobRequest {
    document: String,
    destination: Option<std::path::PathBuf>,
    #[serde(default)]
    priority: i32,
    speed_limit_bps: Option<u64>,
    #[serde(default)]
    overwrite: bool,
}

pub(super) async fn create_metalink_job(
    State(s): State<ApiState>,
    Json(request): Json<MetalinkJobRequest>,
) -> Result<(StatusCode, Json<Job>)> {
    let file = crate::services::metalink::parse(request.document.as_bytes())?;
    let mut mirrors = file.mirrors.into_iter().map(|item| item.url);
    let source = mirrors
        .next()
        .ok_or_else(|| crate::error::RavynError::Invalid("Metalink has no mirrors".into()))?;
    let metalink = crate::core::models::MetalinkMetadata {
        size: file.size,
        piece_length: file.pieces.as_ref().map(|pieces| pieces.length),
        piece_sha256: file.pieces.map(|pieces| pieces.sha256).unwrap_or_default(),
    };
    let result = s
        .manager
        .create(CreateJob {
            kind: JobKind::Http,
            source,
            destination: request.destination,
            filename: Some(file.name),
            priority: request.priority,
            speed_limit_bps: request.speed_limit_bps,
            expected_sha256: file.sha256,
            duplicate_policy: DuplicatePolicy::default(),
            options: DownloadOptions {
                mirrors: mirrors.collect(),
                metalink: Some(metalink),
                overwrite: request.overwrite,
                ..DownloadOptions::default()
            },
        })
        .await;
    let job = audited(&s.repository, "job.metalink.create", "job", None, result).await?;
    Ok((StatusCode::CREATED, Json(job.redacted())))
}
pub(super) async fn create_batch(
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
pub(super) enum BulkJobAction {
    Pause,
    Resume,
    Cancel,
    Retry,
    Delete,
}

#[derive(Debug, Deserialize)]
pub(super) struct BulkJobActionRequest {
    /// An empty list applies the action to all jobs.
    #[serde(default)]
    ids: Vec<Uuid>,
    action: BulkJobAction,
}

#[derive(Serialize)]
pub(super) struct BulkJobActionResult {
    id: Uuid,
    success: bool,
    error: Option<String>,
}

pub(super) async fn apply_job_action(
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
pub(super) async fn pause(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
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
pub(super) async fn resume(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
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
pub(super) async fn cancel(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
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
pub(super) async fn retry(State(s): State<ApiState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
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
pub(super) async fn delete_job(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
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
