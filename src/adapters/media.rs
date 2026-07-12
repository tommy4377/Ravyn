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

/// Exercises the yt-dlp machine-progress parser without starting a process.
/// This is intentionally small and public for the cargo-fuzz target.
pub fn parse_ytdlp_progress_for_fuzzing(line: &str) -> Option<ProgressSnapshot> {
    parse_progress(uuid::Uuid::nil(), line, Instant::now())
}

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

mod ytdlp;

use self::ytdlp::*;
