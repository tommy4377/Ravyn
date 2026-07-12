//! Browser-scoped token and import handlers.

use super::*;

pub(super) async fn create_browser_token(
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

pub(super) async fn list_browser_tokens(
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

pub(super) async fn revoke_browser_token(
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

pub(super) async fn sniff_page(
    State(s): State<ApiState>,
    Json(request): Json<SniffRequest>,
) -> Result<Json<SniffResult>> {
    Ok(Json(s.manager.sniff_page(&request).await?))
}

#[derive(Deserialize)]
pub(super) struct BrowserImportItem {
    url: String,
    kind: ResourceKind,
}

#[derive(Deserialize)]
pub(super) struct BrowserImportRequest {
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
pub(super) struct BrowserImportDefaults {
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

pub(super) async fn import_browser_resources(
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
