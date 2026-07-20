//! Job creation, editing, pause/resume/cancel/retry, and deletion.

use sha2::Digest;
use std::time::Duration;
use uuid::Uuid;

use crate::{
    core::{
        events::Event,
        models::{CreateJob, DuplicatePolicy, Job, JobKind, JobStatus, UpdateJob},
    },
    error::{RavynError, Result},
    services::{
        checksum, dedup,
        library::{LibraryCategory, category_directory, classify_name_with_overrides},
        presets,
        rules::{self},
        security,
    },
};

use crate::core::manager::{JobManager, validate_tags, validate_torrent_options};

fn automatic_library_destination(
    config: &crate::config::Config,
    request: &CreateJob,
) -> Option<std::path::PathBuf> {
    let root = config
        .library_auto_organize
        .then(|| config.effective_library_root())
        .flatten()?;
    let candidate = request
        .filename
        .as_ref()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            url::Url::parse(&request.source).ok().and_then(|url| {
                url.path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .filter(|value| !value.is_empty())
                    .map(std::path::PathBuf::from)
            })
        });
    let category = candidate
        .as_deref()
        .and_then(|path| classify_name_with_overrides(path, &config.library_category_overrides))
        .unwrap_or(match request.kind {
            JobKind::Media
                if request
                    .options
                    .media
                    .as_ref()
                    .is_some_and(|media| media.audio_only) =>
            {
                LibraryCategory::Music
            }
            JobKind::Media
                if request
                    .options
                    .media
                    .as_ref()
                    .is_some_and(|media| media.playlist) =>
            {
                LibraryCategory::Playlists
            }
            JobKind::Media => LibraryCategory::Videos,
            JobKind::Torrent => LibraryCategory::Torrents,
            JobKind::Http => LibraryCategory::Downloads,
        });
    Some(category_directory(&root, category))
}

impl JobManager {
    pub async fn create(&self, request: CreateJob) -> Result<Job> {
        self.create_internal(request, None).await
    }

