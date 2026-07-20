//! Bounded update I/O, persistence, and path utilities.

use super::*;

pub(super) fn update_directory(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_cache_dir()
        .map(|path| path.join("updates"))
        .map_err(|error| format!("failed to resolve the update state directory: {error}"))
}

pub(super) fn read_last_result(app: &AppHandle) -> Result<Option<AppUpdateResult>, String> {
    read_json_file(&update_directory(app)?.join(UPDATE_RESULT_FILENAME))
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    if bytes.is_empty() || bytes.len() > 64 * 1024 {
        return Err(format!("{} is empty or oversized", path.display()));
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    serde_json::from_slice(bytes)
        .map(Some)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

pub(super) fn write_json_atomic_sync(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize update state: {error}"))?;
    write_bytes_atomic_sync(path, &bytes)
}

pub(super) fn write_bytes_atomic_sync(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "the update state path has no parent directory".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create the update state directory: {error}"))?;
    let temporary = path.with_extension("tmp");
    let _ = std::fs::remove_file(&temporary);
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("failed to write update state: {error}"))?;
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|error| format!("failed to replace update state: {error}"))?;
    }
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("failed to activate update state: {error}"))
}

pub(super) async fn read_response_bounded(
    response: reqwest::Response,
    limit: u64,
    label: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, String> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    loop {
        let next = tokio::select! {
            _ = cancellation.cancelled() => return Err(UPDATE_CANCELLED_ERROR.into()),
            next = stream.next() => next,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|error| format!("failed to read {label}: {error}"))?;
        let next_len = bytes.len().saturating_add(chunk.len());
        if u64::try_from(next_len).unwrap_or(u64::MAX) > limit {
            return Err(format!("{label} exceeds the maximum size"));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(format!("{label} is empty"));
    }
    Ok(bytes)
}

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
    normalize(left) == normalize(right)
}

pub(super) fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

