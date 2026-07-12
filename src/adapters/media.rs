use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::Config,
    core::{
        models::{Job, JobStatus, MediaOptions, OutputType, ProgressSnapshot},
        progress::ProgressPublisher,
    },
    download::adapter::{DownloadAdapter, DownloadOutcome, ProducedArtifact},
    error::{RavynError, Result},
    services::process as process_supervisor,
    storage::{MediaItemDescriptor, Repository},
};

const PROGRESS_PREFIX: &str = "ravyn-progress:";
const FILE_PREFIX: &str = "ravyn-file:";
const ITEM_SEEN_PREFIX: &str = "ravyn-item-seen:";
const ITEM_START_PREFIX: &str = "ravyn-item-start:";
const ITEM_DONE_PREFIX: &str = "ravyn-item-done:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbeRequest {
    pub url: String,
    #[serde(default)]
    pub cookies_from_browser: Option<String>,
    #[serde(default)]
    pub cookies_file: Option<PathBuf>,
    #[serde(default)]
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbe {
    pub id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub webpage_url: Option<String>,
    pub extractor: Option<String>,
    pub duration: Option<f64>,
    pub live_status: Option<String>,
    pub thumbnail: Option<String>,
    pub uploader: Option<String>,
    pub playlist_count: Option<u64>,
    #[serde(default)]
    pub formats: Vec<MediaFormat>,
    #[serde(default)]
    pub subtitles: Vec<String>,
    #[serde(default)]
    pub automatic_captions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFormat {
    pub format_id: String,
    pub extension: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub bitrate_kbps: Option<f64>,
    pub audio_bitrate_kbps: Option<f64>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub protocol: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyCompatibility {
    Compatible,
    Incompatible,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyStatus {
    pub name: &'static str,
    pub path: PathBuf,
    pub available: bool,
    pub version: Option<String>,
    pub compatibility: DependencyCompatibility,
    pub missing_capabilities: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawProbe {
    id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    webpage_url: Option<String>,
    extractor: Option<String>,
    duration: Option<f64>,
    live_status: Option<String>,
    thumbnail: Option<String>,
    uploader: Option<String>,
    playlist_count: Option<u64>,
    #[serde(default)]
    formats: Vec<RawFormat>,
    #[serde(default)]
    subtitles: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    automatic_captions: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawFormat {
    format_id: String,
    ext: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
    vcodec: Option<String>,
    acodec: Option<String>,
    tbr: Option<f64>,
    abr: Option<f64>,
    filesize: Option<u64>,
    filesize_approx: Option<u64>,
    protocol: Option<String>,
    format_note: Option<String>,
}

pub struct MediaAdapter {
    config: Arc<Config>,
    progress_publisher: ProgressPublisher,
    repository: Repository,
    dependency_cache: tokio::sync::Mutex<Option<(Instant, DependencyStatus)>>,
}

impl MediaAdapter {
    pub fn new(
        config: Arc<Config>,
        progress_publisher: ProgressPublisher,
        repository: Repository,
    ) -> Self {
        Self {
            config,
            progress_publisher,
            repository,
            dependency_cache: tokio::sync::Mutex::new(None),
        }
    }

    async fn cached_ytdlp_status(&self) -> DependencyStatus {
        const CACHE_TTL: Duration = Duration::from_secs(300);
        if let Some((checked_at, status)) = self.dependency_cache.lock().await.as_ref() {
            if checked_at.elapsed() < CACHE_TTL {
                return status.clone();
            }
        }
        let status = check_ytdlp_dependency(&self.config.ytdlp).await;
        *self.dependency_cache.lock().await = Some((Instant::now(), status.clone()));
        status
    }

    async fn ensure_ytdlp_compatible(&self) -> Result<()> {
        let status = self.cached_ytdlp_status().await;
        if !status.available {
            return Err(RavynError::Process(
                status
                    .error
                    .unwrap_or_else(|| "yt-dlp is not available".into()),
            ));
        }
        if matches!(status.compatibility, DependencyCompatibility::Incompatible) {
            return Err(RavynError::Process(format!(
                "yt-dlp is missing required capabilities: {}",
                status.missing_capabilities.join(", ")
            )));
        }
        if matches!(status.compatibility, DependencyCompatibility::Unknown) {
            tracing::warn!(
                version = status.version.as_deref().unwrap_or("unknown"),
                error = status
                    .error
                    .as_deref()
                    .unwrap_or("capability probe unavailable"),
                "yt-dlp compatibility could not be confirmed"
            );
        }
        Ok(())
    }

    async fn resolve_job_secrets(&self, job: &Job) -> Result<Job> {
        let mut resolved = job.clone();
        if let Some(secret_id) = resolved.options_json.proxy_secret_id {
            resolved.options_json.proxy = Some(
                self.repository
                    .resolve_secret_reference(secret_id, "proxy_credentials")
                    .await?,
            );
        }
        if let Some(secret_id) = resolved.options_json.cookies_secret_id {
            let secret = self
                .repository
                .resolve_secret_reference(secret_id, "cookies")
                .await?;
            let cookies: std::collections::BTreeMap<String, String> = serde_json::from_str(&secret)
                .map_err(|_| {
                    RavynError::Invalid(
                        "cookie secret must be a JSON object of string name/value pairs".into(),
                    )
                })?;
            resolved.options_json.cookies.extend(cookies);
        }
        if let Some(secret_id) = resolved.options_json.authentication_header_secret_id {
            let secret = self
                .repository
                .resolve_secret_reference(secret_id, "authentication_header")
                .await?;
            resolved
                .options_json
                .headers
                .insert("Authorization".into(), secret);
        }
        Ok(resolved)
    }

    pub async fn probe(&self, request: &MediaProbeRequest) -> Result<MediaProbe> {
        self.ensure_ytdlp_compatible().await?;
        if request.url.trim().is_empty() {
            return Err(RavynError::Invalid("media URL must not be empty".into()));
        }

        let mut command = Command::new(&self.config.ytdlp);
        command
            .arg("--ignore-config")
            .arg("--dump-single-json")
            .arg("--skip-download")
            .arg("--no-warnings")
            .arg("--no-color")
            .arg(&request.url)
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped());
        append_probe_network_options(&mut command, request);

        let maximum_output = self.config.media_probe_max_mib.saturating_mul(1024 * 1024);
        let limits = process_supervisor::ProcessLimits {
            wall_time: Duration::from_secs(self.config.media_probe_timeout_secs),
            stdout_bytes: maximum_output,
            stderr_bytes: maximum_output,
            ..process_supervisor::ProcessLimits::default()
        };
        let output =
            process_supervisor::run(&mut command, &limits, None, CancellationToken::new()).await?;
        if output.stdout_truncated || output.stderr_truncated {
            return Err(RavynError::Process(format!(
                "yt-dlp probe output exceeded the {} MiB limit",
                self.config.media_probe_max_mib
            )));
        }
        if !output.status.success() {
            return Err(RavynError::Process(process_error(
                "yt-dlp probe",
                output.status,
                &output.stderr,
            )));
        }

        let raw: RawProbe = serde_json::from_slice(&output.stdout)?;
        Ok(MediaProbe {
            id: raw.id,
            title: raw.title,
            description: raw.description,
            webpage_url: raw.webpage_url,
            extractor: raw.extractor,
            duration: raw.duration,
            live_status: raw.live_status,
            thumbnail: raw.thumbnail,
            uploader: raw.uploader,
            playlist_count: raw.playlist_count,
            formats: raw
                .formats
                .into_iter()
                .map(|format| MediaFormat {
                    format_id: format.format_id,
                    extension: format.ext,
                    width: format.width,
                    height: format.height,
                    fps: format.fps,
                    video_codec: normalize_codec(format.vcodec),
                    audio_codec: normalize_codec(format.acodec),
                    bitrate_kbps: format.tbr,
                    audio_bitrate_kbps: format.abr,
                    filesize: format.filesize,
                    filesize_approx: format.filesize_approx,
                    protocol: format.protocol,
                    note: format.format_note,
                })
                .collect(),
            subtitles: raw.subtitles.into_iter().map(|(key, _)| key).collect(),
            automatic_captions: raw
                .automatic_captions
                .into_iter()
                .map(|(key, _)| key)
                .collect(),
        })
    }

    pub async fn dependency_status(&self) -> Vec<DependencyStatus> {
        let yt_dlp = self.cached_ytdlp_status().await;
        let ffmpeg = check_dependency("ffmpeg", &self.config.ffmpeg, ["-version"]).await;
        vec![yt_dlp, ffmpeg]
    }
}

#[async_trait]
impl DownloadAdapter for MediaAdapter {
    async fn run(&self, job: &Job, cancellation: CancellationToken) -> Result<DownloadOutcome> {
        self.ensure_ytdlp_compatible().await?;
        let resolved_job = self.resolve_job_secrets(job).await?;
        let job = &resolved_job;
        tokio::fs::create_dir_all(&job.destination).await?;
        let options = job.options_json.media.clone().unwrap_or_default();
        let template = output_template(job, &options);

        let archive_path = self
            .config
            .data_dir
            .join("media-archives")
            .join(format!("{}.txt", job.id));
        self.repository.export_media_archive(&archive_path).await?;

        let mut command = Command::new(&self.config.ytdlp);
        command
            .arg("--ignore-config")
            .arg("--newline")
            .arg("--no-color")
            .arg("--no-warnings")
            .arg("--progress")
            .arg("--progress-delta")
            .arg("0.25")
            .arg("--progress-template")
            .arg("download:ravyn-progress:%(progress.downloaded_bytes)s|%(progress.total_bytes,progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.playlist_index)s|%(info.playlist_count)s")
            .arg("--print")
            .arg("video:ravyn-item-seen:%(.{id,title,webpage_url,extractor,extractor_key,playlist_id,playlist_title,playlist_index,playlist_count,ext})j")
            .arg("--print")
            .arg("before_dl:ravyn-item-start:%(.{id,title,webpage_url,extractor,extractor_key,playlist_id,playlist_title,playlist_index,playlist_count,ext})j")
            .arg("--print")
            .arg("after_move:ravyn-item-done:%(.{id,title,webpage_url,extractor,extractor_key,playlist_id,playlist_title,playlist_index,playlist_count,filepath,ext,requested_subtitles,thumbnails,infojson_filename,description_filename,requested_downloads,__files_to_move,chapters})j")
            .arg("--print")
            .arg("after_move:ravyn-file:%(filepath)s")
            .arg("--download-archive")
            .arg(&archive_path)
            .arg("--output")
            .arg(&template)
            .arg("--paths")
            .arg(&job.destination)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        crate::services::security::validate_relative_template(
            template.to_string_lossy().as_ref(),
            "media output template",
        )?;
        append_download_options(
            &mut command,
            job,
            &options,
            self.config.global_speed_limit_bps,
        )?;
        command.arg(&job.source);
        let process_limits = process_supervisor::ProcessLimits::default();
        process_supervisor::configure(&mut command, &process_limits);

        let mut child = command.spawn().map_err(|error| {
            RavynError::Process(format!(
                "failed to start {}: {error}",
                self.config.ytdlp.display()
            ))
        })?;
        let process_guard = process_supervisor::ProcessGuard::attach(&child, &process_limits)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RavynError::Process("yt-dlp stdout was unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| RavynError::Process("yt-dlp stderr was unavailable".into()))?;

        let stderr_task = tokio::spawn(collect_stderr(stderr));
        let mut lines = BufReader::new(stdout).lines();
        let mut files = Vec::new();
        let mut artifacts = Vec::new();
        let started = Instant::now();
        let deadline = tokio::time::sleep(process_limits.wall_time);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    process_guard.terminate(&mut child).await;
                    let _ = stderr_task.await;
                    let _ = self
                        .repository
                        .mark_unfinished_media_items_failed(job.id, "media download was cancelled")
                        .await;
                    return Err(RavynError::Cancelled);
                }
                _ = &mut deadline => {
                    process_guard.terminate(&mut child).await;
                    let _ = stderr_task.await;
                    let message = "yt-dlp download exceeded its wall-clock limit";
                    let _ = self.repository.mark_unfinished_media_items_failed(job.id, message).await;
                    return Err(RavynError::Process(message.into()));
                }
                line = lines.next_line() => {
                    match line? {
                        Some(line) => {
                            if let Some(payload) = line.strip_prefix(ITEM_SEEN_PREFIX) {
                                match parse_media_item(payload) {
                                    Ok((descriptor, _)) => {
                                        self.repository.observe_media_item(job.id, &descriptor).await?;
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, job_id = %job.id, "ignored invalid yt-dlp item metadata");
                                    }
                                }
                            } else if let Some(payload) = line.strip_prefix(ITEM_START_PREFIX) {
                                match parse_media_item(payload) {
                                    Ok((descriptor, _)) => {
                                        self.repository.begin_media_item(job.id, &descriptor).await?;
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, job_id = %job.id, "ignored invalid yt-dlp item-start metadata");
                                    }
                                }
                            } else if let Some(payload) = line.strip_prefix(ITEM_DONE_PREFIX) {
                                match parse_media_completion(payload) {
                                    Ok(completion) => {
                                        let destination = Path::new(&job.destination);
                                        let Some(primary) = completion.primary_path else {
                                            tracing::warn!(job_id = %job.id, "yt-dlp item completion did not include a filepath");
                                            continue;
                                        };
                                        let primary = resolve_output_path(&primary, destination)?;
                                        self.repository
                                            .complete_media_item(job.id, &completion.descriptor, &primary)
                                            .await?;
                                        for mut artifact in completion.artifacts {
                                            artifact.path = resolve_output_path(&artifact.path, destination)?;
                                            if !tokio::fs::try_exists(&artifact.path).await? {
                                                tracing::debug!(path = %artifact.path.display(), job_id = %job.id, "yt-dlp reported an auxiliary output that no longer exists");
                                                continue;
                                            }
                                            if !files.contains(&artifact.path) {
                                                files.push(artifact.path.clone());
                                            }
                                            if !artifacts.iter().any(|existing: &ProducedArtifact| existing.path == artifact.path) {
                                                artifacts.push(artifact);
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, job_id = %job.id, "ignored invalid yt-dlp item-completion metadata");
                                    }
                                }
                            } else if let Some(path) = line.strip_prefix(FILE_PREFIX) {
                                let path = PathBuf::from(path.trim());
                                ensure_output_under_destination(&path, Path::new(&job.destination))?;
                                if !files.contains(&path) {
                                    files.push(path.clone());
                                }
                                if !artifacts.iter().any(|artifact: &ProducedArtifact| artifact.path == path) {
                                    artifacts.push(ProducedArtifact::new(path));
                                }
                            } else if let Some(progress) = parse_progress(job.id, &line, started) {
                                self.progress_publisher.publish(progress).await?;
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        let status = child.wait().await?;
        let stderr = stderr_task
            .await
            .map_err(|error| RavynError::Internal(format!("stderr task failed: {error}")))??;
        let mut process_message =
            (!status.success()).then(|| process_error("yt-dlp download", status, &stderr));
        if let Some(message) = process_message.as_deref() {
            self.repository
                .mark_unfinished_media_items_failed(job.id, message)
                .await?;
        } else {
            let (_skipped, incomplete) = self
                .repository
                .finalize_media_items_after_success(job.id)
                .await?;
            if incomplete > 0 {
                process_message = Some(format!(
                    "yt-dlp completed, but {incomplete} media item(s) did not report a final output"
                ));
            }
        }
        let _ = tokio::fs::remove_file(&archive_path).await;
        if files.is_empty() {
            if let Some(message) = process_message.as_ref() {
                return Err(RavynError::Process(message.clone()));
            }
            let summary = self.repository.media_item_summary(job.id).await?;
            self.repository
                .append_job_log(
                    job.id,
                    "media",
                    "info",
                    "MEDIA_NO_NEW_OUTPUTS",
                    &format!(
                        "yt-dlp completed without creating a new file; {} item(s) were observed and {} were skipped",
                        summary.total, summary.skipped
                    ),
                )
                .await?;
        }

        Ok(DownloadOutcome {
            primary_path: (files.len() == 1).then(|| files[0].clone()),
            files,
            artifacts,
            terminal_status: process_message.as_ref().map(|_| JobStatus::Partial),
            terminal_message: process_message,
        })
    }
}

fn append_probe_network_options(command: &mut Command, request: &MediaProbeRequest) {
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

fn append_download_options(
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

fn output_template(job: &Job, options: &MediaOptions) -> PathBuf {
    let filename = job
        .filename
        .as_deref()
        .or(options.output_template.as_deref())
        .unwrap_or("%(title).180B [%(id)s].%(ext)s");
    PathBuf::from(filename)
}

fn format_selector(options: &MediaOptions) -> String {
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

struct MediaCompletion {
    descriptor: MediaItemDescriptor,
    primary_path: Option<PathBuf>,
    artifacts: Vec<ProducedArtifact>,
}

fn parse_media_completion(payload: &str) -> Result<MediaCompletion> {
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

fn push_media_artifact(
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

fn object_path(value: &serde_json::Value) -> Option<PathBuf> {
    value
        .as_object()
        .and_then(|object| {
            value_string(object.get("filepath"))
                .or_else(|| value_string(object.get("filename")))
                .or_else(|| value_string(object.get("path")))
        })
        .map(PathBuf::from)
}

fn media_output_type(path: &Path, primary: bool) -> OutputType {
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

fn parse_media_item(payload: &str) -> Result<(MediaItemDescriptor, Option<PathBuf>)> {
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

fn compact_media_metadata(
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

fn value_string(value: Option<&serde_json::Value>) -> Option<String> {
    match value {
        Some(serde_json::Value::String(value)) if !value.is_empty() && value != "NA" => {
            Some(value.clone())
        }
        Some(serde_json::Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn value_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value {
        Some(serde_json::Value::Number(value)) => value.as_u64(),
        Some(serde_json::Value::String(value)) => value.parse().ok(),
        _ => None,
    }
}

fn validate_header(name: &str, value: &str) -> Result<()> {
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

fn parse_progress(job_id: uuid::Uuid, line: &str, started: Instant) -> Option<ProgressSnapshot> {
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

fn parse_optional_u64(value: &str) -> Option<u64> {
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

fn normalize_codec(value: Option<String>) -> Option<String> {
    value.filter(|codec| codec != "none")
}

async fn collect_stderr(mut stderr: tokio::process::ChildStderr) -> Result<Vec<u8>> {
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

fn resolve_output_path(path: &Path, destination: &Path) -> Result<PathBuf> {
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

fn ensure_output_under_destination(path: &Path, destination: &Path) -> Result<()> {
    resolve_output_path(path, destination).map(|_| ())
}

const REQUIRED_YTDLP_CAPABILITIES: &[(&str, &str)] = &[
    ("ignore_config", "--ignore-config"),
    ("structured_json", "--dump-single-json"),
    ("structured_print", "--print"),
    ("progress_template", "--progress-template"),
    ("download_archive", "--download-archive"),
    ("ffmpeg_location", "--ffmpeg-location"),
];

async fn check_ytdlp_dependency(program: &Path) -> DependencyStatus {
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

async fn check_dependency<const N: usize>(
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

async fn dependency_output<const N: usize>(
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

fn process_error(name: &str, status: std::process::ExitStatus, stderr: &[u8]) -> String {
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
