use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::{
    Client, StatusCode,
    header::{CONTENT_RANGE, HeaderMap, IF_RANGE, RANGE, RETRY_AFTER},
};
use sha2::{Digest, Sha256};
use tokio::{
    fs::OpenOptions,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom},
    sync::{Mutex, Notify, Semaphore},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    core::{metrics::Metrics, rate_limit::RateLimiters},
    error::{RavynError, Result},
    storage::{
        Repository, host_profiles,
        segments::{self, SegmentRecord},
    },
};

const MIN_WORK_UNIT_BYTES: u64 = 8 * 1024 * 1024;
const WORK_UNITS_PER_WORKER: usize = 4;
const MAX_VERIFIED_PIECE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct SegmentPlan {
    pub start: u64,
    pub end: u64,
}

pub fn plan(total: u64, requested: usize) -> Vec<SegmentPlan> {
    let count = requested.max(1).min(total.max(1) as usize);
    let size = total.div_ceil(count as u64);
    (0..count)
        .filter_map(|index| {
            let start = index as u64 * size;
            (start < total).then_some(SegmentPlan {
                start,
                end: (start + size - 1).min(total - 1),
            })
        })
        .collect()
}

/// Produces more durable work units than active workers. Workers pull the next
/// incomplete unit from a shared queue, preventing one slow tail segment from
/// determining the completion time of the whole download.
pub fn plan_work_units(total: u64, workers: usize) -> Vec<SegmentPlan> {
    let workers = workers.max(1);
    let maximum_units = workers.saturating_mul(WORK_UNITS_PER_WORKER);
    let useful_units =
        usize::try_from(total.div_ceil(MIN_WORK_UNIT_BYTES).max(1)).unwrap_or(usize::MAX);
    plan(
        total,
        maximum_units
            .min(useful_units)
            .max(workers.min(useful_units)),
    )
}

async fn ensure_layout(
    repository: &Repository,
    job_id: Uuid,
    path: &Path,
    total: u64,
    expected: &[SegmentPlan],
) -> Result<Vec<SegmentRecord>> {
    let existing = segments::list(repository.pool(), job_id).await?;
    let physical_len = tokio::fs::metadata(path).await.ok().map(|m| m.len());
    let compatible = physical_len == Some(total)
        && existing.len() == expected.len()
        && existing.iter().zip(expected).all(|(a, b)| {
            let length = b.end - b.start + 1;
            a.start == b.start
                && a.end == b.end
                && a.downloaded <= length
                && a.completed == (a.downloaded == length)
        });
    if compatible {
        return Ok(existing);
    }

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .await
        .map_err(|error| {
            RavynError::Internal(format!(
                "could not initialize segmented partial file {}: {error}",
                path.display()
            ))
        })?;
    file.set_len(total).await?;
    file.sync_data().await?;
    let records = expected
        .iter()
        .enumerate()
        .map(|(index, item)| SegmentRecord {
            index,
            start: item.start,
            end: item.end,
            downloaded: 0,
            completed: false,
        })
        .collect::<Vec<_>>();
    segments::replace(repository.pool(), job_id, &records).await?;
    Ok(records)
}

#[derive(Clone)]
struct CommonArgs {
    repository: Repository,
    job_id: Uuid,
    sources: Arc<[SourceContext]>,
    next_source: Arc<AtomicUsize>,
    path: PathBuf,
    total: u64,
    limiters: RateLimiters,
    max_retries: u32,
    cancellation: CancellationToken,
    progress: Arc<AtomicU64>,
    metrics: Metrics,
    piece_sha256: Option<Arc<[String]>>,
    quarantined_sources: Arc<Mutex<HashSet<usize>>>,
    host_circuit_threshold: u32,
    host_circuit_cooldown_secs: u64,
    completion_flags: Arc<[AtomicBool]>,
    completion_notify: Arc<Notify>,
}

