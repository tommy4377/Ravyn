use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures_util::StreamExt;
use percent_encoding::percent_decode_str;
use reqwest::{
    Client, StatusCode,
    header::{
        AUTHORIZATION, CONTENT_RANGE, COOKIE, HeaderMap, HeaderName, HeaderValue, IF_RANGE,
        PROXY_AUTHORIZATION, RANGE, REFERER, RETRY_AFTER, USER_AGENT,
    },
};
use sha2::{Digest, Sha256};
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, Semaphore},
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::Config,
    core::{
        bandwidth::{FairBandwidthScheduler, FlowClass, FlowConfig},
        models::{browser_cookie_header, CreateJob, DuplicatePolicy, Job, JobKind, ProgressSnapshot},
        progress::ProgressPublisher,
        rate_limit::RateLimiters,
    },
    download::{
        adapter::{DownloadAdapter, DownloadOutcome},
        probe, segmented,
    },
    error::{RavynError, Result},
    services::{checksum, filename, rules, security},
    storage::{Repository, host_profiles, segments},
};

pub struct HttpAdapter {
    config: Arc<Config>,
    host_limits: Mutex<HashMap<String, Arc<Semaphore>>>,
    bandwidth: FairBandwidthScheduler,
    progress_publisher: ProgressPublisher,
    repository: Repository,
}

impl HttpAdapter {
    pub fn new(
        config: Arc<Config>,
        progress_publisher: ProgressPublisher,
        repository: Repository,
    ) -> Result<Self> {
        Ok(Self {
            bandwidth: FairBandwidthScheduler::new(config.global_speed_limit_bps),
            config,
            host_limits: Mutex::new(HashMap::new()),
            progress_publisher,
            repository,
        })
    }

    pub fn set_global_speed_limit(&self, bytes_per_second: u64) {
        self.bandwidth.set_capacity_bps(bytes_per_second);
    }

    fn destination(&self, job: &Job, metadata: &probe::RemoteMetadata) -> PathBuf {
        let name = job
            .filename
            .clone()
            .or_else(|| content_disposition_filename(metadata.content_disposition.as_deref()))
            .unwrap_or_else(|| filename::from_url(&metadata.final_url));
        PathBuf::from(&job.destination).join(filename::sanitize(&name))
    }

    async fn client_for_job(&self, job: &Job, source: &str) -> Result<Client> {
        let url = url::Url::parse(source)?;
        let host = url
            .host_str()
            .ok_or_else(|| RavynError::Invalid("download URL has no host".into()))?;
        let started = Instant::now();
        let resolved = security::resolve_network_source(&self.config, source).await;
        self.progress_publisher
            .metrics()
            .dns_resolved(resolved.is_ok(), started.elapsed());
        let addresses = resolved?;
        build_client(
            &self.config,
            job.options_json.proxy.as_deref().map(str::trim),
            Some((host, &addresses)),
        )
    }

    async fn resolve_job_secrets(&self, job: &Job) -> Result<Job> {
        let mut resolved = job.clone();
        if let Some(secret_id) = resolved.options_json.proxy_secret_id {
            resolved.options_json.proxy = Some(
                self.repository
                    .resolve_secret_reference(secret_id, "proxy_credentials")
                    .await?,
            );
        }
        if let Some(secret_id) = resolved.options_json.cookies_secret_id {
            let secret = self
                .repository
                .resolve_secret_reference(secret_id, "cookies")
                .await?;
            let cookies: std::collections::BTreeMap<String, String> = serde_json::from_str(&secret)
                .map_err(|_| {
                    RavynError::Invalid(
                        "cookie secret must be a JSON object of string name/value pairs".into(),
                    )
                })?;
            resolved.options_json.cookies.extend(cookies);
        }
        if let Some(secret_id) = resolved.options_json.authentication_header_secret_id {
            let secret = self
                .repository
                .resolve_secret_reference(secret_id, "authentication_header")
                .await?;
            resolved
                .options_json
                .headers
                .insert(AUTHORIZATION.as_str().to_owned(), secret);
        }
        Ok(resolved)
    }

    async fn host_limit(&self, url: &str) -> Result<Arc<Semaphore>> {
        let url = url::Url::parse(url)?;
        let host = url
            .host_str()
            .ok_or_else(|| RavynError::Invalid("download URL has no host".into()))?
            .to_ascii_lowercase();
        let mut limits = self.host_limits.lock().await;
        Ok(limits
            .entry(host)
            .or_insert_with(|| Arc::new(Semaphore::new(self.config.max_connections_per_host)))
            .clone())
    }

