//! Normalization of raw rqbit payloads into Ravyn's typed torrent
//! contracts, plus engine-id/source validation and compatibility checks.

use super::*;

pub(super) fn validate_engine_id(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(RavynError::Invalid(
            "invalid torrent engine identifier".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_source(source: &str) -> Result<()> {
    let source = source.trim();
    if source.is_empty() {
        return Err(RavynError::Invalid(
            "torrent source must not be empty".into(),
        ));
    }
    if source.starts_with("magnet:")
        || source.starts_with("http://")
        || source.starts_with("https://")
        || source.to_ascii_lowercase().ends_with(".torrent")
    {
        return Ok(());
    }
    Err(RavynError::Invalid(
        "torrent source must be a magnet URI, HTTP(S) URL, or .torrent file".into(),
    ))
}

pub(super) async fn ensure_success(response: Response, operation: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    let message = format!("{operation} failed with HTTP {status}: {}", truncate(&body));
    match status {
        StatusCode::NOT_FOUND => Err(RavynError::NotFound(message)),
        StatusCode::CONFLICT => Err(RavynError::Conflict(message)),
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
            Err(RavynError::Invalid(message))
        }
        _ => Err(RavynError::Protocol(message)),
    }
}

pub(super) async fn decode_json(response: Response, operation: &str) -> Result<Value> {
    let response = ensure_success(response, operation).await?;
    let bytes = response.bytes().await?;
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok(value),
        Err(error) => {
            let text = String::from_utf8_lossy(&bytes).trim().to_owned();
            if text.is_empty() {
                Err(RavynError::Protocol(format!(
                    "{operation} returned invalid JSON: {error}"
                )))
            } else {
                Ok(Value::String(text))
            }
        }
    }
}

pub(super) fn engine_list_from_value(raw: Value) -> TorrentEngineList {
    let items = direct_array(&raw, &["torrents", "items"])
        .or_else(|| raw.as_array())
        .map(|items| {
            items
                .iter()
                .cloned()
                .map(|item| {
                    let progress = direct_f64(&item, &["progress", "fraction", "percent"])
                        .map(|value| if value > 1.0 { value / 100.0 } else { value });
                    TorrentEngineTorrent {
                        torrent_id: direct_string(
                            &item,
                            &["id", "torrent_id", "torrentId", "info_hash", "infohash"],
                        ),
                        info_hash: direct_string(&item, &["info_hash", "infohash", "infoHash"]),
                        name: direct_string(&item, &["name", "title"]),
                        output_folder: direct_string(
                            &item,
                            &["output_folder", "outputFolder", "destination"],
                        ),
                        state: direct_string(&item, &["state", "status"]),
                        downloaded_bytes: direct_u64(
                            &item,
                            &["downloaded_bytes", "progress_bytes", "downloaded"],
                        ),
                        total_bytes: direct_u64(
                            &item,
                            &["total_bytes", "totalBytes", "size_bytes", "length"],
                        ),
                        progress,
                        raw: item,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    TorrentEngineList {
        torrents: items,
        raw,
    }
}

pub(super) fn global_stats_from_value(raw: Value) -> TorrentGlobalStats {
    let aggregate = typed_aggregate(&raw);
    TorrentGlobalStats {
        downloaded_bytes: aggregate
            .as_ref()
            .and_then(|value| value.downloaded_bytes.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| direct_u64(&raw, &["downloaded_bytes", "downloaded"])),
        uploaded_bytes: aggregate
            .as_ref()
            .and_then(|value| value.uploaded_bytes.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| direct_u64(&raw, &["uploaded_bytes", "uploaded"])),
        download_speed_bps: aggregate
            .as_ref()
            .and_then(|value| value.download_speed_bps.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| direct_u64(&raw, &["download_speed_bps", "download_speed"])),
        upload_speed_bps: aggregate
            .as_ref()
            .and_then(|value| value.upload_speed_bps.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| direct_u64(&raw, &["upload_speed_bps", "upload_speed"])),
        active_torrents: direct_u64(
            &raw,
            &[
                "active_torrents",
                "torrents",
                "torrent_count",
                "num_torrents",
            ],
        ),
        raw,
    }
}

pub(super) fn details_from_value(torrent_id: String, raw: Value) -> TorrentDetails {
    TorrentDetails {
        torrent_id,
        info_hash: direct_string(&raw, &["info_hash", "infohash", "infoHash"])
            .or_else(|| find_string(&raw, &["info_hash", "infohash", "infoHash"])),
        name: direct_string(&raw, &["name", "title"])
            .or_else(|| find_string(&raw, &["name", "title"])),
        state: direct_string(&raw, &["state", "status"])
            .or_else(|| find_string(&raw, &["state", "status"])),
        total_bytes: direct_u64(&raw, &["total_bytes", "totalBytes", "size_bytes", "length"])
            .or_else(|| find_u64(&raw, &["total_bytes", "totalBytes", "size_bytes", "length"])),
        files: collect_files(&raw),
        raw,
    }
}

pub(super) fn peer_stats_from_value(raw: Value) -> TorrentPeerStats {
    let peers = direct_array(&raw, &["peers", "peer_stats", "items"])
        .or_else(|| raw.as_array())
        .map(|items| {
            items
                .iter()
                .cloned()
                .map(|item| TorrentPeer {
                    address: direct_string(
                        &item,
                        &["address", "addr", "remote_addr", "ip", "peer_addr"],
                    ),
                    client: direct_string(&item, &["client", "client_name", "user_agent"]),
                    state: direct_string(&item, &["state", "status"]),
                    downloaded_bytes: direct_u64(
                        &item,
                        &["downloaded_bytes", "downloaded", "bytes_downloaded"],
                    ),
                    uploaded_bytes: direct_u64(
                        &item,
                        &["uploaded_bytes", "uploaded", "bytes_uploaded"],
                    ),
                    download_speed_bps: direct_u64(
                        &item,
                        &["download_speed_bps", "download_speed", "downloadSpeed"],
                    ),
                    upload_speed_bps: direct_u64(
                        &item,
                        &["upload_speed_bps", "upload_speed", "uploadSpeed"],
                    ),
                    raw: item,
                })
                .collect()
        })
        .unwrap_or_default();
    TorrentPeerStats { peers, raw }
}

pub(super) fn direct_array<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Vec<Value>> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_array))
}

pub(super) fn direct_string(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    keys.iter().find_map(|key| match object.get(*key) {
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.clone()),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    })
}

pub(super) fn direct_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key).and_then(value_u64))
}