impl CommonArgs {
    fn mark_completed(&self, index: usize) {
        if let Some(flag) = self.completion_flags.get(index) {
            flag.store(true, Ordering::Release);
            self.completion_notify.notify_waiters();
        }
    }
}

/// A mirror admitted only after its range support, length, and validator have
/// been checked against the primary object.
#[derive(Clone)]
pub struct SourceContext {
    pub client: Client,
    pub url: String,
    pub host: String,
    pub throughput_score: u64,
    pub headers: HeaderMap,
    pub validator: Option<String>,
    pub host_limit: Arc<Semaphore>,
}

#[derive(Clone)]
pub struct PieceChecksums {
    pub length: u64,
    pub sha256: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn download(
    repository: Repository,
    job_id: Uuid,
    client: Client,
    url: String,
    headers: HeaderMap,
    validator: Option<String>,
    path: PathBuf,
    total: u64,
    worker_count: usize,
    limiters: RateLimiters,
    host_limit: Arc<Semaphore>,
    max_retries: u32,
    cancellation: CancellationToken,
    progress: Arc<AtomicU64>,
    metrics: Metrics,
) -> Result<()> {
    download_multi(
        repository,
        job_id,
        vec![SourceContext {
            host: url::Url::parse(&url)
                .ok()
                .and_then(|value| value.host_str().map(str::to_owned))
                .unwrap_or_default(),
            throughput_score: 0,
            client,
            url,
            headers,
            validator,
            host_limit,
        }],
        path,
        total,
        worker_count,
        limiters,
        max_retries,
        cancellation,
        progress,
        metrics,
        None,
        u32::MAX,
        0,
        None,
    )
    .await
    .map(|_| ())
}

#[allow(clippy::too_many_arguments)]
pub async fn download_multi(
    repository: Repository,
    job_id: Uuid,
    sources: Vec<SourceContext>,
    path: PathBuf,
    total: u64,
    worker_count: usize,
    limiters: RateLimiters,
    max_retries: u32,
    cancellation: CancellationToken,
    progress: Arc<AtomicU64>,
    metrics: Metrics,
    piece_checksums: Option<PieceChecksums>,
    host_circuit_threshold: u32,
    host_circuit_cooldown_secs: u64,
    expected_sha256: Option<String>,
) -> Result<bool> {
    if sources.is_empty() {
        return Err(RavynError::Invalid(
            "segmented transfer requires at least one admitted source".into(),
        ));
    }
    if let Some(expected) = expected_sha256.as_deref() {
        crate::services::checksum::validate_sha256(expected)?;
    }
    let (expected, piece_sha256) = match piece_checksums {
        Some(pieces) => {
            if pieces.length == 0 || pieces.length > MAX_VERIFIED_PIECE_BYTES {
                return Err(RavynError::Invalid(format!(
                    "verified piece length must be between 1 and {MAX_VERIFIED_PIECE_BYTES} bytes"
                )));
            }
            let piece_count = usize::try_from(total.div_ceil(pieces.length))
                .map_err(|_| RavynError::Invalid("piece count exceeds platform limits".into()))?;
            let expected = (0..piece_count)
                .map(|index| {
                    let start = index as u64 * pieces.length;
                    SegmentPlan {
                        start,
                        end: (start + pieces.length - 1).min(total - 1),
                    }
                })
                .collect::<Vec<_>>();
            if expected.len() != pieces.sha256.len()
                || expected
                    .iter()
                    .enumerate()
                    .any(|(index, item)| item.start != index as u64 * pieces.length)
            {
                return Err(RavynError::Invalid(
                    "piece checksum layout does not cover the object exactly".into(),
                ));
            }
            (expected, Some(Arc::<[String]>::from(pieces.sha256)))
        }
        None => (plan_work_units(total, worker_count), None),
    };
    let records = ensure_layout(&repository, job_id, &path, total, &expected).await?;
    progress.store(
        records.iter().map(|item| item.downloaded).sum(),
        Ordering::Relaxed,
    );
    let completion_flags: Arc<[AtomicBool]> = records
        .iter()
        .map(|record| AtomicBool::new(record.completed))
        .collect::<Vec<_>>()
        .into();
    let completion_notify = Arc::new(Notify::new());

    let pending = records
        .into_iter()
        .filter(|item| !item.completed)
        .collect::<VecDeque<_>>();
    let active_workers = worker_count.max(1).min(pending.len().max(1));
    let queue = Arc::new(Mutex::new(pending));
    let common = CommonArgs {
        repository: repository.clone(),
        job_id,
        sources: sources.into(),
        next_source: Arc::new(AtomicUsize::new(0)),
        path: path.clone(),
        total,
        limiters,
        max_retries,
        cancellation: cancellation.clone(),
        progress,
        metrics,
        piece_sha256,
        quarantined_sources: Arc::new(Mutex::new(HashSet::new())),
        host_circuit_threshold,
        host_circuit_cooldown_secs,
        completion_flags: completion_flags.clone(),
        completion_notify: completion_notify.clone(),
    };

    let mut hash_task = expected_sha256.as_ref().map(|_| {
        let path = path.clone();
        let plans = expected.clone();
        let cancellation = cancellation.clone();
        tokio::spawn(async move {
            hash_completed_ranges(
                &path,
                &plans,
                &completion_flags,
                &completion_notify,
                &cancellation,
            )
            .await
        })
    });

    let mut tasks = JoinSet::new();
    for _ in 0..active_workers {
        let queue = queue.clone();
        let args = common.clone();
        tasks.spawn(async move {
            loop {
                let record = queue.lock().await.pop_front();
                let Some(record) = record else { return Ok(()) };
                let started = std::time::Instant::now();
                let result = download_segment(args.clone(), record).await;
                let outcome = match &result {
                    Ok(()) => "success",
                    Err(RavynError::Cancelled) => "cancelled",
                    Err(_) => "failure",
                };
                args.metrics.work_unit_finished(outcome, started.elapsed());
                result?;
            }
        });
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                cancellation.cancel();
                while tasks.join_next().await.is_some() {}
                drain_hash_task(&mut hash_task).await;
                return Err(error);
            }
            Err(error) => {
                cancellation.cancel();
                while tasks.join_next().await.is_some() {}
                drain_hash_task(&mut hash_task).await;
                return Err(RavynError::Internal(format!(
                    "segment worker failed: {error}"
                )));
            }
        }
    }

    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .await
        .map_err(|error| {
            RavynError::Internal(format!(
                "could not reopen completed partial file {} for synchronization: {error}",
                path.display()
            ))
        })?
        .sync_all()
        .await
        .map_err(|error| {
            RavynError::Internal(format!(
                "could not synchronize completed partial file {}: {error}",
                path.display()
            ))
        })?;
    let records = segments::list(repository.pool(), job_id).await?;
    if records.iter().any(|record| !record.completed) {
        return Err(RavynError::Protocol(
            "not every work unit reached durable completion".into(),
        ));
    }
    if let (Some(expected_hash), Some(task)) = (expected_sha256.as_deref(), hash_task.take()) {
        let actual_hash = task
            .await
            .map_err(|error| RavynError::Internal(format!("segment hasher failed: {error}")))??;
        if !actual_hash.eq_ignore_ascii_case(expected_hash) {
            repository.clear_segments(job_id).await?;
            if tokio::fs::try_exists(&path).await? {
                tokio::fs::remove_file(&path).await?;
            }
            return Err(RavynError::Invalid(format!(
                "segmented SHA-256 mismatch: expected {expected_hash}, got {actual_hash}"
            )));
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn drain_hash_task(task: &mut Option<tokio::task::JoinHandle<Result<String>>>) {
    if let Some(task) = task.take() {
        let _ = task.await;
    }
}

async fn hash_completed_ranges(
    path: &Path,
    plans: &[SegmentPlan],
    completion_flags: &[AtomicBool],
    completion_notify: &Notify,
    cancellation: &CancellationToken,
) -> Result<String> {
    if plans.len() != completion_flags.len() {
        return Err(RavynError::Internal(
            "segment hash plan and completion state differ".into(),
        ));
    }
    let mut file = OpenOptions::new().read(true).open(path).await?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut hasher = Sha256::new();
    for (index, plan) in plans.iter().enumerate() {
        loop {
            let notified = completion_notify.notified();
            if completion_flags[index].load(Ordering::Acquire) {
                break;
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
                _ = notified => {}
            }
        }
        file.seek(SeekFrom::Start(plan.start)).await?;
        let mut remaining = plan.end - plan.start + 1;
        while remaining > 0 {
            let wanted = usize::try_from(remaining.min(buffer.len() as u64)).map_err(|_| {
                RavynError::Internal("segment hash range exceeds platform limits".into())
            })?;
            let read = tokio::select! {
                _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
                read = file.read(&mut buffer[..wanted]) => read?,
            };
            if read == 0 {
                return Err(RavynError::Protocol(
                    "completed segment ended while hashing".into(),
                ));
            }
            hasher.update(&buffer[..read]);
            remaining -= read as u64;
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn download_segment(args: CommonArgs, record: SegmentRecord) -> Result<()> {
    if args.piece_sha256.is_some() {
        return download_verified_piece(args, record).await;
    }
    let expected = record.end - record.start + 1;
    let mut downloaded = record.downloaded;
    if downloaded == expected {
        segments::update(
            args.repository.pool(),
            args.job_id,
            record.index,
            downloaded,
            true,
        )
        .await?;
        args.mark_completed(record.index);
        return Ok(());
    }

    for attempt in 0..=args.max_retries {
        let start = record.start + downloaded;
        let source_index = args.next_source.fetch_add(1, Ordering::Relaxed) % args.sources.len();
        let source = &args.sources[source_index];
        let permit = tokio::select! {
            _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
            permit = source.host_limit.clone().acquire_owned() => permit.map_err(|_| RavynError::Cancelled)?,
        };
        let mut request = source
            .client
            .get(&source.url)
            .headers(source.headers.clone())
            .header(RANGE, format!("bytes={start}-{}", record.end));
        if let Some(value) = source.validator.as_deref() {
            request = request.header(IF_RANGE, value);
        }
        let response = tokio::select! {
            _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
            response = request.send() => response,
        };
        let response = match response {
            Ok(response) => response,
            Err(error)
                if attempt < args.max_retries
                    && (error.is_timeout() || error.is_connect() || error.is_request()) =>
            {
                drop(permit);
                backoff(attempt, None, &args.cancellation).await?;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(RavynError::Protocol(format!(
                "server rejected range {start}-{} with {}",
                record.end,
                response.status()
            )));
        }
        validate_content_range(
            response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok()),
            start,
            record.end,
            args.total,
        )?;
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs);
        let mut file = OpenOptions::new()
            .write(true)
            .open(&args.path)
            .await
            .map_err(|error| {
                RavynError::Internal(format!(
                    "could not open segmented partial file {}: {error}",
                    args.path.display()
                ))
            })?;
        file.seek(SeekFrom::Start(start)).await?;
        let mut stream = response.bytes_stream();
        let mut stream_failed = false;
        let mut last_persisted = downloaded;
        while let Some(item) = tokio::select! {
            _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
            item = stream.next() => item
        } {
            let chunk = match item {
                Ok(chunk) => chunk,
                Err(_) => {
                    stream_failed = true;
                    break;
                }
            };
            if downloaded + chunk.len() as u64 > expected {
                return Err(RavynError::Protocol(
                    "range body exceeded work-unit boundary".into(),
                ));
            }
            tokio::select! {
                _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
                _ = args.limiters.consume(chunk.len()) => {}
            }
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            args.progress
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            if downloaded.saturating_sub(last_persisted) >= 8 * 1024 * 1024 {
                file.sync_data().await?;
                segments::update(
                    args.repository.pool(),
                    args.job_id,
                    record.index,
                    downloaded,
                    false,
                )
                .await?;
                last_persisted = downloaded;
            }
        }
        file.flush().await?;
        file.sync_data().await?;
        let complete = downloaded == expected;
        segments::update(
            args.repository.pool(),
            args.job_id,
            record.index,
            downloaded,
            complete,
        )
        .await?;
        drop(permit);
        if complete {
            args.mark_completed(record.index);
            return Ok(());
        }
        if !stream_failed || attempt >= args.max_retries {
            return Err(RavynError::Protocol(format!(
                "short range body: expected {expected}, received {downloaded}"
            )));
        }
        backoff(attempt, retry_after, &args.cancellation).await?;
    }
    Err(RavynError::Protocol(
        "work-unit retry budget exhausted".into(),
    ))
}

async fn download_verified_piece(args: CommonArgs, record: SegmentRecord) -> Result<()> {
    let expected_len = record.end - record.start + 1;
    if record.completed && record.downloaded == expected_len {
        return Ok(());
    }
    if record.downloaded != 0 {
        segments::update(args.repository.pool(), args.job_id, record.index, 0, false).await?;
    }

    let attempts = usize::try_from(args.max_retries)
        .unwrap_or(usize::MAX)
        .saturating_add(args.sources.len())
        .max(1);
    let mut last_error = None;
    for attempt in 0..attempts {
        let candidates = available_sources(&args, if attempt == 0 { 2 } else { 1 }).await;
        if candidates.is_empty() {
            return Err(last_error.unwrap_or_else(|| {
                RavynError::Protocol("every admitted mirror was quarantined".into())
            }));
        }

        let mut tasks = JoinSet::new();
        let primary_args = args.clone();
        let primary_record = record.clone();
        let primary_source = candidates[0];
        tasks.spawn(async move {
            fetch_verified_piece(primary_args, primary_record, primary_source).await
        });

        if candidates.len() > 1 {
            let hedge_args = args.clone();
            let hedge_record = record.clone();
            let hedge_source = candidates[1];
            tasks.spawn(async move {
                tokio::select! {
                    _ = hedge_args.cancellation.cancelled() => Err(RavynError::Cancelled),
                    _ = tokio::time::sleep(Duration::from_millis(250)) => {
                        fetch_verified_piece(hedge_args, hedge_record, hedge_source).await
                    }
                }
            });
        }

        let mut verified = None;
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok(Ok(bytes)) => {
                    verified = Some(bytes);
                    tasks.shutdown().await;
                    break;
                }
                Ok(Err(RavynError::Cancelled)) if args.cancellation.is_cancelled() => {
                    tasks.shutdown().await;
                    return Err(RavynError::Cancelled);
                }
                Ok(Err(error)) => last_error = Some(error),
                Err(error) => {
                    last_error = Some(RavynError::Internal(format!(
                        "verified piece task failed: {error}"
                    )));
                }
            }
        }

        if let Some(bytes) = verified {
            let mut file = OpenOptions::new().write(true).open(&args.path).await?;
            file.seek(SeekFrom::Start(record.start)).await?;
            file.write_all(&bytes).await?;
            file.flush().await?;
            file.sync_data().await?;
            segments::update(
                args.repository.pool(),
                args.job_id,
                record.index,
                expected_len,
                true,
            )
            .await?;
            args.mark_completed(record.index);
            args.progress.fetch_add(expected_len, Ordering::Relaxed);
            return Ok(());
        }
        if attempt + 1 < attempts {
            backoff(attempt as u32, None, &args.cancellation).await?;
        }
    }
    Err(last_error
        .unwrap_or_else(|| RavynError::Protocol("verified piece retry budget exhausted".into())))
}

async fn available_sources(args: &CommonArgs, limit: usize) -> Vec<usize> {
    let quarantined = args.quarantined_sources.lock().await;
    let start = args.next_source.fetch_add(1, Ordering::Relaxed);
    (0..args.sources.len())
        .map(|offset| (start + offset) % args.sources.len())
        .filter(|index| !quarantined.contains(index))
        .take(limit)
        .collect()
}

async fn fetch_verified_piece(
    args: CommonArgs,
    record: SegmentRecord,
    source_index: usize,
) -> Result<Vec<u8>> {
    let started = std::time::Instant::now();
    let result = fetch_verified_piece_inner(args.clone(), record, source_index).await;
    let source = &args.sources[source_index];
    match &result {
        Ok(bytes) => {
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let throughput = (bytes.len() as f64 / elapsed) as u64;
            if let Err(error) =
                host_profiles::record_success(args.repository.pool(), &source.host, throughput)
                    .await
            {
                tracing::warn!(host = %source.host, %error, "failed to persist mirror success profile");
            }
        }
        Err(error) if error.failure_class().penalizes_host() => {
            let range_failure = error.failure_class() == crate::error::FailureClass::MalformedRange;
            if let Err(profile_error) = host_profiles::record_failure(
                args.repository.pool(),
                &source.host,
                &error.to_string(),
                range_failure,
                args.host_circuit_threshold,
                args.host_circuit_cooldown_secs,
            )
            .await
            {
                tracing::warn!(host = %source.host, %profile_error, "failed to persist mirror failure profile");
            }
        }
        _ => {}
    }
    result
}

async fn fetch_verified_piece_inner(
    args: CommonArgs,
    record: SegmentRecord,
    source_index: usize,
) -> Result<Vec<u8>> {
    let expected_hash = args
        .piece_sha256
        .as_ref()
        .and_then(|hashes| hashes.get(record.index))
        .ok_or_else(|| {
            RavynError::Invalid("piece checksum index is outside the admitted layout".into())
        })?;
    let expected_len = record.end - record.start + 1;
    let capacity = usize::try_from(expected_len)
        .map_err(|_| RavynError::Invalid("verified piece exceeds platform limits".into()))?;
    let source = &args.sources[source_index];
    let permit = tokio::select! {
        _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
        permit = source.host_limit.clone().acquire_owned() => permit.map_err(|_| RavynError::Cancelled)?,
    };
    let mut request = source
        .client
        .get(&source.url)
        .headers(source.headers.clone())
        .header(RANGE, format!("bytes={}-{}", record.start, record.end));
    if let Some(value) = source.validator.as_deref() {
        request = request.header(IF_RANGE, value);
    }
    let response = tokio::select! {
        _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
        response = request.send() => response?,
    };
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(RavynError::Protocol(format!(
            "mirror rejected verified range {}-{} with {}",
            record.start,
            record.end,
            response.status()
        )));
    }
    validate_content_range(
        response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok()),
        record.start,
        record.end,
        args.total,
    )?;

    let mut bytes = Vec::with_capacity(capacity);
    let mut stream = response.bytes_stream();
    while let Some(item) = tokio::select! {
        _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
        item = stream.next() => item,
    } {
        let chunk = item?;
        if bytes.len().saturating_add(chunk.len()) > capacity {
            return Err(RavynError::Protocol(
                "verified range body exceeded its piece boundary".into(),
            ));
        }
        tokio::select! {
            _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
            _ = args.limiters.consume(chunk.len()) => {}
        }
        bytes.extend_from_slice(&chunk);
    }
    drop(permit);
    if bytes.len() != capacity {
        return Err(RavynError::Protocol(format!(
            "short verified range body: expected {capacity}, received {}",
            bytes.len()
        )));
    }
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(expected_hash) {
        args.quarantined_sources.lock().await.insert(source_index);
        return Err(RavynError::Protocol(format!(
            "mirror returned corrupt piece {}",
            record.index
        )));
    }
    Ok(bytes)
}

