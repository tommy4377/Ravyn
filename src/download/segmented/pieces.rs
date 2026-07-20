//! Checksum-verified piece downloading and retry helpers.

use super::*;

pub(super) async fn download_verified_piece(args: CommonArgs, record: SegmentRecord) -> Result<()> {
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

pub(super) async fn available_sources(args: &CommonArgs, limit: usize) -> Vec<usize> {
    let quarantined = args.quarantined_sources.lock().await;
    let start = args.next_source.fetch_add(1, Ordering::Relaxed);
    (0..args.sources.len())
        .map(|offset| (start + offset) % args.sources.len())
        .filter(|index| !quarantined.contains(index))
        .take(limit)
        .collect()
}

pub(super) async fn fetch_verified_piece(
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

pub(super) async fn fetch_verified_piece_inner(
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

pub(super) async fn backoff(
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

pub(super) fn validate_content_range(value: Option<&str>, start: u64, end: u64, total: u64) -> Result<()> {
    let value = value.ok_or_else(|| RavynError::Protocol("missing Content-Range".into()))?;
    let expected = format!("bytes {start}-{end}/{total}");
    if value != expected {
        return Err(RavynError::Protocol(format!(
            "unexpected Content-Range: {value}; expected {expected}"
        )));
    }
    Ok(())
}

