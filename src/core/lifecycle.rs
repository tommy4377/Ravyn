//! Job creation, editing, pause/resume/cancel/retry, and deletion.

use sha2::Digest;
use std::time::Duration;
use uuid::Uuid;

use crate::{
    core::{
        events::Event,
        models::{CreateJob, Job, JobKind, JobStatus, UpdateJob},
    },
    error::{RavynError, Result},
    services::{
        dedup,
        rules::{self},
        security,
    },
};

use crate::core::manager::{JobManager, validate_tags, validate_torrent_options};

impl JobManager {
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
                return Err(RavynError::Conflict(
                    "worker did not stop cooperatively within 10 seconds".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}