    fn request_headers(&self, job: &Job) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        for (name, value) in &job.options_json.headers {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                RavynError::Invalid(format!("invalid header name {name}: {error}"))
            })?;
            let value = HeaderValue::from_str(value)
                .map_err(|error| RavynError::Invalid(format!("invalid header value: {error}")))?;
            headers.insert(name, value);
        }
        if !job.options_json.cookies.is_empty() {
            let cookie = job
                .options_json
                .cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; ");
            headers.insert(
                COOKIE,
                HeaderValue::from_str(&cookie).map_err(|error| {
                    RavynError::Invalid(format!("invalid cookie value: {error}"))
                })?,
            );
        }
        if let Some(user_agent) = job.options_json.user_agent.as_deref() {
            headers.insert(
                USER_AGENT,
                HeaderValue::from_str(user_agent)
                    .map_err(|error| RavynError::Invalid(format!("invalid user agent: {error}")))?,
            );
        }
        if let Some(referer) = job.options_json.referer.as_deref() {
            headers.insert(
                REFERER,
                HeaderValue::from_str(referer)
                    .map_err(|error| RavynError::Invalid(format!("invalid referer: {error}")))?,
            );
        }
        Ok(headers)
    }

    /// Builds request headers for a concrete URL without forwarding credentials
    /// across origins. Ravyn follows redirects manually, so it must reproduce the
    /// credential-stripping behavior normally provided by the HTTP client.
    fn request_headers_for_url(&self, job: &Job, target: &url::Url) -> Result<HeaderMap> {
        let mut headers = self.request_headers(job)?;
        let source = url::Url::parse(&job.source)?;
        if !same_origin(&source, target) {
            // Custom headers can carry API keys under arbitrary names. Do not
            // forward any user-provided header to a different origin. Keep only
            // the explicitly configured User-Agent, which is not a credential.
            headers.clear();
            if let Some(user_agent) = job.options_json.user_agent.as_deref() {
                headers.insert(
                    USER_AGENT,
                    HeaderValue::from_str(user_agent).map_err(|error| {
                        RavynError::Invalid(format!("invalid user agent: {error}"))
                    })?,
                );
            }
            headers.remove(AUTHORIZATION);
            headers.remove(PROXY_AUTHORIZATION);
            headers.remove(COOKIE);
            headers.remove(REFERER);
        } else if let Some(browser_cookie) =
            browser_cookie_header(&job.options_json.browser_cookies, target)
        {
            let combined = headers
                .get(COOKIE)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .map_or(browser_cookie.clone(), |legacy| {
                    format!("{legacy}; {browser_cookie}")
                });
            headers.insert(
                COOKIE,
                HeaderValue::from_str(&combined).map_err(|error| {
                    RavynError::Invalid(format!("invalid browser cookie value: {error}"))
                })?,
            );
        }
        Ok(headers)
    }

    async fn apply_post_probe_rules(
        &self,
        job: &Job,
        metadata: &probe::RemoteMetadata,
    ) -> Result<Job> {
        let default_destination = self.config.effective_download_dir();
        let current_destination = PathBuf::from(&job.destination);
        let automatic_destination = job.options_json.library_auto_destination;
        let mut request = CreateJob {
            preset_id: None,
            kind: JobKind::Http,
            source: job.source.clone(),
            // An automatically selected library destination is not an explicit
            // user override. Leave the destination open during the MIME-aware
            // rule pass so a more specific post-probe rule can replace it.
            destination: (!automatic_destination && current_destination != default_destination)
                .then_some(current_destination.clone()),
            filename: job.filename.clone(),
            priority: job.priority,
            speed_limit_bps: job
                .speed_limit_bps
                .and_then(|value| u64::try_from(value).ok()),
            expected_sha256: job.expected_sha256.clone(),
            duplicate_policy: DuplicatePolicy::Allow,
            options: job.options_json.clone(),
        };
        let extension = url::Url::parse(&metadata.final_url).ok().and_then(|url| {
            Path::new(url.path())
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_owned)
        });
        let loaded_rules = self.repository.list_rules().await?;
        rules::apply_matching(
            &loaded_rules,
            &mut request,
            metadata.content_type.as_deref(),
            extension.as_deref(),
        );
        let destination = request
            .destination
            .clone()
            .unwrap_or_else(|| current_destination.clone());
        if automatic_destination && request.destination.is_some() {
            request.options.library_auto_destination = false;
        }
        security::validate_output_path(&self.config, &destination)?;
        self.repository
            .update_job_routing(
                job.id,
                &destination,
                request.speed_limit_bps,
                &request.options,
            )
            .await?;
        self.repository
            .attach_tags(job.id, &request.options.tags)
            .await?;
        let mut effective = job.clone();
        effective.destination = destination
            .to_str()
            .ok_or_else(|| RavynError::Invalid("destination path must be UTF-8".into()))?
            .to_owned();
        effective.speed_limit_bps = request
            .speed_limit_bps
            .map(i64::try_from)
            .transpose()
            .map_err(|_| RavynError::Invalid("speed limit exceeds SQLite integer range".into()))?;
        effective.options_json = request.options;
        Ok(effective)
    }
}

