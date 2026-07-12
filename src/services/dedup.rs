use std::path::Path;

use crate::{
    core::models::{CreateJob, DuplicatePolicy, Job},
    error::{RavynError, Result},
    storage::Repository,
};

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
        (DuplicatePolicy::ReuseExisting, Some(job)) => Ok(Some(job)),
        _ => Ok(None),
    }
}
