//! The job manager's shared state and constructor. Behavior lives in the
//! sibling modules (`lifecycle`, `dispatcher`, `execution`, `bulk`,
//! `automation`, `maintenance`, `media_control`, `torrent_control`), each
//! contributing its own `impl JobManager` block.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{
    sync::Mutex,
    task::{AbortHandle, JoinHandle},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    adapters::{media::MediaAdapter, torrent::TorrentAdapter},
    config::{Config, PersistentSettings},
    core::{
        events::EventBus,
        metrics::Metrics,
        models::{FfmpegPreset, JobKind, OutputSourceKind, OutputType, PostAction},
        progress::{self, ProgressReceiver},
    },
    download::http::HttpAdapter,
    error::{RavynError, Result},
    services::sniffer::SnifferService,
    storage::Repository,
};

pub(crate) struct TrackedTask {
    pub(crate) name: String,
    pub(crate) handle: JoinHandle<()>,
}

pub(crate) struct ActiveJob {
    pub(crate) cancellation: CancellationToken,
    pub(crate) abort: Option<AbortHandle>,
}

pub(crate) struct ConcurrencyGate {
    limit: AtomicUsize,
    in_use: AtomicUsize,
}

impl ConcurrencyGate {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit: AtomicUsize::new(limit.max(1)),
            in_use: AtomicUsize::new(0),
        }
    }

    pub(crate) fn set_limit(&self, limit: usize) {
        self.limit.store(limit.max(1), Ordering::Release);
    }

    pub(crate) fn try_acquire(self: &Arc<Self>) -> Option<ConcurrencyPermit> {
        let mut current = self.in_use.load(Ordering::Acquire);
        loop {
            if current >= self.limit.load(Ordering::Acquire) {
                return None;
            }
            match self.in_use.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(ConcurrencyPermit(self.clone())),
                Err(actual) => current = actual,
            }
        }
    }
}

pub(crate) struct ConcurrencyPermit(Arc<ConcurrencyGate>);

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        self.0.in_use.fetch_sub(1, Ordering::AcqRel);
    }
}

pub struct JobManager {
    pub(crate) config: Arc<Config>,
    pub(crate) repository: Repository,
    pub(crate) events: EventBus,
    pub(crate) metrics: Metrics,
    pub(crate) http: Arc<HttpAdapter>,
    pub(crate) media: Arc<MediaAdapter>,
    pub(crate) torrent: Arc<TorrentAdapter>,
    pub(crate) sniffer: Arc<SnifferService>,
    pub(crate) semaphore: Arc<ConcurrencyGate>,
    pub(crate) active: Mutex<HashMap<Uuid, ActiveJob>>,
    pub(crate) idempotency: Mutex<()>,
    pub(crate) tasks: Mutex<Vec<TrackedTask>>,
    pub(crate) progress_receiver: Mutex<Option<ProgressReceiver>>,
    pub(crate) started: AtomicBool,
    pub(crate) accepting_tasks: AtomicBool,
    pub(crate) shutdown: CancellationToken,
}

pub(crate) fn validate_tags(tags: &[String]) -> Result<()> {
    let mut normalized = std::collections::HashSet::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.len() > 80 {
            return Err(RavynError::Invalid(
                "tag names may not exceed 80 characters".into(),
            ));
        }
        normalized.insert(tag.to_ascii_lowercase());
    }
    if normalized.len() > 64 {
        return Err(RavynError::Invalid("a job may have at most 64 tags".into()));
    }
    Ok(())
}

pub(crate) fn validate_torrent_options(
    options: &crate::core::models::TorrentOptions,
) -> Result<()> {
    if options.seed_after_download && !options.keep_managed {
        return Err(RavynError::Invalid(
            "seed_after_download requires keep_managed to remain enabled".into(),
        ));
    }
    if !options.seed_after_download
        && (options.max_seed_ratio.is_some()
            || options.max_seed_time_secs.is_some()
            || options.min_seed_time_secs > 0)
    {
        return Err(RavynError::Invalid(
            "torrent seeding limits require seed_after_download to be enabled".into(),
        ));
    }
    if let Some(ratio) = options.max_seed_ratio {
        if !ratio.is_finite() || ratio <= 0.0 || ratio > 10_000.0 {
            return Err(RavynError::Invalid(
                "max_seed_ratio must be a finite value between 0 and 10000".into(),
            ));
        }
    }
    if let Some(seconds) = options.max_seed_time_secs {
        if seconds == 0 || seconds > 315_576_000 {
            return Err(RavynError::Invalid(
                "max_seed_time_secs must be between 1 second and 10 years".into(),
            ));
        }
        if options.min_seed_time_secs > seconds {
            return Err(RavynError::Invalid(
                "min_seed_time_secs may not exceed max_seed_time_secs".into(),
            ));
        }
    }
    if options.min_seed_time_secs > 315_576_000 {
        return Err(RavynError::Invalid(
            "min_seed_time_secs may not exceed 10 years".into(),
        ));
    }
    Ok(())
}

pub(crate) fn output_source(kind: JobKind) -> OutputSourceKind {
    match kind {
        JobKind::Http => OutputSourceKind::Http,
        JobKind::Media => OutputSourceKind::Media,
        JobKind::Torrent => OutputSourceKind::Torrent,
    }
}

