//! yt-dlp command construction, structured-output parsing, progress
//! decoding, and capability probing.

use super::*;

pub(super) fn append_probe_network_options(command: &mut Command, request: &MediaProbeRequest) {
    if let Some(value) = request.cookies_from_browser.as_deref() {
        command.arg("--cookies-from-browser").arg(value);
    }
    if let Some(value) = request.cookies_file.as_deref() {
        command.arg("--cookies").arg(value);
    }
    if let Some(value) = request.proxy.as_deref() {
        command.arg("--proxy").arg(value);
    }
}

pub(super) fn append_download_options(
    command: &mut Command,
    job: &Job,
    options: &MediaOptions,
    global_limit: u64,
) -> Result<()> {
    if options.playlist {
        command.arg("--yes-playlist");
    } else {
        command.arg("--no-playlist");
    }
    if let Some(start) = options.playlist_start {
        command.arg("--playlist-start").arg(start.to_string());
    }
    if let Some(end) = options.playlist_end {
        command.arg("--playlist-end").arg(end.to_string());
    }

    if options.audio_only {
        command.arg("--extract-audio");
        command
            .arg("--audio-format")
            .arg(options.audio_format.as_deref().unwrap_or("best"));
        if let Some(quality) = options.audio_quality.as_deref() {
            command.arg("--audio-quality").arg(quality);
        }
    } else {
        command.arg("--format").arg(format_selector(options));
        command
            .arg("--merge-output-format")
            .arg(options.merge_output_format.as_deref().unwrap_or("mkv"));
    }

    if options.write_subtitles {
        command.arg("--write-subs");
    }
    if options.write_automatic_subtitles {
        command.arg("--write-auto-subs");
    }
    if !options.subtitle_languages.is_empty() {
        command
            .arg("--sub-langs")
            .arg(options.subtitle_languages.join(","));
    }
    if options.embed_subtitles {
        command.arg("--embed-subs");
    }
    if options.write_thumbnail {
        command.arg("--write-thumbnail");
    }
    if options.embed_thumbnail {
        command.arg("--embed-thumbnail");
    }
    if options.write_info_json {
        command.arg("--write-info-json");
    }
    if options.write_description {
        command.arg("--write-description");
    }
    if options.embed_metadata {
        command.arg("--embed-metadata");
    }
    if !options.sponsorblock_remove.is_empty() {
        command
            .arg("--sponsorblock-remove")
            .arg(options.sponsorblock_remove.join(","));
    }
    if let Some(value) = options.concurrent_fragments {
        command
            .arg("--concurrent-fragments")
            .arg(value.clamp(1, 32).to_string());
    }
    if let Some(value) = options.cookies_from_browser.as_deref() {
        command.arg("--cookies-from-browser").arg(value);
    }
    if let Some(value) = options.cookies_file.as_deref() {
        command.arg("--cookies").arg(value);
    }
    if let Some(value) = job.options_json.proxy.as_deref() {
        command.arg("--proxy").arg(value);
    }
    if let Some(value) = job.options_json.user_agent.as_deref() {
        command.arg("--user-agent").arg(value);
    }
    if let Some(value) = job.options_json.referer.as_deref() {
        command.arg("--referer").arg(value);
    }
    if !job.options_json.cookies.is_empty() {
        let value = job
            .options_json
            .cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        validate_header("Cookie", &value)?;
        command.arg("--add-header").arg(format!("Cookie:{value}"));
    }
    for (name, value) in &job.options_json.headers {
        validate_header(name, value)?;
        command.arg("--add-header").arg(format!("{name}:{value}"));
    }
    let job_limit = job.speed_limit_bps.filter(|value| *value > 0).unwrap_or(0) as u64;
    let effective_limit = match (job_limit, global_limit) {
        (0, 0) => 0,
        (0, global) => global,
        (job, 0) => job,
        (job, global) => job.min(global),
    };
    if effective_limit > 0 {
        command.arg("--limit-rate").arg(effective_limit.to_string());
    }

    Ok(())
}