pub(super) fn direct_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key).and_then(value_f64))
}

pub(super) fn probe_from_value(raw: Value) -> TorrentProbe {
    let files = collect_files(&raw);
    TorrentProbe {
        torrent_id: extract_torrent_id(&raw),
        info_hash: find_string(&raw, &["info_hash", "infohash", "infoHash"]),
        name: find_string(&raw, &["name", "title"]),
        total_bytes: find_u64(&raw, &["total_bytes", "totalBytes", "size_bytes", "length"]),
        files,
        raw,
    }
}

pub(super) const REQUIRED_RQBIT_APIS: &[&str] = &[
    "GET /torrents",
    "POST /torrents",
    "GET /torrents/{id}/stats/v1",
    "POST /torrents/{id}/add_peers",
    "POST /torrents/{id}/pause",
    "POST /torrents/{id}/start",
    "POST /torrents/{id}/delete",
    "POST /torrents/{id}/forget",
    "POST /torrents/{id}/update_only_files",
];

pub(super) fn evaluate_rqbit_compatibility(
    root: &RqbitRootDto,
) -> (TorrentApiCompatibility, Vec<String>) {
    if root
        .server
        .as_deref()
        .is_some_and(|server| !server.eq_ignore_ascii_case("rqbit"))
    {
        return (
            TorrentApiCompatibility::Incompatible,
            REQUIRED_RQBIT_APIS
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        );
    }
    if root.apis.is_empty() {
        return (TorrentApiCompatibility::Unknown, Vec::new());
    }
    let available = root
        .apis
        .keys()
        .map(|value| normalize_api_signature(value))
        .collect::<BTreeSet<_>>();
    let missing = REQUIRED_RQBIT_APIS
        .iter()
        .filter(|required| !available.contains(**required))
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        (TorrentApiCompatibility::Compatible, missing)
    } else {
        (TorrentApiCompatibility::Incompatible, missing)
    }
}

