use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use futures_util::{StreamExt, stream};
use reqwest::{Client, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    config::Config,
    core::{
        events::{Event, EventBus},
        models::{Job, JobStatus, ProgressSnapshot, TorrentOptions},
        progress::ProgressPublisher,
    },
    download::adapter::{DownloadAdapter, DownloadOutcome},
    error::{RavynError, Result},
    storage::Repository,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentProbeRequest {
    pub source: String,
    pub destination: Option<String>,
    pub file_regex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub index: usize,
    pub path: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentProbe {
    pub torrent_id: Option<String>,
    pub info_hash: Option<String>,
    pub name: Option<String>,
    pub total_bytes: Option<u64>,
    pub files: Vec<TorrentFile>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSnapshot {
    pub torrent_id: String,
    pub info_hash: Option<String>,
    pub name: Option<String>,
    pub state: String,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub download_speed_bps: u64,
    pub upload_speed_bps: u64,
    pub peers_connected: u64,
    pub seeders: u64,
    pub leechers: u64,
    pub finished: bool,
    pub progress: Option<f64>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentEngineTorrent {
    pub torrent_id: Option<String>,
    pub info_hash: Option<String>,
    pub name: Option<String>,
    pub output_folder: Option<String>,
    pub state: Option<String>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub progress: Option<f64>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentEngineList {
    pub torrents: Vec<TorrentEngineTorrent>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentGlobalStats {
    pub downloaded_bytes: Option<u64>,
    pub uploaded_bytes: Option<u64>,
    pub download_speed_bps: Option<u64>,
    pub upload_speed_bps: Option<u64>,
    pub active_torrents: Option<u64>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentDetails {
    pub torrent_id: String,
    pub info_hash: Option<String>,
    pub name: Option<String>,
    pub state: Option<String>,
    pub total_bytes: Option<u64>,
    pub files: Vec<TorrentFile>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentPeer {
    pub address: Option<String>,
    pub client: Option<String>,
    pub state: Option<String>,
    pub downloaded_bytes: Option<u64>,
    pub uploaded_bytes: Option<u64>,
    pub download_speed_bps: Option<u64>,
    pub upload_speed_bps: Option<u64>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentPeerStats {
    pub peers: Vec<TorrentPeer>,
    pub raw: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct RqbitCredentialsSecret {
    username: String,
    password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RqbitNumber {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Text(String),
}

impl RqbitNumber {
    fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Unsigned(value) => Some(*value),
            Self::Signed(value) => u64::try_from(*value).ok(),
            Self::Float(value)
                if value.is_finite() && *value >= 0.0 && *value <= u64::MAX as f64 =>
            {
                Some(value.trunc() as u64)
            }
            Self::Text(value) => value.trim().parse::<u64>().ok().or_else(|| {
                value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value >= 0.0 && *value <= u64::MAX as f64)
                    .map(|value| value.trunc() as u64)
            }),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Unsigned(value) => Some(*value as f64),
            Self::Signed(value) => Some(*value as f64),
            Self::Float(value) if value.is_finite() => Some(*value),
            Self::Text(value) => value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RqbitBoolean {
    Boolean(bool),
    Number(RqbitNumber),
    Text(String),
}

impl RqbitBoolean {
    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            Self::Number(value) => value.as_u64().map(|value| value != 0),
            Self::Text(value) => match value.trim().to_ascii_lowercase().as_str() {
                "true" | "yes" | "1" => Some(true),
                "false" | "no" | "0" => Some(false),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RqbitAggregateDto {
    #[serde(
        default,
        alias = "progress_bytes",
        alias = "downloaded",
        alias = "bytes_downloaded"
    )]
    downloaded_bytes: Option<RqbitNumber>,
    #[serde(default, alias = "totalBytes", alias = "size_bytes", alias = "length")]
    total_bytes: Option<RqbitNumber>,
    #[serde(default, alias = "uploaded", alias = "bytes_uploaded")]
    uploaded_bytes: Option<RqbitNumber>,
    #[serde(default, alias = "download_speed", alias = "downloadSpeed")]
    download_speed_bps: Option<RqbitNumber>,
    #[serde(default, alias = "upload_speed", alias = "uploadSpeed")]
    upload_speed_bps: Option<RqbitNumber>,
    #[serde(default, alias = "peers", alias = "live_peers")]
    peers_connected: Option<RqbitNumber>,
    #[serde(default, alias = "seeds")]
    seeders: Option<RqbitNumber>,
    #[serde(default, alias = "leeches")]
    leechers: Option<RqbitNumber>,
    #[serde(default, alias = "status")]
    state: Option<String>,
    #[serde(default, alias = "fraction", alias = "percent")]
    progress: Option<RqbitNumber>,
    #[serde(
        default,
        alias = "complete",
        alias = "completed",
        alias = "is_finished"
    )]
    finished: Option<RqbitBoolean>,
}

impl RqbitAggregateDto {
    fn has_statistics(&self) -> bool {
        self.downloaded_bytes.is_some()
            || self.total_bytes.is_some()
            || self.uploaded_bytes.is_some()
            || self.download_speed_bps.is_some()
            || self.upload_speed_bps.is_some()
            || self.progress.is_some()
            || self.finished.is_some()
            || self.state.is_some()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RqbitStatsEnvelope {
    #[serde(flatten)]
    root: RqbitAggregateDto,
    #[serde(default)]
    stats: Option<RqbitAggregateDto>,
    #[serde(default)]
    live: Option<RqbitAggregateDto>,
    #[serde(default)]
    torrent: Option<RqbitAggregateDto>,
    #[serde(default)]
    details: Option<RqbitAggregateDto>,
    #[serde(default)]
    session: Option<RqbitAggregateDto>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RqbitFileDto {
    #[serde(default, alias = "id")]
    index: Option<RqbitNumber>,
    #[serde(default, alias = "name", alias = "filename")]
    path: Option<String>,
    #[serde(default, alias = "length", alias = "size_bytes")]
    size: Option<RqbitNumber>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RqbitFilesEnvelope {
    #[serde(default, alias = "file_infos", alias = "fileInfos")]
    files: Option<Vec<RqbitFileDto>>,
    #[serde(default)]
    details: Option<Box<RqbitFilesEnvelope>>,
    #[serde(default)]
    torrent: Option<Box<RqbitFilesEnvelope>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RqbitRootDto {
    server: Option<String>,
    version: Option<String>,
    #[serde(default)]
    apis: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TorrentApiCompatibility {
    Compatible,
    Incompatible,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentDependencyStatus {
    pub api_url: String,
    pub available: bool,
    pub server: Option<String>,
    pub version: Option<String>,
    pub compatibility: TorrentApiCompatibility,
    pub missing_required_apis: Vec<String>,
    pub error: Option<String>,
}

pub struct TorrentAdapter {
    client: Client,
    stats_client: Client,
    base_url: Url,
    repository: Repository,
    progress_publisher: ProgressPublisher,
    events: EventBus,
    max_local_torrent_bytes: u64,
    refresh_concurrency: usize,
    basic_auth: Option<(String, String)>,
}

impl TorrentAdapter {
    pub async fn new(
        config: Arc<Config>,
        repository: Repository,
        progress_publisher: ProgressPublisher,
        events: EventBus,
    ) -> Result<Self> {
        let base_url = Url::parse(config.rqbit_api.trim_end_matches('/'))?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(config.rqbit_timeout_secs.min(30)))
            .read_timeout(Duration::from_secs(config.rqbit_timeout_secs))
            .timeout(Duration::from_secs(config.rqbit_timeout_secs))
            .tcp_nodelay(true)
            .pool_max_idle_per_host(8)
            .build()?;
        let stats_client = Client::builder()
            .connect_timeout(Duration::from_secs(config.rqbit_stats_timeout_secs.min(10)))
            .read_timeout(Duration::from_secs(config.rqbit_stats_timeout_secs))
            .timeout(Duration::from_secs(config.rqbit_stats_timeout_secs))
            .tcp_nodelay(true)
            .pool_max_idle_per_host(16)
            .build()?;
        let basic_auth = if let Some(secret_id) = config.rqbit_credentials_secret_id {
            let secret = repository
                .resolve_secret_reference(secret_id, "rqbit_credentials")
                .await?;
            let credentials: RqbitCredentialsSecret =
                serde_json::from_str(&secret).map_err(|_| {
                    RavynError::Invalid(
                        "rqbit credential secret must be JSON with username and password".into(),
                    )
                })?;
            if credentials.username.trim().is_empty() || credentials.password.is_empty() {
                return Err(RavynError::Invalid(
                    "rqbit credential secret contains an empty username or password".into(),
                ));
            }
            Some((credentials.username, credentials.password))
        } else {
            config
                .rqbit_username
                .clone()
                .zip(config.rqbit_password.clone())
        };
        Ok(Self {
            client,
            stats_client,
            base_url,
            repository,
            progress_publisher,
            events,
            max_local_torrent_bytes: config.max_torrent_mib.saturating_mul(1024 * 1024),
            refresh_concurrency: config.torrent_refresh_concurrency.max(1),
            basic_auth,
        })
    }

    pub async fn dependency_status(&self) -> TorrentDependencyStatus {
        match self.request(reqwest::Method::GET, "").send().await {
            Ok(response) if response.status().is_success() => {
                match response.json::<RqbitRootDto>().await {
                    Ok(value) => {
                        let (compatibility, missing_required_apis) =
                            evaluate_rqbit_compatibility(&value);
                        TorrentDependencyStatus {
                            api_url: self.base_url.to_string(),
                            available: true,
                            server: value.server,
                            version: value.version,
                            compatibility,
                            missing_required_apis,
                            error: None,
                        }
                    }
                    Err(error) => TorrentDependencyStatus {
                        api_url: self.base_url.to_string(),
                        available: false,
                        server: None,
                        version: None,
                        compatibility: TorrentApiCompatibility::Unknown,
                        missing_required_apis: Vec::new(),
                        error: Some(format!("invalid rqbit response: {error}")),
                    },
                }
            }
            Ok(response) => TorrentDependencyStatus {
                api_url: self.base_url.to_string(),
                available: false,
                server: None,
                version: None,
                compatibility: TorrentApiCompatibility::Unknown,
                missing_required_apis: Vec::new(),
                error: Some(format!("rqbit returned HTTP {}", response.status())),
            },
            Err(error) => TorrentDependencyStatus {
                api_url: self.base_url.to_string(),
                available: false,
                server: None,
                version: None,
                compatibility: TorrentApiCompatibility::Unknown,
                missing_required_apis: Vec::new(),
                error: Some(error.to_string()),
            },
        }
    }

    pub async fn probe(&self, request: &TorrentProbeRequest) -> Result<TorrentProbe> {
        validate_source(&request.source)?;
        let mut query = vec![("list_only", "true".to_owned())];
        if let Some(destination) = request.destination.as_deref() {
            query.push(("output_folder", destination.to_owned()));
        }
        if let Some(regex) = request.file_regex.as_deref() {
            query.push(("only_files_regex", regex.to_owned()));
        }
        let response = self
            .source_request("torrents", &request.source, &query)
            .await?;
        let raw = decode_json(response, "torrent probe").await?;
        Ok(probe_from_value(raw))
    }

    pub async fn list(&self) -> Result<TorrentEngineList> {
        let raw = decode_json(
            self.request(reqwest::Method::GET, "torrents")
                .send()
                .await?,
            "list torrents",
        )
        .await?;
        Ok(engine_list_from_value(raw))
    }

    pub async fn global_stats(&self) -> Result<TorrentGlobalStats> {
        let raw = self.get_json("stats", "torrent engine statistics").await?;
        Ok(global_stats_from_value(raw))
    }

    pub async fn dht_stats(&self) -> Result<Value> {
        self.get_json("dht/stats", "DHT statistics").await
    }

    pub async fn dht_table(&self) -> Result<Value> {
        self.get_json("dht/table", "DHT routing table").await
    }

    pub async fn details(&self, torrent_id: &str) -> Result<TorrentDetails> {
        validate_engine_id(torrent_id)?;
        let raw = self
            .get_json(&format!("torrents/{torrent_id}"), "torrent details")
            .await?;
        Ok(details_from_value(torrent_id.to_owned(), raw))
    }

    pub async fn stats(&self, torrent_id: &str) -> Result<TorrentSnapshot> {
        validate_engine_id(torrent_id)?;
        let raw = decode_json(
            self.request_with(
                &self.stats_client,
                reqwest::Method::GET,
                &format!("torrents/{torrent_id}/stats/v1"),
            )
            .send()
            .await?,
            "torrent statistics",
        )
        .await?;
        Ok(snapshot_from_value(torrent_id.to_owned(), raw))
    }

    pub async fn peer_stats(&self, torrent_id: &str) -> Result<TorrentPeerStats> {
        validate_engine_id(torrent_id)?;
        let raw = self
            .get_json(
                &format!("torrents/{torrent_id}/peer_stats"),
                "torrent peer statistics",
            )
            .await?;
        Ok(peer_stats_from_value(raw))
    }

    pub async fn add_peers(&self, torrent_id: &str, peers: &[String]) -> Result<()> {
        validate_engine_id(torrent_id)?;
        if peers.is_empty() {
            return Err(RavynError::Invalid("at least one peer is required".into()));
        }
        let body = peers
            .iter()
            .map(|peer| peer.trim())
            .filter(|peer| !peer.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if body.is_empty() {
            return Err(RavynError::Invalid(
                "at least one valid peer is required".into(),
            ));
        }
        ensure_success(
            self.request(
                reqwest::Method::POST,
                &format!("torrents/{torrent_id}/add_peers"),
            )
            .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(body)
            .send()
            .await?,
            "add torrent peers",
        )
        .await?;
        Ok(())
    }

    pub async fn update_files(&self, torrent_id: &str, files: &[usize]) -> Result<()> {
        validate_engine_id(torrent_id)?;
        self.post_json(
            &format!("torrents/{torrent_id}/update_only_files"),
            &json!({ "only_files": files }),
            "update torrent files",
        )
        .await
    }

    pub async fn pause_torrent(&self, torrent_id: &str) -> Result<()> {
        validate_engine_id(torrent_id)?;
        self.post_empty(&format!("torrents/{torrent_id}/pause"), "pause torrent")
            .await
    }

    pub async fn start_torrent(&self, torrent_id: &str) -> Result<()> {
        validate_engine_id(torrent_id)?;
        self.post_empty(&format!("torrents/{torrent_id}/start"), "start torrent")
            .await
    }

    pub async fn remove_torrent(&self, torrent_id: &str, delete_files: bool) -> Result<()> {
        validate_engine_id(torrent_id)?;
        let action = if delete_files { "delete" } else { "forget" };
        self.post_empty(&format!("torrents/{torrent_id}/{action}"), "remove torrent")
            .await
    }

    pub async fn pause_job(&self, job_id: Uuid) -> Result<()> {
        if let Some(record) = self.repository.get_torrent_record(job_id).await? {
            self.pause_torrent(&record.torrent_id).await?;
        }
        Ok(())
    }

    pub async fn resume_job(&self, job_id: Uuid) -> Result<()> {
        if let Some(record) = self.repository.get_torrent_record(job_id).await? {
            self.start_torrent(&record.torrent_id).await?;
        }
        Ok(())
    }

    pub async fn remove_job(&self, job_id: Uuid, delete_files: bool) -> Result<()> {
        if let Some(record) = self.repository.get_torrent_record(job_id).await? {
            self.remove_torrent(&record.torrent_id, delete_files)
                .await?;
            self.repository.delete_torrent_record(job_id).await?;
        }
        Ok(())
    }

    /// Refreshes persisted statistics for torrents that remain managed after completion.
    pub async fn monitor_managed(self: Arc<Self>, cancellation: CancellationToken) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = interval.tick() => {
                    let records = match self.repository.list_torrent_records().await {
                        Ok(records) => records,
                        Err(error) => {
                            tracing::warn!(%error, "failed to list managed torrents");
                            continue;
                        }
                    };
                    let adapter = self.clone();
                    stream::iter(records)
                        .for_each_concurrent(self.refresh_concurrency, move |record| {
                            let adapter = adapter.clone();
                            async move {
                                let job = match adapter.repository.get_job(record.job_id).await {
                                    Ok(job) => job,
                                    Err(RavynError::NotFound(_)) => {
                                        let _ = adapter.repository.delete_torrent_record(record.job_id).await;
                                        return;
                                    }
                                    Err(error) => {
                                        tracing::debug!(%error, job_id = %record.job_id, "failed to load torrent job");
                                        return;
                                    }
                                };
                                if matches!(job.status, JobStatus::Downloading | JobStatus::Probing) {
                                    return;
                                }
                                if matches!(job.status, JobStatus::Paused | JobStatus::Cancelled) {
                                    if let Err(error) = adapter.pause_torrent(&record.torrent_id).await {
                                        tracing::debug!(%error, job_id = %record.job_id, "failed to enforce paused torrent state");
                                    }
                                }
                                match adapter.stats(&record.torrent_id).await {
                                    Ok(snapshot) => {
                                        let changed = snapshot.state != record.state
                                            || snapshot.downloaded_bytes != record.downloaded_bytes
                                            || snapshot.uploaded_bytes != record.uploaded_bytes
                                            || snapshot.download_speed_bps != record.download_speed_bps
                                            || snapshot.upload_speed_bps != record.upload_speed_bps
                                            || snapshot.peers_connected != record.peers_connected;
                                        if changed {
                                            if let Err(error) = adapter.repository.upsert_torrent_record(record.job_id, &snapshot).await {
                                                tracing::debug!(%error, job_id = %record.job_id, "failed to persist torrent snapshot");
                                            }
                                        }
                                        if job.status == JobStatus::Seeding {
                                            if let Err(error) = adapter.enforce_seeding_policy(&job, &snapshot).await {
                                                tracing::warn!(%error, job_id = %record.job_id, "failed to enforce torrent seeding policy");
                                            }
                                        }
                                    }
                                    Err(RavynError::NotFound(_)) => {
                                        let _ = adapter.repository.delete_torrent_record(record.job_id).await;
                                        if job.status == JobStatus::Seeding {
                                            let _ = adapter
                                                .repository
                                                .stop_torrent_seeding(record.job_id, "engine_missing", None)
                                                .await;
                                            let _ = adapter.repository.set_status(record.job_id, JobStatus::Completed, None).await;
                                            adapter.events.publish(Event::JobStatus {
                                                job_id: record.job_id,
                                                status: JobStatus::Completed,
                                                error: None,
                                            });
                                        }
                                    }
                                    Err(error) => {
                                        tracing::debug!(%error, job_id = %record.job_id, "failed to refresh torrent snapshot");
                                    }
                                }
                            }
                        })
                        .await;
                }
            }
        }
    }

    async fn enforce_seeding_policy(&self, job: &Job, snapshot: &TorrentSnapshot) -> Result<bool> {
        let options = job.options_json.torrent.clone().unwrap_or_default();
        if !options.seed_after_download || !options.keep_managed {
            return Ok(false);
        }
        let state = self
            .repository
            .begin_torrent_seeding(job.id, &snapshot.torrent_id)
            .await?;
        let ratio = (snapshot.downloaded_bytes > 0)
            .then(|| snapshot.uploaded_bytes as f64 / snapshot.downloaded_bytes as f64);
        self.repository
            .update_torrent_seeding_ratio(job.id, ratio)
            .await?;
        let elapsed =
            u64::try_from((Utc::now() - state.started_at).num_seconds().max(0)).unwrap_or_default();
        if elapsed < options.min_seed_time_secs {
            return Ok(false);
        }
        let stop_reason = if options
            .max_seed_ratio
            .zip(ratio)
            .is_some_and(|(limit, current)| current >= limit)
        {
            Some("ratio_limit")
        } else if options
            .max_seed_time_secs
            .is_some_and(|limit| elapsed >= limit)
        {
            Some("time_limit")
        } else {
            None
        };
        let Some(stop_reason) = stop_reason else {
            return Ok(false);
        };
        self.pause_torrent(&snapshot.torrent_id).await?;
        self.repository
            .stop_torrent_seeding(job.id, stop_reason, ratio)
            .await?;
        self.repository
            .set_status(job.id, JobStatus::Completed, None)
            .await?;
        self.repository
            .append_job_log(
                job.id,
                "torrent",
                "info",
                "SEEDING_POLICY_COMPLETE",
                &format!("torrent seeding stopped after reaching {stop_reason}"),
            )
            .await?;
        self.events.publish(Event::JobStatus {
            job_id: job.id,
            status: JobStatus::Completed,
            error: None,
        });
        Ok(true)
    }

    async fn add_or_resume(&self, job: &Job, options: &TorrentOptions) -> Result<String> {
        if let Some(record) = self.repository.get_torrent_record(job.id).await? {
            self.start_torrent(&record.torrent_id).await?;
            if !options.selected_files.is_empty() {
                self.update_files(&record.torrent_id, &options.selected_files)
                    .await?;
            }
            return Ok(record.torrent_id);
        }

        let mut query = vec![
            ("output_folder", job.destination.clone()),
            ("overwrite", options.overwrite.to_string()),
            ("list_only", "false".to_owned()),
        ];
        if let Some(regex) = options.file_regex.as_deref() {
            query.push(("only_files_regex", regex.to_owned()));
        }
        let response = self.source_request("torrents", &job.source, &query).await?;
        let raw = decode_json(response, "add torrent").await?;
        let torrent_id = extract_torrent_id(&raw).ok_or_else(|| {
            RavynError::Protocol(format!(
                "rqbit accepted the torrent but did not return an id: {raw}"
            ))
        })?;
        let snapshot = snapshot_from_value(torrent_id.clone(), raw);
        self.repository
            .upsert_torrent_record(job.id, &snapshot)
            .await?;
        if !options.selected_files.is_empty() {
            self.update_files(&torrent_id, &options.selected_files)
                .await?;
        }
        Ok(torrent_id)
    }

    async fn source_request(
        &self,
        path: &str,
        source: &str,
        query: &[(&str, String)],
    ) -> Result<Response> {
        let builder = self.request(reqwest::Method::POST, path).query(query);
        let response = if source.starts_with("magnet:")
            || source.starts_with("http://")
            || source.starts_with("https://")
        {
            builder
                .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(source.to_owned())
                .send()
                .await?
        } else {
            let path = Path::new(source);
            let metadata = tokio::fs::symlink_metadata(path).await.map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    RavynError::NotFound(source.to_owned())
                } else {
                    error.into()
                }
            })?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(RavynError::Invalid(
                    "local torrent source must be a regular non-symlink file".into(),
                ));
            }
            if metadata.len() > self.max_local_torrent_bytes {
                return Err(RavynError::Invalid(format!(
                    "torrent file is {} bytes; the configured limit is {} bytes",
                    metadata.len(),
                    self.max_local_torrent_bytes
                )));
            }
            builder
                .header(reqwest::header::CONTENT_TYPE, "application/x-bittorrent")
                .body(tokio::fs::read(path).await?)
                .send()
                .await?
        };
        ensure_success(response, "submit torrent").await
    }

    async fn get_json(&self, path: &str, operation: &str) -> Result<Value> {
        decode_json(
            self.request(reqwest::Method::GET, path).send().await?,
            operation,
        )
        .await
    }

    async fn post_json(&self, path: &str, body: &Value, operation: &str) -> Result<()> {
        ensure_success(
            self.request(reqwest::Method::POST, path)
                .json(body)
                .send()
                .await?,
            operation,
        )
        .await?;
        Ok(())
    }

    async fn post_empty(&self, path: &str, operation: &str) -> Result<()> {
        ensure_success(
            self.request(reqwest::Method::POST, path).send().await?,
            operation,
        )
        .await?;
        Ok(())
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.request_with(&self.client, method, path)
    }

    fn request_with(
        &self,
        client: &Client,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::RequestBuilder {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/{}", path.trim_start_matches('/')));
        let request = client.request(method, url);
        if let Some((username, password)) = self.basic_auth.as_ref() {
            request.basic_auth(username, Some(password))
        } else {
            request
        }
    }
}

#[async_trait]
impl DownloadAdapter for TorrentAdapter {
    async fn run(&self, job: &Job, cancellation: CancellationToken) -> Result<DownloadOutcome> {
        tokio::fs::create_dir_all(&job.destination).await?;
        let options = job.options_json.torrent.clone().unwrap_or_default();
        let torrent_id = self.add_or_resume(job, &options).await?;
        let poll_interval = Duration::from_millis(options.poll_interval_ms.clamp(250, 30_000));
        let mut consecutive_poll_failures = 0_u8;

        loop {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    // Pausing rather than terminating rqbit preserves fast-resume state.
                    let _ = self.pause_torrent(&torrent_id).await;
                    return Err(RavynError::Cancelled);
                }
                _ = tokio::time::sleep(poll_interval) => {
                    let snapshot = match self.stats(&torrent_id).await {
                        Ok(snapshot) => {
                            consecutive_poll_failures = 0;
                            snapshot
                        }
                        Err(error) if consecutive_poll_failures < 10 => {
                            consecutive_poll_failures += 1;
                            tracing::warn!(
                                %error,
                                %torrent_id,
                                attempt = consecutive_poll_failures,
                                "rqbit statistics are temporarily unavailable"
                            );
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    self.repository.upsert_torrent_record(job.id, &snapshot).await?;
                    self.progress_publisher.torrent_telemetry(
                        job.id,
                        snapshot.download_speed_bps,
                        snapshot.upload_speed_bps,
                        snapshot.peers_connected,
                    );
                    self.progress_publisher.publish(ProgressSnapshot {
                        job_id: job.id,
                        downloaded_bytes: snapshot.downloaded_bytes,
                        total_bytes: snapshot.total_bytes,
                        bytes_per_second: snapshot.download_speed_bps,
                    }).await?;

                    if snapshot.finished {
                        let details = self.details(&torrent_id).await?;
                        let selected = options.selected_files.as_slice();
                        let mut files = Vec::new();
                        for file in details.files {
                            if !selected.is_empty() && !selected.contains(&file.index) {
                                continue;
                            }
                            let path = std::path::Path::new(&job.destination).join(&file.path);
                            crate::services::security::validate_regular_file_under(
                                &path,
                                std::path::Path::new(&job.destination),
                                "torrent output",
                            )?;
                            files.push(path);
                        }
                        if !options.seed_after_download {
                            self.pause_torrent(&torrent_id).await?;
                        }
                        if !options.keep_managed {
                            self.remove_torrent(&torrent_id, false).await?;
                            self.repository.delete_torrent_record(job.id).await?;
                        }
                        let terminal_status = if options.seed_after_download && options.keep_managed {
                            self.repository
                                .begin_torrent_seeding(job.id, &torrent_id)
                                .await?;
                            Some(JobStatus::Seeding)
                        } else {
                            Some(JobStatus::Completed)
                        };
                        return Ok(DownloadOutcome {
                            primary_path: (files.len() == 1).then(|| files[0].clone()),
                            files,
                            artifacts: Vec::new(),
                            terminal_status,
                            terminal_message: None,
                        });
                    }
                }
            }
        }
    }
}

mod wire;

use self::wire::*;