pub(super) fn output_template(job: &Job, options: &MediaOptions) -> PathBuf {
    let filename = job
        .filename
        .as_deref()
        .or(options.output_template.as_deref())
        .unwrap_or("%(title).180B [%(id)s].%(ext)s");
    PathBuf::from(filename)
}

pub(super) fn format_selector(options: &MediaOptions) -> String {
    if let Some(format) = options.format.as_deref() {
        return format.to_owned();
    }
    match options.max_height {
        Some(height) => {
            format!("bestvideo[height<={height}]+bestaudio/best[height<={height}]/best")
        }
        None => "bestvideo+bestaudio/best".to_owned(),
    }
}

pub(super) struct MediaCompletion {
    pub(super) descriptor: MediaItemDescriptor,
    pub(super) primary_path: Option<PathBuf>,
    pub(super) artifacts: Vec<ProducedArtifact>,
}

pub(super) fn parse_media_completion(payload: &str) -> Result<MediaCompletion> {
    let raw: serde_json::Value = serde_json::from_str(payload.trim())
        .map_err(|error| RavynError::Protocol(format!("invalid yt-dlp item metadata: {error}")))?;
    let object = raw
        .as_object()
        .ok_or_else(|| RavynError::Protocol("yt-dlp item metadata was not a JSON object".into()))?;
    let (descriptor, primary_path) = parse_media_item(payload)?;
    let mut artifacts = Vec::new();
    if let Some(path) = primary_path.as_ref() {
        push_media_artifact(
            &mut artifacts,
            path.clone(),
            media_output_type(path, true),
            &descriptor,
            "primary",
            true,
        );
    }
    if let Some(serde_json::Value::Object(subtitles)) = object.get("requested_subtitles") {
        for value in subtitles.values() {
            if let Some(path) = object_path(value) {
                push_media_artifact(
                    &mut artifacts,
                    path,
                    OutputType::Subtitle,
                    &descriptor,
                    "subtitle",
                    false,
                );
            }
        }
    }
    if let Some(serde_json::Value::Array(thumbnails)) = object.get("thumbnails") {
        for value in thumbnails {
            if let Some(path) = object_path(value) {
                push_media_artifact(
                    &mut artifacts,
                    path,
                    OutputType::Thumbnail,
                    &descriptor,
                    "thumbnail",
                    false,
                );
            }
        }
    }
    for (field, role) in [
        ("infojson_filename", "metadata"),
        ("description_filename", "description"),
    ] {
        if let Some(path) = value_string(object.get(field)).map(PathBuf::from) {
            push_media_artifact(
                &mut artifacts,
                path,
                OutputType::Metadata,
                &descriptor,
                role,
                false,
            );
        }
    }
    if let Some(serde_json::Value::Array(downloads)) = object.get("requested_downloads") {
        for value in downloads {
            if let Some(path) = object_path(value) {
                let output_type = media_output_type(&path, false);
                let role = match output_type {
                    OutputType::Video => "video",
                    OutputType::Audio => "audio",
                    _ => "auxiliary",
                };
                push_media_artifact(
                    &mut artifacts,
                    path,
                    output_type,
                    &descriptor,
                    role,
                    matches!(output_type, OutputType::Video | OutputType::Audio),
                );
            }
        }
    }
    if let Some(serde_json::Value::Object(files)) = object.get("__files_to_move") {
        for value in files.values() {
            if let Some(path) = value_string(Some(value)).map(PathBuf::from) {
                let output_type = media_output_type(&path, false);
                push_media_artifact(
                    &mut artifacts,
                    path,
                    output_type,
                    &descriptor,
                    "auxiliary",
                    false,
                );
            }
        }
    }
    Ok(MediaCompletion {
        descriptor,
        primary_path,
        artifacts,
    })
}

