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
