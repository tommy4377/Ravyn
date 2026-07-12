use futures_util::FutureExt;
use sha2::Digest;
use std::{
    collections::HashMap,
    future::Future,
    panic::AssertUnwindSafe,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex, Semaphore},
    task::{AbortHandle, JoinHandle},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    adapters::{
        media::{DependencyStatus, MediaAdapter, MediaProbe, MediaProbeRequest},
        torrent::{
            TorrentAdapter, TorrentDependencyStatus, TorrentDetails, TorrentEngineList,
            TorrentGlobalStats, TorrentPeerStats, TorrentProbe, TorrentProbeRequest,
            TorrentSnapshot,
        },
    },
    config::{Config, PersistentSettings},
    core::{
        events::{Event, EventBus},
        metrics::Metrics,
        models::{
            CreateJob, DuplicatePolicy, FfmpegPreset, Job, JobKind, JobStatus, OutputSourceKind,
            OutputState, OutputType, PostAction, UpdateJob,
        },
        progress::{self, ProgressReceiver},
    },
    download::{adapter::DownloadAdapter, http::HttpAdapter},
    error::{RavynError, Result},
    postprocess,
    services::{
        browser::{self, BrowserTokenRecord, CreateBrowserToken, IssuedBrowserToken},
        checksum, dedup,
        imports::{ImportDefaults, ImportItemResult, ImportResult, ImportTextRequest},
        rules::{self, Rule},
        scheduler,
        schedules::{ScheduleInput, ScheduleMode},
        security,
        sniffer::{SniffRequest, SniffResult, SnifferService},
    },
    storage::{Repository, RuleInput, TorrentRecord},
};

struct TrackedTask {
    name: String,
    handle: JoinHandle<()>,
}

struct ActiveJob {
    cancellation: CancellationToken,
    abort: Option<AbortHandle>,
}

pub struct JobManager {
    config: Arc<Config>,
    repository: Repository,
    events: EventBus,
    metrics: Metrics,
    http: Arc<HttpAdapter>,
    media: Arc<MediaAdapter>,
    torrent: Arc<TorrentAdapter>,
    sniffer: Arc<SnifferService>,
    semaphore: Arc<Semaphore>,
    active: Mutex<HashMap<Uuid, ActiveJob>>,
    idempotency: Mutex<()>,
    tasks: Mutex<Vec<TrackedTask>>,
    progress_receiver: Mutex<Option<ProgressReceiver>>,
    started: AtomicBool,
    accepting_tasks: AtomicBool,
    shutdown: CancellationToken,
}