pub(super) fn push_media_artifact(
    artifacts: &mut Vec<ProducedArtifact>,
    path: PathBuf,
    output_type: OutputType,
    descriptor: &MediaItemDescriptor,
    role: &str,
    postprocess: bool,
) {
    if artifacts.iter().any(|artifact| artifact.path == path) {
        return;
    }
    let chapter_count = descriptor
        .metadata
        .get("chapter_count")
        .and_then(serde_json::Value::as_u64);
    artifacts.push(ProducedArtifact {
        path,
        output_type: Some(output_type),
        media_item_key: Some(descriptor.item_key.clone()),
        role: Some(role.to_owned()),
        metadata: serde_json::json!({
            "media_item_key": descriptor.item_key,
            "extractor": descriptor.extractor,
            "media_id": descriptor.media_id,
            "playlist_id": descriptor.playlist_id,
            "playlist_index": descriptor.playlist_index,
            "role": role,
            "chapter_count": chapter_count,
        }),
        postprocess,
    });
}

pub(super) fn object_path(value: &serde_json::Value) -> Option<PathBuf> {
    value
        .as_object()
        .and_then(|object| {
            value_string(object.get("filepath"))
                .or_else(|| value_string(object.get("filename")))
                .or_else(|| value_string(object.get("path")))
        })
        .map(PathBuf::from)
}

pub(super) fn media_output_type(path: &Path, primary: bool) -> OutputType {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "mp4" | "mkv" | "webm" | "mov" | "avi" => OutputType::Video,
        "mp3" | "m4a" | "aac" | "flac" | "opus" | "wav" => OutputType::Audio,
        "srt" | "vtt" | "ass" | "lrc" => OutputType::Subtitle,
        "jpg" | "jpeg" | "png" | "webp" | "avif" => OutputType::Thumbnail,
        "json" | "description" => OutputType::Metadata,
        _ if primary => OutputType::Primary,
        _ => OutputType::Other,
    }
}

pub(super) fn parse_media_item(payload: &str) -> Result<(MediaItemDescriptor, Option<PathBuf>)> {
    let raw: serde_json::Value = serde_json::from_str(payload.trim())
        .map_err(|error| RavynError::Protocol(format!("invalid yt-dlp item metadata: {error}")))?;
    let object = raw
        .as_object()
        .ok_or_else(|| RavynError::Protocol("yt-dlp item metadata was not a JSON object".into()))?;
    let extractor = value_string(object.get("extractor_key"))
        .or_else(|| value_string(object.get("extractor")))
        .map(|value| value.to_ascii_lowercase());
    let media_id = value_string(object.get("id"));
    let webpage_url = value_string(object.get("webpage_url"));
    let playlist_id = value_string(object.get("playlist_id"));
    let playlist_index = value_u64(object.get("playlist_index"));
    let identity = if let (Some(extractor), Some(media_id)) = (&extractor, &media_id) {
        format!("{extractor}:{media_id}")
    } else if let (Some(playlist_id), Some(index)) = (&playlist_id, playlist_index) {
        format!("playlist:{playlist_id}:{index}")
    } else if let Some(url) = &webpage_url {
        format!("url:{url}")
    } else {
        let digest = <sha2::Sha256 as sha2::Digest>::digest(payload.as_bytes());
        format!("metadata:{}", hex::encode(digest))
    };
    let item_key = if identity.len() <= 1024 {
        identity
    } else {
        let digest = <sha2::Sha256 as sha2::Digest>::digest(identity.as_bytes());
        format!("sha256:{}", hex::encode(digest))
    };
    let path = value_string(object.get("filepath")).map(PathBuf::from);
    Ok((
        MediaItemDescriptor {
            item_key,
            extractor,
            media_id,
            title: value_string(object.get("title")),
            webpage_url,
            playlist_id,
            playlist_title: value_string(object.get("playlist_title")),
            playlist_index,
            playlist_count: value_u64(object.get("playlist_count")),
            extension: value_string(object.get("ext")),
            metadata: compact_media_metadata(object),
        },
        path,
    ))
}

