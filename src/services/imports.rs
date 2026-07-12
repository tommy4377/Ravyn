use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::models::{DownloadOptions, DuplicatePolicy, Job, JobKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportDefaults {
    pub kind: JobKind,
    pub destination: Option<PathBuf>,
    pub priority: i32,
    pub speed_limit_bps: Option<u64>,
    pub duplicate_policy: DuplicatePolicy,
    pub options: DownloadOptions,
}

impl Default for ImportDefaults {
    fn default() -> Self {
        Self {
            kind: JobKind::Http,
            destination: None,
            priority: 0,
            speed_limit_bps: None,
            duplicate_policy: DuplicatePolicy::ReuseExisting,
            options: DownloadOptions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ImportTextRequest {
    pub text: String,
    pub defaults: ImportDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportItemResult {
    pub source: String,
    pub job: Option<Job>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub accepted: usize,
    pub rejected: usize,
    pub duplicate_lines: usize,
    /// True when valid unique input lines exceeded the configured batch limit.
    pub truncated: bool,
    pub items: Vec<ImportItemResult>,
}

impl ImportResult {
    pub fn redact_sensitive(mut self) -> Self {
        for item in &mut self.items {
            item.job = item.job.take().map(Job::redacted);
        }
        self
    }
}

pub fn parse_lines(text: &str, maximum: usize) -> (Vec<String>, usize, bool) {
    let mut seen = std::collections::HashSet::new();
    let mut urls = Vec::new();
    let mut duplicates = 0;
    let mut truncated = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if seen.insert(line.to_owned()) {
            if urls.len() < maximum {
                urls.push(line.to_owned());
            } else {
                truncated = true;
            }
        } else {
            duplicates += 1;
        }
    }
    (urls, duplicates, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_empty_lines_and_duplicates() {
        let (items, duplicates, truncated) = parse_lines(
            "# comment\nhttps://a.test/file\r\n\nhttps://a.test/file\nhttps://b.test/file",
            100,
        );
        assert_eq!(items.len(), 2);
        assert_eq!(duplicates, 1);
        assert!(!truncated);
    }

    #[test]
    fn reports_truncated_unique_lines() {
        let (items, duplicates, truncated) =
            parse_lines("https://a.test\nhttps://b.test\nhttps://c.test", 2);
        assert_eq!(items.len(), 2);
        assert_eq!(duplicates, 0);
        assert!(truncated);
    }
}
