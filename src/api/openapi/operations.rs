//! The declarative operation table that must stay in exact parity
//! with the Axum router.

use super::OperationSpec;

pub(super) const OPERATIONS: &[OperationSpec] = &[
    op!("get", "/health", "health", "System", "Basic liveness check"),
    op!(
        "get",
        "/health/live",
        "liveness",
        "System",
        "Process liveness check"
    ),
    op!(
        "get",
        "/health/ready",
        "readiness",
        "System",
        "Backend readiness check"
    ),
    op!(
        "get",
        "/metrics",
        "metrics",
        "System",
        "OpenMetrics endpoint"
    ),
    op!(
        "get",
        "/openapi.json",
        "openapiDocument",
        "System",
        "OpenAPI document"
    ),
    op!(
        "get",
        "/v1/jobs",
        "listJobs",
        "Jobs",
        "List jobs",
        "200",
        true,
        false,
        Some("JobPage")
    ),
    op!(
        "post",
        "/v1/jobs",
        "createJob",
        "Jobs",
        "Create a job",
        "201",
        false,
        true,
        Some("Job")
    ),
    op!(
        "post",
        "/v1/jobs/metalink",
        "createMetalinkJob",
        "Jobs",
        "Create an HTTP job from a bounded Metalink v4 document",
        "201",
        false,
        true,
        Some("Job")
    ),
    op!(
        "post",
        "/v1/jobs/batch",
        "createJobBatch",
        "Jobs",
        "Create multiple jobs",
        "207",
        false,
        true,
        None
    ),
    op!(
        "post",
        "/v1/jobs/actions",
        "applyBulkJobAction",
        "Jobs",
        "Apply an action to multiple jobs",
        "200",
        false,
        true,
        None
    ),
    op!(
        "post",
        "/v1/jobs/import-text",
        "importTextJobs",
        "Jobs",
        "Import jobs from text",
        "207",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/jobs/{id}",
        "getJob",
        "Jobs",
        "Get a job",
        "200",
        false,
        false,
        Some("Job")
    ),
    op!(
        "patch",
        "/v1/jobs/{id}",
        "updateJob",
        "Jobs",
        "Update a job",
        "200",
        false,
        true,
        Some("Job")
    ),
    op!(
        "delete",
        "/v1/jobs/{id}",
        "deleteJob",
        "Jobs",
        "Delete a job",
        "204",
        false,
        false,
        None
    ),
    op!(
        "get",
        "/v1/jobs/{id}/outputs",
        "listJobOutputs",
        "Jobs",
        "List job output artifacts",
        "200",
        true,
        false,
        Some("JobOutputPage")
    ),
    op!(
        "get",
        "/v1/jobs/{id}/media-items",
        "listMediaItems",
        "Media",
        "List persisted media or playlist items",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "get",
        "/v1/jobs/{id}/media-items/{item_id}/outputs",
        "listMediaItemOutputs",
        "Media",
        "List output artifacts linked to one media item",
        "200",
        false,
        false,
        None
    ),
    op!(
        "get",
        "/v1/jobs/{id}/media-summary",
        "mediaItemSummary",
        "Media",
        "Get aggregate state for a media or playlist job",
        "200",
        false,
        false,
        None
    ),
    op!(
        "post",
        "/v1/jobs/{id}/media-items/{item_id}/retry",
        "retryMediaItem",
        "Media",
        "Create a retry job for one media item",
        "201",
        false,
        false,
        Some("Job")
    ),
    op!(
        "post",
        "/v1/jobs/{id}/media-items/retry-failed",
        "retryFailedMediaItems",
        "Media",
        "Retry failed media items in a bounded batch",
        "200",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/jobs/{id}/segments",
        "listJobSegments",
        "Jobs",
        "List persisted HTTP segments",
        "200",
        true,
        false,
        Some("SegmentPage")
    ),
    op!(
        "get",
        "/v1/jobs/{id}/actions",
        "listJobActions",
        "Jobs",
        "List post-processing journal entries",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "get",
        "/v1/jobs/{id}/logs",
        "listJobLogs",
        "Jobs",
        "List job logs",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/jobs/{id}/pause",
        "pauseJob",
        "Jobs",
        "Pause a job",
        "204",
        false,
        false,
        None
    ),
    op!(
        "post",
        "/v1/jobs/{id}/resume",
        "resumeJob",
        "Jobs",
        "Resume a job",
        "204",
        false,
        false,
        None
    ),
    op!(
        "post",
        "/v1/jobs/{id}/cancel",
        "cancelJob",
        "Jobs",
        "Cancel a job",
        "204",
        false,
        false,
        None
    ),
    op!(
        "post",
        "/v1/jobs/{id}/retry",
        "retryJob",
        "Jobs",
        "Retry a job",
        "204",
        false,
        false,
        None
    ),
    op!(
        "post",
        "/v1/media/probe",
        "probeMedia",
        "Media",
        "Probe media formats",
        "200",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/media/archive",
        "listMediaArchive",
        "Media",
        "List persistent downloaded media identities",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "delete",
        "/v1/media/archive",
        "deleteMediaArchiveEntry",
        "Media",
        "Delete one persistent media identity",
        "204",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/system/dependencies",
        "listDependencies",
        "System",
        "Inspect external dependencies"
    ),
    op!(
        "get",
        "/v1/system/capabilities",
        "systemCapabilities",
        "System",
        "Inspect backend capabilities"
    ),
    op!(
        "get",
        "/v1/settings",
        "getSettings",
        "Settings",
        "Get persistent settings"
    ),
    op!(
        "patch",
        "/v1/settings",
        "updateSettings",
        "Settings",
        "Update persistent settings",
        "200",
        false,
        true,
        None
    ),
    op!(
        "post",
        "/v1/settings/reset",
        "resetSettings",
        "Settings",
        "Reset persistent settings"
    ),
    op!(
        "get",
        "/v1/system/database",
        "databaseStatus",
        "Database",
        "Inspect database integrity"
    ),
    op!(
        "post",
        "/v1/system/database/backup",
        "createDatabaseBackup",
        "Database",
        "Create an online database backup",
        "201",
        false,
        false,
        None
    ),
    op!(
        "get",
        "/v1/system/database/backups",
        "listDatabaseBackups",
        "Database",
        "List database backups",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/system/database/backups/{name}/verify",
        "verifyDatabaseBackup",
        "Database",
        "Verify a database backup"
    ),
    op!(
        "post",
        "/v1/system/database/backups/{name}/restore",
        "scheduleDatabaseRestore",
        "Database",
        "Stage a backup for restore on restart",
        "202",
        false,
        false,
        Some("RestoreStatus")
    ),
    op!(
        "get",
        "/v1/system/database/restore",
        "databaseRestoreStatus",
        "Database",
        "Get pending and last restore status",
        "200",
        false,
        false,
        Some("RestoreStatus")
    ),
    op!(
        "delete",
        "/v1/system/database/restore",
        "cancelDatabaseRestore",
        "Database",
        "Cancel a pending restore",
        "200",
        false,
        false,
        Some("RestoreStatus")
    ),
    op!(
        "post",
        "/v1/system/maintenance",
        "runMaintenance",
        "Database",
        "Run retention maintenance",
        "200",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/audit",
        "listAudit",
        "Audit",
        "List audit records",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "get",
        "/v1/secrets",
        "listSecrets",
        "Secrets",
        "List secret references",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/secrets",
        "putSecret",
        "Secrets",
        "Store or update a platform secret",
        "201",
        false,
        true,
        None
    ),
    op!(
        "delete",
        "/v1/secrets/{id}",
        "deleteSecret",
        "Secrets",
        "Delete a secret reference",
        "204",
        false,
        false,
        None
    ),
    op!(
        "get",
        "/v1/system/hosts",
        "listHostProfiles",
        "System",
        "List learned host profiles",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/system/hosts/reset",
        "resetHostProfiles",
        "System",
        "Reset learned host profiles"
    ),
    op!(
        "post",
        "/v1/torrents/probe",
        "probeTorrent",
        "Torrents",
        "Probe a torrent or magnet",
        "200",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/torrents",
        "listManagedTorrents",
        "Torrents",
        "List torrents managed by Ravyn",
        "200",
        true,
        false,
        Some("TorrentPage")
    ),
    op!(
        "get",
        "/v1/torrents/engine",
        "listEngineTorrents",
        "Torrents",
        "List torrents known by rqbit",
        "200",
        false,
        false,
        Some("TorrentEngineList")
    ),
    op!(
        "get",
        "/v1/torrents/engine/stats",
        "torrentEngineStats",
        "Torrents",
        "Get rqbit engine statistics",
        "200",
        false,
        false,
        Some("TorrentGlobalStats")
    ),
    op!(
        "get",
        "/v1/torrents/dht/stats",
        "torrentDhtStats",
        "Torrents",
        "Get DHT statistics"
    ),
    op!(
        "get",
        "/v1/torrents/dht/table",
        "torrentDhtTable",
        "Torrents",
        "Get DHT routing table"
    ),
    op!(
        "get",
        "/v1/torrents/{id}",
        "torrentDetails",
        "Torrents",
        "Get torrent details",
        "200",
        false,
        false,
        Some("TorrentDetails")
    ),
    op!(
        "get",
        "/v1/torrents/{id}/stats",
        "torrentStats",
        "Torrents",
        "Get torrent statistics"
    ),
    op!(
        "get",
        "/v1/torrents/{id}/seeding",
        "torrentSeedingState",
        "Torrents",
        "Get persisted torrent seeding policy state",
        "200",
        false,
        false,
        None
    ),
    op!(
        "get",
        "/v1/torrents/{id}/peers",
        "torrentPeers",
        "Torrents",
        "List torrent peers",
        "200",
        false,
        false,
        Some("TorrentPeerStats")
    ),
    op!(
        "post",
        "/v1/torrents/{id}/peers",
        "addTorrentPeers",
        "Torrents",
        "Add torrent peers",
        "204",
        false,
        true,
        None
    ),
    op!(
        "post",
        "/v1/torrents/{id}/files",
        "updateTorrentFiles",
        "Torrents",
        "Update torrent file selection",
        "204",
        false,
        true,
        None
    ),
    op!(
        "post",
        "/v1/torrents/{id}/remove",
        "removeTorrent",
        "Torrents",
        "Remove a torrent",
        "204",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/rules",
        "listRules",
        "Rules",
        "List rules",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/rules",
        "createRule",
        "Rules",
        "Create a rule",
        "201",
        false,
        true,
        None
    ),
    op!("get", "/v1/rules/{id}", "getRule", "Rules", "Get a rule"),
    op!(
        "put",
        "/v1/rules/{id}",
        "updateRule",
        "Rules",
        "Replace a rule",
        "200",
        false,
        true,
        None
    ),
    op!(
        "delete",
        "/v1/rules/{id}",
        "deleteRule",
        "Rules",
        "Delete a rule",
        "204",
        false,
        false,
        None
    ),
    op!(
        "get",
        "/v1/tags",
        "listTags",
        "Tags",
        "List tags",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "delete",
        "/v1/tags/{id}",
        "deleteTag",
        "Tags",
        "Delete a tag",
        "204",
        false,
        false,
        None
    ),
    op!(
        "get",
        "/v1/jobs/{id}/tags",
        "listJobTags",
        "Tags",
        "List job tags"
    ),
    op!(
        "put",
        "/v1/jobs/{id}/tags",
        "replaceJobTags",
        "Tags",
        "Replace job tags",
        "200",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/pages",
        "listPages",
        "Pages",
        "List monitored pages",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/pages/resources",
        "listPageResources",
        "Pages",
        "List resources remembered for a page",
        "200",
        false,
        true,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/pages/history/clear",
        "clearPageHistory",
        "Pages",
        "Clear page resource history",
        "200",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/schedules",
        "listSchedules",
        "Schedules",
        "List schedules",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/schedules",
        "createSchedule",
        "Schedules",
        "Create a schedule",
        "201",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/schedules/{id}",
        "getSchedule",
        "Schedules",
        "Get a schedule"
    ),
    op!(
        "put",
        "/v1/schedules/{id}",
        "updateSchedule",
        "Schedules",
        "Replace a schedule",
        "200",
        false,
        true,
        None
    ),
    op!(
        "delete",
        "/v1/schedules/{id}",
        "deleteSchedule",
        "Schedules",
        "Delete a schedule",
        "204",
        false,
        false,
        None
    ),
    op!(
        "get",
        "/v1/schedules/{id}/executions",
        "listScheduleExecutions",
        "Schedules",
        "List schedule executions",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/schedules/{id}/run-now",
        "runScheduleNow",
        "Schedules",
        "Run a schedule immediately",
        "201",
        false,
        false,
        None
    ),
    op!(
        "post",
        "/v1/schedules/{id}/enable",
        "enableSchedule",
        "Schedules",
        "Enable a schedule"
    ),
    op!(
        "post",
        "/v1/schedules/{id}/disable",
        "disableSchedule",
        "Schedules",
        "Disable a schedule"
    ),
    op!(
        "get",
        "/v1/schedule-executions/{id}",
        "getScheduleExecution",
        "Schedules",
        "Get a schedule execution"
    ),
    op!(
        "post",
        "/v1/schedule-executions/{id}/cancel",
        "cancelScheduleExecution",
        "Schedules",
        "Cancel a running schedule execution"
    ),
    op!(
        "get",
        "/v1/browser/tokens",
        "listBrowserTokens",
        "Browser",
        "List browser bridge tokens",
        "200",
        true,
        false,
        Some("GenericPage")
    ),
    op!(
        "post",
        "/v1/browser/tokens",
        "createBrowserToken",
        "Browser",
        "Create a browser bridge token",
        "201",
        false,
        true,
        None
    ),
    op!(
        "delete",
        "/v1/browser/tokens/{id}",
        "revokeBrowserToken",
        "Browser",
        "Revoke a browser bridge token",
        "204",
        false,
        false,
        None
    ),
    op!(
        "post",
        "/v1/browser/sniff",
        "sniffPage",
        "Browser",
        "Sniff static page resources",
        "200",
        false,
        true,
        None
    ),
    op!(
        "post",
        "/v1/browser/import",
        "importBrowserResources",
        "Browser",
        "Import browser-observed resources",
        "207",
        false,
        true,
        None
    ),
    op!(
        "get",
        "/v1/events",
        "streamEvents",
        "Events",
        "Stream replayable server-sent events"
    ),
];