pub(super) fn compact_media_metadata(
    object: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut compact = serde_json::Map::new();
    for key in [
        "id",
        "title",
        "webpage_url",
        "extractor",
        "extractor_key",
        "playlist_id",
        "playlist_title",
        "playlist_index",
        "playlist_count",
        "ext",
        "duration",
        "live_status",
        "uploader",
        "format_id",
    ] {
        if let Some(value) = object.get(key) {
            if !value.is_null() {
                compact.insert(key.to_owned(), value.clone());
            }
        }
    }
    if let Some(count) = object
        .get("chapters")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
    {
        compact.insert("chapter_count".into(), serde_json::json!(count));
    }
    serde_json::Value::Object(compact)
}

pub(super) fn value_string(value: Option<&serde_json::Value>) -> Option<String> {
    match value {
        Some(serde_json::Value::String(value)) if !value.is_empty() && value != "NA" => {
            Some(value.clone())
        }
        Some(serde_json::Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn value_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value {
        Some(serde_json::Value::Number(value)) => value.as_u64(),
        Some(serde_json::Value::String(value)) => value.parse().ok(),
        _ => None,
    }
}

pub(super) fn validate_header(name: &str, value: &str) -> Result<()> {
    if name.trim().is_empty() || name.contains(['\r', '\n', ':']) {
        return Err(RavynError::Invalid(format!(
            "invalid media header name: {name:?}"
        )));
    }
    if value.contains(['\r', '\n']) {
        return Err(RavynError::Invalid(format!(
            "invalid value for media header {name:?}"
        )));
    }
    Ok(())
}

pub(super) fn parse_progress(
    job_id: uuid::Uuid,
    line: &str,
    started: Instant,
) -> Option<ProgressSnapshot> {
    let payload = line.strip_prefix(PROGRESS_PREFIX)?;
    let mut fields = payload.split('|');
    let item_downloaded = parse_optional_u64(fields.next()?)?;
    let item_total = fields.next().and_then(parse_optional_u64);
    let reported_speed = fields.next().and_then(parse_optional_u64);
    let _eta = fields.next();
    let playlist_index = fields
        .next()
        .and_then(parse_optional_u64)
        .unwrap_or(1)
        .max(1);
    let playlist_count = fields
        .next()
        .and_then(parse_optional_u64)
        .unwrap_or(1)
        .max(1);
    let (downloaded, total) = match item_total {
        Some(total) if playlist_count > 1 => (
            total
                .saturating_mul(playlist_index.saturating_sub(1))
                .saturating_add(item_downloaded.min(total)),
            Some(total.saturating_mul(playlist_count)),
        ),
        total => (item_downloaded, total),
    };
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let measured_speed = (downloaded as f64 / elapsed) as u64;
    Some(ProgressSnapshot {
        job_id,
        downloaded_bytes: downloaded,
        total_bytes: total,
        bytes_per_second: reported_speed.unwrap_or(measured_speed),
    })
}

pub(super) fn parse_optional_u64(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || matches!(value, "NA" | "N/A" | "None" | "null") {
        return None;
    }
    value.parse::<u64>().ok().or_else(|| {
        value
            .parse::<f64>()
            .ok()
            .map(|number| number.max(0.0) as u64)
    })
}

pub(super) fn normalize_codec(value: Option<String>) -> Option<String> {
    value.filter(|codec| codec != "none")
}

pub(super) async fn collect_stderr(mut stderr: tokio::process::ChildStderr) -> Result<Vec<u8>> {
    const MAX_ERROR_BYTES: usize = 64 * 1024;
    let mut ring = Vec::with_capacity(MAX_ERROR_BYTES);
    let mut buffer = [0_u8; 8192];
    loop {
        let read = stderr.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if read >= MAX_ERROR_BYTES {
            ring.clear();
            ring.extend_from_slice(&buffer[read - MAX_ERROR_BYTES..read]);
            continue;
        }
        let overflow = ring
            .len()
            .saturating_add(read)
            .saturating_sub(MAX_ERROR_BYTES);
        if overflow > 0 {
            ring.drain(..overflow);
        }
        ring.extend_from_slice(&buffer[..read]);
    }
    Ok(ring)
}

pub(super) fn resolve_output_path(path: &Path, destination: &Path) -> Result<PathBuf> {
    let destination = if destination.is_absolute() {
        destination.to_path_buf()
    } else {
        std::env::current_dir()?.join(destination)
    };
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        destination.join(path)
    };
    if !path.starts_with(&destination) {
        return Err(RavynError::Invalid(format!(
            "yt-dlp produced a path outside the job destination: {}",
            path.display()
        )));
    }
    Ok(path)
}

pub(super) fn ensure_output_under_destination(path: &Path, destination: &Path) -> Result<()> {
    resolve_output_path(path, destination).map(|_| ())
}

pub(super) const REQUIRED_YTDLP_CAPABILITIES: &[(&str, &str)] = &[
    ("ignore_config", "--ignore-config"),
    ("structured_json", "--dump-single-json"),
    ("structured_print", "--print"),
    ("progress_template", "--progress-template"),
    ("download_archive", "--download-archive"),
    ("ffmpeg_location", "--ffmpeg-location"),
];

pub(super) async fn check_ytdlp_dependency(program: &Path) -> DependencyStatus {
    let version_output = dependency_output(program, ["--version"]).await;
    let version_output = match version_output {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return DependencyStatus {
                name: "yt-dlp",
                path: program.to_owned(),
                available: false,
                version: None,
                compatibility: DependencyCompatibility::Unknown,
                missing_capabilities: Vec::new(),
                error: Some(process_error("yt-dlp", output.status, &output.stderr)),
            };
        }
        Err(error) => {
            return DependencyStatus {
                name: "yt-dlp",
                path: program.to_owned(),
                available: false,
                version: None,
                compatibility: DependencyCompatibility::Unknown,
                missing_capabilities: Vec::new(),
                error: Some(error.to_string()),
            };
        }
    };
    let version = String::from_utf8_lossy(&version_output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&version_output.stderr).lines())
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_owned());

    let help_output = dependency_output(program, ["--help"]).await;
    let help = match help_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        Ok(output) => {
            return DependencyStatus {
                name: "yt-dlp",
                path: program.to_owned(),
                available: true,
                version,
                compatibility: DependencyCompatibility::Unknown,
                missing_capabilities: Vec::new(),
                error: Some(process_error(
                    "yt-dlp capability probe",
                    output.status,
                    &output.stderr,
                )),
            };
        }
        Err(error) => {
            return DependencyStatus {
                name: "yt-dlp",
                path: program.to_owned(),
                available: true,
                version,
                compatibility: DependencyCompatibility::Unknown,
                missing_capabilities: Vec::new(),
                error: Some(error.to_string()),
            };
        }
    };
    let missing_capabilities = REQUIRED_YTDLP_CAPABILITIES
        .iter()
        .filter(|(_, flag)| !help.contains(flag))
        .map(|(capability, _)| (*capability).to_owned())
        .collect::<Vec<_>>();
    DependencyStatus {
        name: "yt-dlp",
        path: program.to_owned(),
        available: true,
        version,
        compatibility: if missing_capabilities.is_empty() {
            DependencyCompatibility::Compatible
        } else {
            DependencyCompatibility::Incompatible
        },
        missing_capabilities,
        error: None,
    }
}

