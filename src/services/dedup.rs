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
    let existing = repository
        .find_duplicate(&request.source, &destination)
        .await?;

    match (request.duplicate_policy, existing) {
        (DuplicatePolicy::Reject, Some(_)) => Err(RavynError::Conflict(
            "an equivalent download already exists".into(),
        )),
        (DuplicatePolicy::ReuseExisting | DuplicatePolicy::Skip, Some(job)) => Ok(Some(job)),
        _ => Ok(None),
    }
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
    repository.find_active_library_entry_by_sha256(expected).await
}
