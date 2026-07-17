//! Imports, rules, tags, monitored pages, and schedule handlers.

use super::*;

pub(super) async fn list_schedule_executions(
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

pub(super) async fn get_schedule_execution(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ScheduleExecutionRecord>> {
    Ok(Json(s.repository.get_schedule_execution(id).await?))
}
pub(super) async fn run_schedule_now(
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
pub(super) async fn enable_schedule(
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
pub(super) async fn disable_schedule(
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
pub(super) async fn cancel_schedule_execution(
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
pub(super) async fn import_text(
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

pub(super) async fn list_rules(
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

#[derive(Debug, Deserialize)]
pub(super) struct RulePreviewRequest {
    pub request: CreateJob,
    pub mime: Option<String>,
    pub extension: Option<String>,
}

pub(super) async fn preview_rules(
    State(s): State<ApiState>,
    Json(input): Json<RulePreviewRequest>,
) -> Result<Json<crate::services::rules::RulePreview>> {
    const MAX_PREVIEW_RULES: usize = 1_000;
    let rules = s
        .repository
        .list_rules_page(0, MAX_PREVIEW_RULES + 1, None)
        .await?;
    if rules.len() > MAX_PREVIEW_RULES {
        return Err(crate::error::RavynError::Invalid(format!(
            "rule preview is limited to {MAX_PREVIEW_RULES} rules"
        )));
    }
    Ok(Json(crate::services::rules::preview_matching(
        &rules,
        &input.request,
        input.mime.as_deref(),
        input.extension.as_deref(),
    )))
}

pub(super) async fn get_rule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::services::rules::Rule>> {
    Ok(Json(s.repository.get_rule(id).await?))
}

pub(super) async fn create_rule(
    State(s): State<ApiState>,
    Json(input): Json<RuleInput>,
) -> Result<(StatusCode, Json<crate::services::rules::Rule>)> {
    let result = s.manager.create_rule(input).await;
    let rule = audited(&s.repository, "rule.create", "rule", None, result).await?;
    s.manager.events().publish(Event::RuleChanged);
    Ok((StatusCode::CREATED, Json(rule)))
}

pub(super) async fn update_rule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(input): Json<RuleInput>,
) -> Result<Json<crate::services::rules::Rule>> {
    let result = s.manager.update_rule(id, input).await;
    let rule = audited(
        &s.repository,
        "rule.update",
        "rule",
        Some(&id.to_string()),
        result,
    )
    .await?;
    s.manager.events().publish(Event::RuleChanged);
    Ok(Json(rule))
}

pub(super) async fn delete_rule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let result = s.repository.delete_rule(id).await;
    audited(
        &s.repository,
        "rule.delete",
        "rule",
        Some(&id.to_string()),
        result,
    )
    .await?;
    s.manager.events().publish(Event::RuleChanged);
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_tags(
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

pub(super) async fn delete_tag(
    State(s): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<StatusCode> {
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

pub(super) async fn list_job_tags(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<String>>> {
    Ok(Json(s.repository.list_job_tags(id).await?))
}

#[derive(Deserialize)]
pub(super) struct ReplaceTags {
    tags: Vec<String>,
}

pub(super) async fn replace_job_tags(
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
pub(super) struct PageUrlRequest {
    page_url: String,
    cursor: Option<String>,
    limit: Option<usize>,
    search: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ClearPageHistoryRequest {
    page_url: Option<String>,
}

pub(super) async fn list_pages(
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

pub(super) async fn list_page_resources(
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

pub(super) async fn clear_page_history(
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

pub(super) async fn list_schedules(
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

pub(super) async fn get_schedule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Schedule>> {
    Ok(Json(s.repository.get_schedule(id).await?))
}

pub(super) async fn create_schedule(
    State(s): State<ApiState>,
    Json(input): Json<ScheduleInput>,
) -> Result<(StatusCode, Json<Schedule>)> {
    let result = s.manager.create_schedule(input).await;
    let schedule = audited(&s.repository, "schedule.create", "schedule", None, result).await?;
    Ok((StatusCode::CREATED, Json(schedule)))
}

pub(super) async fn update_schedule(
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

pub(super) async fn delete_schedule(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
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
