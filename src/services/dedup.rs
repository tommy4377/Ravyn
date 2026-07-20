use std::path::Path;

use crate::{
    core::models::{CreateJob, DuplicatePolicy, Job},
    error::{RavynError, Result},
    storage::{LibraryEntry, Repository},
};

/// Resolves duplicate jobs that already exist in the queue or history.
pub async fn resolve(
    repository: &Repository,
    request: &CreateJob,
    default_destination: &Path,
) -> Result<Option<Job>> {
    let destination = request
        .destination
        .as_deref()
        .unwrap_or(default_destination)
        .to_string_lossy()
        .to_string();
    let expected_fingerprint = request_fingerprint(request, &destination)?;
    let existing = repository
        .find_duplicate_candidates(&request.source, &destination, 50)
        .await?
        .into_iter()
        .find(|job| job_fingerprint(job).is_ok_and(|value| value == expected_fingerprint));

    match (request.duplicate_policy, existing) {
        (DuplicatePolicy::Reject, Some(_)) => Err(RavynError::Conflict(
            "an equivalent download already exists".into(),
        )),
        (DuplicatePolicy::ReuseExisting | DuplicatePolicy::Skip, Some(job)) => Ok(Some(job)),
        _ => Ok(None),
    }
}

fn request_fingerprint(request: &CreateJob, destination: &str) -> Result<String> {
    let mut media = request.options.media.clone();
    if let Some(media) = media.as_mut() {
        media.cookies_from_browser = None;
        media.cookies_file = None;
    }
    Ok(serde_json::to_string(&serde_json::json!({
        "kind": request.kind,
        "source": &request.source,
        "destination": destination,
        "filename": &request.filename,
        "expected_sha256": &request.expected_sha256,
        "media": media,
        "torrent": &request.options.torrent,
        "metalink": &request.options.metalink,
        "post_actions": &request.options.post_actions,
    }))?)
}

fn job_fingerprint(job: &Job) -> Result<String> {
    let mut media = job.options_json.media.clone();
    if let Some(media) = media.as_mut() {
        media.cookies_from_browser = None;
        media.cookies_file = None;
    }
    Ok(serde_json::to_string(&serde_json::json!({
        "kind": job.kind,
        "source": &job.source,
        "destination": &job.destination,
        "filename": &job.filename,
        "expected_sha256": &job.expected_sha256,
        "media": media,
        "torrent": &job.options_json.torrent,
        "metalink": &job.options_json.metalink,
        "post_actions": &job.options_json.post_actions,
    }))?)
}

/// Finds a verified local object that can satisfy the request without network transfer.
pub async fn cache_candidate(
    repository: &Repository,
    request: &CreateJob,
) -> Result<Option<LibraryEntry>> {
    if !matches!(
        request.duplicate_policy,
        DuplicatePolicy::ReuseExisting | DuplicatePolicy::Skip
    ) {
        return Ok(None);
    }
    let Some(expected) = request.expected_sha256.as_deref() else {
        return Ok(None);
    };
    repository
        .find_active_library_entry_by_sha256(expected)
        .await
}
