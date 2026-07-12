use std::{net::SocketAddr, path::PathBuf, time::Duration};

use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::error::Result;

/// Runtime configuration. Defaults are conservative and can be tuned after benchmarking.
#[derive(Debug, Clone, Parser)]
#[command(name = "ravyn", version, about = "Ravyn download manager backend")]
pub struct Config {
    #[arg(long, env = "RAVYN_DATA_DIR", default_value = "./ravyn-data")]
    pub data_dir: PathBuf,
    #[arg(long, env = "RAVYN_DOWNLOAD_DIR")]
    pub download_dir: Option<PathBuf>,
    #[arg(long, env = "RAVYN_COOKIE_DIR")]
    pub cookie_dir: Option<PathBuf>,
    #[arg(long, env = "RAVYN_LISTEN", default_value = "127.0.0.1:47821")]
    pub listen: SocketAddr,
    #[arg(long, env = "RAVYN_MAX_ACTIVE", default_value_t = 4)]
    pub max_active: usize,
    #[arg(long, env = "RAVYN_MAX_SEGMENTS", default_value_t = 8)]
    pub max_segments: usize,
    #[arg(long, env = "RAVYN_SEGMENT_THRESHOLD_MIB", default_value_t = 16)]
    pub segment_threshold_mib: u64,
    #[arg(long, env = "RAVYN_MAX_RETRIES", default_value_t = 4)]
    pub max_retries: u32,
    #[arg(long, env = "RAVYN_MAX_CONNECTIONS_PER_HOST", default_value_t = 8)]
    pub max_connections_per_host: usize,
    #[arg(long, env = "RAVYN_HOST_CIRCUIT_THRESHOLD", default_value_t = 4)]
    pub host_circuit_threshold: u32,
    #[arg(long, env = "RAVYN_HOST_CIRCUIT_COOLDOWN_SECS", default_value_t = 60)]
    pub host_circuit_cooldown_secs: u64,
    #[arg(long, env = "RAVYN_GLOBAL_SPEED_LIMIT_BPS", default_value_t = 0)]
    pub global_speed_limit_bps: u64,
    #[arg(long, env = "RAVYN_MAX_TORRENT_MIB", default_value_t = 16)]
    pub max_torrent_mib: u64,
    #[arg(long, env = "RAVYN_API_TOKEN")]
    pub api_token: Option<String>,
    #[arg(long, env = "RAVYN_ALLOW_REMOTE_API", default_value_t = false)]
    pub allow_remote_api: bool,
    /// Assert that a trusted local reverse proxy terminates TLS before this listener.
    #[arg(
        long,
        env = "RAVYN_REMOTE_API_BEHIND_TLS_PROXY",
        default_value_t = false
    )]
    pub remote_api_behind_tls_proxy: bool,
    #[arg(long, env = "RAVYN_ALLOW_PRIVATE_NETWORK", default_value_t = false)]
    pub allow_private_network: bool,
    #[arg(long, env = "RAVYN_MAX_HTML_MIB", default_value_t = 8)]
    pub max_html_mib: u64,
    #[arg(long, env = "RAVYN_MAX_SNIFF_RESOURCES", default_value_t = 5_000)]
    pub max_sniff_resources: usize,
    #[arg(long, env = "RAVYN_MAX_BATCH_URLS", default_value_t = 10_000)]
    pub max_batch_urls: usize,
    #[arg(long, env = "RAVYN_MAX_API_BODY_MIB", default_value_t = 8)]
    pub max_api_body_mib: usize,
    #[arg(long, env = "RAVYN_API_REQUEST_TIMEOUT_SECS", default_value_t = 120)]
    pub api_request_timeout_secs: u64,
    #[arg(long, env = "RAVYN_API_MAX_CONCURRENT_REQUESTS", default_value_t = 128)]
    pub api_max_concurrent_requests: usize,
    #[arg(long, env = "RAVYN_API_RATE_LIMIT_PER_MINUTE", default_value_t = 1200)]
    pub api_rate_limit_per_minute: u64,
    #[arg(long, env = "RAVYN_API_RATE_LIMIT_BURST", default_value_t = 200)]
    pub api_rate_limit_burst: u64,
    #[arg(long, env = "RAVYN_CONNECT_TIMEOUT_SECS", default_value_t = 15)]
    pub connect_timeout_secs: u64,
    #[arg(long, env = "RAVYN_READ_TIMEOUT_SECS", default_value_t = 60)]
    pub read_timeout_secs: u64,
    #[arg(long, env = "RAVYN_MEDIA_PROBE_TIMEOUT_SECS", default_value_t = 120)]
    pub media_probe_timeout_secs: u64,
    #[arg(long, env = "RAVYN_MEDIA_PROBE_MAX_MIB", default_value_t = 32)]
    pub media_probe_max_mib: usize,
    #[arg(long, env = "RAVYN_YTDLP", default_value = "yt-dlp")]
    pub ytdlp: PathBuf,
    #[arg(long, env = "RAVYN_RQBIT", default_value = "rqbit")]
    pub rqbit: PathBuf,
    /// Base URL of the rqbit HTTP API. Ravyn never exposes rqbit directly.
    #[arg(long, env = "RAVYN_RQBIT_API", default_value = "http://127.0.0.1:3030")]
    pub rqbit_api: String,
    #[arg(long, env = "RAVYN_RQBIT_USERNAME")]
    pub rqbit_username: Option<String>,
    #[arg(long, env = "RAVYN_RQBIT_PASSWORD")]
    pub rqbit_password: Option<String>,
    /// Secret-reference UUID containing JSON {"username":"...","password":"..."}.
    #[arg(long, env = "RAVYN_RQBIT_CREDENTIALS_SECRET_ID")]
    pub rqbit_credentials_secret_id: Option<uuid::Uuid>,
    /// Time allowed for rqbit API operations before they are considered unavailable.
    #[arg(long, env = "RAVYN_RQBIT_TIMEOUT_SECS", default_value_t = 120)]
    pub rqbit_timeout_secs: u64,
    #[arg(long, env = "RAVYN_RQBIT_STATS_TIMEOUT_SECS", default_value_t = 10)]
    pub rqbit_stats_timeout_secs: u64,
    #[arg(long, env = "RAVYN_TORRENT_REFRESH_CONCURRENCY", default_value_t = 8)]
    pub torrent_refresh_concurrency: usize,
    #[arg(long, env = "RAVYN_FFMPEG", default_value = "ffmpeg")]
    pub ffmpeg: PathBuf,
    /// Enables explicitly marked arbitrary FFmpeg arguments. Named presets do
    /// not require this flag.
    #[arg(long, env = "RAVYN_ALLOW_UNSAFE_FFMPEG", default_value_t = false)]
    pub allow_unsafe_ffmpeg: bool,
    #[arg(long, env = "RAVYN_IMAGE_CONVERTER", default_value = "magick")]
    pub image_converter: PathBuf,
    #[arg(long, env = "RAVYN_AVIF_QUALITY", default_value_t = 65)]
    pub avif_quality: u8,
    #[arg(long, env = "RAVYN_7Z", default_value = "7z")]
    pub seven_zip: PathBuf,
    #[arg(long, env = "RAVYN_MAX_EXTRACT_MIB", default_value_t = 10_240)]
    pub max_extract_mib: u64,
    #[arg(long, env = "RAVYN_MAX_EXTRACT_FILES", default_value_t = 100_000)]
    pub max_extract_files: usize,
    #[arg(long, env = "RAVYN_MAX_EXTRACT_DEPTH", default_value_t = 64)]
    pub max_extract_depth: usize,
    #[arg(long, env = "RAVYN_MAX_EXTRACT_RATIO", default_value_t = 1_000)]
    pub max_extract_ratio: u64,
}

