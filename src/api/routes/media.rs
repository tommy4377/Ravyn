//! Media item, archive, and probe handlers.

use super::*;

pub(super) async fn media_item_summary(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::storage::MediaItemSummary>> {
    Ok(Json(s.repository.media_item_summary(id).await?))
}

pub(super) async fn list_media_items(
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

pub(super) async fn list_media_item_outputs(
    State(s): State<ApiState>,
    Path((job_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<MediaItemOutputRecord>>> {
    Ok(Json(
        s.repository
            .list_media_item_outputs(job_id, item_id)
            .await?,
    ))
}

pub(super) async fn retry_media_item(
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
pub(super) struct RetryFailedMediaItemsRequest {
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(super) struct MediaItemRetryResult {
    item_id: Uuid,
    job: Option<Job>,
    error_code: Option<&'static str>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RetryFailedMediaItemsResponse {
    attempted: usize,
    accepted: usize,
    failed: usize,
    results: Vec<MediaItemRetryResult>,
}

pub(super) async fn retry_failed_media_items(
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

pub(super) async fn list_media_archive(
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
pub(super) struct RemoveMediaArchiveRequest {
    extractor: String,
    media_id: String,
}

pub(super) async fn remove_media_archive(
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
pub(super) async fn probe_media(
    State(s): State<ApiState>,
    Json(request): Json<MediaProbeRequest>,
) -> Result<Json<MediaProbe>> {
    Ok(Json(s.manager.probe_media(&request).await?))
}