impl HttpAdapter {
    async fn run_source(
        &self,
        job: &Job,
        cancellation: CancellationToken,
    ) -> Result<DownloadOutcome> {
        let host = Self::host_from_source(&job.source)?;
        if let Some(profile) = host_profiles::get(self.repository.pool(), &host).await? {
            if let Some(until) = profile
                .circuit_open_until
                .filter(|until| *until > chrono::Utc::now())
            {
                self.progress_publisher
                    .metric_event("http_circuit_rejections");
                return Err(RavynError::Unavailable(format!(
                    "host circuit is open for {host} until {until}"
                )));
            }
        }

        let initial_bytes = job.downloaded_bytes.max(0) as u64;
        let started = Instant::now();
        let result = self.run_job(job, cancellation).await;
        match &result {
            Ok(_) => {
                if let Ok(updated) = self.repository.get_job(job.id).await {
                    let bytes =
                        (updated.downloaded_bytes.max(0) as u64).saturating_sub(initial_bytes);
                    let elapsed = started.elapsed().as_secs_f64().max(0.001);
                    let throughput = (bytes as f64 / elapsed) as u64;
                    if let Err(error) =
                        host_profiles::record_success(self.repository.pool(), &host, throughput)
                            .await
                    {
                        tracing::warn!(%host, %error, "failed to persist host success profile");
                    }
                }
            }
            Err(error) if error.failure_class().penalizes_host() => {
                let range_failure =
                    error.failure_class() == crate::error::FailureClass::MalformedRange;
                if let Err(profile_error) = host_profiles::record_failure(
                    self.repository.pool(),
                    &host,
                    &error.to_string(),
                    range_failure,
                    self.config.host_circuit_threshold,
                    self.config.host_circuit_cooldown_secs,
                )
                .await
                {
                    tracing::warn!(%host, %profile_error, "failed to persist host failure profile");
                }
            }
            _ => {}
        }
        result
    }

    async fn run_job(&self, job: &Job, cancellation: CancellationToken) -> Result<DownloadOutcome> {
        // A completed transfer may have crashed before the executor verified,
        // post-processed, and finalized the job. Reuse the durable file instead
        // of treating it as a conflicting new download or fetching it again.
        if job.transfer_mode == "complete" {
            if let Some(filename) = job.filename.as_deref() {
                let checkpoint = PathBuf::from(&job.destination).join(filename);
                security::validate_output_path(&self.config, &checkpoint)?;
                match fs::symlink_metadata(&checkpoint).await {
                    Ok(metadata)
                        if metadata.is_file()
                            && !metadata.file_type().is_symlink()
                            && job
                                .total_bytes
                                .and_then(|value| u64::try_from(value).ok())
                                .is_none_or(|expected| expected == metadata.len()) =>
                    {
                        return Ok(DownloadOutcome {
                            primary_path: Some(checkpoint.clone()),
                            files: vec![checkpoint],
                            artifacts: Vec::new(),
                            terminal_status: None,
                            terminal_message: None,
                        });
                    }
                    Ok(_) | Err(_) => {
                        // The checkpoint is incomplete or no longer exists.
                        // Reset only the transfer marker; normal remote identity
                        // reconciliation below will decide whether partial data
                        // can be resumed safely.
                        self.repository.set_transfer_mode(job.id, "none").await?;
                    }
                }
            } else {
                self.repository.set_transfer_mode(job.id, "none").await?;
            }
        }

        let mut current_url = url::Url::parse(&job.source)?;
        let metadata = {
            let mut resolved = None;
            for _ in 0..=10 {
                let client = self.client_for_job(job, current_url.as_str()).await?;
                let headers = self.request_headers_for_url(job, &current_url)?;
                match probe::probe(&client, current_url.as_str(), &headers).await? {
                    probe::ProbeResult::Metadata(metadata) => {
                        resolved = Some(metadata);
                        break;
                    }
                    probe::ProbeResult::Redirect(location) => {
                        self.progress_publisher.metric_event("http_redirects");
                        current_url = current_url.join(&location)?;
                        security::validate_network_source(&self.config, current_url.as_str())?;
                    }
                }
            }
            resolved.ok_or_else(|| {
                RavynError::Protocol("download probe exceeded the redirect limit".into())
            })?
        };
        let mut job = self.apply_post_probe_rules(job, &metadata).await?;
        if let Some(metalink) = job.options_json.metalink.as_ref() {
            if metadata.length != Some(metalink.size) {
                return Err(RavynError::Protocol(format!(
                    "Metalink mirror size mismatch: expected {}, received {:?}",
                    metalink.size, metadata.length
                )));
            }
        }
        fs::create_dir_all(&job.destination).await?;
        let client = self.client_for_job(&job, &metadata.final_url).await?;
        let final_url = url::Url::parse(&metadata.final_url)?;
        let headers = self.request_headers_for_url(&job, &final_url)?;
        let host_limit = self.host_limit(&metadata.final_url).await?;

        let mut output = self.destination(&job, &metadata);
        if job.filename.is_none() {
            if let Some(resolved_name) = output
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
            {
                self.repository
                    .update_job_fields(job.id, None, None, None, Some(&resolved_name), None)
                    .await?;
                job.filename = Some(resolved_name);
            }
        }
        if fs::try_exists(&output).await? && !job.options_json.overwrite {
            let mut occupied_partial = output.as_os_str().to_os_string();
            occupied_partial.push(".ravyn.part");
            let owns_resume_state =
                job.downloaded_bytes > 0 || fs::try_exists(Path::new(&occupied_partial)).await?;
            if owns_resume_state {
                // The path belongs to this job's own resume state; renaming
                // now would orphan the partial data.
                return Err(RavynError::Conflict(format!(
                    "{} already exists",
                    output.display()
                )));
            }
            let directory = PathBuf::from(&job.destination);
            let current_name = output
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| crate::services::filename::from_url(&metadata.final_url));
            let unique_name =
                crate::services::filename::next_available(&current_name, |candidate| {
                    let candidate_path = directory.join(candidate);
                    let mut candidate_partial = candidate_path.as_os_str().to_os_string();
                    candidate_partial.push(".ravyn.part");
                    candidate_path.exists() || Path::new(&candidate_partial).exists()
                });
            self.repository
                .update_job_fields(job.id, None, None, None, Some(&unique_name), None)
                .await?;
            job.filename = Some(unique_name);
            output = self.destination(&job, &metadata);
        }
        let mut partial_name = output.as_os_str().to_os_string();
        partial_name.push(".ravyn.part");
        let partial = PathBuf::from(partial_name);
        let partial_len = fs::metadata(&partial)
            .await
            .ok()
            .map(|metadata| metadata.len());
        let resume_reset = self
            .repository
            .set_remote_identity(
                job.id,
                &metadata.final_url,
                metadata.etag.as_deref(),
                metadata.last_modified.as_deref(),
                metadata.length,
                partial_len,
            )
            .await?;
        if resume_reset && fs::try_exists(&partial).await? {
            remove_file_with_retry(&partial).await?;
        }