pub(super) fn normalize_api_signature(value: &str) -> String {
    let mut parts = value.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let path = parts.next().unwrap_or_default();
    let normalized_path = path
        .split('/')
        .map(|segment| {
            if segment.starts_with('{') && segment.ends_with('}') {
                "{id}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    format!("{method} {normalized_path}")
}

pub(super) fn snapshot_from_value(torrent_id: String, raw: Value) -> TorrentSnapshot {
    let typed = typed_aggregate(&raw);
    let downloaded = typed
        .as_ref()
        .and_then(|value| value.downloaded_bytes.as_ref())
        .and_then(RqbitNumber::as_u64)
        .or_else(|| {
            find_u64(
                &raw,
                &[
                    "downloaded_bytes",
                    "progress_bytes",
                    "downloaded",
                    "bytes_downloaded",
                ],
            )
        })
        .unwrap_or_default();
    let total = typed
        .as_ref()
        .and_then(|value| value.total_bytes.as_ref())
        .and_then(RqbitNumber::as_u64)
        .or_else(|| find_u64(&raw, &["total_bytes", "totalBytes", "size_bytes", "length"]));
    let progress = typed
        .as_ref()
        .and_then(|value| value.progress.as_ref())
        .and_then(RqbitNumber::as_f64)
        .or_else(|| find_f64(&raw, &["progress", "fraction", "percent"]))
        .map(|value| if value > 1.0 { value / 100.0 } else { value })
        .or_else(|| {
            total
                .filter(|value| *value > 0)
                .map(|value| downloaded as f64 / value as f64)
        });
    let state = typed
        .as_ref()
        .and_then(|value| value.state.as_ref())
        .cloned()
        .or_else(|| find_string(&raw, &["state", "status"]))
        .unwrap_or_else(|| "unknown".into());
    let finished = typed
        .as_ref()
        .and_then(|value| value.finished.as_ref())
        .and_then(RqbitBoolean::as_bool)
        .or_else(|| find_bool(&raw, &["finished", "complete", "completed", "is_finished"]))
        .unwrap_or(false)
        || progress.is_some_and(|value| value >= 1.0)
        || matches!(state.as_str(), "completed" | "finished" | "seeding");

    TorrentSnapshot {
        torrent_id,
        info_hash: find_string(&raw, &["info_hash", "infohash", "infoHash"]),
        name: find_string(&raw, &["name", "title"]),
        state,
        downloaded_bytes: downloaded,
        uploaded_bytes: typed
            .as_ref()
            .and_then(|value| value.uploaded_bytes.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| find_u64(&raw, &["uploaded_bytes", "uploaded", "bytes_uploaded"]))
            .unwrap_or_default(),
        total_bytes: total,
        download_speed_bps: typed
            .as_ref()
            .and_then(|value| value.download_speed_bps.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| {
                find_u64(
                    &raw,
                    &["download_speed", "download_speed_bps", "downloadSpeed"],
                )
            })
            .unwrap_or_default(),
        upload_speed_bps: typed
            .as_ref()
            .and_then(|value| value.upload_speed_bps.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| find_u64(&raw, &["upload_speed", "upload_speed_bps", "uploadSpeed"]))
            .unwrap_or_default(),
        peers_connected: typed
            .as_ref()
            .and_then(|value| value.peers_connected.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| find_u64(&raw, &["peers", "peers_connected", "live_peers"]))
            .unwrap_or_default(),
        seeders: typed
            .as_ref()
            .and_then(|value| value.seeders.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| find_u64(&raw, &["seeders", "seeds"]))
            .unwrap_or_default(),
        leechers: typed
            .as_ref()
            .and_then(|value| value.leechers.as_ref())
            .and_then(RqbitNumber::as_u64)
            .or_else(|| find_u64(&raw, &["leechers", "leeches"]))
            .unwrap_or_default(),
        finished,
        progress,
        raw,
    }
}

pub(super) fn typed_aggregate(raw: &Value) -> Option<RqbitAggregateDto> {
    let envelope: RqbitStatsEnvelope = serde_json::from_value(raw.clone()).ok()?;
    [
        envelope.stats,
        envelope.live,
        envelope.torrent,
        envelope.details,
        envelope.session,
    ]
    .into_iter()
    .flatten()
    .find(RqbitAggregateDto::has_statistics)
    .or_else(|| envelope.root.has_statistics().then_some(envelope.root))
}

pub(super) fn extract_torrent_id(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.trim().is_empty() => Some(value.trim().to_owned()),
        Value::Number(value) => Some(value.to_string()),
        _ => find_string(
            value,
            &["id", "torrent_id", "torrentId", "info_hash", "infohash"],
        )
        .or_else(|| find_u64(value, &["id", "torrent_id"]).map(|value| value.to_string())),
    }
}

pub(super) fn collect_files(value: &Value) -> Vec<TorrentFile> {
    if let Ok(envelope) = serde_json::from_value::<RqbitFilesEnvelope>(value.clone()) {
        if let Some(files) = typed_files(&envelope) {
            return files
                .iter()
                .enumerate()
                .filter_map(|(position, item)| {
                    let path = item.path.clone()?;
                    Some(TorrentFile {
                        index: item
                            .index
                            .as_ref()
                            .and_then(RqbitNumber::as_u64)
                            .and_then(|value| usize::try_from(value).ok())
                            .unwrap_or(position),
                        path,
                        size_bytes: item.size.as_ref().and_then(RqbitNumber::as_u64),
                    })
                })
                .collect();
        }
    }

    let array = find_array(value, &["files", "file_infos", "fileInfos"]);
    array
        .map(|items| {
            items
                .iter()
                .enumerate()
                .filter_map(|(position, item)| {
                    let path = find_string(item, &["path", "name", "filename"])?;
                    Some(TorrentFile {
                        index: find_u64(item, &["index", "id"])
                            .and_then(|value| usize::try_from(value).ok())
                            .unwrap_or(position),
                        path,
                        size_bytes: find_u64(item, &["size", "length", "size_bytes"]),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn typed_files(envelope: &RqbitFilesEnvelope) -> Option<&[RqbitFileDto]> {
    envelope
        .files
        .as_deref()
        .or_else(|| envelope.details.as_deref().and_then(typed_files))
        .or_else(|| envelope.torrent.as_deref().and_then(typed_files))
}

pub(super) fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    find_value(value, keys).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

pub(super) fn value_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(value) => value.as_u64().or_else(|| {
            let value = value.as_f64()?;
            (value.is_finite() && value >= 0.0 && value <= u64::MAX as f64)
                .then(|| value.trunc() as u64)
        }),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

pub(super) fn value_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64().filter(|value| value.is_finite()),
        Value::String(value) => value.parse::<f64>().ok().filter(|value| value.is_finite()),
        _ => None,
    }
}

pub(super) fn value_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

pub(super) fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    find_value(value, keys).and_then(value_u64)
}

pub(super) fn find_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    find_value(value, keys).and_then(value_f64)
}

pub(super) fn find_bool(value: &Value, keys: &[&str]) -> Option<bool> {
    find_value(value, keys).and_then(value_bool)
}

pub(super) fn find_array<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Vec<Value>> {
    find_value(value, keys).and_then(Value::as_array)
}

pub(super) fn find_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(value) = object.get(*key) {
            return Some(value);
        }
    }
    // rqbit has changed response envelopes between releases. Only inspect
    // documented aggregate containers; never recurse into peers or files.
    for container in ["stats", "live", "torrent", "details", "session"] {
        if let Some(Value::Object(map)) = object.get(container) {
            for key in keys {
                if let Some(value) = map.get(*key) {
                    return Some(value);
                }
            }
        }
    }
    None
}

pub(super) fn truncate(value: &str) -> String {
    const LIMIT: usize = 512;
    let mut chars = value.chars();
    let output: String = chars.by_ref().take(LIMIT).collect();
    if chars.next().is_some() {
        format!("{output}…")
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_peer_level_progress_when_aggregate_is_missing() {
        let value = json!({"peers": [{"downloaded": 999, "progress": 1.0}]});
        let snapshot = snapshot_from_value("x".into(), value);
        assert_eq!(snapshot.downloaded_bytes, 0);
        assert!(!snapshot.finished);
    }

    #[test]
    fn parses_flexible_torrent_statistics() {
        let value = json!({
            "id": 7,
            "name": "archive",
            "stats": {
                "progress_bytes": 50,
                "total_bytes": 100,
                "download_speed": 12,
                "uploaded_bytes": 4
            }
        });
        let snapshot = snapshot_from_value("7".into(), value);
        assert_eq!(snapshot.downloaded_bytes, 50);
        assert_eq!(snapshot.total_bytes, Some(100));
        assert_eq!(snapshot.progress, Some(0.5));
        assert!(!snapshot.finished);
    }

    #[test]
    fn rqbit_api_compatibility_normalizes_path_parameters() {
        let root = RqbitRootDto {
            server: Some("rqbit".into()),
            version: Some("test".into()),
            apis: REQUIRED_RQBIT_APIS
                .iter()
                .map(|signature| {
                    (
                        signature.replace("{id}", "{id_or_infohash}"),
                        "supported".to_owned(),
                    )
                })
                .collect(),
        };
        let (compatibility, missing) = evaluate_rqbit_compatibility(&root);
        assert!(matches!(compatibility, TorrentApiCompatibility::Compatible));
        assert!(missing.is_empty());
    }

    #[test]
    fn rqbit_api_compatibility_reports_missing_endpoints() {
        let root = RqbitRootDto {
            server: Some("rqbit".into()),
            version: None,
            apis: [("GET /torrents".into(), "supported".into())]
                .into_iter()
                .collect(),
        };
        let (compatibility, missing) = evaluate_rqbit_compatibility(&root);
        assert!(matches!(
            compatibility,
            TorrentApiCompatibility::Incompatible
        ));
        assert!(!missing.is_empty());
    }

    #[test]
    fn parses_typed_string_statistics_from_known_envelope() {
        let value = json!({
            "stats": {
                "downloaded_bytes": "75",
                "total_bytes": "100",
                "uploaded_bytes": "25",
                "download_speed_bps": "12",
                "status": "downloading"
            }
        });
        let snapshot = snapshot_from_value("typed".into(), value);
        assert_eq!(snapshot.downloaded_bytes, 75);
        assert_eq!(snapshot.uploaded_bytes, 25);
        assert_eq!(snapshot.progress, Some(0.75));
    }

    #[test]
    fn parses_typed_engine_list_without_recursive_field_leakage() {
        let list = engine_list_from_value(json!({"torrents": [
            {"id": 7, "name": "archive", "progress": 50, "stats": {"downloaded": 999}}
        ]}));
        assert_eq!(list.torrents.len(), 1);
        assert_eq!(list.torrents[0].torrent_id.as_deref(), Some("7"));
        assert_eq!(list.torrents[0].progress, Some(0.5));
        assert_eq!(list.torrents[0].downloaded_bytes, None);
    }

    #[test]
    fn parses_typed_peer_and_global_statistics() {
        let peers = peer_stats_from_value(json!({"peers": [
            {"address": "127.0.0.1:6881", "download_speed": 42}
        ]}));
        assert_eq!(peers.peers.len(), 1);
        assert_eq!(peers.peers[0].download_speed_bps, Some(42));

        let stats = global_stats_from_value(json!({"stats": {
            "downloaded_bytes": "100",
            "uploaded_bytes": 20,
            "download_speed": 5
        }}));
        assert_eq!(stats.downloaded_bytes, Some(100));
        assert_eq!(stats.uploaded_bytes, Some(20));
        assert_eq!(stats.download_speed_bps, Some(5));
    }

    #[test]
    fn extracts_files_from_nested_response() {
        let value = json!({"details": {"files": [
            {"path": "one.bin", "length": 10},
            {"path": "two.bin", "length": 20}
        ]}});
        let files = collect_files(&value);
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].index, 1);
        assert_eq!(files[1].size_bytes, Some(20));
    }
}

#[cfg(test)]
mod security_tests {
    use serde_json::json;

    use super::{collect_files, snapshot_from_value, validate_engine_id};

    #[test]
    fn parses_nested_torrent_statistics() {
        let snapshot = snapshot_from_value(
            "42".into(),
            json!({
                "stats": {
                    "downloaded_bytes": 500,
                    "total_bytes": 1000,
                    "download_speed_bps": 25,
                    "uploaded_bytes": 12,
                    "state": "downloading"
                }
            }),
        );
        assert_eq!(snapshot.downloaded_bytes, 500);
        assert_eq!(snapshot.total_bytes, Some(1000));
        assert_eq!(snapshot.progress, Some(0.5));
        assert!(!snapshot.finished);
    }

    #[test]
    fn detects_finished_torrent_from_progress() {
        let snapshot =
            snapshot_from_value("7".into(), json!({"progress": 100, "status": "seeding"}));
        assert!(snapshot.finished);
        assert_eq!(snapshot.progress, Some(1.0));
    }

    #[test]
    fn extracts_file_list() {
        let files = collect_files(&json!({
            "details": {"files": [
                {"index": 3, "path": "video.mkv", "size": 99}
            ]}
        }));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].index, 3);
        assert_eq!(files[0].path, "video.mkv");
    }

    #[test]
    fn rejects_engine_path_injection() {
        assert!(validate_engine_id("abc123").is_ok());
        assert!(validate_engine_id("../stats").is_err());
        assert!(validate_engine_id("id?x=1").is_err());
    }
}
