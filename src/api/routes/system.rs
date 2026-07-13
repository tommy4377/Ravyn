//! Health, readiness, metrics, backups, restore, secrets, audit,
//! maintenance, settings, capabilities, and the SSE event stream.

use super::*;

pub(super) async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub(super) async fn openapi() -> Json<Value> {
    Json(crate::api::openapi::document())
}

#[derive(Serialize)]
pub(super) struct Readiness {
    ready: bool,
    database_writable: bool,
    download_root_writable: bool,
    progress_writer_running: bool,
    accepting_tasks: bool,
}

pub(super) async fn readiness(State(s): State<ApiState>) -> impl IntoResponse {
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

pub(super) async fn metrics(State(s): State<ApiState>) -> Result<impl IntoResponse> {
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
    let download_dir = s.manager.config().effective_download_dir();
    let (free_bytes, temporary_bytes) = tokio::task::spawn_blocking(move || {
        (
            crate::core::metrics::free_disk_space(&download_dir),
            crate::core::metrics::temporary_disk_usage(&download_dir),
        )
    })
    .await
    .unwrap_or((None, 0));
    if let Some(free_bytes) = free_bytes {
        body.push_str("# HELP ravyn_disk_free_bytes Free bytes on the download filesystem.\n# TYPE ravyn_disk_free_bytes gauge\n");
        body.push_str(&format!("ravyn_disk_free_bytes {free_bytes}\n"));
    }
    body.push_str("# HELP ravyn_temporary_disk_usage_bytes Bytes held by Ravyn partial files and extraction staging directories.\n# TYPE ravyn_temporary_disk_usage_bytes gauge\n");
    body.push_str(&format!(
        "ravyn_temporary_disk_usage_bytes {temporary_bytes}\n"
    ));
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
pub(super) struct DatabaseStatus {
    integrity: String,
}

pub(super) async fn database_status(State(s): State<ApiState>) -> Result<Json<DatabaseStatus>> {
    Ok(Json(DatabaseStatus {
        integrity: s.repository.integrity_check().await?,
    }))
}

pub(super) async fn backup_database(
    State(s): State<ApiState>,
) -> Result<(StatusCode, Json<Value>)> {
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
pub(super) async fn list_backups(
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
pub(super) async fn verify_backup(
    State(s): State<ApiState>,
    Path(name): Path<String>,
) -> Result<Json<Value>> {
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

pub(super) async fn schedule_database_restore(
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

pub(super) async fn database_restore_status(
    State(s): State<ApiState>,
) -> Result<Json<crate::storage::recovery::RestoreStatus>> {
    Ok(Json(s.manager.database_restore_status().await?))
}

pub(super) async fn cancel_database_restore(
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
pub(super) async fn list_audit(
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

pub(super) async fn verify_audit_chain(
    State(s): State<ApiState>,
) -> Result<Json<AuditChainStatus>> {
    Ok(Json(s.repository.verify_audit_chain().await?))
}

#[derive(Deserialize)]
pub(super) struct PutSecretRequest {
    name: String,
    secret_type: String,
    secret: String,
}

pub(super) async fn list_secrets(
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

pub(super) async fn put_secret(
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

pub(super) async fn delete_secret(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
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
pub(super) struct MaintenanceRequest {
    retention_days: u32,
}

pub(super) async fn run_maintenance(
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
#[derive(serde::Serialize)]
pub(super) struct Dependencies {
    media: Vec<DependencyStatus>,
    torrent: TorrentDependencyStatus,
}

pub(super) async fn dependencies(State(s): State<ApiState>) -> Json<Dependencies> {
    let (media, torrent) = tokio::join!(
        s.manager.media_dependencies(),
        s.manager.torrent_dependencies()
    );
    Json(Dependencies { media, torrent })
}

#[derive(Serialize)]
pub(super) struct SystemCapabilities {
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

pub(super) async fn system_capabilities(
    State(s): State<ApiState>,
) -> Result<Json<SystemCapabilities>> {
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
            "metalink_v4",
            "piece_verified_mirror_failover",
            "managed_engine_activation",
            "concurrent_multi_source",
            "speculative_http",
            "active_range_splitting",
            "organized_download_library",
            "library_content_classification",
            "library_category_overrides",
            "library_duplicate_candidates",
            "download_presets",
            "filename_templates",
            "local_cache_reuse",
            "library_import_and_relocation",
            "download_trash",
            "download_basket",
            "user_profiles",
            "explainable_trust_score",
            "cleanup_policies",
            "personal_statistics",
        ],
        disabled_features: vec!["native_tls", "http3"],
        platform: std::env::consts::OS,
        authentication_modes,
    }))
}

#[derive(Serialize)]
pub(super) struct SettingsResponse {
    values: PersistentSettings,
    application: std::collections::BTreeMap<&'static str, &'static str>,
    restart_required: bool,
}

pub(super) fn settings_response(
    values: PersistentSettings,
    restart_required: bool,
) -> SettingsResponse {
    let mut application = std::collections::BTreeMap::new();
    for key in [
        "download_dir",
        "library_root",
        "library_auto_organize",
        "library_category_overrides",
        "max_active",
        "max_segments",
        "segment_threshold_mib",
        "max_connections_per_host",
        "global_speed_limit_bps",
        "bandwidth_schedule",
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
            if matches!(
                key,
                "max_active"
                    | "global_speed_limit_bps"
                    | "bandwidth_schedule"
                    | "api_request_timeout_secs"
                    | "api_max_concurrent_requests"
                    | "api_rate_limit_per_minute"
                    | "api_rate_limit_burst"
            ) {
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

pub(super) async fn get_settings(State(s): State<ApiState>) -> Result<Json<SettingsResponse>> {
    let values = s
        .repository
        .load_persistent_settings()
        .await?
        .unwrap_or_else(|| PersistentSettings::from_config(&s.manager.config()));
    Ok(Json(settings_response(values, false)))
}

pub(super) fn settings_patch_requires_restart(patch: &PersistentSettingsPatch) -> bool {
    patch.download_dir.is_some()
        || patch.library_root.is_some()
        || patch.library_auto_organize.is_some()
        || patch.library_category_overrides.is_some()
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
}

#[derive(Serialize)]
pub(super) struct SettingsIssue {
    field: &'static str,
    message: String,
}

#[derive(Serialize)]
pub(super) struct SettingsValidationResponse {
    valid: bool,
    restart_required: bool,
    issues: Vec<SettingsIssue>,
}

/// Computes the per-field validation report for a settings patch. Each
/// supplied field is applied alone to the known-good running configuration so
/// blame lands on the field that actually fails; a failure that only appears
/// when the fields combine is reported as `(combination)`.
pub(super) fn settings_validation_issues(
    current: &PersistentSettings,
    base: &crate::config::Config,
    patch: &PersistentSettingsPatch,
) -> Vec<SettingsIssue> {
    let mut merged = current.clone();
    merged.merge(patch.clone());
    let mut candidate = base.clone();
    let mut issues = Vec::new();
    if let Err(error) = merged.apply_to(&mut candidate) {
        for (field, single) in single_field_patches(patch) {
            let mut isolated = current.clone();
            isolated.merge(single);
            let mut candidate = base.clone();
            if let Err(field_error) = isolated.apply_to(&mut candidate) {
                issues.push(SettingsIssue {
                    field,
                    message: field_error.to_string(),
                });
            }
        }
        if issues.is_empty() {
            issues.push(SettingsIssue {
                field: "(combination)",
                message: error.to_string(),
            });
        }
    }
    issues
}

fn single_field_patches(
    patch: &PersistentSettingsPatch,
) -> Vec<(&'static str, PersistentSettingsPatch)> {
    macro_rules! explode {
        ($($field:ident),+ $(,)?) => {{
            let mut out = Vec::new();
            $(
                if patch.$field.is_some() {
                    let mut single = PersistentSettingsPatch::default();
                    single.$field = patch.$field.clone();
                    out.push((stringify!($field), single));
                }
            )+
            out
        }};
    }
    explode!(
        download_dir,
        library_root,
        library_auto_organize,
        library_category_overrides,
        max_active,
        max_segments,
        segment_threshold_mib,
        max_connections_per_host,
        global_speed_limit_bps,
        bandwidth_schedule,
        ytdlp,
        ffmpeg,
        rqbit_api,
        rqbit_credentials_secret_id,
        seven_zip,
        max_extract_mib,
        max_extract_files,
        max_extract_depth,
        max_extract_ratio,
        max_retries,
        host_circuit_threshold,
        host_circuit_cooldown_secs,
        max_torrent_mib,
        max_html_mib,
        max_sniff_resources,
        max_batch_urls,
        connect_timeout_secs,
        read_timeout_secs,
        media_probe_timeout_secs,
        media_probe_max_mib,
        rqbit_timeout_secs,
        rqbit_stats_timeout_secs,
        torrent_refresh_concurrency,
        image_converter,
        avif_quality,
        cookie_dir,
        api_request_timeout_secs,
        api_max_concurrent_requests,
        api_rate_limit_per_minute,
        api_rate_limit_burst,
    )
}

/// Validates a settings patch without persisting or applying anything and
/// returns every failing field instead of only the first error.
pub(super) async fn validate_settings(
    State(s): State<ApiState>,
    Json(patch): Json<PersistentSettingsPatch>,
) -> Result<Json<SettingsValidationResponse>> {
    let current = s
        .repository
        .load_persistent_settings()
        .await?
        .unwrap_or_else(|| PersistentSettings::from_config(&s.manager.config()));
    let issues = settings_validation_issues(&current, &s.manager.config(), &patch);
    Ok(Json(SettingsValidationResponse {
        valid: issues.is_empty(),
        restart_required: settings_patch_requires_restart(&patch),
        issues,
    }))
}

pub(super) async fn patch_settings(
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
        s.manager.apply_live_settings(&values)?;
        s.protection
            .reconfigure(
                values.api_max_concurrent_requests,
                values.api_rate_limit_per_minute,
                values.api_rate_limit_burst,
                Duration::from_secs(values.api_request_timeout_secs),
            )
            .await;
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

pub(super) async fn reset_settings(State(s): State<ApiState>) -> Result<Json<SettingsResponse>> {
    let result: Result<SettingsResponse> = async {
        s.repository.reset_persistent_settings().await?;
        let values = PersistentSettings::from_config(&s.base_config);
        s.manager.apply_live_settings(&values)?;
        s.protection
            .reconfigure(
                values.api_max_concurrent_requests,
                values.api_rate_limit_per_minute,
                values.api_rate_limit_burst,
                Duration::from_secs(values.api_request_timeout_secs),
            )
            .await;
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

pub(super) async fn list_host_profiles(
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

pub(super) async fn reset_host_profiles(
    State(s): State<ApiState>,
) -> Result<Json<serde_json::Value>> {
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
pub(super) async fn events(
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

pub(super) fn to_sse_event(event: crate::core::events::SequencedEvent) -> SseEvent {
    SseEvent::default()
        .id(event.sequence.to_string())
        .json_data(event)
        .unwrap_or_else(|_| SseEvent::default().data("{}"))
}

#[cfg(test)]
mod settings_validation_tests {
    use clap::Parser as _;

    use super::*;

    #[test]
    fn every_failing_field_is_reported_with_isolated_blame() {
        let config = crate::config::Config::try_parse_from([
            "ravyn",
            "--data-dir",
            "data",
            "--download-dir",
            "downloads",
        ])
        .unwrap();
        let current = PersistentSettings::from_config(&config);

        let valid_patch = PersistentSettingsPatch {
            max_retries: Some(5),
            ..Default::default()
        };
        assert!(settings_validation_issues(&current, &config, &valid_patch).is_empty());

        let invalid_patch = PersistentSettingsPatch {
            max_segments: Some(0),
            max_torrent_mib: Some(1_000_000),
            max_retries: Some(5),
            ..Default::default()
        };
        let issues = settings_validation_issues(&current, &config, &invalid_patch);
        let fields = issues.iter().map(|issue| issue.field).collect::<Vec<_>>();
        assert!(fields.contains(&"max_segments"), "{fields:?}");
        assert!(fields.contains(&"max_torrent_mib"), "{fields:?}");
        assert!(!fields.contains(&"max_retries"), "{fields:?}");
        assert!(issues.iter().all(|issue| !issue.message.trim().is_empty()));
    }
}