pub(super) async fn check_dependency<const N: usize>(
    name: &'static str,
    program: &Path,
    args: [&str; N],
) -> DependencyStatus {
    match dependency_output(program, args).await {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let version = stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_owned());
            DependencyStatus {
                name,
                path: program.to_owned(),
                available: true,
                version,
                compatibility: DependencyCompatibility::Compatible,
                missing_capabilities: Vec::new(),
                error: None,
            }
        }
        Ok(output) => DependencyStatus {
            name,
            path: program.to_owned(),
            available: false,
            version: None,
            compatibility: DependencyCompatibility::Unknown,
            missing_capabilities: Vec::new(),
            error: Some(process_error(name, output.status, &output.stderr)),
        },
        Err(error) => DependencyStatus {
            name,
            path: program.to_owned(),
            available: false,
            version: None,
            compatibility: DependencyCompatibility::Unknown,
            missing_capabilities: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

pub(super) async fn dependency_output<const N: usize>(
    program: &Path,
    args: [&str; N],
) -> Result<process_supervisor::ProcessOutput> {
    let mut command = Command::new(program);
    command.args(args);
    let limits = process_supervisor::ProcessLimits {
        wall_time: Duration::from_secs(15),
        stdout_bytes: 1024 * 1024,
        stderr_bytes: 1024 * 1024,
        ..process_supervisor::ProcessLimits::default()
    };
    process_supervisor::run(&mut command, &limits, None, CancellationToken::new()).await
}

pub(super) fn process_error(name: &str, status: std::process::ExitStatus, stderr: &[u8]) -> String {
    let message = String::from_utf8_lossy(stderr);
    let message = message.trim();
    if message.is_empty() {
        format!("{name} exited with {status}")
    } else {
        format!("{name} exited with {status}: {message}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_machine_progress() {
        let snapshot = parse_progress(
            uuid::Uuid::nil(),
            "ravyn-progress:1024|2048|512|2",
            Instant::now(),
        )
        .expect("progress should parse");
        assert_eq!(snapshot.downloaded_bytes, 1024);
        assert_eq!(snapshot.total_bytes, Some(2048));
        assert_eq!(snapshot.bytes_per_second, 512);
    }

    #[test]
    fn ignores_unknown_progress_values() {
        let snapshot = parse_progress(
            uuid::Uuid::nil(),
            "ravyn-progress:1024|NA|N/A|NA",
            Instant::now(),
        )
        .expect("progress should parse");
        assert_eq!(snapshot.total_bytes, None);
        assert!(snapshot.bytes_per_second > 0);
    }

    #[test]
    fn builds_height_limited_selector() {
        let options = MediaOptions {
            max_height: Some(1080),
            ..MediaOptions::default()
        };
        assert!(format_selector(&options).contains("height<=1080"));
    }

    #[test]
    fn parses_structured_media_item_metadata() {
        let (item, path) = parse_media_item(
            r#"{"id":"abc","extractor":"youtube","title":"Example","playlist_id":"pl","playlist_index":2,"playlist_count":5,"filepath":"/tmp/example.mp4","ext":"mp4"}"#,
        )
        .unwrap();
        assert_eq!(item.item_key, "youtube:abc");
        assert_eq!(item.playlist_index, Some(2));
        assert_eq!(path, Some(PathBuf::from("/tmp/example.mp4")));
    }

    #[test]
    fn parses_auxiliary_media_outputs() {
        let completion = parse_media_completion(
            r#"{
                "id":"abc",
                "extractor":"youtube",
                "filepath":"/tmp/video.mkv",
                "requested_subtitles":{"en":{"filepath":"/tmp/video.en.vtt"}},
                "thumbnails":[{"filepath":"/tmp/video.webp"}],
                "infojson_filename":"/tmp/video.info.json",
                "description_filename":"/tmp/video.description"
            }"#,
        )
        .unwrap();
        assert_eq!(
            completion.primary_path,
            Some(PathBuf::from("/tmp/video.mkv"))
        );
        assert!(
            completion
                .artifacts
                .iter()
                .any(|item| item.role.as_deref() == Some("primary"))
        );
        assert!(
            completion
                .artifacts
                .iter()
                .any(|item| item.role.as_deref() == Some("subtitle"))
        );
        assert!(
            completion
                .artifacts
                .iter()
                .any(|item| item.role.as_deref() == Some("thumbnail"))
        );
        assert!(
            completion
                .artifacts
                .iter()
                .any(|item| item.role.as_deref() == Some("metadata"))
        );
        assert!(
            completion
                .artifacts
                .iter()
                .any(|item| item.role.as_deref() == Some("description"))
        );
    }

    #[test]
    fn rejects_header_injection() {
        assert!(validate_header("X-Test\r\nInjected", "value").is_err());
        assert!(validate_header("X-Test", "value\r\nInjected").is_err());
    }
}
