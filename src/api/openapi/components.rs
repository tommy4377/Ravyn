//! Reusable OpenAPI component schemas.

use serde_json::{Value, json};

pub(super) fn schemas() -> Value {
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
        "LibraryEntry": {
            "type":"object",
            "required":["id","source_url","mirrors","path","filename","category","media_metadata","torrent_metadata","tags","state","imported","downloaded_at","created_at","updated_at"],
            "properties": {
                "id":{"type":"string","format":"uuid"},
                "job_id":{"type":["string","null"],"format":"uuid"},
                "source_url":{"type":"string"},
                "mirrors":{"type":"array","items":{"type":"string"}},
                "sha256":{"type":["string","null"],"pattern":"^[0-9a-fA-F]{64}$"},
                "size_bytes":{"type":["integer","null"],"minimum":0},
                "path":{"type":"string"},
                "filename":{"type":"string"},
                "category":{"type":"string","enum":["downloads","videos","music","documents","images","archives","torrents","playlists","temporary","other"]},
                "mime_type":{"type":["string","null"]},
                "media_metadata":{},
                "torrent_metadata":{},
                "tags":{"type":"array","items":{"type":"string"}},
                "trust":{"oneOf":[{"$ref":"#/components/schemas/TrustReport"},{"type":"null"}]},
                "state":{"type":"string","enum":["active","trashed","missing"]},
                "trash_path":{"type":["string","null"]},
                "imported":{"type":"boolean"},
                "downloaded_at":{"type":"string","format":"date-time"},
                "created_at":{"type":"string","format":"date-time"},
                "updated_at":{"type":"string","format":"date-time"}
            }
        },
        "LibraryPage": {
            "allOf":[
                {"$ref":"#/components/schemas/GenericPage"},
                {"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/LibraryEntry"}}}}
            ]
        },
        "DuplicateCandidate": {
            "type":"object",
            "required":["entry","matches"],
            "properties": {
                "entry":{"$ref":"#/components/schemas/LibraryEntry"},
                "matches":{"type":"array","items":{"type":"string","enum":["sha256","size_bytes","filename"]}}
            }
        },
        "DuplicateCandidateList": {
            "type":"array",
            "items":{"$ref":"#/components/schemas/DuplicateCandidate"}
        },
        "DeleteLibraryResult": {
            "type":"object",
            "required":["purged","entry"],
            "properties": {
                "purged":{"type":"boolean"},
                "entry":{"oneOf":[{"$ref":"#/components/schemas/LibraryEntry"},{"type":"null"}]}
            }
        },
        "TemplatePreview": {
            "type":"object",
            "required":["rendered","missing_variables"],
            "properties": {
                "rendered":{"type":"string"},
                "missing_variables":{"type":"array","items":{"type":"string"}}
            }
        },
        "LibraryImportStatus": {
            "type":"object",
            "required":["running","scanned","imported","duplicates","skipped","errors"],
            "properties": {
                "run_id":{"type":["string","null"],"format":"uuid"},
                "running":{"type":"boolean"},
                "root":{"type":["string","null"]},
                "scanned":{"type":"integer","minimum":0},
                "imported":{"type":"integer","minimum":0},
                "duplicates":{"type":"integer","minimum":0},
                "skipped":{"type":"integer","minimum":0},
                "errors":{"type":"array","items":{"type":"string"}},
                "started_at":{"type":["string","null"],"format":"date-time"},
                "completed_at":{"type":["string","null"],"format":"date-time"}
            }
        },
        "VerifyLibraryReport": {
            "type":"object",
            "required":["checked","missing"],
            "properties": {
                "checked":{"type":"integer","minimum":0},
                "missing":{"type":"integer","minimum":0}
            }
        },
        "RelocationReport": {
            "type":"object",
            "required":["scanned","repaired","unmatched"],
            "properties": {
                "scanned":{"type":"integer","minimum":0},
                "repaired":{"type":"integer","minimum":0},
                "unmatched":{"type":"integer","minimum":0}
            }
        },
        "DownloadPreset": {
            "type":"object",
            "required":["id","name","payload","created_at","updated_at"],
            "properties": {
                "id":{"type":"string","format":"uuid"},
                "name":{"type":"string"},
                "payload":{"type":"object","additionalProperties":true},
                "created_at":{"type":"string","format":"date-time"},
                "updated_at":{"type":"string","format":"date-time"}
            }
        },
        "DownloadPresetList": {
            "type":"array",
            "items":{"$ref":"#/components/schemas/DownloadPreset"}
        },
        "BasketItem": {
            "type":"object",
            "required":["id","position","request","created_at","updated_at"],
            "properties": {
                "id":{"type":"string","format":"uuid"},
                "position":{"type":"integer","minimum":0},
                "request":{"type":"object","additionalProperties":true},
                "preset_id":{"type":["string","null"],"format":"uuid"},
                "created_at":{"type":"string","format":"date-time"},
                "updated_at":{"type":"string","format":"date-time"}
            }
        },
        "BasketItemList": {
            "type":"array",
            "items":{"$ref":"#/components/schemas/BasketItem"}
        },
        "BasketStartResult": {
            "type":"object",
            "required":["started","failed","items"],
            "properties": {
                "started":{"type":"integer","minimum":0},
                "failed":{"type":"integer","minimum":0},
                "items":{"type":"array","items":{"type":"object","additionalProperties":true}}
            }
        },
        "UserProfile": {
            "type":"object",
            "required":["id","name","settings_patch","active","created_at","updated_at"],
            "properties": {
                "id":{"type":"string","format":"uuid"},
                "name":{"type":"string"},
                "settings_patch":{"type":"object","additionalProperties":true},
                "default_preset_id":{"type":["string","null"],"format":"uuid"},
                "active":{"type":"boolean"},
                "created_at":{"type":"string","format":"date-time"},
                "updated_at":{"type":"string","format":"date-time"}
            }
        },
        "UserProfileList": {
            "type":"array",
            "items":{"$ref":"#/components/schemas/UserProfile"}
        },
        "ActivateProfileResponse": {
            "type":"object",
            "required":["profile","restart_required"],
            "properties": {
                "profile":{"$ref":"#/components/schemas/UserProfile"},
                "restart_required":{"type":"boolean"}
            }
        },
        "TrustFactor": {
            "type":"object",
            "required":["code","label","points","satisfied","explanation"],
            "properties": {
                "code":{"type":"string"},
                "label":{"type":"string"},
                "points":{"type":"integer"},
                "satisfied":{"type":"boolean"},
                "explanation":{"type":"string"}
            }
        },
        "TrustReport": {
            "type":"object",
            "required":["score","level","factors"],
            "properties": {
                "score":{"type":"integer","minimum":0,"maximum":100},
                "level":{"type":"string"},
                "factors":{"type":"array","items":{"$ref":"#/components/schemas/TrustFactor"}}
            }
        },
        "CleanupPolicies": {
            "type":"object",
            "required":["temporary_max_age_days","trash_retention_days","log_retention_days","cache_retention_days"],
            "properties": {
                "temporary_max_age_days":{"type":"integer","minimum":1,"maximum":3650},
                "trash_retention_days":{"type":"integer","minimum":1,"maximum":3650},
                "log_retention_days":{"type":"integer","minimum":1,"maximum":3650},
                "cache_retention_days":{"type":"integer","minimum":1,"maximum":3650}
            }
        },
        "CleanupReport": {
            "type":"object",
            "required":["temporary_files_removed","temporary_bytes_removed","cache_files_removed","cache_bytes_removed","trash_entries_purged","job_logs_removed"],
            "properties": {
                "temporary_files_removed":{"type":"integer","minimum":0},
                "temporary_bytes_removed":{"type":"integer","minimum":0},
                "cache_files_removed":{"type":"integer","minimum":0},
                "cache_bytes_removed":{"type":"integer","minimum":0},
                "trash_entries_purged":{"type":"integer","minimum":0},
                "job_logs_removed":{"type":"integer","minimum":0}
            }
        },
        "PersonalStatistics": {
            "type":"object",
            "required":["total_files","total_downloaded_bytes","active_storage_bytes","trashed_storage_bytes","average_speed_bps","saved_bandwidth_bytes","duplicate_avoidance_count","categories","monthly_activity","yearly_activity"],
            "properties": {
                "total_files":{"type":"integer","minimum":0},
                "total_downloaded_bytes":{"type":"integer","minimum":0},
                "active_storage_bytes":{"type":"integer","minimum":0},
                "trashed_storage_bytes":{"type":"integer","minimum":0},
                "average_speed_bps":{"type":"integer","minimum":0},
                "saved_bandwidth_bytes":{"type":"integer","minimum":0},
                "duplicate_avoidance_count":{"type":"integer","minimum":0},
                "categories":{"type":"object","additionalProperties":{"type":"object","additionalProperties":true}},
                "monthly_activity":{"type":"array","items":{"type":"object","additionalProperties":true}},
                "yearly_activity":{"type":"array","items":{"type":"object","additionalProperties":true}}
            }
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
        },
        "ComponentOverview": {
            "type": "object",
            "required": ["setup_profile", "features", "components", "platform", "manifest_provider"],
            "properties": {
                "setup_profile": {"type": "string", "enum": ["minimal", "recommended", "full", "custom"]},
                "features": {"type": "array", "items": {"$ref": "#/components/schemas/FeatureStatus"}},
                "components": {"type": "array", "items": {"$ref": "#/components/schemas/ComponentStatus"}},
                "platform": {"type": "string"},
                "manifest_provider": {"type": "string"}
            }
        },
        "FeatureStatus": {
            "type": "object",
            "required": ["feature", "enabled", "satisfied", "required_components"],
            "properties": {
                "feature": {"type": "string", "enum": ["standard_downloads", "video_extraction", "media_merging", "torrent_support", "archive_extraction"]},
                "enabled": {"type": "boolean"},
                "satisfied": {"type": "boolean"},
                "required_components": {"type": "array", "items": {"type": "string", "enum": ["ytdlp", "ffmpeg", "rqbit", "seven_zip"]}}
            }
        },
        "ComponentStatus": {
            "type": "object",
            "required": ["component", "state", "enabled"],
            "properties": {
                "component": {"type": "string", "enum": ["ytdlp", "ffmpeg", "rqbit", "seven_zip"]},
                "state": {"type": "string", "enum": ["not_installed", "queued", "downloading", "verifying", "installing", "installed", "update_available", "failed", "unsupported", "custom_path"]},
                "enabled": {"type": "boolean"},
                "managed_version": {"type": ["string", "null"]},
                "managed_path": {"type": ["string", "null"]},
                "custom_path": {"type": ["string", "null"]},
                "effective_path": {"type": ["string", "null"]},
                "error_message": {"type": ["string", "null"]},
                "last_checked_at": {"type": ["string", "null"], "format": "date-time"},
                "install_started_at": {"type": ["string", "null"], "format": "date-time"},
                "install_completed_at": {"type": ["string", "null"], "format": "date-time"}
            }
        },
        "SaveFeatureSelections": {
            "type": "object",
            "required": ["setup_profile", "features"],
            "properties": {
                "setup_profile": {"type": "string", "enum": ["minimal", "recommended", "full", "custom"]},
                "features": {"type": "array", "items": {"$ref": "#/components/schemas/FeatureSelection"}}
            }
        },
        "FeatureSelection": {
            "type": "object",
            "required": ["feature", "enabled"],
            "properties": {
                "feature": {"type": "string", "enum": ["standard_downloads", "video_extraction", "media_merging", "torrent_support", "archive_extraction"]},
                "enabled": {"type": "boolean"}
            }
        },
        "SetupState": {
            "type": "object",
            "required": ["completed", "app_version", "platform", "features_selected", "library_prepared", "data_dir"],
            "properties": {
                "completed": {"type": "boolean"},
                "completed_at": {"type": ["string", "null"], "format": "date-time"},
                "completed_app_version": {"type": ["string", "null"]},
                "app_version": {"type": "string"},
                "platform": {"type": "string"},
                "setup_profile": {"type": ["string", "null"], "enum": ["minimal", "recommended", "full", "custom", null]},
                "features_selected": {"type": "boolean"},
                "library_root": {"type": ["string", "null"]},
                "library_prepared": {"type": "boolean"},
                "data_dir": {"type": "string"}
            }
        },
        "PrepareLibraryResult": {
            "type": "object",
            "required": ["path", "existed", "directories", "restart_required"],
            "properties": {
                "path": {"type": "string"},
                "existed": {"type": "boolean"},
                "directories": {"type": "array", "items": {"type": "string"}},
                "available_bytes": {"type": ["integer", "null"], "minimum": 0},
                "restart_required": {"type": "boolean"}
            }
        },
        "InstallComponentRequest": {
            "type": "object",
            "properties": {
                "force": {"type": "boolean", "default": false}
            }
        }
    })
}
