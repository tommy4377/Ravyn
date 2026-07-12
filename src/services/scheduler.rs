use std::{sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

use crate::{core::manager::JobManager, error::Result, storage::Repository};

pub async fn run(
    repository: Repository,
    manager: Arc<JobManager>,
    cancellation: CancellationToken,
) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(15));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            _ = interval.tick() => {
                if let Err(error) = repository.advance_skipped_schedules().await {
                    tracing::warn!(%error, "failed to advance skipped schedules");
                }
                loop {
                    let Some(claim) = repository.claim_due_schedule(Duration::from_secs(60)).await? else { break; };
                    match execute_with_renewal(&repository, &manager, &claim, &cancellation).await {
                        Ok(_) => {
                            if let Err(error) = repository.complete_schedule_claim(&claim).await {
                                tracing::error!(%error, schedule_id = %claim.schedule.id, "failed to complete schedule lease");
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, schedule_id = %claim.schedule.id, "scheduled job was not created");
                            let _ = repository.release_schedule_claim(&claim, &error.to_string()).await;
                        }
                    }
                }
            }
        }
    }
}

async fn execute_with_renewal(
    repository: &Repository,
    manager: &Arc<JobManager>,
    claim: &crate::storage::ScheduleClaim,
    cancellation: &CancellationToken,
) -> Result<()> {
    let Some(execution_id) = repository.begin_schedule_execution(claim).await? else {
        // The intended run time already has a durable execution record. This
        // claim is a retry/recovery duplicate and must not create work twice.
        return Ok(());
    };
    let execution = manager.execute_schedule(&claim.schedule);
    tokio::pin!(execution);
    let mut renewal = tokio::time::interval(Duration::from_secs(20));
    renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    renewal.tick().await;

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                repository.finish_schedule_execution(execution_id, "lease_lost", Some("scheduler is shutting down")).await?;
                return Err(crate::error::RavynError::Cancelled);
            }
            result = &mut execution => {
                match result {
                    Ok(()) => repository.finish_schedule_execution(execution_id, "completed", None).await?,
                    Err(ref error) => repository.finish_schedule_execution(execution_id, "failed", Some(&error.to_string())).await?,
                }
                return result;
            }
            _ = renewal.tick() => {
                if repository
                    .schedule_execution_cancellation_requested(execution_id)
                    .await?
                {
                    repository
                        .finish_schedule_execution(
                            execution_id,
                            "cancelled",
                            Some("schedule execution was cancelled or replaced"),
                        )
                        .await?;
                    return Err(crate::error::RavynError::Cancelled);
                }
                if let Err(error) = repository.renew_schedule_claim(claim, Duration::from_secs(60)).await {
                    repository.finish_schedule_execution(execution_id, "lease_lost", Some(&error.to_string())).await?;
                    return Err(error);
                }
            }
        }
    }
}
