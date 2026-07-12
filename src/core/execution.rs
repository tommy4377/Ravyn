//! End-to-end execution of a claimed job through its engine, checksum,
//! and post-processing phases.

use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::{
    core::{
        events::Event,
        models::{Job, JobKind, JobStatus, OutputState, OutputType, PostAction},
    },
    download::adapter::DownloadAdapter,
    error::RavynError,
    postprocess,
    services::checksum,
};

use crate::core::manager::{JobManager, output_source, output_type, post_action_name};

impl JobManager {
    pub(crate) async fn execute(&self, job: Job, token: CancellationToken) {
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