    async fn create_internal(&self, mut request: CreateJob, forced_id: Option<Uuid>) -> Result<Job> {
        if self.repository.library_move_blocks_new_jobs().await? {
            return Err(RavynError::Conflict(
                "new downloads are paused while the Library is moving or waiting for restart"
                    .into(),
            ));
        }
        if request.preset_id.is_none() {
            request.preset_id = self
                .repository
                .get_active_user_profile()
                .await?
                .and_then(|profile| profile.default_preset_id);
        }
        let preset_subdirectory = if let Some(preset_id) = request.preset_id {
            let preset = self.repository.get_download_preset(preset_id).await?;
            presets::apply(&preset, &mut request)?
        } else {
            None
        };
        let extension = url::Url::parse(&request.source).ok().and_then(|url| {
            std::path::Path::new(url.path())
                .extension()
                .and_then(|v| v.to_str())
                .map(str::to_owned)
        });
        let loaded_rules = self.repository.list_rules().await?;
        rules::apply_matching(&loaded_rules, &mut request, None, extension.as_deref());
        if request.duplicate_policy == DuplicatePolicy::Overwrite {
            request.options.overwrite = true;
        }
        validate_tags(&request.options.tags)?;
        validate_filename(request.filename.as_deref())?;
        if let Some(expected) = request.expected_sha256.as_deref() {
            checksum::validate_sha256(expected)?;
        }
        if request
            .speed_limit_bps
            .is_some_and(|value| value > i64::MAX as u64)
        {
            return Err(RavynError::Invalid(
                "speed limit exceeds SQLite integer range".into(),
            ));
        }
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
        if let Some(metalink) = request.options.metalink.as_ref() {
            if request.kind != JobKind::Http || metalink.size == 0 {
                return Err(RavynError::Invalid(
                    "Metalink metadata requires an HTTP job with a positive size".into(),
                ));
            }
            match metalink.piece_length {
                Some(length) if length > 0 => {
                    let expected = metalink.size.div_ceil(length);
                    if metalink.piece_sha256.len() as u64 != expected
                        || metalink.piece_sha256.len() > 16_384
                        || metalink.piece_sha256.iter().any(|hash| {
                            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                    {
                        return Err(RavynError::Invalid(
                            "Metalink piece hashes do not match the declared layout".into(),
                        ));
                    }
                }
                None if metalink.piece_sha256.is_empty() => {}
                _ => {
                    return Err(RavynError::Invalid(
                        "Metalink piece hashes require a positive piece length".into(),
                    ));
                }
            }
        }
        let automatic_destination = if request.destination.is_none() {
            automatic_library_destination(&self.config, &request)
        } else {
            None
        };
        request.options.library_auto_destination = automatic_destination.is_some();
        let mut destination = request
            .destination
            .clone()
            .or(automatic_destination)
            .unwrap_or_else(|| self.config.effective_download_dir());
        if let Some(subdirectory) = preset_subdirectory.as_ref() {
            destination = destination.join(subdirectory);
        }
        request.destination = Some(destination.clone());
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
        let cache_candidate = match dedup::cache_candidate(&self.repository, &request).await? {
            Some(entry) => match self.cache_entry_is_verified(&entry, &request).await {
                Ok(true) => Some(entry),
                Ok(false) => None,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        entry_id = %entry.id,
                        "local cache candidate could not be verified and will not be reused"
                    );
                    None
                }
            },
            None => None,
        };
        if request.options.library_auto_destination {
            if let (Some(entry), Some(root)) = (
                cache_candidate.as_ref(),
                self.config.effective_library_root(),
            ) {
                let mut cache_destination = category_directory(&root, entry.category);
                if let Some(subdirectory) = preset_subdirectory.as_ref() {
                    cache_destination = cache_destination.join(subdirectory);
                }
                security::validate_output_path(&self.config, &cache_destination)?;
                request.destination = Some(cache_destination);
            }
        }
        let tags = request.options.tags.clone();
        let requested_initial_pause = request.options.initially_paused;
        if cache_candidate.is_some() {
            // Keep the dispatcher away from the job until the verified local
            // object has been staged as a durable completed transfer checkpoint.
            request.options.initially_paused = true;
        }
        let job = if let Some(id) = forced_id {
            self.repository
                .insert_job_with_tags_id(
                    request,
                    self.config.effective_download_dir(),
                    &tags,
                    id,
                )
                .await?
        } else {
            self.repository
                .insert_job_with_tags(request, self.config.effective_download_dir(), &tags)
                .await?
        };
        if let Some(entry) = cache_candidate {
            match self.stage_cached_entry(job.clone(), entry).await {
                Ok(staged) => {
                    if !requested_initial_pause {
                        self.repository
                            .transition_status(
                                staged.id,
                                &[JobStatus::Paused],
                                JobStatus::Queued,
                                None,
                            )
                            .await?;
                        self.events.publish(Event::QueueChanged);
                    }
                    return self.repository.get_job(staged.id).await;
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = self
                        .repository
                        .set_status(job.id, JobStatus::Failed, Some(&message))
                        .await;
                    return Err(error);
                }
            }
        }
        self.events.publish(Event::QueueChanged);
        Ok(job)
    }

    async fn cache_entry_is_verified(
        &self,
        entry: &crate::storage::LibraryEntry,
        request: &CreateJob,
    ) -> Result<bool> {
        let Some(expected) = request.expected_sha256.as_deref() else {
            return Ok(false);
        };
        security::validate_output_path(&self.config, &entry.path)?;
        let metadata = match tokio::fs::symlink_metadata(&entry.path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Ok(false);
        }
        if entry.size_bytes.is_some_and(|size| size != metadata.len()) {
            return Ok(false);
        }
        let actual =
            checksum::sha256(&entry.path, &tokio_util::sync::CancellationToken::new()).await?;
        Ok(actual.eq_ignore_ascii_case(expected))
    }

