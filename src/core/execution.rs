//! End-to-end execution of a claimed job through its engine, checksum,
//! and post-processing phases.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

use crate::{
    core::{
        events::Event,
        models::{Job, JobKind, JobOutput, JobStatus, OutputState, OutputType, PostAction},
    },
    download::adapter::DownloadAdapter,
    error::RavynError,
    postprocess,
    services::{checksum, library, security},
    storage::NewLibraryEntry,
};

use crate::core::manager::{JobManager, output_source, output_type, post_action_name};

impl JobManager {
    async fn record_library_outputs(
        &self,
        job: &Job,
        cancellation: &CancellationToken,
    ) -> crate::error::Result<()> {
        let outputs = self.repository.list_job_outputs(job.id).await?;
        for mut output in outputs {
            if !matches!(output.state, OutputState::Ready | OutputState::Moved) {
                continue;
            }
            let metadata = match tokio::fs::symlink_metadata(&output.current_path).await {
                Ok(metadata) if !metadata.file_type().is_symlink() => metadata,
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if metadata.is_file() {
                let (path, metadata) = self
                    .organize_primary_library_output(job, &mut output, metadata)
                    .await?;
                self.record_library_file(job, &output, &path, &metadata, true, cancellation)
                    .await?;
                continue;
            }
            if !metadata.is_dir() {
                continue;
            }

            // Extracted directory outputs are expanded into bounded file entries so
            // deleting the source archive never makes the generated content disappear
            // from the persistent library.
            let mut queue = VecDeque::from([(output.current_path.clone(), 0_usize)]);
            let mut visited = 0_usize;
            while let Some((directory, depth)) = queue.pop_front() {
                if cancellation.is_cancelled() {
                    return Err(RavynError::Cancelled);
                }
                let mut entries = tokio::fs::read_dir(&directory).await?;
                while let Some(entry) = entries.next_entry().await? {
                    if cancellation.is_cancelled() {
                        return Err(RavynError::Cancelled);
                    }
                    visited = visited.saturating_add(1);
                    if visited > self.config.max_extract_files {
                        return Err(RavynError::Conflict(format!(
                            "library indexing exceeded the configured {} entry limit",
                            self.config.max_extract_files
                        )));
                    }
                    let path = entry.path();
                    let metadata = tokio::fs::symlink_metadata(&path).await?;
                    if metadata.file_type().is_symlink() {
                        continue;
                    }
                    if metadata.is_dir() {
                        if depth < self.config.max_extract_depth {
                            queue.push_back((path, depth + 1));
                        }
                        continue;
                    }
                    if metadata.is_file() {
                        self.record_library_file(
                            job,
                            &output,
                            &path,
                            &metadata,
                            false,
                            cancellation,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn organize_primary_library_output(
        &self,
        job: &Job,
        output: &mut JobOutput,
        metadata: std::fs::Metadata,
    ) -> crate::error::Result<(PathBuf, std::fs::Metadata)> {
        if !job.options_json.library_auto_destination
            || job.kind != JobKind::Http
            || output.output_type != OutputType::Primary
            || output.state != OutputState::Ready
        {
            return Ok((output.current_path.clone(), metadata));
        }

        let Some(root) = self.config.effective_library_root() else {
            return Ok((output.current_path.clone(), metadata));
        };
        let category = library::classify_file_with_overrides(
            &output.current_path,
            output.mime_type.as_deref(),
            &self.config.library_category_overrides,
        )
        .await?;
        let job_destination = Path::new(&job.destination);
        let Ok(relative) = output.current_path.strip_prefix(job_destination) else {
            return Ok((output.current_path.clone(), metadata));
        };
        if relative.as_os_str().is_empty() {
            return Ok((output.current_path.clone(), metadata));
        }

        let preferred = library::category_directory(&root, category).join(relative);
        if preferred == output.current_path {
            return Ok((output.current_path.clone(), metadata));
        }
        let target = available_organized_path(&preferred).await?;
        security::validate_output_path(&self.config, &target)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        move_regular_file_without_replacement(&output.current_path, &target).await?;
        if let Err(error) = self
            .repository
            .update_output_path(job, output.id, &target, OutputState::Moved)
            .await
        {
            if let Err(rollback_error) =
                move_regular_file_without_replacement(&target, &output.current_path).await
            {
                tracing::error!(
                    %rollback_error,
                    source = %target.display(),
                    destination = %output.current_path.display(),
                    "failed to roll back an automatically organized output"
                );
            }
            return Err(error);
        }

        output.current_path = target.clone();
        output.state = OutputState::Moved;
        let metadata = tokio::fs::symlink_metadata(&target).await?;
        Ok((target, metadata))
    }

    async fn record_library_file(
        &self,
        job: &Job,
        output: &JobOutput,
        path: &Path,
        metadata: &std::fs::Metadata,
        persist_output_checksum: bool,
        cancellation: &CancellationToken,
    ) -> crate::error::Result<()> {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Ok(());
        }
        let existing_sha256 = output
            .checksum_algorithm
            .as_deref()
            .filter(|algorithm| algorithm.eq_ignore_ascii_case("sha256"))
            .and(output.checksum_value.as_deref());
        let sha256 = if persist_output_checksum && existing_sha256.is_some() {
            existing_sha256.map(str::to_owned)
        } else {
            let value = checksum::sha256(path, cancellation).await?;
            if persist_output_checksum {
                self.repository
                    .set_output_checksum(output.id, "sha256", &value)
                    .await?;
            }
            Some(value)
        };
        let category = library::classify_file_with_overrides(
            path,
            output.mime_type.as_deref(),
            &self.config.library_category_overrides,
        )
        .await?;
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                RavynError::Invalid(format!(
                    "library output has no valid filename: {}",
                    path.display()
                ))
            })?
            .to_owned();
        self.repository
            .upsert_library_entry(NewLibraryEntry {
                job_id: Some(job.id),
                source_url: job.source.clone(),
                mirrors: job.options_json.mirrors.clone(),
                sha256,
                size_bytes: Some(metadata.len()),
                path: path.to_path_buf(),
                filename,
                category,
                mime_type: output.mime_type.clone(),
                media_metadata: if job.kind == JobKind::Media {
                    output.metadata.clone()
                } else {
                    serde_json::json!({})
                },
                torrent_metadata: if job.kind == JobKind::Torrent {
                    output.metadata.clone()
                } else {
                    serde_json::json!({})
                },
                tags: job.options_json.tags.clone(),
                trust: None,
                imported: false,
                downloaded_at: chrono::Utc::now(),
            })
            .await?;
        Ok(())
    }

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
            Ok(outcome) => async {
                let verified_primary_checksum = if let Some(path) = outcome.primary_path.as_deref()
                {
                    if let Some(expected) = job.expected_sha256.as_deref() {
                        let _ = self
                            .repository
                            .set_status(job.id, JobStatus::Verifying, None)
                            .await;
                        match checksum::verify_and_return(path, expected, &token).await {
                            Ok(actual) => Some(actual),
                            Err(error)
                                if error.failure_class()
                                    == crate::error::FailureClass::ChecksumMismatch =>
                            {
                                let quarantine =
                                    self.config.data_dir.join("quarantine").join("checksum");
                                match checksum::quarantine_corrupt_output(path, &quarantine, job.id)
                                    .await
                                {
                                    Ok(Some(destination)) => tracing::warn!(
                                        job_id = %job.id,
                                        source = %path.display(),
                                        destination = %destination.display(),
                                        "quarantined output that failed checksum verification"
                                    ),
                                    Ok(None) => {}
                                    Err(cleanup_error) => tracing::error!(
                                        job_id = %job.id,
                                        source = %path.display(),
                                        %cleanup_error,
                                        "failed to isolate output that failed checksum verification"
                                    ),
                                }
                                return Err(error);
                            }
                            Err(error) => return Err(error),
                        }
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
                                    produced_artifact.role.as_deref().unwrap_or(if is_primary {
                                        "primary"
                                    } else {
                                        "auxiliary"
                                    }),
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
                if !registered.is_empty() && !job.options_json.post_actions.is_empty() {
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
                }
                if let Err(error) = self.record_library_outputs(&job, &token).await {
                    tracing::warn!(
                        %error,
                        job_id = %job.id,
                        "download completed but persistent library indexing failed"
                    );
                    let _ = self
                        .repository
                        .append_job_log(
                            job.id,
                            "library",
                            "warning",
                            "LIBRARY_INDEX_FAILED",
                            &error.public_message(),
                        )
                        .await;
                }
                Ok(())
            }
            .await,
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

async fn available_organized_path(preferred: &Path) -> crate::error::Result<PathBuf> {
    if !tokio::fs::try_exists(preferred).await? {
        return Ok(preferred.to_path_buf());
    }

    for suffix in 1_u32..=10_000 {
        let candidate = path_with_numeric_suffix(preferred, suffix)?;
        if !tokio::fs::try_exists(&candidate).await? {
            return Ok(candidate);
        }
    }
    Err(RavynError::Conflict(format!(
        "could not allocate a unique organized path for {}",
        preferred.display()
    )))
}

fn path_with_numeric_suffix(path: &Path, suffix: u32) -> crate::error::Result<PathBuf> {
    let stem = path.file_stem().ok_or_else(|| {
        RavynError::Invalid(format!(
            "automatic organization target has no filename: {}",
            path.display()
        ))
    })?;
    let mut filename = stem.to_os_string();
    filename.push(format!(" ({suffix})"));
    if let Some(extension) = path.extension() {
        filename.push(".");
        filename.push(extension);
    }
    Ok(path.with_file_name(filename))
}

async fn move_regular_file_without_replacement(
    source: &Path,
    destination: &Path,
) -> crate::error::Result<()> {
    let source_metadata = tokio::fs::symlink_metadata(source).await?;
    if !source_metadata.is_file() || source_metadata.file_type().is_symlink() {
        return Err(RavynError::Conflict(format!(
            "automatic organization requires a regular non-symlink file: {}",
            source.display()
        )));
    }

    match tokio::fs::hard_link(source, destination).await {
        Ok(()) => {
            if let Err(error) = tokio::fs::remove_file(source).await {
                let _ = tokio::fs::remove_file(destination).await;
                return Err(error.into());
            }
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RavynError::Conflict(format!(
                "automatic organization target already exists: {}",
                destination.display()
            )));
        }
        Err(_) => {}
    }

    let mut input = tokio::fs::File::open(source).await?;
    let mut output = match tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .await
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RavynError::Conflict(format!(
                "automatic organization target already exists: {}",
                destination.display()
            )));
        }
        Err(error) => return Err(error.into()),
    };
    if let Err(error) = tokio::io::copy(&mut input, &mut output).await {
        drop(output);
        let _ = tokio::fs::remove_file(destination).await;
        return Err(error.into());
    }
    if let Err(error) = output.sync_all().await {
        drop(output);
        let _ = tokio::fs::remove_file(destination).await;
        return Err(error.into());
    }
    drop(output);
    if let Err(error) = tokio::fs::set_permissions(destination, source_metadata.permissions()).await
    {
        let _ = tokio::fs::remove_file(destination).await;
        return Err(error.into());
    }
    if let Err(error) = tokio::fs::remove_file(source).await {
        let _ = tokio::fs::remove_file(destination).await;
        return Err(error.into());
    }
    Ok(())
}