        let priority = u32::try_from(job.priority.max(0)).unwrap_or(u32::MAX);
        let bandwidth_flow = self.bandwidth.register_scoped(
            job.id,
            FlowConfig {
                weight: priority.saturating_add(1),
                class: if job.priority < 0 {
                    FlowClass::Background
                } else {
                    FlowClass::Foreground
                },
                min_bps: None,
                max_bps: job
                    .speed_limit_bps
                    .and_then(|value| u64::try_from(value).ok()),
            },
        );
        let limiters = RateLimiters::single(bandwidth_flow.limiter());
        let progress = Arc::new(AtomicU64::new(0));

        let requested_segments = job
            .options_json
            .segments
            .unwrap_or(self.config.max_segments)
            .clamp(1, self.config.max_segments.max(1));
        let host = Self::host_from_source(&metadata.final_url)?;
        let profile = host_profiles::get(self.repository.pool(), &host).await?;
        if let Some(until) = profile
            .as_ref()
            .and_then(|profile| profile.circuit_open_until)
            .filter(|until| *until > chrono::Utc::now())
        {
            self.progress_publisher
                .metric_event("http_circuit_rejections");
            return Err(RavynError::Unavailable(format!(
                "host circuit is open for {host} until {until}"
            )));
        }
        let adaptive_segments = metadata
            .length
            .map(|total| {
                let base = crate::download::planner::adaptive_segment_count(
                    total,
                    requested_segments,
                    self.config.max_segments,
                );
                profile.as_ref().map_or(base, |profile| {
                    crate::download::planner::profile_adjusted_segment_count(
                        base,
                        profile.consecutive_failures,
                        profile.range_failures,
                        profile.average_throughput_bps,
                    )
                })
            })
            .unwrap_or(1);
        let use_segments = metadata.range_supported
            && metadata.length.is_some_and(|length| {
                length >= self.config.segment_threshold() && adaptive_segments > 1
            });