    async fn stage_cached_entry(
        &self,
        job: Job,
        entry: crate::storage::LibraryEntry,
    ) -> Result<Job> {
        security::validate_output_path(&self.config, &entry.path)?;
        let source_metadata = tokio::fs::symlink_metadata(&entry.path).await?;
        if !source_metadata.is_file() || source_metadata.file_type().is_symlink() {
            return Err(RavynError::Conflict(
                "the cached library object is not a regular file".into(),
            ));
        }
        let destination = std::path::PathBuf::from(&job.destination);
        tokio::fs::create_dir_all(&destination).await?;
        let filename = job
            .filename
            .clone()
            .unwrap_or_else(|| crate::services::filename::sanitize(&entry.filename));
        let target = destination.join(&filename);
        security::validate_output_path(&self.config, &target)?;
        if target != entry.path {
            materialize_cached_file(&entry.path, &target, job.options_json.overwrite).await?;
        }
        if job.filename.as_deref() != Some(filename.as_str()) {
            self.repository
                .update_job_fields(job.id, None, None, None, Some(&filename), None)
                .await?;
        }
        self.repository
            .update_progress(job.id, source_metadata.len(), Some(source_metadata.len()))
            .await?;
        self.repository.set_transfer_mode(job.id, "complete").await?;
        self.repository
            .increment_stat_counter("duplicate_avoidance_count", 1)
            .await?;
        self.repository
            .increment_stat_counter("saved_bandwidth_bytes", source_metadata.len())
            .await?;
        self.events.publish(Event::Progress(
            crate::core::models::ProgressSnapshot {
                job_id: job.id,
                downloaded_bytes: source_metadata.len(),
                total_bytes: Some(source_metadata.len()),
                bytes_per_second: 0,
            },
        ));
        self.repository.get_job(job.id).await
    }