fn validate_tags(tags: &[String]) -> Result<()> {
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

fn validate_torrent_options(options: &crate::core::models::TorrentOptions) -> Result<()> {
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

fn output_source(kind: JobKind) -> OutputSourceKind {
    match kind {
        JobKind::Http => OutputSourceKind::Http,
        JobKind::Media => OutputSourceKind::Media,
        JobKind::Torrent => OutputSourceKind::Torrent,
    }
}

fn output_type(kind: JobKind, path: &std::path::Path, primary: bool) -> OutputType {
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

fn post_action_name(action: &PostAction) -> &'static str {
    match action {
        PostAction::VerifySha256 { .. } => "verify_sha256",
        PostAction::Extract { .. } => "extract",
        PostAction::ConvertMedia { .. } => "convert_media",
        PostAction::Move { .. } => "move",
        PostAction::Open => "open",
    }
}

fn preset_extension(preset: FfmpegPreset) -> Option<&'static str> {
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
            semaphore: Arc::new(Semaphore::new(config.max_active.max(1))),
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

    pub fn apply_live_settings(&self, settings: &PersistentSettings) {
        self.http
            .set_global_speed_limit(settings.global_speed_limit_bps);
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

    pub async fn backup_database(&self) -> Result<std::path::PathBuf> {
        let directory = self.config.data_dir.join("backups");
        tokio::fs::create_dir_all(&directory).await?;
        let destination = directory.join(format!(
            "ravyn-{}-{}.sqlite3",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
            Uuid::new_v4()
        ));
        self.repository.backup_to(&destination).await?;
        Ok(destination)
    }

    pub async fn list_backups(&self) -> Result<Vec<serde_json::Value>> {
        let directory = self.config.data_dir.join("backups");
        tokio::fs::create_dir_all(&directory).await?;
        let mut entries = tokio::fs::read_dir(&directory).await?;
        let mut backups = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("sqlite3")
            {
                backups.push(serde_json::json!({
                    "name": entry.file_name().to_string_lossy(),
                    "size_bytes": metadata.len(),
                    "modified_at": metadata.modified().ok().map(chrono::DateTime::<chrono::Utc>::from)
                }));
            }
        }
        backups.sort_by(|left, right| right["name"].as_str().cmp(&left["name"].as_str()));
        Ok(backups)
    }

    pub async fn verify_backup(&self, name: &str) -> Result<String> {
        if name.is_empty()
            || std::path::Path::new(name).components().count() != 1
            || !name.ends_with(".sqlite3")
        {
            return Err(RavynError::Invalid("invalid backup name".into()));
        }
        let path = self.config.data_dir.join("backups").join(name);
        if !tokio::fs::try_exists(&path).await? {
            return Err(RavynError::NotFound(format!("backup {name}")));
        }
        Repository::verify_database_file(&path).await
    }

    pub async fn schedule_database_restore(
        &self,
        name: &str,
    ) -> Result<crate::storage::recovery::RestoreStatus> {
        let integrity = self.verify_backup(name).await?;
        if integrity != "ok" {
            return Err(RavynError::Invalid(format!(
                "backup integrity check failed: {integrity}"
            )));
        }
        let path = self.config.data_dir.join("backups").join(name);
        crate::storage::recovery::schedule(&self.config.data_dir, &path, name).await
    }

    pub async fn database_restore_status(&self) -> Result<crate::storage::recovery::RestoreStatus> {
        crate::storage::recovery::status(&self.config.data_dir).await
    }

    pub async fn cancel_database_restore(&self) -> Result<crate::storage::recovery::RestoreStatus> {
        crate::storage::recovery::cancel(&self.config.data_dir).await
    }

    pub async fn sniff_page(&self, request: &SniffRequest) -> Result<SniffResult> {
        self.sniffer.sniff(request).await
    }

    pub async fn import_text(&self, request: ImportTextRequest) -> Result<ImportResult> {
        let (sources, duplicate_lines, truncated) =
            crate::services::imports::parse_lines(&request.text, self.config.max_batch_urls);
        let mut result = self
            .import_urls(sources, request.defaults, duplicate_lines)
            .await?;
        result.truncated = truncated;
        Ok(result)
    }

    pub async fn import_urls(
        &self,
        sources: Vec<String>,
        defaults: ImportDefaults,
        duplicate_lines: usize,
    ) -> Result<ImportResult> {
        if sources.len() > self.config.max_batch_urls {
            return Err(RavynError::Invalid(format!(
                "batch contains more than {} URLs",
                self.config.max_batch_urls
            )));
        }
        let mut items = Vec::with_capacity(sources.len());
        let mut accepted = 0;
        let mut rejected = 0;
        for source in sources {
            let request = CreateJob {
                kind: defaults.kind,
                source: source.clone(),
                destination: defaults.destination.clone(),
                filename: None,
                priority: defaults.priority,
                speed_limit_bps: defaults.speed_limit_bps,
                expected_sha256: None,
                duplicate_policy: defaults.duplicate_policy,
                options: defaults.options.clone(),
            };
            match self.create(request).await {
                Ok(job) => {
                    accepted += 1;
                    items.push(ImportItemResult {
                        source,
                        job: Some(job),
                        error: None,
                    });
                }
                Err(error) => {
                    rejected += 1;
                    items.push(ImportItemResult {
                        source,
                        job: None,
                        error: Some(error.to_string()),
                    });
                }
            }
        }
        Ok(ImportResult {
            accepted,
            rejected,
            duplicate_lines,
            truncated: false,
            items,
        })
    }

    pub async fn create_batch(&self, requests: Vec<CreateJob>) -> Result<ImportResult> {
        if requests.is_empty() {
            return Err(RavynError::Invalid("batch may not be empty".into()));
        }
        if requests.len() > self.config.max_batch_urls {
            return Err(RavynError::Invalid(format!(
                "batch contains more than {} jobs",
                self.config.max_batch_urls
            )));
        }
        let mut items = Vec::with_capacity(requests.len());
        let mut accepted = 0;
        let mut rejected = 0;
        for request in requests {
            let source = request.source.clone();
            match self.create(request).await {
                Ok(job) => {
                    accepted += 1;
                    items.push(ImportItemResult {
                        source,
                        job: Some(job),
                        error: None,
                    });
                }
                Err(error) => {
                    rejected += 1;
                    items.push(ImportItemResult {
                        source,
                        job: None,
                        error: Some(error.to_string()),
                    });
                }
            }
        }
        Ok(ImportResult {
            accepted,
            rejected,
            duplicate_lines: 0,
            truncated: false,
            items,
        })
    }

    pub async fn create_rule(&self, input: RuleInput) -> Result<Rule> {
        self.validate_rule_input(&input)?;
        self.repository.create_rule(input).await
    }

    pub async fn update_rule(&self, id: Uuid, input: RuleInput) -> Result<Rule> {
        self.validate_rule_input(&input)?;
        self.repository.update_rule(id, input).await
    }

    pub async fn issue_browser_token(
        &self,
        request: CreateBrowserToken,
    ) -> Result<IssuedBrowserToken> {
        let (issued, token_hash) = browser::issue(request)?;
        self.repository
            .insert_browser_token(&issued.record, &token_hash)
            .await?;
        Ok(issued)
    }

    pub async fn list_browser_tokens(&self) -> Result<Vec<BrowserTokenRecord>> {
        self.repository.list_browser_tokens().await
    }

    pub async fn revoke_browser_token(&self, id: Uuid) -> Result<()> {
        self.repository.revoke_browser_token(id).await
    }

    pub async fn create_schedule(&self, input: ScheduleInput) -> Result<crate::storage::Schedule> {
        self.validate_schedule_input(&input).await?;
        self.repository.create_schedule(input).await
    }

    pub async fn update_schedule(
        &self,
        id: Uuid,
        input: ScheduleInput,
    ) -> Result<crate::storage::Schedule> {
        self.validate_schedule_input(&input).await?;
        self.repository.update_schedule(id, input).await
    }

    pub async fn execute_schedule(&self, schedule: &crate::storage::Schedule) -> Result<()> {
        let started = std::time::Instant::now();
        let delay = chrono::Utc::now()
            .signed_duration_since(schedule.next_run_at)
            .to_std()
            .unwrap_or_default();
        let mode = match schedule.mode {
            ScheduleMode::Download => "download",
            ScheduleMode::SniffResources => "sniff_resources",
        };
        let result = async {
            match schedule.mode {
                ScheduleMode::Download => {
                    self.create(schedule.to_create_job()).await?;
                    Ok(())
                }
                ScheduleMode::SniffResources => {
                    let automation = schedule.automation.as_ref().ok_or_else(|| {
                        RavynError::Internal("sniff schedule omitted automation options".into())
                    })?;
                    let sniff = self
                        .sniff_page(&automation.request(schedule.source.clone()))
                        .await?;
                    let mut defaults = automation.import_defaults.clone();
                    if defaults.destination.is_none() {
                        defaults.destination = Some(schedule.destination.clone());
                    }
                    let sources = sniff
                        .resources
                        .iter()
                        .map(|resource| resource.url.clone())
                        .collect::<Vec<_>>();
                    let result = self.import_urls(sources, defaults, 0).await?;
                    let remembered = sniff
                        .resources
                        .iter()
                        .filter(|resource| {
                            result
                                .items
                                .iter()
                                .any(|item| item.source == resource.url && item.job.is_some())
                        })
                        .map(|resource| {
                            (
                                resource.url.clone(),
                                resource.kind.as_str().to_owned(),
                                true,
                            )
                        })
                        .collect::<Vec<_>>();
                    self.repository
                        .remember_page_resources(&sniff.page_url, &remembered)
                        .await?;
                    if result.accepted == 0 && result.rejected > 0 {
                        return Err(RavynError::Process(format!(
                            "all {} discovered resources were rejected",
                            result.rejected
                        )));
                    }
                    Ok(())
                }
            }
        }
        .await;
        self.metrics
            .schedule_finished(mode, result.is_ok(), delay, started.elapsed());
        result
    }

    pub async fn run_schedule_now(
        &self,
        schedule_id: Uuid,
        idempotency_key: Option<&str>,
    ) -> Result<crate::storage::ScheduleExecutionRecord> {
        let mut schedule = self.repository.get_schedule(schedule_id).await?;
        schedule.next_run_at = chrono::Utc::now();
        let request_hash = hex::encode(sha2::Sha256::digest(schedule_id.as_bytes()));
        let _guard = self.idempotency.lock().await;
        if let Some(key) = idempotency_key {
            let key = key.trim();
            if key.is_empty() || key.len() > 200 {
                return Err(RavynError::Invalid(
                    "Idempotency-Key must contain between 1 and 200 characters".into(),
                ));
            }
            if let Some((stored_hash, resource_id)) = self
                .repository
                .get_idempotent_resource("schedule_run_now", key)
                .await?
            {
                if stored_hash != request_hash {
                    return Err(RavynError::Conflict(
                        "Idempotency-Key was already used for a different request".into(),
                    ));
                }
                let id = Uuid::parse_str(&resource_id).map_err(|error| {
                    RavynError::Internal(format!(
                        "stored schedule execution id is invalid: {error}"
                    ))
                })?;
                return self.repository.get_schedule_execution(id).await;
            }
        }
        let claim = crate::storage::ScheduleClaim {
            schedule: schedule.clone(),
            token: format!("run-now-{}", Uuid::new_v4()),
        };
        let execution_id = self
            .repository
            .begin_schedule_execution(&claim)
            .await?
            .ok_or_else(|| {
                RavynError::Conflict("schedule is already running for this instant".into())
            })?;
        if let Some(key) = idempotency_key {
            self.repository
                .put_idempotent_resource(
                    "schedule_run_now",
                    key.trim(),
                    &request_hash,
                    execution_id,
                )
                .await?;
        }
        match self.execute_schedule(&schedule).await {
            Ok(()) => {
                self.repository
                    .finish_schedule_execution(execution_id, "completed", None)
                    .await?
            }
            Err(error) => {
                let message = error.to_string();
                self.repository
                    .finish_schedule_execution(execution_id, "failed", Some(&message))
                    .await?;
            }
        }
        self.repository.get_schedule_execution(execution_id).await
    }

    async fn validate_schedule_input(&self, input: &ScheduleInput) -> Result<()> {
        if input.mode == ScheduleMode::SniffResources
            || matches!(input.kind, JobKind::Http | JobKind::Media)
        {
            security::validate_network_source_resolved(&self.config, &input.source).await?;
        }
        security::validate_output_path(&self.config, &input.destination)?;
        validate_tags(&input.options.tags)?;
        self.validate_post_actions(&input.options.post_actions)?;
        self.validate_download_secret_references(&input.options)
            .await?;
        if let Some(automation) = input.automation.as_ref() {
            if automation
                .max_resources
                .is_some_and(|value| value == 0 || value > self.config.max_sniff_resources)
            {
                return Err(RavynError::Invalid(format!(
                    "schedule max_resources must be between 1 and {}",
                    self.config.max_sniff_resources
                )));
            }
            if automation.extensions.len() > 128
                || automation
                    .extensions
                    .iter()
                    .any(|value| value.trim().is_empty() || value.len() > 32)
            {
                return Err(RavynError::Invalid(
                    "schedule extension filters must contain at most 128 non-empty values of 32 characters".into(),
                ));
            }
            validate_tags(&automation.import_defaults.options.tags)?;
            self.validate_post_actions(&automation.import_defaults.options.post_actions)?;
            self.validate_download_secret_references(&automation.import_defaults.options)
                .await?;
            if let Some(destination) = automation.import_defaults.destination.as_ref() {
                security::validate_output_path(&self.config, destination)?;
            }
        }
        input.validate(chrono::Utc::now())?;
        Ok(())
    }
    fn validate_rule_input(&self, input: &RuleInput) -> Result<()> {
        input.validate()?;
        validate_tags(&input.actions.tags)?;
        self.validate_post_actions(&input.actions.post_actions)?;
        if let Some(destination) = input.actions.destination.as_ref() {
            security::validate_output_path(&self.config, destination)?;
        }
        Ok(())
    }

    fn validate_post_actions(&self, actions: &[PostAction]) -> Result<()> {
        if actions.len() > 32 {
            return Err(RavynError::Invalid(
                "a job may contain at most 32 post-processing actions".into(),
            ));
        }
        for action in actions {
            match action {
                PostAction::Extract {
                    destination: Some(destination),
                    ..
                }
                | PostAction::Move { destination } => {
                    security::validate_output_path(&self.config, destination)?;
                }
                PostAction::ConvertMedia {
                    extension,
                    preset,
                    arguments,
                    unsafe_arguments,
                    ..
                } => {
                    let normalized = extension.trim_start_matches('.');
                    if normalized.is_empty()
                        || extension.len() > 16
                        || extension.contains('/')
                        || extension.contains('\\')
                        || extension.contains("..")
                    {
                        return Err(RavynError::Invalid(
                            "conversion extensions must contain 1 to 16 path-safe characters"
                                .into(),
                        ));
                    }
                    if arguments.len() > 128
                        || arguments.iter().any(|argument| argument.len() > 4_096)
                    {
                        return Err(RavynError::Invalid(
                            "conversion arguments exceed the configured safety limits".into(),
                        ));
                    }
                    if preset.is_some() && (!arguments.is_empty() || *unsafe_arguments) {
                        return Err(RavynError::Invalid(
                            "named FFmpeg presets may not include arbitrary arguments".into(),
                        ));
                    }
                    if let Some(expected) = preset.and_then(preset_extension)
                        && !normalized.eq_ignore_ascii_case(expected)
                    {
                        return Err(RavynError::Invalid(format!(
                            "the selected FFmpeg preset requires the .{expected} extension"
                        )));
                    }
                    if preset.is_none() && (!*unsafe_arguments || !self.config.allow_unsafe_ffmpeg)
                    {
                        return Err(RavynError::Invalid(
                            "conversion requires a named preset; arbitrary arguments require both unsafe_arguments=true and --allow-unsafe-ffmpeg".into(),
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn validate_download_secret_references(
        &self,
        options: &crate::core::models::DownloadOptions,
    ) -> Result<()> {
        for (id, expected_type) in [
            (options.proxy_secret_id, "proxy_credentials"),
            (options.cookies_secret_id, "cookies"),
            (
                options.authentication_header_secret_id,
                "authentication_header",
            ),
        ] {
            let Some(id) = id else {
                continue;
            };
            let reference = self.repository.get_secret_reference(id).await?;
            if reference.secret_type != expected_type {
                return Err(RavynError::Invalid(format!(
                    "secret reference {id} has type {}, expected {expected_type}",
                    reference.secret_type
                )));
            }
        }
        Ok(())
    }

    pub async fn probe_media(&self, request: &MediaProbeRequest) -> Result<MediaProbe> {
        security::validate_network_source_resolved(&self.config, &request.url).await?;
        if let Some(cookie_file) = request.cookies_file.as_deref() {
            security::validate_regular_file_under(
                cookie_file,
                &self.config.effective_cookie_dir(),
                "media probe cookie file",
            )?;
        }
        self.media.probe(request).await
    }

    pub async fn retry_media_item(&self, job_id: Uuid, item_id: Uuid) -> Result<Job> {
        let parent = self.repository.get_job(job_id).await?;
        if parent.kind != JobKind::Media {
            return Err(RavynError::Conflict(
                "media items can be retried only for media jobs".into(),
            ));
        }
        let item = self.repository.get_job_media_item(job_id, item_id).await?;
        if item.state != "failed" {
            return Err(RavynError::Conflict(
                "only failed media items can be retried".into(),
            ));
        }
        if let Some(retry_job_id) = item.retry_job_id {
            match self.repository.get_job(retry_job_id).await {
                Ok(existing)
                    if !matches!(existing.status, JobStatus::Failed | JobStatus::Cancelled) =>
                {
                    if matches!(existing.status, JobStatus::Completed | JobStatus::Partial) {
                        self.reconcile_media_retry_parent(existing.id).await?;
                    }
                    return Ok(existing);
                }
                Ok(_) | Err(RavynError::NotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let mut media = parent.options_json.media.clone().unwrap_or_default();
        let source = if let Some(url) = item.webpage_url.as_deref() {
            media.playlist = false;
            media.playlist_start = None;
            media.playlist_end = None;
            url.to_owned()
        } else if let Some(index) = item
            .playlist_index
            .and_then(|value| u32::try_from(value).ok())
        {
            media.playlist = true;
            media.playlist_start = Some(index);
            media.playlist_end = Some(index);
            parent.source.clone()
        } else {
            return Err(RavynError::Conflict(
                "the media item has neither a webpage URL nor a retryable playlist index".into(),
            ));
        };
        let mut options = parent.options_json.clone();
        options.media = Some(media);
        let retry = self
            .create(CreateJob {
                kind: JobKind::Media,
                source,
                destination: Some(parent.destination.clone().into()),
                filename: None,
                priority: parent.priority,
                speed_limit_bps: parent
                    .speed_limit_bps
                    .and_then(|value| u64::try_from(value).ok()),
                expected_sha256: None,
                duplicate_policy: DuplicatePolicy::Allow,
                options,
            })
            .await?;
        self.repository
            .set_media_item_retry_job(item_id, retry.id)
            .await?;
        Ok(retry)
    }

    async fn reconcile_media_retry_parent(&self, retry_job_id: Uuid) -> Result<()> {
        let Some(parent_job_id) = self
            .repository
            .complete_media_retry_parent(retry_job_id)
            .await?
        else {
            return Ok(());
        };
        let summary = self.repository.media_item_summary(parent_job_id).await?;
        if summary.failed == 0 && summary.planned == 0 && summary.downloading == 0 {
            let parent = self.repository.get_job(parent_job_id).await?;
            if parent.status == JobStatus::Partial {
                self.repository
                    .set_status(parent_job_id, JobStatus::Completed, None)
                    .await?;
                self.events.publish(Event::JobStatus {
                    job_id: parent_job_id,
                    status: JobStatus::Completed,
                    error: None,
                });
                self.repository
                    .append_job_log(
                        parent_job_id,
                        "media",
                        "info",
                        "PLAYLIST_RETRY_RECOVERED",
                        "all failed media items were recovered by retry jobs",
                    )
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn media_dependencies(&self) -> Vec<DependencyStatus> {
        self.media.dependency_status().await
    }
    pub async fn torrent_dependencies(&self) -> TorrentDependencyStatus {
        self.torrent.dependency_status().await
    }

    pub async fn probe_torrent(&self, request: &TorrentProbeRequest) -> Result<TorrentProbe> {
        self.torrent.probe(request).await
    }

    pub async fn managed_torrents(&self) -> Result<Vec<TorrentRecord>> {
        self.repository.list_torrent_records().await
    }

    pub async fn list_engine_torrents(&self) -> Result<TorrentEngineList> {
        self.torrent.list().await
    }

    pub async fn torrent_engine_stats(&self) -> Result<TorrentGlobalStats> {
        self.torrent.global_stats().await
    }

    pub async fn torrent_dht_stats(&self) -> Result<serde_json::Value> {
        self.torrent.dht_stats().await
    }

    pub async fn torrent_dht_table(&self) -> Result<serde_json::Value> {
        self.torrent.dht_table().await
    }

    pub async fn torrent_details(&self, job_id: Uuid) -> Result<TorrentDetails> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.details(&record.torrent_id).await
    }

    pub async fn torrent_stats(&self, job_id: Uuid) -> Result<TorrentSnapshot> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.stats(&record.torrent_id).await
    }

    pub async fn torrent_peers(&self, job_id: Uuid) -> Result<TorrentPeerStats> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.peer_stats(&record.torrent_id).await
    }

    pub async fn add_torrent_peers(&self, job_id: Uuid, peers: &[String]) -> Result<()> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.add_peers(&record.torrent_id, peers).await
    }

    pub async fn update_torrent_files(&self, job_id: Uuid, files: &[usize]) -> Result<()> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.update_files(&record.torrent_id, files).await
    }

    pub async fn remove_torrent(&self, job_id: Uuid, delete_files: bool) -> Result<()> {
        let job = self.repository.get_job(job_id).await?;
        if job.kind != JobKind::Torrent {
            return Err(RavynError::Invalid(format!(
                "job {job_id} is not a torrent"
            )));
        }
        if !matches!(
            job.status,
            JobStatus::Cancelled
                | JobStatus::Completed
                | JobStatus::Partial
                | JobStatus::Failed
                | JobStatus::Seeding
        ) {
            self.cancel(job_id).await?;
        }
        self.torrent.remove_job(job_id, delete_files).await?;
        if let Some(state) = self.repository.get_torrent_seeding_state(job_id).await? {
            if state.stopped_at.is_none() {
                self.repository
                    .stop_torrent_seeding(job_id, "removed", state.last_ratio)
                    .await?;
            }
        }
        self.repository
            .set_status(job_id, JobStatus::Cancelled, None)
            .await?;
        Ok(())
    }

    pub async fn start_workers(self: Arc<Self>) -> Result<()> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(RavynError::Conflict(
                "background workers have already been started".into(),
            ));
        }
        self.repository.recover_interrupted().await?;

        let receiver = self
            .progress_receiver
            .lock()
            .await
            .take()
            .ok_or_else(|| RavynError::Conflict("progress writer is already running".into()))?;

        let manager = self.clone();
        self.spawn_tracked("dispatcher", async move {
            manager.dispatch_loop().await;
        })
        .await?;

        let repository = self.repository.clone();
        let manager = self.clone();
        let cancel = self.shutdown.child_token();
        self.spawn_tracked("scheduler", async move {
            if let Err(error) = scheduler::run(repository, manager, cancel).await {
                tracing::error!(%error, "scheduler stopped");
            }
        })
        .await?;

        let torrent = self.torrent.clone();
        let cancel = self.shutdown.child_token();
        self.spawn_tracked("torrent-monitor", async move {
            torrent.monitor_managed(cancel).await;
        })
        .await?;

        let repository = self.repository.clone();
        let cancel = self.shutdown.child_token();
        self.spawn_tracked("progress-writer", async move {
            if let Err(error) = progress::run_writer(repository, receiver, cancel).await {
                tracing::error!(%error, "durable progress writer stopped");
            }
        })
        .await?;

        Ok(())
    }

    async fn spawn_tracked<F>(&self, name: impl Into<String>, future: F) -> Result<AbortHandle>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if !self.accepting_tasks.load(Ordering::Acquire) {
            return Err(RavynError::Unavailable(
                "task supervisor is shutting down".into(),
            ));
        }
        let name = name.into();
        let task_name = name.clone();
        let handle = tokio::spawn(async move {
            if AssertUnwindSafe(future).catch_unwind().await.is_err() {
                tracing::error!(task = %task_name, "supervised task panicked");
            }
        });
        let abort = handle.abort_handle();
        let mut tasks = self.tasks.lock().await;
        tasks.retain(|task| !task.handle.is_finished());
        tasks.push(TrackedTask { name, handle });
        Ok(abort)
    }

    pub async fn create(&self, mut request: CreateJob) -> Result<Job> {
        let extension = url::Url::parse(&request.source).ok().and_then(|url| {
            std::path::Path::new(url.path())
                .extension()
                .and_then(|v| v.to_str())
                .map(str::to_owned)
        });
        let loaded_rules = self.repository.list_rules().await?;
        rules::apply_matching(&loaded_rules, &mut request, None, extension.as_deref());
        validate_tags(&request.options.tags)?;
        self.validate_post_actions(&request.options.post_actions)?;
        self.validate_download_secret_references(&request.options)
            .await?;
        if let Some(torrent) = request.options.torrent.as_ref() {
            validate_torrent_options(torrent)?;
        }
        if let Some(cookie_file) = request
            .options
            .media
            .as_ref()
            .and_then(|media| media.cookies_file.as_deref())
        {
            security::validate_regular_file_under(
                cookie_file,
                &self.config.effective_cookie_dir(),
                "media cookie file",
            )?;
        }
        if let Some(browser) = request
            .options
            .media
            .as_ref()
            .and_then(|media| media.cookies_from_browser.as_deref())
        {
            let browser_name = browser.split(['+', ':']).next().unwrap_or_default();
            if browser.len() > 256
                || !matches!(
                    browser_name,
                    "brave"
                        | "chrome"
                        | "chromium"
                        | "edge"
                        | "firefox"
                        | "opera"
                        | "safari"
                        | "vivaldi"
                        | "whale"
                )
                || browser.chars().any(|character| {
                    character.is_control() || matches!(character, '\r' | '\n' | '\0')
                })
            {
                return Err(RavynError::Invalid(
                    "cookies_from_browser contains an unsupported or unsafe browser selector"
                        .into(),
                ));
            }
        }
        if matches!(request.kind, JobKind::Http | JobKind::Media) {
            security::validate_network_source_resolved(&self.config, &request.source).await?;
        }
        if !request.options.mirrors.is_empty() {
            if request.kind != JobKind::Http {
                return Err(RavynError::Invalid(
                    "alternate mirrors are supported only for HTTP jobs".into(),
                ));
            }
            if request.options.mirrors.len() > 16 {
                return Err(RavynError::Invalid(
                    "an HTTP job may define at most 16 mirrors".into(),
                ));
            }
            for mirror in &request.options.mirrors {
                security::validate_network_source_resolved(&self.config, mirror).await?;
            }
        }
        let destination = request
            .destination
            .clone()
            .unwrap_or_else(|| self.config.effective_download_dir());
        security::validate_output_path(&self.config, &destination)?;
        if let Some(existing) = dedup::resolve(
            &self.repository,
            &request,
            &self.config.effective_download_dir(),
        )
        .await?
        {
            return Ok(existing);
        }
        let tags = request.options.tags.clone();
        let job = self
            .repository
            .insert_job(request, self.config.effective_download_dir())
            .await?;
        self.repository.attach_tags(job.id, &tags).await?;
        self.events.publish(Event::QueueChanged);
        Ok(job)
    }

    pub async fn create_idempotent(&self, request: CreateJob, key: &str) -> Result<Job> {
        let key = key.trim();
        if key.is_empty() || key.len() > 200 {
            return Err(RavynError::Invalid(
                "Idempotency-Key must contain between 1 and 200 characters".into(),
            ));
        }
        let request_hash = hex::encode(sha2::Sha256::digest(serde_json::to_vec(&request)?));
        let _guard = self.idempotency.lock().await;
        if let Some((stored_hash, resource_id)) = self
            .repository
            .get_idempotent_resource("create_job", key)
            .await?
        {
            if stored_hash != request_hash {
                return Err(RavynError::Conflict(
                    "Idempotency-Key was already used for a different request".into(),
                ));
            }
            let id = Uuid::parse_str(&resource_id).map_err(|error| {
                RavynError::Internal(format!("stored idempotency resource is invalid: {error}"))
            })?;
            return self.repository.get_job(id).await;
        }
        let job = self.create(request).await?;
        self.repository
            .put_idempotent_resource("create_job", key, &request_hash, job.id)
            .await?;
        Ok(job)
    }
    pub async fn update_job(&self, id: Uuid, request: UpdateJob) -> Result<Job> {
        let current = self.repository.get_job(id).await?;
        let routing_change = request.destination.is_some() || request.filename.is_some();
        if routing_change
            && (!matches!(current.status, JobStatus::Queued | JobStatus::Paused)
                || current.downloaded_bytes != 0)
        {
            return Err(RavynError::Conflict(
                "destination and filename are editable only before data has been written".into(),
            ));
        }
        if let Some(destination) = request.destination.as_deref() {
            security::validate_output_path(&self.config, destination)?;
        }
        if let Some(filename) = request.filename.as_deref() {
            if filename.trim().is_empty()
                || filename.len() > 255
                || filename.chars().any(|value| value.is_control())
                || std::path::Path::new(filename).components().count() != 1
            {
                return Err(RavynError::Invalid(
                    "filename must be a single safe path component".into(),
                ));
            }
        }
        if let Some(tags) = request.tags.as_deref() {
            validate_tags(tags)?;
        }
        let updated = self
            .repository
            .update_job_fields(
                id,
                request.priority,
                request.speed_limit_bps,
                request.destination.as_deref(),
                request.filename.as_deref(),
            )
            .await?;
        if let Some(tags) = request.tags.as_deref() {
            self.repository.replace_job_tags(id, tags).await?;
        }
        self.events.publish(Event::QueueChanged);
        Ok(updated)
    }

    pub async fn pause(&self, id: Uuid) -> Result<()> {
        let job = self.repository.get_job(id).await?;
        let allowed = if job.kind == JobKind::Torrent {
            vec![
                JobStatus::Downloading,
                JobStatus::Probing,
                JobStatus::Seeding,
            ]
        } else {
            vec![JobStatus::Downloading, JobStatus::Probing]
        };
        self.repository
            .transition_status(id, &allowed, JobStatus::Paused, None)
            .await?;
        self.cancel_active_and_wait(id).await?;
        if job.kind == JobKind::Torrent {
            self.torrent.pause_job(id).await?;
        }
        self.events.publish(Event::JobStatus {
            job_id: id,
            status: JobStatus::Paused,
            error: None,
        });
        Ok(())
    }

    pub async fn resume(&self, id: Uuid) -> Result<()> {
        if self.active.lock().await.contains_key(&id) {
            return Err(RavynError::Conflict(
                "the previous worker is still shutting down".into(),
            ));
        }
        let job = self.repository.get_job(id).await?;
        let resume_seeding = job.kind == JobKind::Torrent
            && job.status == JobStatus::Paused
            && self
                .repository
                .get_torrent_seeding_state(id)
                .await?
                .is_some_and(|state| state.stopped_at.is_none());
        let destination_status = if resume_seeding {
            JobStatus::Seeding
        } else {
            JobStatus::Queued
        };
        self.repository
            .transition_status(
                id,
                &[JobStatus::Paused, JobStatus::Failed],
                destination_status,
                None,
            )
            .await?;
        if job.kind == JobKind::Torrent {
            if let Err(error) = self.torrent.resume_job(id).await {
                let _ = self
                    .repository
                    .set_status(id, JobStatus::Paused, Some(&error.to_string()))
                    .await;
                return Err(error);
            }
        }
        if resume_seeding {
            self.events.publish(Event::JobStatus {
                job_id: id,
                status: JobStatus::Seeding,
                error: None,
            });
        } else {
            self.events.publish(Event::QueueChanged);
        }
        Ok(())
    }
    pub async fn cancel(&self, id: Uuid) -> Result<()> {
        let job = self.repository.get_job(id).await?;
        self.repository
            .set_status(id, JobStatus::Cancelled, None)
            .await?;
        self.cancel_active_and_wait(id).await?;
        if job.kind == JobKind::Torrent {
            self.torrent.pause_job(id).await?;
            if let Some(state) = self.repository.get_torrent_seeding_state(id).await? {
                if state.stopped_at.is_none() {
                    self.repository
                        .stop_torrent_seeding(id, "cancelled", state.last_ratio)
                        .await?;
                }
            }
        }
        self.events.publish(Event::JobStatus {
            job_id: id,
            status: JobStatus::Cancelled,
            error: None,
        });
        Ok(())
    }

    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let job = self.repository.get_job(id).await?;
        self.cancel(id).await?;
        if job.kind == JobKind::Torrent {
            let delete_files = job
                .options_json
                .torrent
                .as_ref()
                .is_some_and(|options| options.delete_files_on_remove);
            self.torrent.remove_job(id, delete_files).await?;
        }
        self.repository.delete_job(id).await
    }
    pub async fn retry(&self, id: Uuid) -> Result<()> {
        self.repository
            .transition_status(
                id,
                &[JobStatus::Failed, JobStatus::Cancelled, JobStatus::Partial],
                JobStatus::Queued,
                None,
            )
            .await?;
        let kind = self.repository.get_job(id).await?.kind;
        if kind == JobKind::Torrent {
            self.torrent.resume_job(id).await?;
        }
        self.metrics.job_retried(kind);
        self.events.publish(Event::QueueChanged);
        Ok(())
    }
    async fn cancel_active_and_wait(&self, id: Uuid) -> Result<()> {
        let cancellation = self
            .active
            .lock()
            .await
            .get(&id)
            .map(|active| active.cancellation.clone());
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
        } else {
            return Ok(());
        }

        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            if !self.active.lock().await.contains_key(&id) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                let abort = self
                    .active
                    .lock()
                    .await
                    .get(&id)
                    .and_then(|active| active.abort.clone());
                if let Some(abort) = abort {
                    abort.abort();
                }
                return Err(RavynError::Conflict(
                    "worker did not stop cooperatively within 10 seconds".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub async fn shutdown(&self) {
        self.accepting_tasks.store(false, Ordering::Release);
        self.shutdown.cancel();

        let cancellations = self
            .active
            .lock()
            .await
            .values()
            .map(|active| active.cancellation.clone())
            .collect::<Vec<_>>();
        for cancellation in cancellations {
            cancellation.cancel();
        }

        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while !self.active.lock().await.is_empty() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        let tasks = {
            let mut tasks = self.tasks.lock().await;
            std::mem::take(&mut *tasks)
        };
        for mut task in tasks {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                task.handle.abort();
                let _ = task.handle.await;
                continue;
            }
            match tokio::time::timeout(remaining, &mut task.handle).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(task = %task.name, %error, "supervised task ended with a join error");
                }
                Err(_) => {
                    tracing::warn!(task = %task.name, "supervised task exceeded shutdown deadline");
                    task.handle.abort();
                    let _ = task.handle.await;
                }
            }
        }
    }

    async fn dispatch_loop(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(Duration::from_millis(200));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = self.shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let permit = match self.semaphore.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => continue,
                    };
                    let job = match self.repository.claim_next_queued().await {
                        Ok(Some(job)) => job,
                        Ok(None) => {
                            drop(permit);
                            continue;
                        }
                        Err(error) => {
                            tracing::error!(%error, "queue query failed");
                            drop(permit);
                            continue;
                        }
                    };
                    if let Err(error) = self.clone().spawn_job(job, permit).await {
                        tracing::error!(%error, "failed to supervise claimed job");
                    }
                }
            }
        }
    }

    async fn spawn_job(
        self: Arc<Self>,
        job: Job,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<()> {
        if !self.accepting_tasks.load(Ordering::Acquire) {
            self.repository
                .set_status(job.id, JobStatus::Queued, Some("backend is shutting down"))
                .await?;
            return Ok(());
        }

        let cancellation = self.shutdown.child_token();
        self.active.lock().await.insert(
            job.id,
            ActiveJob {
                cancellation: cancellation.clone(),
                abort: None,
            },
        );

        let manager = self.clone();
        let job_id = job.id;
        let task = async move {
            let execution = AssertUnwindSafe(manager.execute(job, cancellation))
                .catch_unwind()
                .await;
            if execution.is_err() {
                let message = "job worker panicked";
                let _ = manager
                    .repository
                    .set_status(job_id, JobStatus::Failed, Some(message))
                    .await;
                manager.events.publish(Event::JobStatus {
                    job_id,
                    status: JobStatus::Failed,
                    error: Some(message.into()),
                });
            }
            manager.active.lock().await.remove(&job_id);
            drop(permit);
        };

        match self.spawn_tracked(format!("job-{job_id}"), task).await {
            Ok(abort) => {
                if let Some(active) = self.active.lock().await.get_mut(&job_id) {
                    active.abort = Some(abort);
                }
                Ok(())
            }
            Err(error) => {
                self.active.lock().await.remove(&job_id);
                self.repository
                    .set_status(
                        job_id,
                        JobStatus::Queued,
                        Some("task supervisor rejected job"),
                    )
                    .await?;
                Err(error)
            }
        }
    }

    async fn execute(&self, job: Job, token: CancellationToken) {
        let current = match self.repository.get_job(job.id).await {
            Ok(current) => current,
            Err(error) => {
                tracing::warn!(%error, job_id = %job.id, "claimed job disappeared before execution");
                return;
            }
        };
        if current.status != JobStatus::Downloading {
            return;
        }
        let started_at = std::time::Instant::now();
        self.metrics.job_started(job.id, job.kind);
        self.events.publish(Event::JobStatus {
            job_id: job.id,
            status: JobStatus::Downloading,
            error: None,
        });
        let _ = self
            .repository
            .append_job_log(
                job.id,
                "manager",
                "info",
                "JOB_STARTED",
                "job execution started",
            )
            .await;
        let adapter: &dyn DownloadAdapter = match job.kind {
            JobKind::Http => self.http.as_ref(),
            JobKind::Media => self.media.as_ref(),
            JobKind::Torrent => self.torrent.as_ref(),
        };
        let adapter_started = std::time::Instant::now();
        let result = adapter.run(&job, token.clone()).await;
        if job.kind == JobKind::Media {
            self.metrics
                .process_finished("yt_dlp", result.is_ok(), adapter_started.elapsed());
        }
        let current = self
            .repository
            .get_job(job.id)
            .await
            .ok()
            .map(|item| item.status);
        if matches!(current, Some(JobStatus::Paused | JobStatus::Cancelled)) {
            return;
        }

        let result_terminal_status = result
            .as_ref()
            .ok()
            .and_then(|outcome| outcome.terminal_status);
        let result_terminal_message = result
            .as_ref()
            .ok()
            .and_then(|outcome| outcome.terminal_message.clone());
        let final_result = match result {
            Ok(outcome) => {
                async {
                    let verified_primary_checksum =
                        if let Some(path) = outcome.primary_path.as_deref() {
                            if let Some(expected) = job.expected_sha256.as_deref() {
                                let _ = self
                                    .repository
                                    .set_status(job.id, JobStatus::Verifying, None)
                                    .await;
                                Some(checksum::verify_and_return(path, expected, &token).await?)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                    let primary_path = outcome.primary_path.clone();
                    let produced = if outcome.artifacts.is_empty() {
                        let files = if outcome.files.is_empty() {
                            outcome.primary_path.clone().into_iter().collect::<Vec<_>>()
                        } else {
                            outcome.files.clone()
                        };
                        files
                            .into_iter()
                            .map(crate::download::adapter::ProducedArtifact::new)
                            .collect::<Vec<_>>()
                    } else {
                        outcome.artifacts
                    };
                    let mut registered = Vec::with_capacity(produced.len());
                    for produced_artifact in produced {
                        let path = produced_artifact.path;
                        let is_primary = primary_path.as_deref() == Some(path.as_path());
                        let artifact = self
                            .repository
                            .register_output_with_metadata(
                                &job,
                                &path,
                                produced_artifact
                                    .output_type
                                    .unwrap_or_else(|| output_type(job.kind, &path, is_primary)),
                                output_source(job.kind),
                                produced_artifact.metadata,
                            )
                            .await?;
                        if is_primary {
                            if let Some(value) = verified_primary_checksum.as_deref() {
                                self.repository
                                    .set_output_checksum(artifact.id, "sha256", value)
                                    .await?;
                            }
                        }
                        if job.kind == JobKind::Media {
                            if let Some(item_key) = produced_artifact.media_item_key.as_deref() {
                                self.repository
                                    .link_media_item_artifact(
                                        job.id,
                                        item_key,
                                        artifact.id,
                                        produced_artifact.role.as_deref().unwrap_or(
                                            if is_primary { "primary" } else { "auxiliary" },
                                        ),
                                    )
                                    .await?;
                            } else {
                                self.repository
                                    .link_media_item_output(job.id, &path, artifact.id)
                                    .await?;
                            }
                        }
                        if produced_artifact.postprocess {
                            registered.push((artifact.id, path));
                        }
                    }
                    if registered.is_empty() || job.options_json.post_actions.is_empty() {
                        return Ok(());
                    }

                    let _ = self
                        .repository
                        .set_status(job.id, JobStatus::PostProcessing, None)
                        .await;
                    for (file_index, (output_id, path)) in registered.into_iter().enumerate() {
                        let mut current_output_id = output_id;
                        let mut current = path;
                        for (action_index, action) in
                            job.options_json.post_actions.iter().enumerate()
                        {
                            let journal_index = file_index
                                .saturating_mul(job.options_json.post_actions.len())
                                .saturating_add(action_index);
                            if let Some(output) = self
                                .repository
                                .begin_job_action(job.id, journal_index, action, &current)
                                .await?
                            {
                                if !tokio::fs::try_exists(&output).await? {
                                    return Err(RavynError::Internal(format!(
                                        "completed post-processing output is missing: {}",
                                        output.display()
                                    )));
                                }
                                if let Some(artifact) = self
                                    .repository
                                    .find_job_output_by_path(job.id, &output)
                                    .await?
                                {
                                    current_output_id = artifact.id;
                                }
                                current = output;
                                continue;
                            }
                            let action_started = std::time::Instant::now();
                            let action_result = postprocess::pipeline::run(
                                self.config.clone(),
                                current.clone(),
                                std::slice::from_ref(action),
                                token.child_token(),
                            )
                            .await;
                            self.metrics.post_action_finished(
                                post_action_name(action),
                                action_result.is_ok(),
                                action_started.elapsed(),
                            );
                            if let Some(tool) = match action {
                                PostAction::Extract { .. } => Some("seven_zip"),
                                PostAction::ConvertMedia { .. } => Some("ffmpeg"),
                                _ => None,
                            } {
                                self.metrics.process_finished(
                                    tool,
                                    action_result.is_ok(),
                                    action_started.elapsed(),
                                );
                            }
                            match action_result {
                                Ok(output) => {
                                    self.repository
                                        .finish_job_action(
                                            job.id,
                                            journal_index,
                                            Ok(output.as_path()),
                                        )
                                        .await?;
                                    match action {
                                        PostAction::VerifySha256 { expected } => {
                                            self.repository
                                                .set_output_checksum(
                                                    current_output_id,
                                                    "sha256",
                                                    expected,
                                                )
                                                .await?;
                                        }
                                        PostAction::Extract { delete_archive, .. } => {
                                            let derived = self
                                                .repository
                                                .register_derived_output(
                                                    &job,
                                                    current_output_id,
                                                    &output,
                                                    OutputType::Directory,
                                                    journal_index,
                                                    serde_json::json!({
                                                        "action": "extract",
                                                        "source": current
                                                    }),
                                                )
                                                .await?;
                                            if *delete_archive {
                                                self.repository
                                                    .set_output_state(
                                                        current_output_id,
                                                        OutputState::Deleted,
                                                    )
                                                    .await?;
                                            }
                                            current_output_id = derived.id;
                                        }
                                        PostAction::ConvertMedia {
                                            extension,
                                            delete_original,
                                            ..
                                        } => {
                                            let derived = self
                                                .repository
                                                .register_derived_output(
                                                    &job,
                                                    current_output_id,
                                                    &output,
                                                    OutputType::ConvertedFile,
                                                    journal_index,
                                                    serde_json::json!({
                                                        "action": "convert_media",
                                                        "extension": extension,
                                                        "source": current
                                                    }),
                                                )
                                                .await?;
                                            if *delete_original {
                                                self.repository
                                                    .set_output_state(
                                                        current_output_id,
                                                        OutputState::Replaced,
                                                    )
                                                    .await?;
                                            }
                                            current_output_id = derived.id;
                                        }
                                        PostAction::Move { .. } => {
                                            self.repository
                                                .update_output_path(
                                                    &job,
                                                    current_output_id,
                                                    &output,
                                                    OutputState::Moved,
                                                )
                                                .await?;
                                        }
                                        PostAction::Open => {}
                                    }
                                    current = output;
                                }
                                Err(error) => {
                                    let message = error.to_string();
                                    self.repository
                                        .finish_job_action(job.id, journal_index, Err(&message))
                                        .await?;
                                    return Err(error);
                                }
                            }
                        }
                    }
                    Ok(())
                }
                .await
            }
            Err(error) => Err(error),
        };
        let current = self
            .repository
            .get_job(job.id)
            .await
            .ok()
            .map(|item| item.status);
        if matches!(current, Some(JobStatus::Paused | JobStatus::Cancelled)) {
            self.metrics.job_finished(
                job.id,
                job.kind,
                "cancelled",
                started_at.elapsed(),
                Some(crate::error::FailureClass::Cancellation),
            );
            return;
        }
        match final_result {
            Ok(()) => {
                let final_status = result_terminal_status.unwrap_or(JobStatus::Completed);
                let _ = self
                    .repository
                    .set_status(job.id, final_status, result_terminal_message.as_deref())
                    .await;
                self.events.publish(Event::JobStatus {
                    job_id: job.id,
                    status: final_status,
                    error: result_terminal_message.clone(),
                });
                let (severity, code, message) = if final_status == JobStatus::Partial {
                    (
                        "warning",
                        "JOB_PARTIAL",
                        result_terminal_message
                            .as_deref()
                            .unwrap_or("job completed with partial failures"),
                    )
                } else {
                    (
                        "info",
                        "JOB_COMPLETED",
                        "job execution reached its terminal success state",
                    )
                };
                let _ = self
                    .repository
                    .append_job_log(job.id, "manager", severity, code, message)
                    .await;
                if job.kind == JobKind::Media {
                    if let Err(error) = self.reconcile_media_retry_parent(job.id).await {
                        tracing::warn!(%error, job_id = %job.id, "failed to reconcile parent media job after retry");
                    }
                }
                self.metrics.job_finished(
                    job.id,
                    job.kind,
                    if final_status == JobStatus::Partial {
                        "partial"
                    } else {
                        "completed"
                    },
                    started_at.elapsed(),
                    None,
                );
            }
            Err(RavynError::Cancelled) => {
                self.metrics.job_finished(
                    job.id,
                    job.kind,
                    "cancelled",
                    started_at.elapsed(),
                    Some(crate::error::FailureClass::Cancellation),
                );
            }
            Err(RavynError::Unavailable(message)) => {
                let delay = Duration::from_secs(self.config.host_circuit_cooldown_secs);
                if let Err(error) = self.repository.defer_job(job.id, delay, &message).await {
                    tracing::error!(job_id = %job.id, %error, "failed to defer unavailable job");
                    let _ = self
                        .repository
                        .set_status(job.id, JobStatus::Failed, Some(&message))
                        .await;
                } else {
                    self.events.publish(Event::JobStatus {
                        job_id: job.id,
                        status: JobStatus::Queued,
                        error: Some(message),
                    });
                }
                self.metrics.job_finished(
                    job.id,
                    job.kind,
                    "deferred",
                    started_at.elapsed(),
                    Some(crate::error::FailureClass::RetryableHttp),
                );
            }
            Err(error) => {
                let failure_class = error.failure_class();
                let public_message = error.public_message();
                let message = error.to_string();
                if job.kind == JobKind::Media {
                    let _ = self
                        .repository
                        .mark_media_retry_parent_failed(job.id, &public_message)
                        .await;
                }
                let _ = self
                    .repository
                    .set_status(job.id, JobStatus::Failed, Some(&message))
                    .await;
                self.events.publish(Event::JobStatus {
                    job_id: job.id,
                    status: JobStatus::Failed,
                    error: Some(message.clone()),
                });
                let _ = self
                    .repository
                    .append_job_log(job.id, "manager", "error", "JOB_FAILED", &message)
                    .await;
                self.metrics.job_finished(
                    job.id,
                    job.kind,
                    "failed",
                    started_at.elapsed(),
                    Some(failure_class),
                );
            }
        }
    }
}
