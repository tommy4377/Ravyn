use std::path::PathBuf;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::{
    core::models::{Job, JobStatus, OutputType},
    error::Result,
};

#[derive(Debug, Clone)]
pub struct ProducedArtifact {
    pub path: PathBuf,
    pub output_type: Option<OutputType>,
    pub media_item_key: Option<String>,
    pub role: Option<String>,
    pub metadata: serde_json::Value,
    pub postprocess: bool,
}

impl ProducedArtifact {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            output_type: None,
            media_item_key: None,
            role: None,
            metadata: serde_json::Value::Object(Default::default()),
            postprocess: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DownloadOutcome {
    /// Primary file produced by the adapter, when a single deterministic file exists.
    pub primary_path: Option<PathBuf>,
    /// Every file produced by the adapter, including playlist entries.
    pub files: Vec<PathBuf>,
    /// Typed artifacts with provenance and auxiliary-output metadata.
    pub artifacts: Vec<ProducedArtifact>,
    /// Optional terminal status requested by adapters with a post-download lifecycle.
    pub terminal_status: Option<JobStatus>,
    /// Optional non-fatal terminal message, such as a partially failed playlist.
    pub terminal_message: Option<String>,
}

#[async_trait]
pub trait DownloadAdapter: Send + Sync {
    async fn run(&self, job: &Job, cancellation: CancellationToken) -> Result<DownloadOutcome>;
}