        let primary_validator = metadata.etag.clone().or(metadata.last_modified.clone());
        let mut admitted_sources = vec![segmented::SourceContext {
            client: client.clone(),
            url: metadata.final_url.clone(),
            host: host.clone(),
            throughput_score: profile
                .as_ref()
                .and_then(|value| value.average_throughput_bps)
                .unwrap_or(0),
            headers: headers.clone(),
            validator: primary_validator.clone(),
            host_limit: host_limit.clone(),
        }];
        if use_segments {
            for mirror in &job.options_json.mirrors {
                if mirror == &job.source || mirror == &metadata.final_url {
                    continue;
                }
                let admission = async {
                    security::validate_network_source(&self.config, mirror)?;
                    let mut mirror_url = url::Url::parse(mirror)?;
                    let mirror_metadata = {
                        let mut resolved = None;
                        for _ in 0..=10 {
                            let mirror_client =
                                self.client_for_job(&job, mirror_url.as_str()).await?;
                            let mirror_headers = self.request_headers_for_url(&job, &mirror_url)?;
                            match probe::probe(&mirror_client, mirror_url.as_str(), &mirror_headers)
                                .await?
                            {
                                probe::ProbeResult::Metadata(value) => {
                                    resolved = Some((mirror_client, value));
                                    break;
                                }
                                probe::ProbeResult::Redirect(location) => {
                                    mirror_url = mirror_url.join(&location)?;
                                    security::validate_network_source(
                                        &self.config,
                                        mirror_url.as_str(),
                                    )?;
                                }
                            }
                        }
                        resolved.ok_or_else(|| {
                            RavynError::Protocol("mirror probe exceeded the redirect limit".into())
                        })?
                    };
                    let (mirror_client, mirror_metadata) = mirror_metadata;
                    if !mirror_metadata.range_supported || mirror_metadata.length != metadata.length
                    {
                        return Err(RavynError::Protocol(
                            "mirror does not expose the same ranged object length".into(),
                        ));
                    }
                    let mirror_validator = mirror_metadata
                        .etag
                        .clone()
                        .or(mirror_metadata.last_modified.clone());
                    let checksum_identity = job.expected_sha256.is_some()
                        || job
                            .options_json
                            .metalink
                            .as_ref()
                            .is_some_and(|value| !value.piece_sha256.is_empty());
                    if !checksum_identity
                        && (primary_validator.is_none() || mirror_validator != primary_validator)
                    {
                        return Err(RavynError::Protocol(
                            "mirror lacks a validator matching the primary object".into(),
                        ));
                    }
                    let mirror_host = Self::host_from_source(&mirror_metadata.final_url)?;
                    let mirror_profile =
                        host_profiles::get(self.repository.pool(), &mirror_host).await?;
                    if let Some(until) = mirror_profile
                        .as_ref()
                        .and_then(|profile| profile.circuit_open_until)
                        .filter(|until| *until > chrono::Utc::now())
                    {
                        return Err(RavynError::Unavailable(format!(
                            "mirror circuit is open for {mirror_host} until {until}"
                        )));
                    }
                    let mirror_limit = self.host_limit(&mirror_metadata.final_url).await?;
                    let mirror_final_url = url::Url::parse(&mirror_metadata.final_url)?;
                    let mirror_headers = self.request_headers_for_url(&job, &mirror_final_url)?;
                    Ok::<_, RavynError>(segmented::SourceContext {
                        client: mirror_client,
                        url: mirror_metadata.final_url,
                        host: mirror_host,
                        throughput_score: mirror_profile
                            .and_then(|value| value.average_throughput_bps)
                            .unwrap_or(0),
                        headers: mirror_headers,
                        validator: mirror_validator,
                        host_limit: mirror_limit,
                    })
                }
                .await;
                match admission {
                    Ok(source) => admitted_sources.push(source),
                    Err(error) => {
                        tracing::warn!(%mirror, %error, "mirror was not admitted for concurrent range scheduling")
                    }
                }
            }
            admitted_sources.sort_by_key(|source| std::cmp::Reverse(source.throughput_score));
        }

        // A dedicated token stops only the reporting task. Cancelling it must
        // never cancel checksum verification or post-processing for the job.
        let reporter_cancellation = cancellation.child_token();
        let reporter = spawn_reporter(
            job.id,
            progress.clone(),
            metadata.length,
            self.progress_publisher.clone(),
            reporter_cancellation.clone(),
        );

        let result: Result<bool> = async {
            if use_segments {
                self.repository
                    .set_transfer_mode(job.id, "segmented")
                    .await?;
                let segmented_result = segmented::download_multi(
                    self.repository.clone(),
                    job.id,
                    admitted_sources,
                    partial.clone(),
                    metadata.length.ok_or_else(|| {
                        RavynError::Protocol("missing length for segmented download".into())
                    })?,
                    adaptive_segments,
                    limiters.clone(),
                    self.config.max_retries,
                    cancellation.child_token(),
                    progress.clone(),
                    self.progress_publisher.metrics(),
                    job.options_json.metalink.as_ref().and_then(|value| {
                        value.piece_length.map(|length| segmented::PieceChecksums {
                            length,
                            sha256: value.piece_sha256.clone(),
                        })
                    }),
                    self.config.host_circuit_threshold,
                    self.config.host_circuit_cooldown_secs,
                    job.expected_sha256.clone(),
                )
                .await;
                match segmented_result {
                    Err(RavynError::Protocol(error)) if !cancellation.is_cancelled() => {
                        self.progress_publisher
                            .metric_event("http_range_fallbacks");
                        tracing::warn!(job_id = %job.id, %error, "segmented transfer failed; resetting state and retrying as a single stream");
                        if let Err(profile_error) = host_profiles::record_range_failure(
                            self.repository.pool(),
                            &host,
                            &error,
                        )
                        .await
                        {
                            tracing::warn!(%host, %profile_error, "failed to persist host range failure");
                        }
                        reset_partial(&self.repository, job.id, &partial).await?;
                        progress.store(0, Ordering::Relaxed);
                        self.repository.set_transfer_mode(job.id, "single").await?;
                        single_stream(
                            &client,
                            &metadata.final_url,
                            headers,
                            &partial,
                            metadata
                                .etag
                                .as_deref()
                                .or(metadata.last_modified.as_deref()),
                            metadata.length,
                            limiters,
                            host_limit,
                            self.config.max_retries,
                            cancellation.child_token(),
                            progress.clone(),
                            job.expected_sha256.as_deref(),
                        )
                        .await
                    }
                    Ok(incrementally_verified) => Ok(incrementally_verified),
                    Err(error) => Err(error),
                }
            } else {
                if job.transfer_mode == "segmented"
                    || !segments::list(self.repository.pool(), job.id)
                        .await?
                        .is_empty()
                {
                    reset_partial(&self.repository, job.id, &partial).await?;
                }
                self.repository.set_transfer_mode(job.id, "single").await?;
                single_stream(
                    &client,
                    &metadata.final_url,
                    headers,
                    &partial,
                    metadata
                        .etag
                        .as_deref()
                        .or(metadata.last_modified.as_deref()),
                    metadata.length,
                    limiters,
                    host_limit,
                    self.config.max_retries,
                    cancellation.child_token(),
                    progress.clone(),
                    job.expected_sha256.as_deref(),
                )
                .await
            }
        }
        .await;

