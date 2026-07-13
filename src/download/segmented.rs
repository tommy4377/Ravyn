use std::{
    collections::{HashMap, HashSet, VecDeque},
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

const MIN_WORK_UNIT_BYTES: u64 = if cfg!(test) {
    64 * 1024
} else {
    8 * 1024 * 1024
};
const WORK_UNITS_PER_WORKER: usize = 4;
const MAX_VERIFIED_PIECE_BYTES: u64 = 64 * 1024 * 1024;
/// An active range is split only while at least this many bytes remain, so
/// both halves stay useful work units.
const SPLIT_MIN_REMAINING_BYTES: u64 = 2 * MIN_WORK_UNIT_BYTES;
/// Hard ceiling for the bytes one speculative duplicate may buffer in memory.
const MAX_SPECULATIVE_BYTES: u64 = 64 * 1024 * 1024;
/// A speculative duplicate is not worth its connection below this remainder.
const HEDGE_MIN_REMAINING_BYTES: u64 = if cfg!(test) { 16 * 1024 } else { 1024 * 1024 };
/// How long an idle worker observes a unit before calling it slow.
const SLOW_TAIL_OBSERVATION_MILLIS: u64 = 500;
/// Durable progress checkpoints during streaming.
const PERSIST_INTERVAL_BYTES: u64 = if cfg!(test) {
    64 * 1024
} else {
    8 * 1024 * 1024
};

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

/// Byte ranges that have reached durable completion. The incremental
/// whole-file hasher consumes the contiguous prefix as it grows, which stays
/// correct even when work units are split or hedged at runtime.
struct DurableCoverage {
    ranges: std::sync::Mutex<Vec<(u64, u64)>>,
    notify: Notify,
}

impl DurableCoverage {
    fn new() -> Self {
        Self {
            ranges: std::sync::Mutex::new(Vec::new()),
            notify: Notify::new(),
        }
    }

    fn mark(&self, start: u64, end: u64) {
        let mut ranges = self.ranges.lock().expect("durable coverage lock poisoned");
        ranges.push((start, end));
        ranges.sort_unstable();
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
        for (start, end) in ranges.drain(..) {
            match merged.last_mut() {
                Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        *ranges = merged;
        drop(ranges);
        self.notify.notify_waiters();
    }

    /// Exclusive end of the durable range that starts at byte zero.
    fn contiguous_prefix_end(&self) -> u64 {
        let ranges = self.ranges.lock().expect("durable coverage lock poisoned");
        match ranges.first() {
            Some(&(0, end)) => end.saturating_add(1),
            _ => 0,
        }
    }
}

/// Live coordination state for one plain (non-piece) work unit.
struct UnitState {
    index: usize,
    start: u64,
    /// Inclusive end. Only ever shrinks, when an idle worker splits the tail
    /// of this unit into a new work unit.
    dynamic_end: AtomicU64,
    /// Absolute offset of the next byte the owning worker will write.
    position: AtomicU64,
    /// Absolute offset below which the owner's bytes are known durable.
    synced: AtomicU64,
    /// The unit has been persisted complete by its owner or a winning hedge.
    done: AtomicBool,
    /// A speculative duplicate was already claimed for this unit.
    hedged: AtomicBool,
    /// Cancelled when a hedge wins so the owner stops streaming.
    owner_cancel: CancellationToken,
    /// Cancelled when the owner completes so a losing hedge stops.
    hedge_cancel: CancellationToken,
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
    coverage: Arc<DurableCoverage>,
    /// Serializes unit completion, splits, and hedge commits.
    active_units: Arc<Mutex<HashMap<usize, Arc<UnitState>>>>,
    next_index: Arc<AtomicUsize>,
    /// At most one speculative duplicate is in flight per transfer.
    hedge_slot: Arc<Semaphore>,
    has_expected_sha256: bool,
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
    let records = ensure_layout(
        &repository,
        job_id,
        &path,
        total,
        &expected,
        piece_sha256.is_some(),
    )
    .await?;
    progress.store(
        records.iter().map(|item| item.downloaded).sum(),
        Ordering::Relaxed,
    );
    let coverage = Arc::new(DurableCoverage::new());
    for record in &records {
        if record.completed {
            coverage.mark(record.start, record.end);
        }
    }
    let next_index = records
        .iter()
        .map(|record| record.index + 1)
        .max()
        .unwrap_or(0);

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
        coverage: coverage.clone(),
        active_units: Arc::new(Mutex::new(HashMap::new())),
        next_index: Arc::new(AtomicUsize::new(next_index)),
        hedge_slot: Arc::new(Semaphore::new(1)),
        has_expected_sha256: expected_sha256.is_some(),
    };

    let mut hash_task = expected_sha256.as_ref().map(|_| {
        let path = path.clone();
        let coverage = coverage.clone();
        let cancellation = cancellation.clone();
        tokio::spawn(
            async move { hash_durable_prefix(&path, total, &coverage, &cancellation).await },
        )
    });

    let mut tasks = JoinSet::new();
    for _ in 0..active_workers {
        let queue = queue.clone();
        let args = common.clone();
        tasks.spawn(async move {
            loop {
                let record = queue.lock().await.pop_front();
                let Some(record) = record else {
                    if args.piece_sha256.is_some() {
                        return Ok(());
                    }
                    match idle_assist(&args, &queue).await? {
                        IdleOutcome::Continue => continue,
                        IdleOutcome::Finished => return Ok(()),
                    }
                };
                let started = std::time::Instant::now();
                let result = run_work_unit(&args, record).await;
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

async fn ensure_layout(
    repository: &Repository,
    job_id: Uuid,
    path: &Path,
    total: u64,
    expected: &[SegmentPlan],
    strict: bool,
) -> Result<Vec<SegmentRecord>> {
    let existing = segments::list(repository.pool(), job_id).await?;
    let physical_len = tokio::fs::metadata(path).await.ok().map(|m| m.len());
    let compatible = physical_len == Some(total)
        && !existing.is_empty()
        && if strict {
            existing.len() == expected.len()
                && existing.iter().zip(expected).all(|(a, b)| {
                    let length = b.end - b.start + 1;
                    a.start == b.start
                        && a.end == b.end
                        && a.downloaded <= length
                        && a.completed == (a.downloaded == length)
                })
        } else {
            layout_tiles_exactly(&existing, total)
        };
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

/// Accepts any exact tiling of the object so that ranges split at runtime
/// resume without losing progress. Piece-verified layouts stay strict because
/// their checksum indices are positional.
fn layout_tiles_exactly(records: &[SegmentRecord], total: u64) -> bool {
    let mut sorted: Vec<&SegmentRecord> = records.iter().collect();
    sorted.sort_by_key(|record| record.start);
    let mut next = 0_u64;
    for record in sorted {
        if record.start != next || record.end < record.start || record.end >= total {
            return false;
        }
        let length = record.end - record.start + 1;
        // A crash between a split's progress clamp and the owner's completion
        // checkpoint can leave a fully downloaded record not yet marked
        // completed; that resumes cleanly, so only the reverse is corrupt.
        if record.downloaded > length || (record.completed && record.downloaded != length) {
            return false;
        }
        next = record.end + 1;
    }
    next == total
}

async fn drain_hash_task(task: &mut Option<tokio::task::JoinHandle<Result<String>>>) {
    if let Some(task) = task.take() {
        let _ = task.await;
    }
}

/// Hashes the durable contiguous prefix of the partial file as it grows.
/// Every byte is hashed exactly once, after the range containing it has been
/// durably marked complete.
async fn hash_durable_prefix(
    path: &Path,
    total: u64,
    coverage: &DurableCoverage,
    cancellation: &CancellationToken,
) -> Result<String> {
    let mut file = OpenOptions::new().read(true).open(path).await?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut hasher = Sha256::new();
    let mut hashed = 0_u64;
    while hashed < total {
        let frontier = loop {
            let notified = coverage.notify.notified();
            let frontier = coverage.contiguous_prefix_end().min(total);
            if frontier > hashed {
                break frontier;
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
                _ = notified => {}
            }
        };
        file.seek(SeekFrom::Start(hashed)).await?;
        let mut remaining = frontier - hashed;
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
        hashed = frontier;
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn run_work_unit(args: &CommonArgs, record: SegmentRecord) -> Result<()> {
    if args.piece_sha256.is_some() {
        return download_verified_piece(args.clone(), record).await;
    }
    let unit = Arc::new(UnitState {
        index: record.index,
        start: record.start,
        dynamic_end: AtomicU64::new(record.end),
        position: AtomicU64::new(record.start + record.downloaded),
        synced: AtomicU64::new(record.start + record.downloaded),
        done: AtomicBool::new(false),
        hedged: AtomicBool::new(false),
        owner_cancel: args.cancellation.child_token(),
        hedge_cancel: args.cancellation.child_token(),
    });
    args.active_units
        .lock()
        .await
        .insert(record.index, unit.clone());
    let result = match download_segment(args, &record, &unit).await {
        // A winning hedge cancels the owner; that is a success, not a job
        // cancellation.
        Err(RavynError::Cancelled)
            if unit.done.load(Ordering::Acquire) && !args.cancellation.is_cancelled() =>
        {
            Ok(())
        }
        other => other,
    };
    args.active_units.lock().await.remove(&record.index);
    // A losing hedge must not outlive its unit.
    unit.hedge_cancel.cancel();
    result
}

async fn download_segment(
    args: &CommonArgs,
    record: &SegmentRecord,
    unit: &UnitState,
) -> Result<()> {
    let mut downloaded = record.downloaded;
    if try_complete(args, unit, &mut downloaded).await? {
        return Ok(());
    }

    for attempt in 0..=args.max_retries {
        let request_end = unit.dynamic_end.load(Ordering::Acquire);
        let start = record.start + downloaded;
        if start > request_end {
            // A split moved the boundary below what is already written.
            if try_complete(args, unit, &mut downloaded).await? {
                return Ok(());
            }
        }
        let request_budget = request_end.saturating_sub(start) + 1;
        let source_index = args.next_source.fetch_add(1, Ordering::Relaxed) % args.sources.len();
        let source = &args.sources[source_index];
        let permit = tokio::select! {
            _ = unit.owner_cancel.cancelled() => return Err(RavynError::Cancelled),
            permit = source.host_limit.clone().acquire_owned() => permit.map_err(|_| RavynError::Cancelled)?,
        };
        let mut request = source
            .client
            .get(&source.url)
            .headers(source.headers.clone())
            .header(RANGE, format!("bytes={start}-{request_end}"));
        if let Some(value) = source.validator.as_deref() {
            request = request.header(IF_RANGE, value);
        }
        let response = tokio::select! {
            _ = unit.owner_cancel.cancelled() => return Err(RavynError::Cancelled),
            response = request.send() => response,
        };
        let response = match response {
            Ok(response) => response,
            Err(error)
                if attempt < args.max_retries
                    && (error.is_timeout() || error.is_connect() || error.is_request()) =>
            {
                drop(permit);
                backoff(attempt, None, &unit.owner_cancel).await?;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(RavynError::Protocol(format!(
                "server rejected range {start}-{request_end} with {}",
                response.status()
            )));
        }
        validate_content_range(
            response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok()),
            start,
            request_end,
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
        let mut received = 0_u64;
        let mut last_persisted = downloaded;
        while let Some(item) = tokio::select! {
            _ = unit.owner_cancel.cancelled() => return Err(RavynError::Cancelled),
            item = stream.next() => item
        } {
            let chunk = match item {
                Ok(chunk) => chunk,
                Err(_) => {
                    stream_failed = true;
                    break;
                }
            };
            received += chunk.len() as u64;
            if received > request_budget {
                return Err(RavynError::Protocol(
                    "range body exceeded work-unit boundary".into(),
                ));
            }
            tokio::select! {
                _ = unit.owner_cancel.cancelled() => return Err(RavynError::Cancelled),
                _ = args.limiters.consume(chunk.len()) => {}
            }
            // The unit may have been split since the request started; never
            // write or count bytes beyond the current boundary.
            let limit = unit.dynamic_end.load(Ordering::Acquire) - record.start + 1;
            let allowed = limit.saturating_sub(downloaded);
            if allowed == 0 {
                break;
            }
            let write_len = (chunk.len() as u64).min(allowed);
            let write_bytes = usize::try_from(write_len).unwrap_or(chunk.len());
            file.write_all(&chunk[..write_bytes]).await?;
            downloaded += write_len;
            unit.position
                .store(record.start + downloaded, Ordering::Release);
            args.progress.fetch_add(write_len, Ordering::Relaxed);
            if write_len < chunk.len() as u64 {
                break;
            }
            if downloaded.saturating_sub(last_persisted) >= PERSIST_INTERVAL_BYTES {
                file.sync_data().await?;
                unit.synced
                    .store(record.start + downloaded, Ordering::Release);
                if !persist_partial(args, unit, downloaded).await? {
                    break;
                }
                last_persisted = downloaded;
            }
        }
        file.flush().await?;
        file.sync_data().await?;
        unit.synced
            .store(record.start + downloaded, Ordering::Release);
        drop(permit);
        if try_complete(args, unit, &mut downloaded).await? {
            return Ok(());
        }
        persist_partial(args, unit, downloaded).await?;
        if !stream_failed || attempt >= args.max_retries {
            let expected = unit.dynamic_end.load(Ordering::Acquire) - record.start + 1;
            return Err(RavynError::Protocol(format!(
                "short range body: expected {expected}, received {downloaded}"
            )));
        }
        backoff(attempt, retry_after, &unit.owner_cancel).await?;
    }
    Err(RavynError::Protocol(
        "work-unit retry budget exhausted".into(),
    ))
}

/// Persists partial progress unless the unit was already completed elsewhere.
/// Returns false when the caller should stop streaming.
async fn persist_partial(args: &CommonArgs, unit: &UnitState, downloaded: u64) -> Result<bool> {
    let _registry = args.active_units.lock().await;
    if unit.done.load(Ordering::Acquire) {
        return Ok(false);
    }
    let limit = unit.dynamic_end.load(Ordering::Acquire) - unit.start + 1;
    segments::update(
        args.repository.pool(),
        args.job_id,
        unit.index,
        downloaded.min(limit),
        false,
    )
    .await?;
    Ok(true)
}

/// Completes the unit if its (possibly shrunk) range is fully written,
/// clamping any bytes that raced past a split boundary. Serialized with
/// splits and hedge commits through the unit registry lock.
async fn try_complete(args: &CommonArgs, unit: &UnitState, downloaded: &mut u64) -> Result<bool> {
    let _registry = args.active_units.lock().await;
    if unit.done.load(Ordering::Acquire) {
        return Ok(true);
    }
    let end = unit.dynamic_end.load(Ordering::Acquire);
    let expected = end - unit.start + 1;
    if *downloaded > expected {
        args.progress
            .fetch_sub(*downloaded - expected, Ordering::Relaxed);
        *downloaded = expected;
    }
    if *downloaded < expected {
        return Ok(false);
    }
    segments::update(
        args.repository.pool(),
        args.job_id,
        unit.index,
        expected,
        true,
    )
    .await?;
    unit.done.store(true, Ordering::Release);
    unit.hedge_cancel.cancel();
    args.coverage.mark(unit.start, end);
    Ok(true)
}

enum IdleOutcome {
    Continue,
    Finished,
}

/// An idle worker with an empty queue helps finish the tail: it splits the
/// largest still-active range while enough bytes remain, otherwise it may run
/// one bounded speculative duplicate of a slow unit. Returns `Finished` only
/// when no queued or active work remains.
async fn idle_assist(
    args: &CommonArgs,
    queue: &Arc<Mutex<VecDeque<SegmentRecord>>>,
) -> Result<IdleOutcome> {
    if args.cancellation.is_cancelled() {
        return Err(RavynError::Cancelled);
    }
    let mut best_split: Option<Arc<UnitState>> = None;
    let mut best_split_remaining = 0_u64;
    let mut best_hedge: Option<Arc<UnitState>> = None;
    let mut best_hedge_remaining = 0_u64;
    let any_active = {
        let registry = args.active_units.lock().await;
        for unit in registry.values() {
            if unit.done.load(Ordering::Acquire) || unit.hedged.load(Ordering::Acquire) {
                continue;
            }
            let end = unit.dynamic_end.load(Ordering::Acquire);
            let position = unit.position.load(Ordering::Acquire);
            if position > end {
                continue;
            }
            let remaining = end - position + 1;
            if remaining >= SPLIT_MIN_REMAINING_BYTES && remaining > best_split_remaining {
                best_split_remaining = remaining;
                best_split = Some(unit.clone());
            }
            let hedge_span =
                end.saturating_sub(unit.synced.load(Ordering::Acquire).min(position)) + 1;
            if remaining >= HEDGE_MIN_REMAINING_BYTES
                && hedge_span <= MAX_SPECULATIVE_BYTES
                && remaining > best_hedge_remaining
            {
                best_hedge_remaining = remaining;
                best_hedge = Some(unit.clone());
            }
        }
        !registry.is_empty()
    };
    if !any_active {
        return Ok(if queue.lock().await.is_empty() {
            IdleOutcome::Finished
        } else {
            IdleOutcome::Continue
        });
    }
    if let Some(unit) = best_split {
        split_unit(args, queue, &unit).await?;
        return Ok(IdleOutcome::Continue);
    }
    if let Some(unit) = best_hedge {
        hedge_unit(args, &unit).await?;
        return Ok(IdleOutcome::Continue);
    }
    // Nothing to help with right now; wait for movement.
    tokio::select! {
        _ = args.cancellation.cancelled() => Err(RavynError::Cancelled),
        _ = args.coverage.notify.notified() => Ok(IdleOutcome::Continue),
        _ = tokio::time::sleep(Duration::from_millis(200)) => Ok(IdleOutcome::Continue),
    }
}

/// Splits the un-downloaded tail of an active unit into a new durable work
/// unit so an idle worker can download it concurrently.
async fn split_unit(
    args: &CommonArgs,
    queue: &Arc<Mutex<VecDeque<SegmentRecord>>>,
    unit: &UnitState,
) -> Result<()> {
    let new_record = {
        let _registry = args.active_units.lock().await;
        if unit.done.load(Ordering::Acquire) || unit.hedged.load(Ordering::Acquire) {
            return Ok(());
        }
        let end = unit.dynamic_end.load(Ordering::Acquire);
        let position = unit.position.load(Ordering::Acquire).max(unit.start);
        if position > end {
            return Ok(());
        }
        let remaining = end - position + 1;
        if remaining < SPLIT_MIN_REMAINING_BYTES {
            return Ok(());
        }
        let new_start = position + remaining / 2;
        let new_record = SegmentRecord {
            index: args.next_index.fetch_add(1, Ordering::Relaxed),
            start: new_start,
            end,
            downloaded: 0,
            completed: false,
        };
        segments::split(
            args.repository.pool(),
            args.job_id,
            unit.index,
            new_start - 1,
            &new_record,
        )
        .await?;
        unit.dynamic_end.store(new_start - 1, Ordering::Release);
        new_record
    };
    queue.lock().await.push_back(new_record);
    args.metrics.event("http_range_splits");
    Ok(())
}

/// Runs at most one bounded speculative duplicate of a slow unit. The
/// duplicate is admitted only with an identity anchor (a whole-file checksum
/// or a source validator), buffers into isolated memory, never queues for a
/// connection, and the loser is always cancelled and cleaned up.
async fn hedge_unit(args: &CommonArgs, unit: &UnitState) -> Result<()> {
    let Ok(_slot) = args.hedge_slot.clone().try_acquire_owned() else {
        // Another hedge is in flight; check back on the next idle pass.
        return Ok(());
    };
    {
        let _registry = args.active_units.lock().await;
        if unit.done.load(Ordering::Acquire) || unit.hedged.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
    }
    // Slow-tail observation window.
    let position_before = unit.position.load(Ordering::Acquire);
    let job_before = args.progress.load(Ordering::Relaxed);
    tokio::select! {
        _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
        _ = unit.hedge_cancel.cancelled() => return Ok(()),
        _ = tokio::time::sleep(Duration::from_millis(SLOW_TAIL_OBSERVATION_MILLIS)) => {}
    }
    if unit.done.load(Ordering::Acquire) {
        return Ok(());
    }
    let position_after = unit.position.load(Ordering::Acquire);
    let unit_rate = position_after.saturating_sub(position_before) * 2;
    let job_rate = args
        .progress
        .load(Ordering::Relaxed)
        .saturating_sub(job_before)
        * 2;
    let end = unit.dynamic_end.load(Ordering::Acquire);
    if position_after > end {
        return Ok(());
    }
    let remaining = end - position_after + 1;
    let projected_secs = remaining / unit_rate.max(1);
    // A stalled unit is always hedged; a moving unit only when it is both far
    // from finishing and clearly below the transfer's aggregate pace, so a
    // healthy large tail is never duplicated.
    let slow = unit_rate == 0 || (projected_secs > 5 && unit_rate.saturating_mul(2) < job_rate);
    if !slow {
        // Not actually slow; surrender the claim so a later pass can re-check.
        unit.hedged.store(false, Ordering::Release);
        return Ok(());
    }
    for source_index in available_sources(args, args.sources.len()).await {
        let source = &args.sources[source_index];
        if !args.has_expected_sha256 && source.validator.is_none() {
            // Without an identity anchor a duplicate could silently write a
            // different object's bytes.
            continue;
        }
        // Connection guardrail: speculation never queues for a permit.
        let Ok(permit) = source.host_limit.clone().try_acquire_owned() else {
            continue;
        };
        let hedge_start = unit
            .synced
            .load(Ordering::Acquire)
            .min(unit.position.load(Ordering::Acquire));
        let end = unit.dynamic_end.load(Ordering::Acquire);
        if hedge_start > end {
            return Ok(());
        }
        let outcome = fetch_hedge_range(args, unit, source_index, hedge_start, end).await;
        drop(permit);
        match outcome {
            Ok(Some(bytes)) => {
                if commit_hedge(args, unit, hedge_start, &bytes).await? {
                    args.metrics.event("http_speculation_wins");
                } else {
                    args.metrics.event("http_speculation_losses");
                }
                return Ok(());
            }
            Ok(None) => {
                args.metrics.event("http_speculation_losses");
                return Ok(());
            }
            Err(RavynError::Cancelled) if args.cancellation.is_cancelled() => {
                return Err(RavynError::Cancelled);
            }
            Err(error) => {
                if error.failure_class().penalizes_host()
                    && let Err(profile_error) = host_profiles::record_failure(
                        args.repository.pool(),
                        &source.host,
                        &error.to_string(),
                        error.failure_class() == crate::error::FailureClass::MalformedRange,
                        args.host_circuit_threshold,
                        args.host_circuit_cooldown_secs,
                    )
                    .await
                {
                    tracing::warn!(host = %source.host, %profile_error, "failed to persist speculative failure profile");
                }
                tracing::debug!(%error, host = %source.host, "speculative range duplicate failed");
                args.metrics.event("http_speculation_losses");
            }
        }
    }
    Ok(())
}

/// Fetches one speculative duplicate range into isolated memory. Returns
/// `Ok(None)` when the owner finished first and the hedge lost.
async fn fetch_hedge_range(
    args: &CommonArgs,
    unit: &UnitState,
    source_index: usize,
    start: u64,
    end: u64,
) -> Result<Option<Vec<u8>>> {
    let source = &args.sources[source_index];
    let capacity = usize::try_from(end - start + 1)
        .map_err(|_| RavynError::Invalid("speculative range exceeds platform limits".into()))?;
    let started = std::time::Instant::now();
    let mut request = source
        .client
        .get(&source.url)
        .headers(source.headers.clone())
        .header(RANGE, format!("bytes={start}-{end}"));
    if let Some(value) = source.validator.as_deref() {
        request = request.header(IF_RANGE, value);
    }
    let response = tokio::select! {
        _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
        _ = unit.hedge_cancel.cancelled() => return Ok(None),
        response = request.send() => response?,
    };
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(RavynError::Protocol(format!(
            "speculative source rejected range {start}-{end} with {}",
            response.status()
        )));
    }
    validate_content_range(
        response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok()),
        start,
        end,
        args.total,
    )?;
    let mut bytes = Vec::with_capacity(capacity);
    let mut stream = response.bytes_stream();
    while let Some(item) = tokio::select! {
        _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
        _ = unit.hedge_cancel.cancelled() => return Ok(None),
        item = stream.next() => item,
    } {
        let chunk = item?;
        if bytes.len().saturating_add(chunk.len()) > capacity {
            return Err(RavynError::Protocol(
                "speculative range body exceeded its boundary".into(),
            ));
        }
        tokio::select! {
            _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
            _ = args.limiters.consume(chunk.len()) => {}
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() != capacity {
        return Err(RavynError::Protocol(format!(
            "short speculative range body: expected {capacity}, received {}",
            bytes.len()
        )));
    }
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let throughput = (bytes.len() as f64 / elapsed) as u64;
    if let Err(error) =
        host_profiles::record_success(args.repository.pool(), &source.host, throughput).await
    {
        tracing::warn!(host = %source.host, %error, "failed to persist speculative success profile");
    }
    Ok(Some(bytes))
}

/// Commits a winning hedge: writes the duplicate bytes, makes them durable,
/// persists completion, and cancels the losing owner stream. Returns false
/// when the owner won the race instead.
async fn commit_hedge(
    args: &CommonArgs,
    unit: &UnitState,
    start: u64,
    bytes: &[u8],
) -> Result<bool> {
    let _registry = args.active_units.lock().await;
    if unit.done.load(Ordering::Acquire) {
        return Ok(false);
    }
    let end = unit.dynamic_end.load(Ordering::Acquire);
    let needed = usize::try_from(end.saturating_sub(start) + 1)
        .map_err(|_| RavynError::Invalid("speculative range exceeds platform limits".into()))?;
    if needed > bytes.len() {
        return Ok(false);
    }
    let mut file = OpenOptions::new().write(true).open(&args.path).await?;
    file.seek(SeekFrom::Start(start)).await?;
    file.write_all(&bytes[..needed]).await?;
    file.flush().await?;
    file.sync_data().await?;
    let expected = end - unit.start + 1;
    segments::update(
        args.repository.pool(),
        args.job_id,
        unit.index,
        expected,
        true,
    )
    .await?;
    unit.done.store(true, Ordering::Release);
    let position = unit.position.load(Ordering::Acquire);
    args.progress.fetch_add(
        end.saturating_add(1).saturating_sub(position.max(start)),
        Ordering::Relaxed,
    );
    unit.owner_cancel.cancel();
    args.coverage.mark(unit.start, end);
    Ok(true)
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
            args.coverage.mark(record.start, record.end);
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

    #[test]
    fn durable_coverage_merges_ranges_and_tracks_the_prefix() {
        let coverage = DurableCoverage::new();
        assert_eq!(coverage.contiguous_prefix_end(), 0);
        coverage.mark(10, 19);
        assert_eq!(coverage.contiguous_prefix_end(), 0);
        coverage.mark(0, 4);
        assert_eq!(coverage.contiguous_prefix_end(), 5);
        coverage.mark(5, 9);
        assert_eq!(coverage.contiguous_prefix_end(), 20);
        assert_eq!(coverage.ranges.lock().unwrap().len(), 1);
    }

    #[test]
    fn split_layouts_resume_while_gapped_layouts_reset() {
        let record = |index, start, end, downloaded, completed| SegmentRecord {
            index,
            start,
            end,
            downloaded,
            completed,
        };
        // A runtime split re-tiled the object with an out-of-order index.
        let split = vec![
            record(0, 0, 99, 100, true),
            record(2, 100, 149, 0, false),
            record(1, 150, 299, 150, true),
        ];
        assert!(layout_tiles_exactly(&split, 300));
        // A gap must force a layout reset.
        let gapped = vec![record(0, 0, 99, 100, true), record(1, 150, 299, 0, false)];
        assert!(!layout_tiles_exactly(&gapped, 300));
        // An overlap must force a layout reset.
        let overlapping = vec![record(0, 0, 149, 0, false), record(1, 100, 299, 0, false)];
        assert!(!layout_tiles_exactly(&overlapping, 300));
        // Progress beyond a record's length is corrupt state.
        let overfull = vec![record(0, 0, 299, 400, true)];
        assert!(!layout_tiles_exactly(&overfull, 300));
        // A crash can persist full progress before the completion flag; that
        // state must resume instead of resetting.
        let unflagged = vec![record(0, 0, 299, 300, false)];
        assert!(layout_tiles_exactly(&unflagged, 300));
        // The reverse — completed without the bytes — is corrupt.
        let underfull = vec![record(0, 0, 299, 200, true)];
        assert!(!layout_tiles_exactly(&underfull, 300));
    }

    mod live_tail_tests {
        use std::{net::SocketAddr, path::PathBuf};

        use tokio::{
            io::{AsyncReadExt as _, AsyncWriteExt as _},
            net::{TcpListener, TcpStream},
        };

        use super::*;
        use crate::core::{
            models::{CreateJob, DownloadOptions, DuplicatePolicy, JobKind},
            rate_limit::{RateLimiter, RateLimiters},
        };

        /// Serves ranged GETs for `body`. The first ranged connection sends
        /// `stall_after` bytes and then hangs, simulating a stuck tail.
        async fn start_stall_server(body: Vec<u8>, stall_after: usize) -> SocketAddr {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let body = Arc::new(body);
            let stall_taken = Arc::new(AtomicBool::new(false));
            tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        break;
                    };
                    let body = body.clone();
                    let stall_taken = stall_taken.clone();
                    tokio::spawn(async move {
                        let _ = serve_ranged(stream, &body, stall_after, &stall_taken).await;
                    });
                }
            });
            address
        }

        async fn serve_ranged(
            mut stream: TcpStream,
            body: &[u8],
            stall_after: usize,
            stall_taken: &AtomicBool,
        ) -> std::io::Result<()> {
            let mut request = Vec::with_capacity(2048);
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).await?;
                if read == 0 {
                    return Ok(());
                }
                request.extend_from_slice(&buffer[..read]);
                if request.len() > 32 * 1024 {
                    return Ok(());
                }
            }
            let request = String::from_utf8_lossy(&request);
            let range = request
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.trim().eq_ignore_ascii_case("range"))
                .and_then(|(_, value)| {
                    let value = value.trim().strip_prefix("bytes=")?;
                    let (start, end) = value.split_once('-')?;
                    Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))
                });
            let Some((start, end)) = range.filter(|(start, end)| start <= end && *end < body.len())
            else {
                return Ok(());
            };
            let length = end - start + 1;
            let response = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {length}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await?;
            let stall = !stall_taken.swap(true, Ordering::AcqRel);
            let mut sent = 0_usize;
            for chunk in body[start..=end].chunks(8 * 1024) {
                if stall && sent >= stall_after {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    return Ok(());
                }
                stream.write_all(chunk).await?;
                sent += chunk.len();
            }
            Ok(())
        }

        async fn repository_with_job(
            temp: &tempfile::TempDir,
            source_url: &str,
        ) -> (Repository, Uuid) {
            if rustls::crypto::ring::default_provider()
                .install_default()
                .is_err()
            {
                assert!(rustls::crypto::CryptoProvider::get_default().is_some());
            }
            let url = format!("sqlite://{}", temp.path().join("test.sqlite3").display());
            let repository = Repository::connect(&url).await.unwrap();
            let job = repository
                .insert_job(
                    CreateJob {
                        preset_id: None,
                        kind: JobKind::Http,
                        source: source_url.to_owned(),
                        destination: None,
                        filename: Some("payload.bin".into()),
                        priority: 0,
                        speed_limit_bps: None,
                        expected_sha256: None,
                        duplicate_policy: DuplicatePolicy::Allow,
                        options: DownloadOptions::default(),
                    },
                    temp.path().to_path_buf(),
                )
                .await
                .unwrap();
            (repository, job.id)
        }

        fn source_for(address: SocketAddr) -> SourceContext {
            SourceContext {
                client: Client::new(),
                url: format!("http://{address}/payload.bin"),
                host: "127.0.0.1".into(),
                throughput_score: 0,
                headers: HeaderMap::new(),
                validator: None,
                host_limit: Arc::new(Semaphore::new(4)),
            }
        }

        #[allow(clippy::too_many_arguments)]
        async fn transfer(
            repository: &Repository,
            job_id: Uuid,
            source: SourceContext,
            path: &Path,
            body: &[u8],
        ) -> Result<bool> {
            let expected = hex::encode(Sha256::digest(body));
            tokio::time::timeout(
                Duration::from_secs(30),
                download_multi(
                    repository.clone(),
                    job_id,
                    vec![source],
                    path.to_path_buf(),
                    body.len() as u64,
                    2,
                    RateLimiters::single(Arc::new(RateLimiter::new(0))),
                    0,
                    CancellationToken::new(),
                    Arc::new(AtomicU64::new(0)),
                    Metrics::default(),
                    None,
                    u32::MAX,
                    0,
                    Some(expected),
                ),
            )
            .await
            .expect("transfer did not finish before the stall timeout")
        }

        async fn run_transfer(
            body: &[u8],
            stall_after: usize,
        ) -> (tempfile::TempDir, Repository, Uuid, PathBuf) {
            let temp = tempfile::tempdir().unwrap();
            let address = start_stall_server(body.to_vec(), stall_after).await;
            let (repository, job_id) =
                repository_with_job(&temp, &format!("http://{address}/payload.bin")).await;
            let path = temp.path().join("payload.part");
            let verified = transfer(&repository, job_id, source_for(address), &path, body)
                .await
                .expect("transfer failed");
            assert!(verified, "whole-file hash should have been verified");
            (temp, repository, job_id, path)
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_read_only_partial_file_fails_with_a_clear_error() {
            let temp = tempfile::tempdir().unwrap();
            let (repository, job_id) =
                repository_with_job(&temp, "http://127.0.0.1:9/payload.bin").await;
            let path = temp.path().join("payload.part");
            tokio::fs::write(&path, b"stale").await.unwrap();
            let mut permissions = tokio::fs::metadata(&path).await.unwrap().permissions();
            permissions.set_readonly(true);
            tokio::fs::set_permissions(&path, permissions.clone())
                .await
                .unwrap();

            let body = vec![0x11_u8; 256 * 1024];
            let source = source_for("127.0.0.1:9".parse().unwrap());
            let error = transfer(&repository, job_id, source, &path, &body)
                .await
                .expect_err("a read-only partial file must fail the transfer");
            assert!(
                error
                    .to_string()
                    .contains("could not initialize segmented partial file"),
                "unexpected error: {error}"
            );

            // Restore writability so the temporary directory can be removed.
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            tokio::fs::set_permissions(&path, permissions)
                .await
                .unwrap();
        }

        #[cfg(windows)]
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn an_exclusively_locked_partial_file_fails_without_hanging() {
            use std::os::windows::fs::OpenOptionsExt as _;

            let temp = tempfile::tempdir().unwrap();
            let (repository, job_id) =
                repository_with_job(&temp, "http://127.0.0.1:9/payload.bin").await;
            let path = temp.path().join("payload.part");
            std::fs::write(&path, b"stale").unwrap();
            // Deny all sharing so every reopen fails with a sharing violation.
            let _lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(&path)
                .unwrap();

            let body = vec![0x22_u8; 256 * 1024];
            let source = source_for("127.0.0.1:9".parse().unwrap());
            let error = transfer(&repository, job_id, source, &path, &body)
                .await
                .expect_err("a locked partial file must fail the transfer");
            assert!(
                error
                    .to_string()
                    .contains("could not initialize segmented partial file"),
                "unexpected error: {error}"
            );
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_crash_that_lost_the_db_checkpoint_recovers_by_redownloading() {
            // Simulates a crash after the partial file was fully synced but
            // before the segment checkpoint reached the database: the DB
            // claims half the file is missing while the bytes are present.
            let body = (0..1024 * 1024)
                .map(|index| (index % 233) as u8)
                .collect::<Vec<_>>();
            let temp = tempfile::tempdir().unwrap();
            let address = start_stall_server(body.clone(), body.len() + 1).await;
            let (repository, job_id) =
                repository_with_job(&temp, &format!("http://{address}/payload.bin")).await;
            let path = temp.path().join("payload.part");
            tokio::fs::write(&path, &body).await.unwrap();
            let half = body.len() as u64 / 2;
            segments::replace(
                repository.pool(),
                job_id,
                &[
                    SegmentRecord {
                        index: 0,
                        start: 0,
                        end: half - 1,
                        downloaded: half,
                        completed: true,
                    },
                    SegmentRecord {
                        index: 1,
                        start: half,
                        end: body.len() as u64 - 1,
                        downloaded: 0,
                        completed: false,
                    },
                ],
            )
            .await
            .unwrap();

            let verified = transfer(&repository, job_id, source_for(address), &path, &body)
                .await
                .expect("recovery transfer failed");
            assert!(verified);
            assert_eq!(tokio::fs::read(&path).await.unwrap(), body);
            let records = segments::list(repository.pool(), job_id).await.unwrap();
            assert!(records.iter().all(|record| record.completed));
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_stalled_tail_unit_is_completed_by_one_bounded_hedge() {
            // 1 MiB across 8 x 128 KiB units: the stalled remainder is below
            // the split minimum, so only the hedge can finish it.
            let body = (0..1024 * 1024)
                .map(|index| (index % 251) as u8)
                .collect::<Vec<_>>();
            let (_temp, repository, job_id, path) = run_transfer(&body, 64 * 1024).await;
            assert_eq!(tokio::fs::read(&path).await.unwrap(), body);
            let records = segments::list(repository.pool(), job_id).await.unwrap();
            assert!(records.iter().all(|record| record.completed));
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_stalled_wide_unit_is_split_before_the_hedge_finishes_it() {
            // 4 MiB across 8 x 512 KiB units: the stalled remainder is wide
            // enough that idle workers must split it into new work units.
            let body = (0..4 * 1024 * 1024)
                .map(|index| (index % 239) as u8)
                .collect::<Vec<_>>();
            let (_temp, repository, job_id, path) = run_transfer(&body, 64 * 1024).await;
            assert_eq!(tokio::fs::read(&path).await.unwrap(), body);
            let records = segments::list(repository.pool(), job_id).await.unwrap();
            assert!(
                records.len() > 8,
                "expected runtime splits to add work units; got {}",
                records.len()
            );
            assert!(records.iter().all(|record| record.completed));
        }
    }

    #[tokio::test]
    async fn segmented_hasher_waits_for_contiguous_durable_ranges() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("partial.bin");
        let bytes = (0..1024_u32)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&path, &bytes).await.unwrap();
        let coverage = Arc::new(DurableCoverage::new());
        let cancellation = CancellationToken::new();
        let task = {
            let path = path.clone();
            let coverage = coverage.clone();
            let cancellation = cancellation.clone();
            tokio::spawn(
                async move { hash_durable_prefix(&path, 1024, &coverage, &cancellation).await },
            )
        };

        coverage.mark(512, 1023);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!task.is_finished());

        coverage.mark(0, 511);
        let actual = task.await.unwrap().unwrap();
        assert_eq!(actual, format!("{:x}", Sha256::digest(&bytes)));
    }
}