impl Config {
    pub fn database_url(&self) -> String {
        format!("sqlite://{}", self.data_dir.join("ravyn.sqlite3").display())
    }
    pub fn effective_download_dir(&self) -> PathBuf {
        self.download_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("downloads"))
    }
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }
    pub fn effective_cookie_dir(&self) -> PathBuf {
        self.cookie_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("cookies"))
    }
    pub fn read_timeout(&self) -> Duration {
        Duration::from_secs(self.read_timeout_secs)
    }
    pub fn segment_threshold(&self) -> u64 {
        self.segment_threshold_mib * 1024 * 1024
    }
    pub fn validate(&self) -> Result<()> {
        if self.max_active == 0 || self.max_active > 1_024 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_ACTIVE must be between 1 and 1024".into(),
            ));
        }
        if self.max_segments == 0 || self.max_segments > 64 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_SEGMENTS must be between 1 and 64".into(),
            ));
        }
        if self.segment_threshold_mib == 0 || self.segment_threshold_mib > 1_048_576 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_SEGMENT_THRESHOLD_MIB must be between 1 and 1048576".into(),
            ));
        }
        if self.max_retries > 1_000 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_RETRIES must be between 0 and 1000".into(),
            ));
        }
        if self.max_torrent_mib == 0 || self.max_torrent_mib > 1_024 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_TORRENT_MIB must be between 1 and 1024".into(),
            ));
        }
        if self.torrent_refresh_concurrency == 0 || self.torrent_refresh_concurrency > 128 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_TORRENT_REFRESH_CONCURRENCY must be between 1 and 128".into(),
            ));
        }
        let rqbit_url = url::Url::parse(&self.rqbit_api)?;
        if !matches!(rqbit_url.scheme(), "http" | "https") {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_RQBIT_API must use HTTP or HTTPS".into(),
            ));
        }
        let rqbit_loopback = rqbit_url
            .host_str()
            .and_then(|host| host.parse::<std::net::IpAddr>().ok())
            .is_some_and(|address| address.is_loopback())
            || rqbit_url
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case("localhost"));
        let direct_rqbit_credentials = self
            .rqbit_username
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            && self
                .rqbit_password
                .as_deref()
                .is_some_and(|value| !value.is_empty());
        if !rqbit_loopback
            && !direct_rqbit_credentials
            && self.rqbit_credentials_secret_id.is_none()
        {
            return Err(crate::error::RavynError::Invalid(
                "a non-loopback RAVYN_RQBIT_API requires direct credentials or RAVYN_RQBIT_CREDENTIALS_SECRET_ID"
                    .into(),
            ));
        }
        if self.connect_timeout_secs == 0
            || self.read_timeout_secs == 0
            || self.rqbit_timeout_secs == 0
            || self.rqbit_stats_timeout_secs == 0
            || self.media_probe_timeout_secs == 0
        {
            return Err(crate::error::RavynError::Invalid(
                "network timeouts must be greater than zero".into(),
            ));
        }
        if self.media_probe_max_mib == 0 || self.media_probe_max_mib > 512 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MEDIA_PROBE_MAX_MIB must be between 1 and 512".into(),
            ));
        }
        if self.max_html_mib == 0 || self.max_html_mib > 128 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_HTML_MIB must be between 1 and 128".into(),
            ));
        }
        if self.max_sniff_resources == 0 || self.max_sniff_resources > 100_000 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_SNIFF_RESOURCES must be between 1 and 100000".into(),
            ));
        }
        if self.max_batch_urls == 0 || self.max_batch_urls > 100_000 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_BATCH_URLS must be between 1 and 100000".into(),
            ));
        }
        if self.max_api_body_mib == 0 || self.max_api_body_mib > 128 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_API_BODY_MIB must be between 1 and 128".into(),
            ));
        }
        if self.api_request_timeout_secs == 0 || self.api_request_timeout_secs > 86_400 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_API_REQUEST_TIMEOUT_SECS must be between 1 and 86400".into(),
            ));
        }
        if self.api_max_concurrent_requests == 0 || self.api_max_concurrent_requests > 16_384 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_API_MAX_CONCURRENT_REQUESTS must be between 1 and 16384".into(),
            ));
        }
        if self.api_rate_limit_per_minute == 0 || self.api_rate_limit_per_minute > 10_000_000 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_API_RATE_LIMIT_PER_MINUTE must be between 1 and 10000000".into(),
            ));
        }
        if self.api_rate_limit_burst == 0
            || self.api_rate_limit_burst > self.api_rate_limit_per_minute
        {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_API_RATE_LIMIT_BURST must be between 1 and the per-minute limit".into(),
            ));
        }
        if self.avif_quality == 0 || self.avif_quality > 100 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_AVIF_QUALITY must be between 1 and 100".into(),
            ));
        }
        if self.max_connections_per_host == 0 || self.max_connections_per_host > 128 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_CONNECTIONS_PER_HOST must be between 1 and 128".into(),
            ));
        }
        if self.host_circuit_threshold == 0 || self.host_circuit_threshold > 100 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_HOST_CIRCUIT_THRESHOLD must be between 1 and 100".into(),
            ));
        }
        if self.host_circuit_cooldown_secs == 0 || self.host_circuit_cooldown_secs > 86_400 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_HOST_CIRCUIT_COOLDOWN_SECS must be between 1 and 86400".into(),
            ));
        }
        if self.max_extract_mib == 0 || self.max_extract_mib > 1_048_576 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_EXTRACT_MIB must be between 1 and 1048576".into(),
            ));
        }
        if self.max_extract_files == 0 || self.max_extract_files > 10_000_000 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_EXTRACT_FILES must be between 1 and 10000000".into(),
            ));
        }
        if self.max_extract_depth == 0 || self.max_extract_depth > 256 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_EXTRACT_DEPTH must be between 1 and 256".into(),
            ));
        }
        if self.max_extract_ratio == 0 || self.max_extract_ratio > 1_000_000 {
            return Err(crate::error::RavynError::Invalid(
                "RAVYN_MAX_EXTRACT_RATIO must be between 1 and 1000000".into(),
            ));
        }
        Ok(())
    }

    pub async fn prepare_directories(&self) -> Result<()> {
        fs::create_dir_all(&self.data_dir).await?;
        let download_dir = self
            .download_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("downloads"));
        fs::create_dir_all(download_dir).await?;
        fs::create_dir_all(self.effective_cookie_dir()).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BandwidthWindow {
    /// ISO weekday numbers: Monday = 1, Sunday = 7.
    pub weekdays: Vec<u8>,
    /// Inclusive local minute from midnight.
    pub start_minute: u16,
    /// Exclusive local minute from midnight. Values lower than start wrap overnight.
    pub end_minute: u16,
    /// Zero means unlimited, matching the global limiter contract.
    pub limit_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BandwidthSchedule {
    pub timezone: String,
    #[serde(default)]
    pub windows: Vec<BandwidthWindow>,
}

impl Default for BandwidthSchedule {
    fn default() -> Self {
        Self {
            timezone: "UTC".into(),
            windows: Vec::new(),
        }
    }
}

impl BandwidthSchedule {
    pub fn validate(&self) -> Result<()> {
        let _: chrono_tz::Tz = self.timezone.parse().map_err(|_| {
            crate::error::RavynError::Invalid(
                "bandwidth schedule timezone must be a valid IANA name".into(),
            )
        })?;
        if self.windows.len() > 32 {
            return Err(crate::error::RavynError::Invalid(
                "bandwidth schedule may contain at most 32 windows".into(),
            ));
        }
        let mut occupied = [false; 7 * 24 * 60];
        for window in &self.windows {
            if window.weekdays.is_empty()
                || window.weekdays.len() > 7
                || window
                    .weekdays
                    .iter()
                    .any(|weekday| !(1..=7).contains(weekday))
                || window.start_minute >= 24 * 60
                || window.end_minute >= 24 * 60
                || window.start_minute == window.end_minute
            {
                return Err(crate::error::RavynError::Invalid(
                    "bandwidth schedule window has invalid weekdays or minute bounds".into(),
                ));
            }
            for &weekday in &window.weekdays {
                let day = usize::from(weekday - 1);
                let mut minute = usize::from(window.start_minute);
                loop {
                    let target_day = if minute < 24 * 60 { day } else { (day + 1) % 7 };
                    let target_minute = minute % (24 * 60);
                    let index = target_day * 24 * 60 + target_minute;
                    if std::mem::replace(&mut occupied[index], true) {
                        return Err(crate::error::RavynError::Invalid(
                            "bandwidth schedule windows may not overlap".into(),
                        ));
                    }
                    minute += 1;
                    if target_day
                        == if window.end_minute <= window.start_minute {
                            (day + 1) % 7
                        } else {
                            day
                        }
                        && target_minute == usize::from(window.end_minute).saturating_sub(1)
                    {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn effective_limit_at(
        &self,
        base_limit_bps: u64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64> {
        use chrono::{Datelike, Timelike};
        let timezone: chrono_tz::Tz = self.timezone.parse().map_err(|_| {
            crate::error::RavynError::Invalid(
                "bandwidth schedule timezone must be a valid IANA name".into(),
            )
        })?;
        let local = now.with_timezone(&timezone);
        let weekday = local.weekday().number_from_monday() as u8;
        let minute = (local.hour() * 60 + local.minute()) as u16;
        for window in &self.windows {
            let active = if window.start_minute < window.end_minute {
                window.weekdays.contains(&weekday)
                    && minute >= window.start_minute
                    && minute < window.end_minute
            } else {
                (window.weekdays.contains(&weekday) && minute >= window.start_minute)
                    || (window.weekdays.iter().any(|day| day % 7 + 1 == weekday)
                        && minute < window.end_minute)
            };
            if active {
                return Ok(window.limit_bps);
            }
        }
        Ok(base_limit_bps)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentSettings {
    pub download_dir: Option<PathBuf>,
    pub max_active: usize,
    pub max_segments: usize,
    pub segment_threshold_mib: u64,
    pub max_connections_per_host: usize,
    pub global_speed_limit_bps: u64,
    #[serde(default)]
    pub bandwidth_schedule: BandwidthSchedule,
    pub ytdlp: PathBuf,
    pub ffmpeg: PathBuf,
    pub rqbit_api: String,
    pub rqbit_credentials_secret_id: Option<uuid::Uuid>,
    pub seven_zip: PathBuf,
    pub max_extract_mib: u64,
    pub max_extract_files: usize,
    pub max_extract_depth: usize,
    pub max_extract_ratio: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_host_circuit_threshold")]
    pub host_circuit_threshold: u32,
    #[serde(default = "default_host_circuit_cooldown_secs")]
    pub host_circuit_cooldown_secs: u64,
    #[serde(default = "default_max_torrent_mib")]
    pub max_torrent_mib: u64,
    #[serde(default = "default_max_html_mib")]
    pub max_html_mib: u64,
    #[serde(default = "default_max_sniff_resources")]
    pub max_sniff_resources: usize,
    #[serde(default = "default_max_batch_urls")]
    pub max_batch_urls: usize,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_read_timeout_secs")]
    pub read_timeout_secs: u64,
    #[serde(default = "default_media_probe_timeout_secs")]
    pub media_probe_timeout_secs: u64,
    #[serde(default = "default_media_probe_max_mib")]
    pub media_probe_max_mib: usize,
    #[serde(default = "default_rqbit_timeout_secs")]
    pub rqbit_timeout_secs: u64,
    #[serde(default = "default_rqbit_stats_timeout_secs")]
    pub rqbit_stats_timeout_secs: u64,
    #[serde(default = "default_torrent_refresh_concurrency")]
    pub torrent_refresh_concurrency: usize,
    #[serde(default = "default_image_converter")]
    pub image_converter: PathBuf,
    #[serde(default = "default_avif_quality")]
    pub avif_quality: u8,
    #[serde(default)]
    pub cookie_dir: Option<PathBuf>,
    #[serde(default = "default_api_request_timeout_secs")]
    pub api_request_timeout_secs: u64,
    #[serde(default = "default_api_max_concurrent_requests")]
    pub api_max_concurrent_requests: usize,
    #[serde(default = "default_api_rate_limit_per_minute")]
    pub api_rate_limit_per_minute: u64,
    #[serde(default = "default_api_rate_limit_burst")]
    pub api_rate_limit_burst: u64,
}

fn default_max_retries() -> u32 {
    4
}
fn default_host_circuit_threshold() -> u32 {
    4
}
fn default_host_circuit_cooldown_secs() -> u64 {
    60
}
fn default_max_torrent_mib() -> u64 {
    16
}
fn default_max_html_mib() -> u64 {
    8
}
fn default_max_sniff_resources() -> usize {
    5_000
}
fn default_max_batch_urls() -> usize {
    10_000
}
fn default_connect_timeout_secs() -> u64 {
    15
}
fn default_read_timeout_secs() -> u64 {
    60
}
fn default_media_probe_timeout_secs() -> u64 {
    120
}
fn default_media_probe_max_mib() -> usize {
    32
}
fn default_rqbit_timeout_secs() -> u64 {
    120
}
fn default_rqbit_stats_timeout_secs() -> u64 {
    10
}
fn default_torrent_refresh_concurrency() -> usize {
    8
}
fn default_image_converter() -> PathBuf {
    PathBuf::from("magick")
}
fn default_avif_quality() -> u8 {
    65
}

fn default_api_request_timeout_secs() -> u64 {
    120
}
fn default_api_max_concurrent_requests() -> usize {
    128
}
fn default_api_rate_limit_per_minute() -> u64 {
    1_200
}
fn default_api_rate_limit_burst() -> u64 {
    200
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PersistentSettingsPatch {
    pub download_dir: Option<Option<PathBuf>>,
    pub max_active: Option<usize>,
    pub max_segments: Option<usize>,
    pub segment_threshold_mib: Option<u64>,
    pub max_connections_per_host: Option<usize>,
    pub global_speed_limit_bps: Option<u64>,
    pub bandwidth_schedule: Option<BandwidthSchedule>,
    pub ytdlp: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub rqbit_api: Option<String>,
    pub rqbit_credentials_secret_id: Option<Option<uuid::Uuid>>,
    pub seven_zip: Option<PathBuf>,
    pub max_extract_mib: Option<u64>,
    pub max_extract_files: Option<usize>,
    pub max_extract_depth: Option<usize>,
    pub max_extract_ratio: Option<u64>,
    pub max_retries: Option<u32>,
    pub host_circuit_threshold: Option<u32>,
    pub host_circuit_cooldown_secs: Option<u64>,
    pub max_torrent_mib: Option<u64>,
    pub max_html_mib: Option<u64>,
    pub max_sniff_resources: Option<usize>,
    pub max_batch_urls: Option<usize>,
    pub connect_timeout_secs: Option<u64>,
    pub read_timeout_secs: Option<u64>,
    pub media_probe_timeout_secs: Option<u64>,
    pub media_probe_max_mib: Option<usize>,
    pub rqbit_timeout_secs: Option<u64>,
    pub rqbit_stats_timeout_secs: Option<u64>,
    pub torrent_refresh_concurrency: Option<usize>,
    pub image_converter: Option<PathBuf>,
    pub avif_quality: Option<u8>,
    pub cookie_dir: Option<Option<PathBuf>>,
    pub api_request_timeout_secs: Option<u64>,
    pub api_max_concurrent_requests: Option<usize>,
    pub api_rate_limit_per_minute: Option<u64>,
    pub api_rate_limit_burst: Option<u64>,
}

impl PersistentSettings {
    pub fn from_config(config: &Config) -> Self {
        Self {
            download_dir: config.download_dir.clone(),
            max_active: config.max_active,
            max_segments: config.max_segments,
            segment_threshold_mib: config.segment_threshold_mib,
            max_connections_per_host: config.max_connections_per_host,
            global_speed_limit_bps: config.global_speed_limit_bps,
            bandwidth_schedule: BandwidthSchedule::default(),
            ytdlp: config.ytdlp.clone(),
            ffmpeg: config.ffmpeg.clone(),
            rqbit_api: config.rqbit_api.clone(),
            rqbit_credentials_secret_id: config.rqbit_credentials_secret_id,
            seven_zip: config.seven_zip.clone(),
            max_extract_mib: config.max_extract_mib,
            max_extract_files: config.max_extract_files,
            max_extract_depth: config.max_extract_depth,
            max_extract_ratio: config.max_extract_ratio,
            max_retries: config.max_retries,
            host_circuit_threshold: config.host_circuit_threshold,
            host_circuit_cooldown_secs: config.host_circuit_cooldown_secs,
            max_torrent_mib: config.max_torrent_mib,
            max_html_mib: config.max_html_mib,
            max_sniff_resources: config.max_sniff_resources,
            max_batch_urls: config.max_batch_urls,
            connect_timeout_secs: config.connect_timeout_secs,
            read_timeout_secs: config.read_timeout_secs,
            media_probe_timeout_secs: config.media_probe_timeout_secs,
            media_probe_max_mib: config.media_probe_max_mib,
            rqbit_timeout_secs: config.rqbit_timeout_secs,
            rqbit_stats_timeout_secs: config.rqbit_stats_timeout_secs,
            torrent_refresh_concurrency: config.torrent_refresh_concurrency,
            image_converter: config.image_converter.clone(),
            avif_quality: config.avif_quality,
            cookie_dir: config.cookie_dir.clone(),
            api_request_timeout_secs: config.api_request_timeout_secs,
            api_max_concurrent_requests: config.api_max_concurrent_requests,
            api_rate_limit_per_minute: config.api_rate_limit_per_minute,
            api_rate_limit_burst: config.api_rate_limit_burst,
        }
    }

    pub fn apply_to(&self, config: &mut Config) -> Result<()> {
        self.bandwidth_schedule.validate()?;
        config.download_dir = self.download_dir.clone();
        config.max_active = self.max_active;
        config.max_segments = self.max_segments;
        config.segment_threshold_mib = self.segment_threshold_mib;
        config.max_connections_per_host = self.max_connections_per_host;
        config.global_speed_limit_bps = self.global_speed_limit_bps;
        config.ytdlp = self.ytdlp.clone();
        config.ffmpeg = self.ffmpeg.clone();
        config.rqbit_api = self.rqbit_api.clone();
        config.rqbit_credentials_secret_id = self.rqbit_credentials_secret_id;
        config.seven_zip = self.seven_zip.clone();
        config.max_extract_mib = self.max_extract_mib;
        config.max_extract_files = self.max_extract_files;
        config.max_extract_depth = self.max_extract_depth;
        config.max_extract_ratio = self.max_extract_ratio;
        config.max_retries = self.max_retries;
        config.host_circuit_threshold = self.host_circuit_threshold;
        config.host_circuit_cooldown_secs = self.host_circuit_cooldown_secs;
        config.max_torrent_mib = self.max_torrent_mib;
        config.max_html_mib = self.max_html_mib;
        config.max_sniff_resources = self.max_sniff_resources;
        config.max_batch_urls = self.max_batch_urls;
        config.connect_timeout_secs = self.connect_timeout_secs;
        config.read_timeout_secs = self.read_timeout_secs;
        config.media_probe_timeout_secs = self.media_probe_timeout_secs;
        config.media_probe_max_mib = self.media_probe_max_mib;
        config.rqbit_timeout_secs = self.rqbit_timeout_secs;
        config.rqbit_stats_timeout_secs = self.rqbit_stats_timeout_secs;
        config.torrent_refresh_concurrency = self.torrent_refresh_concurrency;
        config.image_converter = self.image_converter.clone();
        config.avif_quality = self.avif_quality;
        config.cookie_dir = self.cookie_dir.clone();
        config.api_request_timeout_secs = self.api_request_timeout_secs;
        config.api_max_concurrent_requests = self.api_max_concurrent_requests;
        config.api_rate_limit_per_minute = self.api_rate_limit_per_minute;
        config.api_rate_limit_burst = self.api_rate_limit_burst;
        config.validate()
    }

    pub fn merge(&mut self, patch: PersistentSettingsPatch) {
        if let Some(value) = patch.download_dir {
            self.download_dir = value;
        }
        if let Some(value) = patch.max_active {
            self.max_active = value;
        }
        if let Some(value) = patch.max_segments {
            self.max_segments = value;
        }
        if let Some(value) = patch.segment_threshold_mib {
            self.segment_threshold_mib = value;
        }
        if let Some(value) = patch.max_connections_per_host {
            self.max_connections_per_host = value;
        }
        if let Some(value) = patch.global_speed_limit_bps {
            self.global_speed_limit_bps = value;
        }
        if let Some(value) = patch.bandwidth_schedule {
            self.bandwidth_schedule = value;
        }
        if let Some(value) = patch.ytdlp {
            self.ytdlp = value;
        }
        if let Some(value) = patch.ffmpeg {
            self.ffmpeg = value;
        }
        if let Some(value) = patch.rqbit_api {
            self.rqbit_api = value;
        }
        if let Some(value) = patch.rqbit_credentials_secret_id {
            self.rqbit_credentials_secret_id = value;
        }
        if let Some(value) = patch.seven_zip {
            self.seven_zip = value;
        }
        if let Some(value) = patch.max_extract_mib {
            self.max_extract_mib = value;
        }
        if let Some(value) = patch.max_extract_files {
            self.max_extract_files = value;
        }
        if let Some(value) = patch.max_extract_depth {
            self.max_extract_depth = value;
        }
        if let Some(value) = patch.max_extract_ratio {
            self.max_extract_ratio = value;
        }
        if let Some(value) = patch.max_retries {
            self.max_retries = value;
        }
        if let Some(value) = patch.host_circuit_threshold {
            self.host_circuit_threshold = value;
        }
        if let Some(value) = patch.host_circuit_cooldown_secs {
            self.host_circuit_cooldown_secs = value;
        }
        if let Some(value) = patch.max_torrent_mib {
            self.max_torrent_mib = value;
        }
        if let Some(value) = patch.max_html_mib {
            self.max_html_mib = value;
        }
        if let Some(value) = patch.max_sniff_resources {
            self.max_sniff_resources = value;
        }
        if let Some(value) = patch.max_batch_urls {
            self.max_batch_urls = value;
        }
        if let Some(value) = patch.connect_timeout_secs {
            self.connect_timeout_secs = value;
        }
        if let Some(value) = patch.read_timeout_secs {
            self.read_timeout_secs = value;
        }
        if let Some(value) = patch.media_probe_timeout_secs {
            self.media_probe_timeout_secs = value;
        }
        if let Some(value) = patch.media_probe_max_mib {
            self.media_probe_max_mib = value;
        }
        if let Some(value) = patch.rqbit_timeout_secs {
            self.rqbit_timeout_secs = value;
        }
        if let Some(value) = patch.rqbit_stats_timeout_secs {
            self.rqbit_stats_timeout_secs = value;
        }
        if let Some(value) = patch.torrent_refresh_concurrency {
            self.torrent_refresh_concurrency = value;
        }
        if let Some(value) = patch.image_converter {
            self.image_converter = value;
        }
        if let Some(value) = patch.avif_quality {
            self.avif_quality = value;
        }
        if let Some(value) = patch.cookie_dir {
            self.cookie_dir = value;
        }
        if let Some(value) = patch.api_request_timeout_secs {
            self.api_request_timeout_secs = value;
        }
        if let Some(value) = patch.api_max_concurrent_requests {
            self.api_max_concurrent_requests = value;
        }
        if let Some(value) = patch.api_rate_limit_per_minute {
            self.api_rate_limit_per_minute = value;
        }
        if let Some(value) = patch.api_rate_limit_burst {
            self.api_rate_limit_burst = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_settings_from_older_json_receive_api_defaults() {
        let json = r#"{
            "download_dir":null,
            "max_active":4,
            "max_segments":8,
            "segment_threshold_mib":16,
            "max_connections_per_host":8,
            "global_speed_limit_bps":0,
            "ytdlp":"yt-dlp",
            "ffmpeg":"ffmpeg",
            "rqbit_api":"http://127.0.0.1:3030",
            "rqbit_credentials_secret_id":null,
            "seven_zip":"7z",
            "max_extract_mib":10240,
            "max_extract_files":100000,
            "max_extract_depth":64,
            "max_extract_ratio":1000
        }"#;
        let settings: PersistentSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.max_retries, 4);
        assert_eq!(settings.host_circuit_threshold, 4);
        assert_eq!(settings.max_sniff_resources, 5_000);
        assert_eq!(settings.api_request_timeout_secs, 120);
        assert_eq!(settings.api_max_concurrent_requests, 128);
        assert_eq!(settings.api_rate_limit_per_minute, 1_200);
        assert_eq!(settings.api_rate_limit_burst, 200);
    }

    #[test]
    fn bandwidth_schedule_applies_named_timezone_windows_and_overnight_wraps() {
        use chrono::TimeZone;

        let schedule = BandwidthSchedule {
            timezone: "Europe/Rome".into(),
            windows: vec![BandwidthWindow {
                weekdays: vec![1],
                start_minute: 23 * 60,
                end_minute: 60,
                limit_bps: 42,
            }],
        };
        schedule.validate().unwrap();

        let monday_late_utc = chrono::Utc
            .with_ymd_and_hms(2026, 7, 13, 21, 30, 0)
            .single()
            .unwrap();
        let tuesday_early_utc = chrono::Utc
            .with_ymd_and_hms(2026, 7, 13, 22, 30, 0)
            .single()
            .unwrap();
        let tuesday_after_utc = chrono::Utc
            .with_ymd_and_hms(2026, 7, 13, 23, 30, 0)
            .single()
            .unwrap();

        assert_eq!(
            schedule.effective_limit_at(1000, monday_late_utc).unwrap(),
            42
        );
        assert_eq!(
            schedule
                .effective_limit_at(1000, tuesday_early_utc)
                .unwrap(),
            42
        );
        assert_eq!(
            schedule
                .effective_limit_at(1000, tuesday_after_utc)
                .unwrap(),
            1000
        );
    }

    #[test]
    fn bandwidth_schedule_rejects_overlap_and_invalid_timezones() {
        let overlapping = BandwidthSchedule {
            timezone: "UTC".into(),
            windows: vec![
                BandwidthWindow {
                    weekdays: vec![1],
                    start_minute: 60,
                    end_minute: 180,
                    limit_bps: 1,
                },
                BandwidthWindow {
                    weekdays: vec![1],
                    start_minute: 120,
                    end_minute: 240,
                    limit_bps: 2,
                },
            ],
        };
        assert!(overlapping.validate().is_err());

        let invalid = BandwidthSchedule {
            timezone: "Not/A_Zone".into(),
            windows: Vec::new(),
        };
        assert!(invalid.validate().is_err());
    }
}