        reporter_cancellation.cancel();
        if let Err(error) = reporter.await {
            tracing::warn!(job_id = %job.id, %error, "progress reporter task failed");
        }
        let incrementally_verified = result?;

        let verification = async {
            if let Some(metalink) = job.options_json.metalink.as_ref() {
                if let Some(piece_length) = metalink.piece_length {
                    checksum::verify_pieces(
                        &partial,
                        piece_length,
                        &metalink.piece_sha256,
                        &cancellation,
                    )
                    .await?;
                }
            }
            if !incrementally_verified && let Some(expected) = job.expected_sha256.as_deref() {
                checksum::verify(&partial, expected, &cancellation).await?;
            }
            Ok::<(), RavynError>(())
        }
        .await;
        if let Err(error) = verification {
            reset_partial(&self.repository, job.id, &partial).await?;
            return Err(error);
        }

        let final_downloaded = progress.load(Ordering::Relaxed);
        self.progress_publisher
            .publish_terminal(ProgressSnapshot {
                job_id: job.id,
                downloaded_bytes: final_downloaded,
                total_bytes: metadata.length,
                bytes_per_second: 0,
            })
            .await?;

        if fs::try_exists(&output).await? {
            if job.options_json.overwrite {
                remove_file_with_retry(&output).await?;
            } else {
                return Err(RavynError::Conflict(format!(
                    "{} appeared while the download was running",
                    output.display()
                )));
            }
        }
        rename_with_retry(&partial, &output).await?;
        self.repository
            .set_transfer_mode(job.id, "complete")
            .await?;

        Ok(DownloadOutcome {
            primary_path: Some(output.clone()),
            files: vec![output],
            artifacts: Vec::new(),
            terminal_status: None,
            terminal_message: None,
        })
    }

    fn host_from_source(source: &str) -> Result<String> {
        let url = url::Url::parse(source)?;
        url.host_str()
            .map(|host| host.to_ascii_lowercase())
            .ok_or_else(|| RavynError::Invalid("download URL has no host".into()))
    }
}

#[async_trait]
impl DownloadAdapter for HttpAdapter {
    async fn run(&self, job: &Job, cancellation: CancellationToken) -> Result<DownloadOutcome> {
        let resolved_job = self.resolve_job_secrets(job).await?;
        let mut sources = Vec::with_capacity(resolved_job.options_json.mirrors.len() + 1);
        sources.push(resolved_job.source.clone());
        for mirror in &resolved_job.options_json.mirrors {
            if !sources.contains(mirror) {
                sources.push(mirror.clone());
            }
        }
        let mut last_error = None;
        for source in sources {
            if cancellation.is_cancelled() {
                return Err(RavynError::Cancelled);
            }
            let mut attempt = resolved_job.clone();
            attempt.source = source.clone();
            match self.run_source(&attempt, cancellation.child_token()).await {
                Ok(outcome) => return Ok(outcome),
                Err(error)
                    if matches!(
                        error.failure_class(),
                        crate::error::FailureClass::Cancellation
                            | crate::error::FailureClass::DiskFull
                            | crate::error::FailureClass::Permission
                    ) =>
                {
                    return Err(error);
                }
                Err(error) => {
                    tracing::warn!(%source, %error, "HTTP source failed; trying the next mirror");
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            RavynError::Invalid("HTTP job did not contain a usable source".into())
        }))
    }
}

fn build_client(
    config: &Config,
    proxy: Option<&str>,
    pinned: Option<(&str, &[std::net::SocketAddr])>,
) -> Result<Client> {
    let mut builder = Client::builder()
        .connect_timeout(config.connect_timeout())
        .read_timeout(config.read_timeout())
        .pool_max_idle_per_host(config.max_connections_per_host)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_nodelay(true)
        .http2_adaptive_window(true)
        .redirect(reqwest::redirect::Policy::none())
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .user_agent("Ravyn/0.1");
    if let Some(proxy) = proxy {
        builder = builder.proxy(reqwest::Proxy::all(proxy)?);
    }
    if let Some((host, addresses)) = pinned {
        builder = builder.resolve_to_addrs(host, addresses);
    }
    Ok(builder.build()?)
}

async fn reset_partial(repository: &Repository, job_id: uuid::Uuid, path: &Path) -> Result<()> {
    if fs::try_exists(path).await? {
        remove_file_with_retry(path).await?;
    }
    repository.clear_segments(job_id).await?;
    repository.set_transfer_mode(job_id, "none").await?;
    Ok(())
}

