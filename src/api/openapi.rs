use serde_json::{Map, Value, json};

struct OperationSpec {
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    tag: &'static str,
    summary: &'static str,
    success_status: &'static str,
    paginated: bool,
    request_body: bool,
    response_schema: Option<&'static str>,
}

macro_rules! op {
    ($method:literal, $path:literal, $id:literal, $tag:literal, $summary:literal) => {
        OperationSpec {
            method: $method,
            path: $path,
            operation_id: $id,
            tag: $tag,
            summary: $summary,
            success_status: "200",
            paginated: false,
            request_body: false,
            response_schema: None,
        }
    };
    ($method:literal, $path:literal, $id:literal, $tag:literal, $summary:literal, $status:literal, $paginated:expr, $body:expr, $schema:expr) => {
        OperationSpec {
            method: $method,
            path: $path,
            operation_id: $id,
            tag: $tag,
            summary: $summary,
            success_status: $status,
            paginated: $paginated,
            request_body: $body,
            response_schema: $schema,
        }
    };
}

const OPERATIONS: &[OperationSpec] = &[
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

pub fn document() -> Value {
    let mut paths = Map::new();
    for spec in OPERATIONS {
        let operation = build_operation(spec);
        let path_item = paths
            .entry(spec.path.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(path_item) = path_item.as_object_mut() {
            path_item.insert(spec.method.to_owned(), operation);
        }
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Ravyn Backend API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Versioned administrative API for the Ravyn download manager backend. Collection cursors are opaque and must not be constructed by clients."
        },
        "servers": [{ "url": "/" }],
        "tags": [
            {"name":"Jobs"},{"name":"Media"},{"name":"Torrents"},{"name":"Rules"},
            {"name":"Tags"},{"name":"Schedules"},{"name":"Pages"},{"name":"Browser"},
            {"name":"Settings"},{"name":"Secrets"},{"name":"Database"},{"name":"Audit"},
            {"name":"Events"},{"name":"System"}
        ],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "bearerAuth": { "type": "http", "scheme": "bearer" }
            },
            "parameters": {
                "Cursor": {
                    "name": "cursor", "in": "query", "required": false,
                    "description": "Opaque cursor returned by the previous response.",
                    "schema": {"type":"string"}
                },
                "Limit": {
                    "name": "limit", "in": "query", "required": false,
                    "schema": {"type":"integer","minimum":1,"maximum":200,"default":50}
                },
                "Search": {
                    "name": "search", "in": "query", "required": false,
                    "schema": {"type":"string","maxLength":256}
                }
            },
            "schemas": schemas()
        },
        "security": [{ "bearerAuth": [] }]
    })
}

fn build_operation(spec: &OperationSpec) -> Value {
    let mut operation = Map::new();
    operation.insert("operationId".into(), json!(spec.operation_id));
    operation.insert("tags".into(), json!([spec.tag]));
    operation.insert("summary".into(), json!(spec.summary));

    let mut parameters = path_parameters(spec.path);
    if spec.paginated {
        parameters.extend([
            json!({"$ref":"#/components/parameters/Cursor"}),
            json!({"$ref":"#/components/parameters/Limit"}),
            json!({"$ref":"#/components/parameters/Search"}),
        ]);
    }
    if !parameters.is_empty() {
        operation.insert("parameters".into(), Value::Array(parameters));
    }

    if spec.request_body {
        operation.insert(
            "requestBody".into(),
            json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": {"type":"object","additionalProperties":true}
                    }
                }
            }),
        );
    }

    let success_content = spec.response_schema.map(|schema| {
        json!({
            "application/json": {
                "schema": {"$ref": format!("#/components/schemas/{schema}")}
            }
        })
    });
    let mut success = Map::new();
    success.insert("description".into(), json!("Success"));
    if let Some(content) = success_content {
        success.insert("content".into(), content);
    }
    let mut responses = Map::new();
    responses.insert(spec.success_status.into(), Value::Object(success));
    for (status, description) in [
        ("400", "Invalid request"),
        ("401", "Authentication required"),
        ("404", "Resource not found"),
        ("409", "State conflict"),
        ("500", "Internal backend error"),
        ("503", "Temporarily unavailable"),
    ] {
        responses.insert(status.into(), error_response(description));
    }
    operation.insert("responses".into(), Value::Object(responses));
    Value::Object(operation)
}

fn path_parameters(path: &str) -> Vec<Value> {
    path.split('/')
        .filter_map(|part| {
            part.strip_prefix('{')
                .and_then(|part| part.strip_suffix('}'))
        })
        .map(|name| {
            json!({
                "name": name,
                "in": "path",
                "required": true,
                "schema": if name == "id" {
                    json!({"type":"string","format":"uuid"})
                } else {
                    json!({"type":"string"})
                }
            })
        })
        .collect()
}

