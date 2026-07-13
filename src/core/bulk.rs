//! Page sniffing, text imports, and batch job creation.

use crate::{
    core::models::CreateJob,
    error::{RavynError, Result},
    services::{
        imports::{ImportDefaults, ImportItemResult, ImportResult, ImportTextRequest},
        sniffer::{SniffRequest, SniffResult},
    },
};

use crate::core::manager::JobManager;

impl JobManager {
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
                preset_id: None,
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
}
