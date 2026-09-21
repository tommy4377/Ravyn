use std::{collections::BTreeMap, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Http,
    Media,
    Torrent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Probing,
    Downloading,
    Paused,
    Verifying,
    PostProcessing,
    Seeding,
    Completed,
    Partial,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    Reject,
    ReuseExisting,
    Skip,
    Overwrite,
    #[default]
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaOptions {
    pub format: Option<String>,
    pub max_height: Option<u32>,
    #[serde(default)]
    pub audio_only: bool,
    pub audio_format: Option<String>,
    pub audio_quality: Option<String>,
    pub merge_output_format: Option<String>,
    #[serde(default)]
    pub playlist: bool,
    pub playlist_start: Option<u32>,
    pub playlist_end: Option<u32>,
    #[serde(default)]
    pub write_subtitles: bool,
    #[serde(default)]
    pub write_automatic_subtitles: bool,
    #[serde(default)]
    pub subtitle_languages: Vec<String>,
    #[serde(default)]
    pub embed_subtitles: bool,
    #[serde(default)]
    pub write_thumbnail: bool,
    #[serde(default)]
    pub embed_thumbnail: bool,
    #[serde(default)]
    pub write_info_json: bool,
    #[serde(default)]
    pub write_description: bool,
    #[serde(default)]
    pub embed_metadata: bool,
    #[serde(default)]
    pub sponsorblock_remove: Vec<String>,
    pub concurrent_fragments: Option<u16>,
    pub cookies_from_browser: Option<String>,
    pub cookies_file: Option<PathBuf>,
    pub output_template: Option<String>,
}

impl Default for MediaOptions {
    fn default() -> Self {
        Self {
            format: None,
            max_height: None,
            audio_only: false,
            audio_format: None,
            audio_quality: None,
            merge_output_format: Some("mkv".into()),
            playlist: false,
            playlist_start: None,
            playlist_end: None,
            write_subtitles: false,
            write_automatic_subtitles: false,
            subtitle_languages: Vec::new(),
            embed_subtitles: false,
            write_thumbnail: false,
            embed_thumbnail: false,
            write_info_json: false,
            write_description: false,
            embed_metadata: true,
            sponsorblock_remove: Vec::new(),
            concurrent_fragments: Some(4),
            cookies_from_browser: None,
            cookies_file: None,
            output_template: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TorrentOptions {
    /// File indexes to download. An empty list downloads every file.
    pub selected_files: Vec<usize>,
    /// Optional rqbit-compatible regular expression used during torrent creation.
    pub file_regex: Option<String>,
    /// Allow rqbit to overwrite existing files in the destination.
    pub overwrite: bool,
    /// Keep the torrent registered in rqbit after the download completes.
    pub keep_managed: bool,
    /// Keep seeding after Ravyn marks the download as complete.
    pub seed_after_download: bool,
    /// Delete downloaded files when a torrent job is explicitly deleted.
    pub delete_files_on_remove: bool,
    /// Polling interval for torrent statistics.
    pub poll_interval_ms: u64,
    /// Optional upload/download ratio after which seeding is stopped.
    pub max_seed_ratio: Option<f64>,
    /// Optional maximum amount of time spent seeding after download completion.
    pub max_seed_time_secs: Option<u64>,
    /// Minimum seeding duration before ratio or time policies may stop the torrent.
    pub min_seed_time_secs: u64,
}

impl Default for TorrentOptions {
    fn default() -> Self {
        Self {
            selected_files: Vec::new(),
            file_regex: None,
            overwrite: false,
            keep_managed: true,
            seed_after_download: true,
            delete_files_on_remove: false,
            poll_interval_ms: 1_000,
            max_seed_ratio: None,
            max_seed_time_secs: None,
            min_seed_time_secs: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DownloadOptions {
    /// Alternate HTTP mirrors tried in order when the primary source fails.
    pub mirrors: Vec<String>,
    /// Verified identity and piece layout imported from a Metalink v4 file.
    pub metalink: Option<MetalinkMetadata>,
    pub headers: BTreeMap<String, String>,
    pub cookies: BTreeMap<String, String>,
    pub proxy: Option<String>,
    /// Reference to a `proxy_credentials` entry in the platform secret store.
    pub proxy_secret_id: Option<Uuid>,
    /// Reference to a `cookies` entry containing a JSON object of cookie name/value pairs.
    pub cookies_secret_id: Option<Uuid>,
    /// Reference to an `authentication_header` entry containing the Authorization value.
    pub authentication_header_secret_id: Option<Uuid>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub segments: Option<usize>,
    pub overwrite: bool,
    /// Internal marker set when Ravyn selected the destination from the library layout.
    /// Client-provided values are ignored during job creation.
    pub library_auto_destination: bool,
    /// Creates the job in the paused state before the dispatcher can claim it.
    #[serde(default)]
    pub initially_paused: bool,
    pub tags: Vec<String>,
    pub post_actions: Vec<PostAction>,
    pub media: Option<MediaOptions>,
    pub torrent: Option<TorrentOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetalinkMetadata {
    pub size: u64,
    pub piece_length: Option<u64>,
    #[serde(default)]
    pub piece_sha256: Vec<String>,
}

impl DownloadOptions {
    pub fn redact_sensitive(&mut self) {
        self.headers.clear();
        self.cookies.clear();
        self.proxy = None;
        if let Some(media) = self.media.as_mut() {
            media.cookies_from_browser = None;
            media.cookies_file = None;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PostAction {
    VerifySha256 {
        expected: String,
    },
    Extract {
        destination: Option<PathBuf>,
        delete_archive: bool,
    },
    ConvertMedia {
        extension: String,
        #[serde(default)]
        preset: Option<FfmpegPreset>,
        #[serde(default)]
        arguments: Vec<String>,
        #[serde(default)]
        unsafe_arguments: bool,
        delete_original: bool,
    },
    Move {
        destination: PathBuf,
    },
    Open,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FfmpegPreset {
    VideoCopy,
    VideoH264,
    VideoH265,
    VideoAv1,
    AudioMp3,
    AudioAac,
    AudioOpus,
    AudioFlac,
    ImageAvif,
    ImageWebp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJob {
    #[serde(default)]
    pub preset_id: Option<Uuid>,
    pub kind: JobKind,
    pub source: String,
    pub destination: Option<PathBuf>,
    pub filename: Option<String>,
    #[serde(default)]
    pub priority: i32,
    pub speed_limit_bps: Option<u64>,
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub duplicate_policy: DuplicatePolicy,
    #[serde(default)]
    pub options: DownloadOptions,
}

impl CreateJob {
    pub fn redacted(mut self) -> Self {
        self.options.redact_sensitive();
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateJob {
    pub priority: Option<i32>,
    /// Omitted leaves the limit unchanged; null removes it.
    pub speed_limit_bps: Option<Option<u64>>,
    pub destination: Option<PathBuf>,
    pub filename: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub kind: JobKind,
    pub source: String,
    pub destination: String,
    pub filename: Option<String>,
    pub status: JobStatus,
    pub priority: i32,
    pub total_bytes: Option<i64>,
    pub downloaded_bytes: i64,
    pub speed_limit_bps: Option<i64>,
    pub expected_sha256: Option<String>,
    pub error: Option<String>,
    pub transfer_mode: String,
    pub options_json: DownloadOptions,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Job {
    pub fn redacted(mut self) -> Self {
        self.options_json.redact_sensitive();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressSnapshot {
    pub job_id: Uuid,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Primary,
    Video,
    Audio,
    Subtitle,
    Thumbnail,
    Metadata,
    TorrentFile,
    ExtractedFile,
    ConvertedFile,
    Archive,
    Directory,
    Temporary,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputState {
    Planned,
    Creating,
    Ready,
    Failed,
    Deleted,
    Moved,
    Replaced,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputSourceKind {
    Http,
    Media,
    Torrent,
    PostProcess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOutput {
    pub id: Uuid,
    pub job_id: Uuid,
    pub output_type: OutputType,
    pub original_path: PathBuf,
    pub current_path: PathBuf,
    pub relative_path: PathBuf,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
    pub checksum_algorithm: Option<String>,
    pub checksum_value: Option<String>,
    pub state: OutputState,
    pub source_kind: OutputSourceKind,
    pub parent_output_id: Option<Uuid>,
    pub producing_action_index: Option<usize>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
