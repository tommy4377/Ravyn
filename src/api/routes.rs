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
        AuditChainStatus, AuditRecord, JobActionRecord, JobListFilter, JobLogRecord,
        MediaArchiveRecord, MediaItemOutputRecord, MediaItemRecord, PageRecord, PageResourceRecord,
        Repository, RuleInput, Schedule, ScheduleExecutionRecord, SecretReference, TagRecord,
        TorrentRecord, host_profiles::HostProfile,
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
    pub protection: super::ApiProtectionState,
    pub library_import_status: crate::services::library::SharedImportStatus,
    pub provisioning_cancellation: tokio_util::sync::CancellationToken,
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
        .route("/v1/jobs/metalink", post(create_metalink_job))
        .route("/v1/jobs/batch", post(create_batch))
        .route("/v1/jobs/actions", post(apply_job_action))
        .route("/v1/jobs/import-text", post(import_text))
        .route(
            "/v1/jobs/{id}",
            get(get_job).patch(update_job).delete(delete_job),
        )
        .route("/v1/jobs/{id}/outputs", get(list_job_outputs))
        .route("/v1/library", get(list_library))
        .route("/v1/library/duplicates", get(find_library_duplicates))
        .route(
            "/v1/library/{id}",
            get(get_library_entry).delete(delete_library_entry),
        )
        .route("/v1/library/{id}/restore", post(restore_library_entry))
        .route("/v1/templates/preview", post(preview_template))
        .route(
            "/v1/library/import",
            get(library_import_status).post(start_library_import),
        )
        .route("/v1/library/verify", post(verify_library))
        .route("/v1/library/relocate", post(relocate_library))
        .route("/v1/presets", get(list_presets).post(create_preset))
        .route(
            "/v1/presets/{id}",
            get(get_preset).put(update_preset).delete(delete_preset),
        )
        .route(
            "/v1/basket",
            get(list_basket).post(add_basket_item).delete(clear_basket),
        )
        .route(
            "/v1/basket/{id}",
            axum::routing::patch(update_basket_item).delete(delete_basket_item),
        )
        .route("/v1/basket/reorder", post(reorder_basket))
        .route("/v1/basket/start", post(start_basket))
        .route("/v1/profiles", get(list_profiles).post(create_profile))
        .route(
            "/v1/profiles/{id}",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route("/v1/profiles/{id}/activate", post(activate_profile))
        .route("/v1/trust/preview", post(preview_trust))
        .route("/v1/jobs/{id}/trust", get(job_trust))
        .route(
            "/v1/system/cleanup-policies",
            get(get_cleanup_policies).put(put_cleanup_policies),
        )
        .route("/v1/system/cleanup", post(run_library_cleanup))
        .route("/v1/statistics", get(personal_statistics))
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
        .route("/v1/settings/validate", post(validate_settings))
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
        .route("/v1/audit/verify", get(verify_audit_chain))
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
        .route("/v1/rules/preview", post(preview_rules))
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
        .route("/v1/components", get(list_components))
        .route("/v1/components/features", post(save_feature_selections))
        .route(
            "/v1/components/{id}",
            axum::routing::delete(remove_component),
        )
        .route("/v1/components/{id}/install", post(install_component))
        .route("/v1/components/{id}/rollback", post(rollback_component))
        .route("/v1/components/{id}/cancel", post(cancel_installation))
        .route("/v1/events", get(events))
        .with_state(state)
}

fn import_status(result: &ImportResult) -> StatusCode {
    if result.rejected > 0 || result.truncated {
        StatusCode::MULTI_STATUS
    } else {
        StatusCode::CREATED
    }
}

mod automation;
mod browser;
mod components;
mod jobs;
mod library;
mod media;
mod system;
mod torrents;

use self::{
    automation::*, browser::*, components::*, jobs::*, library::*, media::*, system::*, torrents::*,
};
