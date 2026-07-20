//! Dynamic work splitting and speculative tail hedging.

use super::*;

pub(super) async fn idle_assist(
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
pub(super) async fn split_unit(
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
pub(super) async fn hedge_unit(args: &CommonArgs, unit: &UnitState) -> Result<()> {
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
pub(super) async fn fetch_hedge_range(
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
pub(super) async fn commit_hedge(
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

