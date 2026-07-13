use std::{collections::HashMap, time::Duration};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    core::{
        events::{Event, EventBus},
        metrics::Metrics,
        models::ProgressSnapshot,
    },
    error::Result,
    storage::Repository,
};

/// Publishes progress to the durable writer and independently fans it out to
/// transient API subscribers. The broadcast bus is intentionally never used as
/// the source of truth for persisted progress.
#[derive(Clone)]
pub struct ProgressPublisher {
    sender: mpsc::Sender<ProgressSnapshot>,
    repository: Repository,
    events: EventBus,
    metrics: Metrics,
}

pub type ProgressReceiver = mpsc::Receiver<ProgressSnapshot>;

pub fn channel(
    capacity: usize,
    repository: Repository,
    events: EventBus,
    metrics: Metrics,
) -> (ProgressPublisher, ProgressReceiver) {
    let (sender, receiver) = mpsc::channel(capacity.max(1));
    (
        ProgressPublisher {
            sender,
            repository,
            events,
            metrics,
        },
        receiver,
    )
}

impl ProgressPublisher {
    pub fn metric_event(&self, name: &'static str) {
        self.metrics.event(name);
    }

    /// Hands out the shared metrics registry for engine-level instrumentation
    /// that outlives individual progress snapshots.
    pub fn metrics(&self) -> Metrics {
        self.metrics.clone()
    }

    pub fn torrent_telemetry(&self, job_id: Uuid, download_bps: u64, upload_bps: u64, peers: u64) {
        self.metrics
            .torrent_telemetry(job_id, download_bps, upload_bps, peers);
    }

    /// Queues a durable progress update with backpressure, then broadcasts the
    /// same snapshot to lossy real-time subscribers.
    pub async fn publish(&self, snapshot: ProgressSnapshot) -> Result<()> {
        self.metrics.progress(&snapshot);
        self.events.publish(Event::Progress(snapshot.clone()));
        if self.sender.send(snapshot.clone()).await.is_err() {
            // A stopped writer must not silently discard authoritative state.
            self.repository
                .update_progress(
                    snapshot.job_id,
                    snapshot.downloaded_bytes,
                    snapshot.total_bytes,
                )
                .await?;
        }
        self.metrics.progress_writer_backlog(
            self.sender
                .max_capacity()
                .saturating_sub(self.sender.capacity()),
        );
        Ok(())
    }

    /// Commits a terminal checkpoint synchronously and then notifies clients.
    pub async fn publish_terminal(&self, snapshot: ProgressSnapshot) -> Result<()> {
        self.metrics.progress(&snapshot);
        self.repository
            .update_progress(
                snapshot.job_id,
                snapshot.downloaded_bytes,
                snapshot.total_bytes,
            )
            .await?;
        self.events.publish(Event::Progress(snapshot));
        Ok(())
    }
}

/// Persists progress independently from the broadcast event stream. Updates are
/// coalesced per job and flushed in a single SQLite transaction.
pub async fn run_writer(
    repository: Repository,
    mut receiver: ProgressReceiver,
    cancellation: CancellationToken,
) -> Result<()> {
    let mut pending = HashMap::<Uuid, ProgressSnapshot>::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                while let Ok(snapshot) = receiver.try_recv() {
                    pending.insert(snapshot.job_id, snapshot);
                }
                flush(&repository, &mut pending).await?;
                return Ok(());
            }
            update = receiver.recv() => {
                match update {
                    Some(snapshot) => {
                        pending.insert(snapshot.job_id, snapshot);
                        if pending.len() >= 128 {
                            flush(&repository, &mut pending).await?;
                        }
                    }
                    None => {
                        flush(&repository, &mut pending).await?;
                        return Ok(());
                    }
                }
            }
            _ = ticker.tick() => {
                flush(&repository, &mut pending).await?;
            }
        }
    }
}

async fn flush(
    repository: &Repository,
    pending: &mut HashMap<Uuid, ProgressSnapshot>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let updates = pending
        .drain()
        .map(|(_, update)| update)
        .collect::<Vec<_>>();
    repository.update_progress_batch(&updates).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{CreateJob, DownloadOptions, DuplicatePolicy, JobKind};
    use std::path::PathBuf;

    #[tokio::test]
    async fn durable_writer_keeps_the_latest_update_per_job() {
        let temp = tempfile::tempdir().unwrap();
        let repository = Repository::connect(&format!(
            "sqlite://{}",
            temp.path().join("progress.sqlite3").display()
        ))
        .await
        .unwrap();
        let job = repository
            .insert_job(
                CreateJob {
                    preset_id: None,
                    kind: JobKind::Http,
                    source: "https://example.test/file.bin".into(),
                    destination: Some(PathBuf::from("downloads")),
                    filename: None,
                    priority: 0,
                    speed_limit_bps: None,
                    expected_sha256: None,
                    duplicate_policy: DuplicatePolicy::Allow,
                    options: DownloadOptions::default(),
                },
                PathBuf::from("downloads"),
            )
            .await
            .unwrap();
        let events = EventBus::new(1);
        let (publisher, receiver) = channel(4, repository.clone(), events, Metrics::default());
        let cancellation = CancellationToken::new();
        let writer = tokio::spawn(run_writer(
            repository.clone(),
            receiver,
            cancellation.clone(),
        ));

        for downloaded in 1..=250_u64 {
            publisher
                .publish(ProgressSnapshot {
                    job_id: job.id,
                    downloaded_bytes: downloaded,
                    total_bytes: Some(250),
                    bytes_per_second: 1,
                })
                .await
                .unwrap();
        }
        cancellation.cancel();
        writer.await.unwrap().unwrap();
        let refreshed = repository.get_job(job.id).await.unwrap();
        assert_eq!(refreshed.downloaded_bytes, 250);
        assert_eq!(refreshed.total_bytes, Some(250));
    }
}
