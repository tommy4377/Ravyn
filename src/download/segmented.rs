use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::{
    Client, StatusCode,
    header::{CONTENT_RANGE, HeaderMap, IF_RANGE, RANGE, RETRY_AFTER},
};
use tokio::{
    fs::OpenOptions,
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
    sync::{Mutex, Semaphore},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    core::{metrics::Metrics, rate_limit::RateLimiters},
    error::{RavynError, Result},
    storage::{
        Repository,
        segments::{self, SegmentRecord},
    },
};

const MIN_WORK_UNIT_BYTES: u64 = 8 * 1024 * 1024;
const WORK_UNITS_PER_WORKER: usize = 4;

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
    workers: usize,
) -> Result<Vec<SegmentRecord>> {
    let existing = segments::list(repository.pool(), job_id).await?;
    let expected = plan_work_units(total, workers);
    let physical_len = tokio::fs::metadata(path).await.ok().map(|m| m.len());
    let compatible = physical_len == Some(total)
        && existing.len() == expected.len()
        && existing.iter().zip(&expected).all(|(a, b)| {
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
        .into_iter()
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
    client: Client,
    url: String,
    headers: HeaderMap,
    validator: Option<String>,
    path: PathBuf,
    total: u64,
    limiters: RateLimiters,
    host_limit: Arc<Semaphore>,
    max_retries: u32,
    cancellation: CancellationToken,
    progress: Arc<AtomicU64>,
    metrics: Metrics,
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
    let records = ensure_layout(&repository, job_id, &path, total, worker_count).await?;
    progress.store(
        records.iter().map(|item| item.downloaded).sum(),
        Ordering::Relaxed,
    );

    let pending = records
        .into_iter()
        .filter(|item| !item.completed)
        .collect::<VecDeque<_>>();
    let active_workers = worker_count.max(1).min(pending.len().max(1));
    let queue = Arc::new(Mutex::new(pending));
    let common = CommonArgs {
        repository: repository.clone(),
        job_id,
        client,
        url,
        headers,
        validator,
        path: path.clone(),
        total,
        limiters,
        host_limit,
        max_retries,
        cancellation: cancellation.clone(),
        progress,
        metrics,
    };

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
                return Err(error);
            }
            Err(error) => {
                cancellation.cancel();
                while tasks.join_next().await.is_some() {}
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
    Ok(())
}

async fn download_segment(args: CommonArgs, record: SegmentRecord) -> Result<()> {
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
        return Ok(());
    }

    for attempt in 0..=args.max_retries {
        let start = record.start + downloaded;
        let permit = tokio::select! {
            _ = args.cancellation.cancelled() => return Err(RavynError::Cancelled),
            permit = args.host_limit.clone().acquire_owned() => permit.map_err(|_| RavynError::Cancelled)?,
        };
        let mut request = args
            .client
            .get(&args.url)
            .headers(args.headers.clone())
            .header(RANGE, format!("bytes={start}-{}", record.end));
        if let Some(value) = args.validator.as_deref() {
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
}
