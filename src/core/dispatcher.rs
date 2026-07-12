//! Worker startup, queue dispatch, tracked tasks, and shutdown.

use futures_util::FutureExt;
use std::{
    future::Future,
    panic::AssertUnwindSafe,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::task::AbortHandle;

use crate::{
    core::{
        events::Event,
        models::{Job, JobStatus},
        progress::{self},
    },
    error::{RavynError, Result},
    services::scheduler,
};

use crate::core::manager::{ActiveJob, JobManager, TrackedTask};

impl JobManager {
    pub async fn start_workers(self: Arc<Self>) -> Result<()> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(RavynError::Conflict(
                "background workers have already been started".into(),
            ));
        }
        self.repository.recover_interrupted().await?;

        let receiver = self
            .progress_receiver
            .lock()
            .await
            .take()
            .ok_or_else(|| RavynError::Conflict("progress writer is already running".into()))?;

        let manager = self.clone();
        self.spawn_tracked("dispatcher", async move {
            manager.dispatch_loop().await;
        })
        .await?;

        let repository = self.repository.clone();
        let manager = self.clone();
        let cancel = self.shutdown.child_token();
        self.spawn_tracked("scheduler", async move {
            if let Err(error) = scheduler::run(repository, manager, cancel).await {
                tracing::error!(%error, "scheduler stopped");
            }
        })
        .await?;

        let repository = self.repository.clone();
        let manager = self.clone();
        let cancel = self.shutdown.child_token();
        self.spawn_tracked("bandwidth-schedule", async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        match repository.load_persistent_settings().await {
                            Ok(Some(settings)) => {
                                if let Err(error) = manager.apply_live_settings(&settings) {
                                    tracing::error!(%error, "bandwidth schedule is invalid");
                                }
                            }
                            Ok(None) => {}
                            Err(error) => tracing::error!(%error, "failed to load bandwidth schedule"),
                        }
                    }
                }
            }
        })
        .await?;

        let torrent = self.torrent.clone();
        let cancel = self.shutdown.child_token();
        self.spawn_tracked("torrent-monitor", async move {
            torrent.monitor_managed(cancel).await;
        })
        .await?;

        let repository = self.repository.clone();
        let cancel = self.shutdown.child_token();
        self.spawn_tracked("progress-writer", async move {
            if let Err(error) = progress::run_writer(repository, receiver, cancel).await {
                tracing::error!(%error, "durable progress writer stopped");
            }
        })
        .await?;

        Ok(())
    }

    pub(crate) async fn spawn_tracked<F>(
        &self,
        name: impl Into<String>,
        future: F,
    ) -> Result<AbortHandle>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if !self.accepting_tasks.load(Ordering::Acquire) {
            return Err(RavynError::Unavailable(
                "task supervisor is shutting down".into(),
            ));
        }
        let name = name.into();
        let task_name = name.clone();
        let handle = tokio::spawn(async move {
            if AssertUnwindSafe(future).catch_unwind().await.is_err() {
                tracing::error!(task = %task_name, "supervised task panicked");
            }
        });
        let abort = handle.abort_handle();
        let mut tasks = self.tasks.lock().await;
        tasks.retain(|task| !task.handle.is_finished());
        tasks.push(TrackedTask { name, handle });
        Ok(abort)
    }
    pub async fn shutdown(&self) {
        self.accepting_tasks.store(false, Ordering::Release);
        self.shutdown.cancel();

        let cancellations = self
            .active
            .lock()
            .await
            .values()
            .map(|active| active.cancellation.clone())
            .collect::<Vec<_>>();
        for cancellation in cancellations {
            cancellation.cancel();
        }

        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while !self.active.lock().await.is_empty() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        let tasks = {
            let mut tasks = self.tasks.lock().await;
            std::mem::take(&mut *tasks)
        };
        for mut task in tasks {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                task.handle.abort();
                let _ = task.handle.await;
                continue;
            }
            match tokio::time::timeout(remaining, &mut task.handle).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(task = %task.name, %error, "supervised task ended with a join error");
                }
                Err(_) => {
                    tracing::warn!(task = %task.name, "supervised task exceeded shutdown deadline");
                    task.handle.abort();
                    let _ = task.handle.await;
                }
            }
        }
    }

    async fn dispatch_loop(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(Duration::from_millis(200));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = self.shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let permit = match self.semaphore.try_acquire() {
                        Some(permit) => permit,
                        None => continue,
                    };
                    let job = match self.repository.claim_next_queued().await {
                        Ok(Some(job)) => job,
                        Ok(None) => {
                            drop(permit);
                            continue;
                        }
                        Err(error) => {
                            tracing::error!(%error, "queue query failed");
                            drop(permit);
                            continue;
                        }
                    };
                    if let Err(error) = self.clone().spawn_job(job, permit).await {
                        tracing::error!(%error, "failed to supervise claimed job");
                    }
                }
            }
        }
    }

    async fn spawn_job(
        self: Arc<Self>,
        job: Job,
        permit: super::manager::ConcurrencyPermit,
    ) -> Result<()> {
        if !self.accepting_tasks.load(Ordering::Acquire) {
            self.repository
                .set_status(job.id, JobStatus::Queued, Some("backend is shutting down"))
                .await?;
            return Ok(());
        }

        let cancellation = self.shutdown.child_token();
        self.active.lock().await.insert(
            job.id,
            ActiveJob {
                cancellation: cancellation.clone(),
                abort: None,
            },
        );

        let manager = self.clone();
        let job_id = job.id;
        let task = async move {
            let execution = AssertUnwindSafe(manager.execute(job, cancellation))
                .catch_unwind()
                .await;
            if execution.is_err() {
                let message = "job worker panicked";
                let _ = manager
                    .repository
                    .set_status(job_id, JobStatus::Failed, Some(message))
                    .await;
                manager.events.publish(Event::JobStatus {
                    job_id,
                    status: JobStatus::Failed,
                    error: Some(message.into()),
                });
            }
            manager.active.lock().await.remove(&job_id);
            drop(permit);
        };

        match self.spawn_tracked(format!("job-{job_id}"), task).await {
            Ok(abort) => {
                if let Some(active) = self.active.lock().await.get_mut(&job_id) {
                    active.abort = Some(abort);
                }
                Ok(())
            }
            Err(error) => {
                self.active.lock().await.remove(&job_id);
                self.repository
                    .set_status(
                        job_id,
                        JobStatus::Queued,
                        Some("task supervisor rejected job"),
                    )
                    .await?;
                Err(error)
            }
        }
    }
}