pub(crate) fn output_type(kind: JobKind, path: &std::path::Path, primary: bool) -> OutputType {
    if primary && kind != JobKind::Torrent {
        return OutputType::Primary;
    }
    if kind == JobKind::Torrent {
        return if path.is_dir() {
            OutputType::Directory
        } else {
            OutputType::TorrentFile
        };
    }
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" | "mkv" | "webm" | "mov" | "avi" => OutputType::Video,
        "mp3" | "m4a" | "aac" | "flac" | "opus" | "wav" => OutputType::Audio,
        "srt" | "vtt" | "ass" | "lrc" => OutputType::Subtitle,
        "jpg" | "jpeg" | "png" | "webp" | "avif" => OutputType::Thumbnail,
        "json" | "description" => OutputType::Metadata,
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" => OutputType::Archive,
        _ => OutputType::Other,
    }
}

pub(crate) fn post_action_name(action: &PostAction) -> &'static str {
    match action {
        PostAction::VerifySha256 { .. } => "verify_sha256",
        PostAction::Extract { .. } => "extract",
        PostAction::ConvertMedia { .. } => "convert_media",
        PostAction::Move { .. } => "move",
        PostAction::Open => "open",
    }
}

pub(crate) fn preset_extension(preset: FfmpegPreset) -> Option<&'static str> {
    match preset {
        FfmpegPreset::VideoCopy => None,
        FfmpegPreset::VideoH264 => Some("mp4"),
        FfmpegPreset::VideoH265 => Some("mp4"),
        FfmpegPreset::VideoAv1 => Some("mkv"),
        FfmpegPreset::AudioMp3 => Some("mp3"),
        FfmpegPreset::AudioAac => Some("m4a"),
        FfmpegPreset::AudioOpus => Some("opus"),
        FfmpegPreset::AudioFlac => Some("flac"),
        FfmpegPreset::ImageAvif => Some("avif"),
        FfmpegPreset::ImageWebp => Some("webp"),
    }
}

impl JobManager {
    pub async fn new(config: Arc<Config>, repository: Repository) -> Result<Self> {
        let events = EventBus::new(2048);
        let metrics = Metrics::default();
        repository.attach_metrics(metrics.clone());
        let (progress_publisher, progress_receiver) =
            progress::channel(2048, repository.clone(), events.clone(), metrics.clone());
        Ok(Self {
            http: Arc::new(HttpAdapter::new(
                config.clone(),
                progress_publisher.clone(),
                repository.clone(),
            )?),
            media: Arc::new(MediaAdapter::new(
                config.clone(),
                progress_publisher.clone(),
                repository.clone(),
            )),
            torrent: Arc::new(
                TorrentAdapter::new(
                    config.clone(),
                    repository.clone(),
                    progress_publisher,
                    events.clone(),
                )
                .await?,
            ),
            sniffer: Arc::new(SnifferService::new(config.clone(), repository.clone())?),
            semaphore: Arc::new(ConcurrencyGate::new(config.max_active)),
            active: Mutex::new(HashMap::new()),
            idempotency: Mutex::new(()),
            tasks: Mutex::new(Vec::new()),
            progress_receiver: Mutex::new(Some(progress_receiver)),
            started: AtomicBool::new(false),
            accepting_tasks: AtomicBool::new(true),
            shutdown: CancellationToken::new(),
            config,
            repository,
            events,
            metrics,
        })
    }
    pub fn events(&self) -> EventBus {
        self.events.clone()
    }
    pub fn metrics(&self) -> Metrics {
        self.metrics.clone()
    }
    pub fn config(&self) -> Arc<Config> {
        self.config.clone()
    }

    pub fn apply_live_settings(&self, settings: &PersistentSettings) -> Result<()> {
        let effective = settings
            .bandwidth_schedule
            .effective_limit_at(settings.global_speed_limit_bps, chrono::Utc::now())?;
        self.http.set_global_speed_limit(effective);
        self.semaphore.set_limit(settings.max_active);
        Ok(())
    }

    pub async fn active_job_count(&self) -> usize {
        self.active.lock().await.len()
    }

    pub fn is_accepting_tasks(&self) -> bool {
        self.accepting_tasks.load(Ordering::Acquire)
    }

    pub async fn progress_writer_is_running(&self) -> bool {
        self.tasks
            .lock()
            .await
            .iter()
            .any(|task| task.name == "progress-writer" && !task.handle.is_finished())
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;

    #[test]
    fn limit_changes_are_atomic_and_do_not_preempt_active_work() {
        let gate = Arc::new(ConcurrencyGate::new(2));
        let first = gate.try_acquire().unwrap();
        let second = gate.try_acquire().unwrap();
        assert!(gate.try_acquire().is_none());
        gate.set_limit(1);
        drop(first);
        assert!(gate.try_acquire().is_none());
        drop(second);
        let only = gate.try_acquire().unwrap();
        assert!(gate.try_acquire().is_none());
        gate.set_limit(3);
        assert!(gate.try_acquire().is_some());
        drop(only);
    }
}