    pub async fn create_idempotent(&self, request: CreateJob, key: &str) -> Result<Job> {
        let key = key.trim();
        if key.is_empty() || key.len() > 200 {
            return Err(RavynError::Invalid(
                "Idempotency-Key must contain between 1 and 200 characters".into(),
            ));
        }
        let request_hash = hex::encode(sha2::Sha256::digest(serde_json::to_vec(&request)?));
        let lock_key = format!("create_job:{key}");
        let key_lock = {
            let mut locks = self.idempotency.lock().await;
            locks
                .entry(lock_key.clone())
                .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let guard = key_lock.lock().await;
        let result = async {
            let reservation = Uuid::new_v4();
            let (stored_hash, resource_id) = self
                .repository
                .reserve_idempotent_resource("create_job", key, &request_hash, reservation)
                .await?;
            if stored_hash != request_hash {
                return Err(RavynError::Conflict(
                    "Idempotency-Key was already used for a different request".into(),
                ));
            }
            let reserved_id = Uuid::parse_str(&resource_id).map_err(|error| {
                RavynError::Internal(format!("stored idempotency resource is invalid: {error}"))
            })?;
            match self.repository.get_job(reserved_id).await {
                Ok(job) => return Ok(job),
                Err(RavynError::NotFound(_)) => {}
                Err(error) => return Err(error),
            }

            let job = self.create_internal(request, Some(reserved_id)).await?;
            if job.id != reserved_id {
                self.repository
                    .update_idempotent_resource("create_job", key, &request_hash, job.id)
                    .await?;
            }
            Ok(job)
        }
        .await;
        drop(guard);
        let mut locks = self.idempotency.lock().await;
        if std::sync::Arc::strong_count(&key_lock) == 2 {
            locks.remove(&lock_key);
        }
        result
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
        validate_filename(request.filename.as_deref())?;
        if let Some(tags) = request.tags.as_deref() {
            validate_tags(tags)?;
        }
        let updated_options = request.destination.as_ref().map(|_| {
            let mut options = current.options_json.clone();
            options.library_auto_destination = false;
            options
        });
        let updated = self
            .repository
            .update_job_fields(
                id,
                request.priority,
                request.speed_limit_bps,
                request.destination.as_deref(),
                request.filename.as_deref(),
                updated_options.as_ref(),
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
                JobStatus::Queued,
                JobStatus::Downloading,
                JobStatus::Probing,
                JobStatus::Seeding,
            ]
        } else {
            vec![
                JobStatus::Queued,
                JobStatus::Downloading,
                JobStatus::Probing,
            ]
        };

        // Pause the external torrent engine before committing the persisted
        // state. If the database transition fails, resume the torrent as a
        // best-effort compensation so runtime and storage do not diverge.
        if job.kind == JobKind::Torrent {
            self.torrent.pause_job(id).await?;
        }
        if let Err(error) = self
            .repository
            .transition_status(id, &allowed, JobStatus::Paused, None)
            .await
        {
            if job.kind == JobKind::Torrent {
                let _ = self.torrent.resume_job(id).await;
            }
            return Err(error);
        }

        // Once the durable state is Paused, the worker may be force-aborted
        // after the cooperative grace period. A late worker observes the
        // persisted pause state and cannot legitimately complete the job.
        self.cancel_active_and_wait(id).await?;
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
                    .set_status(id, job.status, job.error.as_deref())
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

        // A torrent must be stopped before its durable state is changed. This
        // avoids reporting Cancelled while the torrent engine is still active
        // if the external pause operation fails.
        if job.kind == JobKind::Torrent {
            self.torrent.pause_job(id).await?;
        }
        if let Err(error) = self
            .repository
            .set_status(id, JobStatus::Cancelled, None)
            .await
        {
            if job.kind == JobKind::Torrent {
                let _ = self.torrent.resume_job(id).await;
            }
            return Err(error);
        }

        self.cancel_active_and_wait(id).await?;
        if job.kind == JobKind::Torrent {
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
        let job = self.repository.get_job(id).await?;
        self.repository
            .transition_status(
                id,
                &[JobStatus::Failed, JobStatus::Cancelled, JobStatus::Partial],
                JobStatus::Queued,
                None,
            )
            .await?;

        // Stale byte counters from the failed attempt would otherwise leave a
        // frozen progress bar until the restarted transfer reports fresh
        // snapshots (resumed transfers jump back to their real offset).
        self.repository.update_progress(id, 0, None).await?;
        self.events
            .publish(Event::Progress(crate::core::models::ProgressSnapshot {
                job_id: id,
                downloaded_bytes: 0,
                total_bytes: job.total_bytes.and_then(|bytes| u64::try_from(bytes).ok()),
                bytes_per_second: 0,
            }));

        if job.kind == JobKind::Torrent {
            if let Err(error) = self.torrent.resume_job(id).await {
                // Restore both state and counters when the external torrent
                // engine cannot resume. This keeps the retry operation
                // transactional from the API caller's perspective.
                let _ = self
                    .repository
                    .set_status(id, job.status, job.error.as_deref())
                    .await;
                let _ = self
                    .repository
                    .update_progress(
                        id,
                        u64::try_from(job.downloaded_bytes).unwrap_or_default(),
                        job.total_bytes.and_then(|bytes| u64::try_from(bytes).ok()),
                    )
                    .await;
                self.events
                    .publish(Event::Progress(crate::core::models::ProgressSnapshot {
                        job_id: id,
                        downloaded_bytes: u64::try_from(job.downloaded_bytes).unwrap_or_default(),
                        total_bytes: job.total_bytes.and_then(|bytes| u64::try_from(bytes).ok()),
                        bytes_per_second: 0,
                    }));
                return Err(error);
            }
        }
        self.metrics.job_retried(job.kind);
        self.events.publish(Event::QueueChanged);
        Ok(())
    }
    pub(crate) async fn cancel_active_and_wait(&self, id: Uuid) -> Result<()> {
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
                self.active.lock().await.remove(&id);
                self.metrics.job_suspended(id);
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

async fn materialize_cached_file(
    source: &std::path::Path,
    target: &std::path::Path,
    overwrite: bool,
) -> Result<()> {
    let target_exists = tokio::fs::try_exists(target).await?;
    if target_exists && !overwrite {
        return Err(RavynError::Conflict(format!(
            "cache target already exists: {}",
            target.display()
        )));
    }
    if target_exists && tokio::fs::symlink_metadata(target).await?.is_dir() {
        return Err(RavynError::Conflict(format!(
            "cache target is a directory: {}",
            target.display()
        )));
    }

    let parent = target
        .parent()
        .ok_or_else(|| RavynError::Invalid("cache target has no parent directory".into()))?;
    let nonce = Uuid::new_v4();
    let temporary = parent.join(format!(".ravyn-cache-{nonce}.tmp"));
    let backup = parent.join(format!(".ravyn-cache-{nonce}.bak"));

    // Cache materialization must produce an independent file. Hard links would
    // allow later edits to the download to mutate the Library object and break
    // its stored checksum. A future platform-specific reflink optimization may
    // replace this copy while preserving copy-on-write semantics.
    if let Err(error) = tokio::fs::copy(source, &temporary).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }

    if !target_exists {
        if let Err(error) = tokio::fs::rename(&temporary, target).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error.into());
        }
        return Ok(());
    }