fn error_response(description: &str) -> Value {
    json!({
        "description": description,
        "content": {
            "application/json": {
                "schema": {"$ref":"#/components/schemas/ApiError"}
            }
        }
    })
}

fn schemas() -> Value {
    json!({
        "ApiError": {
            "type": "object",
            "required": ["code", "message", "request_id", "retryable", "details"],
            "properties": {
                "code": {"type":"string"},
                "message": {"type":"string"},
                "request_id": {"type":"string","format":"uuid"},
                "retryable": {"type":"boolean"},
                "details": {"type":"object","additionalProperties":true}
            }
        },
        "FfmpegPreset": {
            "type":"string",
            "enum":["video-copy","video-h264","video-h265","video-av1","audio-mp3","audio-aac","audio-opus","audio-flac","image-avif","image-webp"]
        },
        "ConvertMediaPostAction": {
            "type":"object",
            "required":["type","extension","delete_original"],
            "properties": {
                "type":{"const":"convert_media"},
                "extension":{"type":"string","minLength":1,"maxLength":16},
                "preset":{"oneOf":[{"$ref":"#/components/schemas/FfmpegPreset"},{"type":"null"}]},
                "arguments":{"type":"array","maxItems":128,"items":{"type":"string","maxLength":4096}},
                "unsafe_arguments":{"type":"boolean","default":false,"description":"Requires the process-wide --allow-unsafe-ffmpeg opt-in."},
                "delete_original":{"type":"boolean"}
            }
        },
        "Job": {
            "type":"object",
            "required":["id","kind","source","destination","status","priority","downloaded_bytes","transfer_mode","created_at","updated_at"],
            "properties": {
                "id":{"type":"string","format":"uuid"},
                "kind":{"type":"string","enum":["http","media","torrent"]},
                "source":{"type":"string"},
                "destination":{"type":"string"},
                "filename":{"type":["string","null"]},
                "status":{"type":"string","enum":["queued","probing","downloading","paused","verifying","post_processing","completed","partial","failed","cancelled","seeding"]},
                "priority":{"type":"integer"},
                "total_bytes":{"type":["integer","null"]},
                "downloaded_bytes":{"type":"integer"},
                "speed_limit_bps":{"type":["integer","null"]},
                "error":{"type":["string","null"]},
                "transfer_mode":{"type":"string"},
                "created_at":{"type":"string","format":"date-time"},
                "updated_at":{"type":"string","format":"date-time"}
            }
        },
        "JobOutput": {
            "type":"object",
            "required":["id","job_id","output_type","original_path","current_path","relative_path","state","source_kind","created_at","updated_at"],
            "properties": {
                "id":{"type":"string","format":"uuid"},
                "job_id":{"type":"string","format":"uuid"},
                "output_type":{"type":"string"},
                "original_path":{"type":"string"},
                "current_path":{"type":"string"},
                "relative_path":{"type":"string"},
                "size_bytes":{"type":["integer","null"]},
                "mime_type":{"type":["string","null"]},
                "checksum_algorithm":{"type":["string","null"]},
                "checksum_value":{"type":["string","null"]},
                "state":{"type":"string"},
                "source_kind":{"type":"string"},
                "parent_output_id":{"type":["string","null"],"format":"uuid"},
                "producing_action_index":{"type":["integer","null"]},
                "metadata":{"type":"object","additionalProperties":true},
                "created_at":{"type":"string","format":"date-time"},
                "updated_at":{"type":"string","format":"date-time"}
            }
        },
        "GenericPage": {
            "type":"object",
            "required":["items","next_cursor"],
            "properties": {
                "items":{"type":"array","items":{}},
                "next_cursor":{"type":["string","null"]}
            }
        },
        "JobPage": {
            "allOf":[
                {"$ref":"#/components/schemas/GenericPage"},
                {"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/Job"}}}}
            ]
        },
        "JobOutputPage": {
            "allOf":[
                {"$ref":"#/components/schemas/GenericPage"},
                {"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/JobOutput"}}}}
            ]
        },
        "SegmentRecord": {
            "type":"object",
            "required":["index","start","end","downloaded","completed"],
            "properties": {
                "index":{"type":"integer","minimum":0},
                "start":{"type":"integer","minimum":0},
                "end":{"type":"integer","minimum":0},
                "downloaded":{"type":"integer","minimum":0},
                "completed":{"type":"boolean"}
            }
        },
        "SegmentPage": {
            "allOf":[
                {"$ref":"#/components/schemas/GenericPage"},
                {"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/SegmentRecord"}}}}
            ]
        },
        "TorrentRecord": {
            "type":"object",
            "required":["job_id","torrent_id","state","downloaded_bytes","uploaded_bytes","download_speed_bps","upload_speed_bps","peers_connected","seeders","leechers","raw","updated_at"],
            "properties": {
                "job_id":{"type":"string","format":"uuid"},
                "torrent_id":{"type":"string"},
                "info_hash":{"type":["string","null"]},
                "name":{"type":["string","null"]},
                "state":{"type":"string"},
                "downloaded_bytes":{"type":"integer","minimum":0},
                "uploaded_bytes":{"type":"integer","minimum":0},
                "total_bytes":{"type":["integer","null"],"minimum":0},
                "download_speed_bps":{"type":"integer","minimum":0},
                "upload_speed_bps":{"type":"integer","minimum":0},
                "peers_connected":{"type":"integer","minimum":0},
                "seeders":{"type":"integer","minimum":0},
                "leechers":{"type":"integer","minimum":0},
                "raw":{},
                "updated_at":{"type":"string","format":"date-time"}
            }
        },
        "TorrentPage": {
            "allOf":[
                {"$ref":"#/components/schemas/GenericPage"},
                {"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/TorrentRecord"}}}}
            ]
        },
        "TorrentFile": {
            "type":"object",
            "required":["index","path"],
            "properties": {
                "index":{"type":"integer","minimum":0},
                "path":{"type":"string"},
                "size_bytes":{"type":["integer","null"],"minimum":0}
            }
        },
        "TorrentEngineTorrent": {
            "type":"object",
            "required":["raw"],
            "properties": {
                "torrent_id":{"type":["string","null"]},
                "info_hash":{"type":["string","null"]},
                "name":{"type":["string","null"]},
                "output_folder":{"type":["string","null"]},
                "state":{"type":["string","null"]},
                "downloaded_bytes":{"type":["integer","null"],"minimum":0},
                "total_bytes":{"type":["integer","null"],"minimum":0},
                "progress":{"type":["number","null"],"minimum":0},
                "raw":{}
            }
        },
        "TorrentEngineList": {
            "type":"object",
            "required":["torrents","raw"],
            "properties": {
                "torrents":{"type":"array","items":{"$ref":"#/components/schemas/TorrentEngineTorrent"}},
                "raw":{}
            }
        },
        "TorrentGlobalStats": {
            "type":"object",
            "required":["raw"],
            "properties": {
                "downloaded_bytes":{"type":["integer","null"],"minimum":0},
                "uploaded_bytes":{"type":["integer","null"],"minimum":0},
                "download_speed_bps":{"type":["integer","null"],"minimum":0},
                "upload_speed_bps":{"type":["integer","null"],"minimum":0},
                "active_torrents":{"type":["integer","null"],"minimum":0},
                "raw":{}
            }
        },
        "TorrentDetails": {
            "type":"object",
            "required":["torrent_id","files","raw"],
            "properties": {
                "torrent_id":{"type":"string"},
                "info_hash":{"type":["string","null"]},
                "name":{"type":["string","null"]},
                "state":{"type":["string","null"]},
                "total_bytes":{"type":["integer","null"],"minimum":0},
                "files":{"type":"array","items":{"$ref":"#/components/schemas/TorrentFile"}},
                "raw":{}
            }
        },
        "TorrentPeer": {
            "type":"object",
            "required":["raw"],
            "properties": {
                "address":{"type":["string","null"]},
                "client":{"type":["string","null"]},
                "state":{"type":["string","null"]},
                "downloaded_bytes":{"type":["integer","null"],"minimum":0},
                "uploaded_bytes":{"type":["integer","null"],"minimum":0},
                "download_speed_bps":{"type":["integer","null"],"minimum":0},
                "upload_speed_bps":{"type":["integer","null"],"minimum":0},
                "raw":{}
            }
        },
        "TorrentPeerStats": {
            "type":"object",
            "required":["peers","raw"],
            "properties": {
                "peers":{"type":"array","items":{"$ref":"#/components/schemas/TorrentPeer"}},
                "raw":{}
            }
        },
        "RestoreStatus": {
            "type":"object",
            "required":["pending","last_result","restart_required"],
            "properties": {
                "pending":{"type":["object","null"]},
                "last_result":{"type":["object","null"]},
                "restart_required":{"type":"boolean"}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn operations_are_unique_and_restore_is_documented() {
        let mut seen = HashSet::new();
        for operation in OPERATIONS {
            assert!(seen.insert((operation.method, operation.path)));
        }
        assert!(OPERATIONS.len() > 70);
        assert!(OPERATIONS.iter().any(|operation| {
            operation.path == "/v1/system/database/backups/{name}/restore"
                && operation.method == "post"
        }));
    }

    #[test]
    fn generated_document_has_paths_and_security() {
        let document = document();
        assert_eq!(document["openapi"], "3.1.0");
        assert!(document["paths"].as_object().unwrap().len() > 50);
        assert!(document["components"]["securitySchemes"]["bearerAuth"].is_object());
    }
}