async fn backoff(
    attempt: u32,
    retry_after: Option<Duration>,
    cancellation: &CancellationToken,
) -> Result<()> {
    let delay = retry_after
        .unwrap_or_else(|| Duration::from_millis(250_u64.saturating_mul(1_u64 << attempt.min(6))));
    tokio::select! {
        _ = cancellation.cancelled() => Err(RavynError::Cancelled),
        _ = tokio::time::sleep(delay) => Ok(())
    }
}

fn validate_content_range(value: Option<&str>, start: u64, end: u64, total: u64) -> Result<()> {
    let value = value.ok_or_else(|| RavynError::Protocol("missing Content-Range".into()))?;
    let expected = format!("bytes {start}-{end}/{total}");
    if value != expected {
        return Err(RavynError::Protocol(format!(
            "unexpected Content-Range: {value}; expected {expected}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1_000))]

        #[test]
        fn generated_segment_plans_have_exact_non_overlapping_coverage(
            total in 1_u64..=u32::MAX as u64,
            workers in 1_usize..=64,
        ) {
            let items = plan_work_units(total, workers);
            prop_assert!(!items.is_empty());
            prop_assert_eq!(items[0].start, 0);
            prop_assert_eq!(items.last().map(|item| item.end), Some(total - 1));
            prop_assert!(items.iter().all(|item| item.start <= item.end));
            prop_assert!(items.windows(2).all(|pair| pair[0].end + 1 == pair[1].start));
            let covered = items.iter().map(|item| item.end - item.start + 1).sum::<u64>();
            prop_assert_eq!(covered, total);
        }
    }

    #[test]
    fn plans_cover_every_byte_once() {
        let items = plan(10, 3);
        assert_eq!(items[0].start, 0);
        assert_eq!(items.last().unwrap().end, 9);
        assert!(items.windows(2).all(|w| w[0].end + 1 == w[1].start));
    }

    #[test]
    fn work_units_outnumber_workers_for_large_files() {
        let items = plan_work_units(512 * 1024 * 1024, 4);
        assert!(items.len() > 4);
        assert!(items.len() <= 16);
    }

    #[test]
    fn tiny_files_do_not_create_empty_work_units() {
        assert_eq!(plan_work_units(1024, 8).len(), 1);
    }

    #[test]
    fn validates_exact_range_and_total() {
        assert!(validate_content_range(Some("bytes 2-9/10"), 2, 9, 10).is_ok());
        assert!(validate_content_range(Some("bytes 2-9/11"), 2, 9, 10).is_err());
    }

    #[tokio::test]
    async fn segmented_hasher_waits_for_contiguous_durable_ranges() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("partial.bin");
        let bytes = (0..1024_u32)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&path, &bytes).await.unwrap();
        let plans = Arc::new(vec![
            SegmentPlan { start: 0, end: 511 },
            SegmentPlan {
                start: 512,
                end: 1023,
            },
        ]);
        let flags: Arc<[AtomicBool]> = vec![AtomicBool::new(false), AtomicBool::new(false)].into();
        let notify = Arc::new(Notify::new());
        let cancellation = CancellationToken::new();
        let task = {
            let path = path.clone();
            let plans = plans.clone();
            let flags = flags.clone();
            let notify = notify.clone();
            let cancellation = cancellation.clone();
            tokio::spawn(async move {
                hash_completed_ranges(&path, &plans, &flags, &notify, &cancellation).await
            })
        };

        flags[1].store(true, Ordering::Release);
        notify.notify_waiters();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!task.is_finished());

        flags[0].store(true, Ordering::Release);
        notify.notify_waiters();
        let actual = task.await.unwrap().unwrap();
        assert_eq!(actual, format!("{:x}", Sha256::digest(&bytes)));
    }
}