/// Windows can briefly keep an async file handle in a non-shareable state
/// after a cancelled segmented worker has exited. Retry only sharing/access
/// failures; all other filesystem errors are returned immediately.
async fn remove_file_with_retry(path: &Path) -> Result<()> {
    for attempt in 0..=20 {
        match fs::remove_file(path).await {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied && attempt < 20 => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => {
                return Err(RavynError::Internal(format!(
                    "could not remove {}: {error}",
                    path.display()
                )));
            }
        }
    }
    unreachable!("bounded file-removal retry loop must return")
}

async fn rename_with_retry(source: &Path, destination: &Path) -> Result<()> {
    for attempt in 0..=20 {
        match fs::rename(source, destination).await {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied && attempt < 20 => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => {
                return Err(RavynError::Internal(format!(
                    "could not rename {} to {}: {error}",
                    source.display(),
                    destination.display()
                )));
            }
        }
    }
    unreachable!("bounded file-rename retry loop must return")
}

async fn hash_file_prefix(
    path: &Path,
    length: u64,
    cancellation: &CancellationToken,
) -> Result<Sha256> {
    let mut file = fs::File::open(path).await?;
    let mut remaining = length;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut hasher = Sha256::new();
    while remaining > 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).map_err(|_| {
            RavynError::Internal("hash prefix length exceeds platform limits".into())
        })?;
        let read = tokio::select! {
            _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
            read = file.read(&mut buffer[..wanted]) => read?,
        };
        if read == 0 {
            return Err(RavynError::Protocol(
                "partial file ended while rebuilding incremental checksum state".into(),
            ));
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(hasher)
}

#[allow(clippy::too_many_arguments)]
async fn single_stream(
    client: &Client,
    url: &str,
    headers: HeaderMap,
    path: &Path,
    validator: Option<&str>,
    total: Option<u64>,
    limiters: RateLimiters,
    host_limit: Arc<Semaphore>,
    max_retries: u32,
    cancellation: CancellationToken,
    progress: Arc<AtomicU64>,
    expected_sha256: Option<&str>,
) -> Result<bool> {
    if let Some(expected) = expected_sha256 {
        checksum::validate_sha256(expected)?;
    }
    for attempt in 0..=max_retries {
        let mut existing = fs::metadata(path)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if total.is_some_and(|length| existing > length) {
            fs::remove_file(path).await?;
            existing = 0;
        }
        if total == Some(existing) && existing > 0 {
            progress.store(existing, Ordering::Relaxed);
            OpenOptions::new()
                .read(true)
                .open(path)
                .await?
                .sync_all()
                .await?;
            if let Some(expected) = expected_sha256 {
                if let Err(error) = checksum::verify(path, expected, &cancellation).await {
                    remove_file_with_retry(path).await?;
                    progress.store(0, Ordering::Relaxed);
                    return Err(error);
                }
                return Ok(true);
            }
            return Ok(false);
        }

        let permit = tokio::select! {
            _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
            permit = host_limit.clone().acquire_owned() => permit.map_err(|_| RavynError::Cancelled)?,
        };
        let mut request = client.get(url).headers(headers.clone());
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
            if let Some(value) = validator {
                request = request.header(IF_RANGE, value);
            }
        }
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
            response = request.send() => response,
        };
        let response = match response {
            Ok(response) => response,
            Err(error) if is_transient_reqwest(&error) && attempt < max_retries => {
                drop(permit);
                sleep_backoff(attempt, None, &cancellation).await?;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if is_transient_status(response.status()) && attempt < max_retries {
            let retry_after = retry_after(response.headers());
            drop(response);
            drop(permit);
            sleep_backoff(attempt, retry_after, &cancellation).await?;
            continue;
        }
        let append = if existing == 0 {
            if !response.status().is_success() {
                return Err(RavynError::Protocol(format!(
                    "download request returned {}",
                    response.status()
                )));
            }
            false
        } else if response.status() == StatusCode::PARTIAL_CONTENT {
            validate_resume_range(
                response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|value| value.to_str().ok()),
                existing,
                total,
            )?;
            true
        } else if response.status().is_success() {
            existing = 0;
            false
        } else {
            return Err(RavynError::Protocol(format!(
                "resume request returned {}",
                response.status()
            )));
        };

        progress.store(if append { existing } else { 0 }, Ordering::Relaxed);
        let mut hasher = if expected_sha256.is_some() {
            Some(if append {
                hash_file_prefix(path, existing, &cancellation).await?
            } else {
                Sha256::new()
            })
        } else {
            None
        };
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(path)
            .await?;
        let mut stream = response.error_for_status()?.bytes_stream();
        let mut stream_error = None;
        while let Some(item) = tokio::select! {
            _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
            item = stream.next() => item,
        } {
            let chunk = match item {
                Ok(chunk) => chunk,
                Err(error) => {
                    stream_error = Some(error);
                    break;
                }
            };
            tokio::select! {
                _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
                _ = limiters.consume(chunk.len()) => {}
            }
            file.write_all(&chunk).await?;
            if let Some(hasher) = &mut hasher {
                hasher.update(&chunk);
            }
            progress.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
        file.sync_all().await?;
        drop(file);
        drop(permit);
        let actual = fs::metadata(path).await?.len();
        if stream_error.is_none() && total.is_none_or(|expected| actual == expected) {
            if let (Some(expected), Some(hasher)) = (expected_sha256, hasher) {
                let actual_hash = format!("{:x}", hasher.finalize());
                if !actual_hash.eq_ignore_ascii_case(expected) {
                    remove_file_with_retry(path).await?;
                    progress.store(0, Ordering::Relaxed);
                    return Err(RavynError::Protocol(format!(
                        "SHA-256 mismatch: expected {expected}, got {actual_hash}"
                    )));
                }
                return Ok(true);
            }
            return Ok(false);
        }
        if attempt >= max_retries {
            if let Some(error) = stream_error {
                return Err(error.into());
            }
            return Err(RavynError::Protocol(format!(
                "download length mismatch: expected {:?}, received {actual}",
                total
            )));
        }
        sleep_backoff(attempt, None, &cancellation).await?;
    }
    Err(RavynError::Protocol(
        "single-stream retry budget exhausted".into(),
    ))
}

