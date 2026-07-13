//! Media probing, playlist-item retries, and dependency status.

use uuid::Uuid;

use crate::{
    adapters::media::{DependencyStatus, MediaProbe, MediaProbeRequest},
    core::{
        events::Event,
        models::{CreateJob, DuplicatePolicy, Job, JobKind, JobStatus},
    },
    error::{RavynError, Result},
    services::security,
};

use crate::core::manager::JobManager;

impl JobManager {
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
                preset_id: None,
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

    pub(crate) async fn reconcile_media_retry_parent(&self, retry_job_id: Uuid) -> Result<()> {
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
}