    // Windows cannot atomically rename over an existing destination. Move the
    // old file aside only after the replacement has been fully materialized,
    // then restore it if activation fails. This guarantees a copy/link error
    // never destroys the user's pre-existing target.
    if let Err(error) = tokio::fs::rename(target, &backup).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    match tokio::fs::rename(&temporary, target).await {
        Ok(()) => {
            let _ = tokio::fs::remove_file(&backup).await;
            Ok(())
        }
        Err(error) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            if let Err(restore_error) = tokio::fs::rename(&backup, target).await {
                return Err(RavynError::Internal(format!(
                    "failed to activate cached file ({error}) and restore the original target ({restore_error})"
                )));
            }
            Err(error.into())
        }
    }
}

fn validate_filename(filename: Option<&str>) -> Result<()> {
    if let Some(filename) = filename {
        // `components().count() != 1` alone does not reject ".." or "." —
        // both parse to exactly one component (ParentDir / CurDir
        // respectively), so a filename of ".." previously passed this check
        // unmodified. Require that single component to be `Normal`.
        let mut components = std::path::Path::new(filename).components();
        let single_normal_component = matches!(
            (components.next(), components.next()),
            (Some(std::path::Component::Normal(_)), None)
        );
        if filename.trim().is_empty()
            || filename.len() > 255
            || filename.chars().any(|value| value.is_control())
            || !single_normal_component
        {
            return Err(RavynError::Invalid(
                "filename must be a single safe path component".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod validate_filename_tests {
    use super::validate_filename;

    #[test]
    fn accepts_a_normal_filename() {
        assert!(validate_filename(Some("report.pdf")).is_ok());
    }

    #[test]
    fn accepts_none() {
        assert!(validate_filename(None).is_ok());
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        assert!(validate_filename(Some("..")).is_err());
    }

    #[test]
    fn rejects_current_dir() {
        assert!(validate_filename(Some(".")).is_err());
    }

    #[test]
    fn rejects_multi_component_paths() {
        assert!(validate_filename(Some("a/b")).is_err());
        assert!(validate_filename(Some("../secret")).is_err());
    }

    #[test]
    fn rejects_empty_and_control_characters() {
        assert!(validate_filename(Some("  ")).is_err());
        assert!(validate_filename(Some("a\0b")).is_err());
    }
}