/// Validates a resume response's `Content-Range` contract. Public so the
/// protocol boundary can be fuzzed independently of network I/O.
pub fn validate_resume_range(value: Option<&str>, start: u64, total: Option<u64>) -> Result<()> {
    let value = value.ok_or_else(|| RavynError::Protocol("missing Content-Range".into()))?;
    let (range, response_total) = value
        .strip_prefix("bytes ")
        .and_then(|value| value.split_once('/'))
        .ok_or_else(|| RavynError::Protocol(format!("invalid Content-Range: {value}")))?;
    let (response_start, _) = range
        .split_once('-')
        .ok_or_else(|| RavynError::Protocol(format!("invalid Content-Range: {value}")))?;
    let response_start = response_start
        .parse::<u64>()
        .map_err(|_| RavynError::Protocol(format!("invalid Content-Range: {value}")))?;
    if response_start != start {
        return Err(RavynError::Protocol(format!(
            "resume started at {response_start}, expected {start}"
        )));
    }
    if let Some(expected_total) = total {
        let response_total = response_total
            .parse::<u64>()
            .map_err(|_| RavynError::Protocol(format!("invalid Content-Range: {value}")))?;
        if response_total != expected_total {
            return Err(RavynError::Protocol(format!(
                "remote length changed from {expected_total} to {response_total}"
            )));
        }
    }
    Ok(())
}

fn same_origin(left: &url::Url, right: &url::Url) -> bool {
    left.scheme().eq_ignore_ascii_case(right.scheme())
        && left
            .host_str()
            .zip(right.host_str())
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && left.port_or_known_default() == right.port_or_known_default()
}

/// Extracts and sanitizes a filename candidate from Content-Disposition.
/// Public so this untrusted-header parser remains directly fuzzable.
pub fn content_disposition_filename(value: Option<&str>) -> Option<String> {
    let value = value?;
    for part in value.split(';').map(str::trim) {
        if let Some(encoded) = part.strip_prefix("filename*=") {
            let encoded = encoded.trim_matches('"');
            let encoded = encoded.split_once("''").map_or(encoded, |(_, value)| value);
            return Some(percent_decode_str(encoded).decode_utf8_lossy().into_owned());
        }
    }
    for part in value.split(';').map(str::trim) {
        if let Some(name) = part.strip_prefix("filename=") {
            return Some(name.trim_matches('"').to_owned());
        }
    }
    None
}

fn is_transient_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn is_transient_reqwest(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_body() || error.is_request()
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds.min(60)))
}

async fn sleep_backoff(
    attempt: u32,
    retry_after: Option<Duration>,
    cancellation: &CancellationToken,
) -> Result<()> {
    let exponential = Duration::from_millis(250_u64.saturating_mul(1_u64 << attempt.min(7)));
    let jitter = Duration::from_millis((attempt as u64 * 97 + 53) % 211);
    let delay = retry_after
        .unwrap_or(exponential + jitter)
        .min(Duration::from_secs(60));
    tokio::select! {
        _ = cancellation.cancelled() => Err(RavynError::Cancelled),
        _ = tokio::time::sleep(delay) => Ok(()),
    }
}

fn spawn_reporter(
    job_id: uuid::Uuid,
    progress: Arc<AtomicU64>,
    total: Option<u64>,
    progress_publisher: ProgressPublisher,
    cancellation: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        let mut previous = 0u64;
        let mut previous_at = std::time::Instant::now();
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = ticker.tick() => {
                    let current = progress.load(Ordering::Relaxed);
                    let elapsed = previous_at.elapsed().as_secs_f64().max(0.001);
                    let speed = ((current.saturating_sub(previous)) as f64 / elapsed) as u64;
                    previous = current;
                    previous_at = std::time::Instant::now();
                    if let Err(error) = progress_publisher.publish(ProgressSnapshot {
                        job_id,
                        downloaded_bytes: current,
                        total_bytes: total,
                        bytes_per_second: speed,
                    }).await {
                        tracing::warn!(%error, %job_id, "progress reporter could not persist an update");
                        break;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_disposition_filename() {
        assert_eq!(
            content_disposition_filename(Some("attachment; filename*=UTF-8''hello%20world.zip")),
            Some("hello world.zip".into())
        );
    }
}
