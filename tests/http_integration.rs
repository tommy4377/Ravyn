use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use clap::Parser;
use ravyn::{
    Ravyn,
    config::Config,
    core::models::{
        CreateJob, DownloadOptions, DuplicatePolicy, JobKind, JobStatus, MetalinkMetadata,
    },
    services::library::LibraryCategory,
};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{RwLock, oneshot},
};

#[derive(Clone)]
struct ResourceState {
    body: Arc<RwLock<Vec<u8>>>,
    etag: Arc<RwLock<String>>,
    reject_non_probe_ranges: Arc<AtomicBool>,
    range_requests: Arc<AtomicUsize>,
    /// When non-zero, data responses close after this many body bytes.
    fail_after_bytes: Arc<AtomicUsize>,
    delay_per_chunk: Duration,
}

struct TestServer {
    address: SocketAddr,
    state: ResourceState,
    shutdown: Option<oneshot::Sender<()>>,
}

impl TestServer {
    async fn start(body: Vec<u8>, etag: &str, delay_per_chunk: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = ResourceState {
            body: Arc::new(RwLock::new(body)),
            etag: Arc::new(RwLock::new(etag.to_owned())),
            reject_non_probe_ranges: Arc::new(AtomicBool::new(false)),
            range_requests: Arc::new(AtomicUsize::new(0)),
            fail_after_bytes: Arc::new(AtomicUsize::new(0)),
            delay_per_chunk,
        };
        let server_state = state.clone();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break };
                        let state = server_state.clone();
                        tokio::spawn(async move {
                            let _ = handle_connection(stream, state).await;
                        });
                    }
                }
            }
        });
        Self {
            address,
            state,
            shutdown: Some(shutdown_tx),
        }
    }

    fn url(&self) -> String {
        format!("http://{}/payload.bin", self.address)
    }

    async fn replace(&self, body: Vec<u8>, etag: &str) {
        *self.state.body.write().await = body;
        *self.state.etag.write().await = etag.to_owned();
    }

    fn reject_non_probe_ranges(&self, reject: bool) {
        self.state
            .reject_non_probe_ranges
            .store(reject, Ordering::Release);
    }

    fn range_requests(&self) -> usize {
        self.state.range_requests.load(Ordering::Acquire)
    }

    /// Zero disables the failure; any other value truncates data responses
    /// after that many body bytes (probe-sized requests stay unaffected).
    fn fail_after_bytes(&self, bytes: usize) {
        self.state.fail_after_bytes.store(bytes, Ordering::Release);
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

async fn handle_connection(mut stream: TcpStream, state: ResourceState) -> std::io::Result<()> {
    let mut request = Vec::with_capacity(4096);
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
    let mut lines = request.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let method = request_line.split_whitespace().next().unwrap_or_default();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect::<HashMap<_, _>>();

    let body = state.body.read().await.clone();
    let etag = state.etag.read().await.clone();
    let range = headers
        .get("range")
        .and_then(|value| parse_range(value, body.len()));
    let reject_range = state.reject_non_probe_ranges.load(Ordering::Acquire)
        && range.is_some_and(|(start, end)| !(start == 0 && end == 0));
    if range.is_some_and(|(start, end)| !(start == 0 && end == 0)) {
        state.range_requests.fetch_add(1, Ordering::AcqRel);
    }

    if method == "HEAD" {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nETag: {}\r\nConnection: close\r\n\r\n",
            body.len(),
            etag
        );
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    let (status, start, end) = match range.filter(|_| !reject_range) {
        Some((start, end)) => ("206 Partial Content", start, end),
        None => ("200 OK", 0, body.len().saturating_sub(1)),
    };
    let length = if body.is_empty() { 0 } else { end - start + 1 };
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\nETag: {etag}\r\nConnection: close\r\n"
    );
    if status.starts_with("206") {
        response.push_str(&format!(
            "Content-Range: bytes {start}-{end}/{}\r\n",
            body.len()
        ));
    }
    response.push_str("\r\n");
    stream.write_all(response.as_bytes()).await?;
    if method != "HEAD" && !body.is_empty() {
        // Probe-sized requests (single byte) are never truncated so the
        // planner still sees a healthy resource.
        let fail_after = state.fail_after_bytes.load(Ordering::Acquire);
        let truncate = fail_after != 0 && length > 1;
        let mut written = 0_usize;
        for chunk in body[start..=end].chunks(64 * 1024) {
            let allowed = if truncate {
                fail_after.saturating_sub(written).min(chunk.len())
            } else {
                chunk.len()
            };
            stream.write_all(&chunk[..allowed]).await?;
            written += allowed;
            if truncate && written >= fail_after {
                // Close mid-body: the client sees a short read.
                return Ok(());
            }
            if !state.delay_per_chunk.is_zero() {
                tokio::time::sleep(state.delay_per_chunk).await;
            }
        }
    }
    Ok(())
}

fn parse_range(value: &str, total: usize) -> Option<(usize, usize)> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    let start = start.parse::<usize>().ok()?;
    let end = if end.is_empty() {
        total.checked_sub(1)?
    } else {
        end.parse::<usize>().ok()?.min(total.checked_sub(1)?)
    };
    (start <= end).then_some((start, end))
}

fn test_config(root: &Path) -> Config {
    Config::try_parse_from([
        "ravyn",
        "--data-dir",
        root.join("data").to_str().unwrap(),
        "--download-dir",
        root.join("downloads").to_str().unwrap(),
        "--allow-private-network",
        "--library-auto-organize",
        "false",
        "--max-active",
        "1",
        "--max-segments",
        "4",
        "--segment-threshold-mib",
        "1",
        "--max-retries",
        "1",
    ])
    .unwrap()
}

fn organized_test_config(root: &Path) -> Config {
    Config::try_parse_from([
        "ravyn",
        "--data-dir",
        root.join("data").to_str().unwrap(),
        "--download-dir",
        root.join("downloads").to_str().unwrap(),
        "--library-root",
        root.join("Ravyn").to_str().unwrap(),
        "--allow-private-network",
        "--max-active",
        "1",
        "--max-segments",
        "4",
        "--segment-threshold-mib",
        "1",
        "--max-retries",
        "1",
    ])
    .unwrap()
}

fn create_request(url: String, expected_sha256: Option<String>) -> CreateJob {
    CreateJob {
        preset_id: None,
        kind: JobKind::Http,
        source: url,
        destination: None,
        filename: Some("payload.bin".into()),
        priority: 0,
        speed_limit_bps: None,
        expected_sha256,
        duplicate_policy: DuplicatePolicy::Allow,
        options: DownloadOptions {
            overwrite: true,
            segments: Some(4),
            ..DownloadOptions::default()
        },
    }
}

async fn wait_for_status(
    app: &Ravyn,
    id: uuid::Uuid,
    wanted: &[JobStatus],
    timeout: Duration,
) -> ravyn::core::models::Job {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let job = app.repository.get_job(id).await.unwrap();
        if wanted.contains(&job.status) {
            return job;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "job did not reach {:?}; current status: {:?}, error: {:?}",
            wanted,
            job.status,
            job.error
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_library_organization_uses_detected_file_content() {
    let temp = tempfile::tempdir().unwrap();
    let mut body = b"%PDF-1.7\n".to_vec();
    body.resize(128 * 1024, 0x20);
    let server = TestServer::start(body.clone(), "\"pdf-v1\"", Duration::ZERO).await;
    let app = Ravyn::bootstrap(organized_test_config(temp.path()))
        .await
        .unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let job = app
        .manager
        .create(create_request(server.url(), None))
        .await
        .unwrap();
    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(20),
    )
    .await;

    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    let organized = temp.path().join("Ravyn/Documents/payload.bin");
    assert_eq!(tokio::fs::read(&organized).await.unwrap(), body);
    assert!(
        !tokio::fs::try_exists(temp.path().join("Ravyn/Downloads/payload.bin"))
            .await
            .unwrap()
    );
    let outputs = app.repository.list_job_outputs(job.id).await.unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].current_path, organized);
    let library = app
        .repository
        .list_library_entries(&ravyn::storage::LibraryListFilter::default(), 0, 10)
        .await
        .unwrap();
    assert_eq!(library.len(), 1);
    assert_eq!(library[0].category, LibraryCategory::Documents);
    assert_eq!(library[0].path, outputs[0].current_path);
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checksum_and_post_transfer_lifecycle_use_an_uncancelled_job_token() {
    let temp = tempfile::tempdir().unwrap();
    let body = vec![0x5a; 4 * 1024 * 1024];
    let expected = hex::encode(Sha256::digest(&body));
    let server = TestServer::start(body.clone(), "\"v1\"", Duration::ZERO).await;
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let job = app
        .manager
        .create(create_request(server.url(), Some(expected)))
        .await
        .unwrap();
    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(20),
    )
    .await;
    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        body
    );
    let library = app
        .repository
        .list_library_entries(&ravyn::storage::LibraryListFilter::default(), 0, 10)
        .await
        .unwrap();
    assert_eq!(library.len(), 1);
    assert_eq!(library[0].job_id, Some(job.id));
    assert_eq!(
        library[0].sha256.as_deref(),
        completed.expected_sha256.as_deref()
    );
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verified_library_cache_reuse_completes_without_a_second_transfer() {
    let temp = tempfile::tempdir().unwrap();
    let body = vec![0x6b; 2 * 1024 * 1024];
    let expected = hex::encode(Sha256::digest(&body));
    let first_server = TestServer::start(body.clone(), "\"v1\"", Duration::ZERO).await;
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let first = app
        .manager
        .create(create_request(first_server.url(), Some(expected.clone())))
        .await
        .unwrap();
    let completed = wait_for_status(
        &app,
        first.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(20),
    )
    .await;
    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );

    let second_server = TestServer::start(body.clone(), "\"v2\"", Duration::ZERO).await;
    let mut request = create_request(second_server.url(), Some(expected));
    request.filename = Some("cached-copy.bin".into());
    request.duplicate_policy = DuplicatePolicy::ReuseExisting;
    let reused = app.manager.create(request).await.unwrap();
    assert_eq!(reused.status, JobStatus::Completed);
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/cached-copy.bin"))
            .await
            .unwrap(),
        body
    );
    let statistics = app.repository.personal_statistics().await.unwrap();
    assert_eq!(statistics.duplicate_avoidance_count, 1);
    assert_eq!(statistics.saved_bandwidth_bytes, body.len() as u64);
    assert_eq!(second_server.range_requests(), 0);
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changed_remote_identity_discards_persisted_segments_before_resume() {
    let temp = tempfile::tempdir().unwrap();
    let original = vec![0x11; 16 * 1024 * 1024];
    let replacement = vec![0x22; original.len()];
    let server = TestServer::start(original, "\"v1\"", Duration::from_millis(12)).await;
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let job = app
        .manager
        .create(create_request(server.url(), None))
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let current = app.repository.get_job(job.id).await.unwrap();
        if current.downloaded_bytes > 0 {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    app.manager.pause(job.id).await.unwrap();
    server.replace(replacement.clone(), "\"v2\"").await;
    app.manager.resume(job.id).await.unwrap();

    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        replacement
    );
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_server_that_lies_about_ranges_falls_back_to_single_stream() {
    let temp = tempfile::tempdir().unwrap();
    let body = (0..3 * 1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let server = TestServer::start(body.clone(), "\"v1\"", Duration::ZERO).await;
    server.reject_non_probe_ranges(true);
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let job = app
        .manager
        .create(create_request(server.url(), None))
        .await
        .unwrap();
    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(20),
    )
    .await;
    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    assert_eq!(completed.transfer_mode, "complete");
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        body
    );
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn metalink_piece_corruption_is_discarded_before_mirror_failover() {
    let temp = tempfile::tempdir().unwrap();
    let body = (0..3 * 1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let corrupt = vec![0x7f; body.len()];
    let bad_server = TestServer::start(corrupt, "\"bad\"", Duration::ZERO).await;
    let good_server = TestServer::start(body.clone(), "\"good\"", Duration::ZERO).await;
    let piece_length = 1024 * 1024_u64;
    let piece_sha256 = body
        .chunks(piece_length as usize)
        .map(|piece| hex::encode(Sha256::digest(piece)))
        .collect();
    let expected = hex::encode(Sha256::digest(&body));
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();
    let mut request = create_request(bad_server.url(), Some(expected));
    request.options.mirrors = vec![good_server.url()];
    request.options.metalink = Some(MetalinkMetadata {
        size: body.len() as u64,
        piece_length: Some(piece_length),
        piece_sha256,
    });

    let job = app.manager.create(request).await.unwrap();
    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(30),
    )
    .await;

    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        body
    );
    assert!(
        !tokio::fs::try_exists(temp.path().join("downloads/payload.bin.ravyn.part"))
            .await
            .unwrap()
    );
    app.manager.shutdown().await;
}

async fn start_redirect_loop_server() -> (SocketAddr, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let Ok((mut stream, _)) = accepted else { break };
                    tokio::spawn(async move {
                        let mut request = Vec::with_capacity(1024);
                        let mut buffer = [0_u8; 1024];
                        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                            match stream.read(&mut buffer).await {
                                Ok(0) | Err(_) => return,
                                Ok(read) => request.extend_from_slice(&buffer[..read]),
                            }
                            if request.len() > 32 * 1024 {
                                return;
                            }
                        }
                        let response = format!(
                            "HTTP/1.1 302 Found\r\nLocation: http://{address}/payload.bin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    });
                }
            }
        }
    });
    (address, shutdown_tx)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_redirect_loop_fails_with_a_bounded_protocol_error() {
    let temp = tempfile::tempdir().unwrap();
    let (address, _shutdown) = start_redirect_loop_server().await;
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let job = app
        .manager
        .create(create_request(
            format!("http://{address}/payload.bin"),
            None,
        ))
        .await
        .unwrap();
    let failed = wait_for_status(&app, job.id, &[JobStatus::Failed], Duration::from_secs(60)).await;
    assert!(
        failed
            .error
            .as_deref()
            .is_some_and(|error| error.contains("redirect limit")),
        "unexpected failure detail: {:?}",
        failed.error
    );
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn validated_mirrors_supply_work_units_concurrently() {
    let temp = tempfile::tempdir().unwrap();
    let payload = vec![0x6d; 40 * 1024 * 1024];
    let primary = TestServer::start(payload.clone(), "\"shared\"", Duration::from_millis(2)).await;
    let mirror = TestServer::start(payload.clone(), "\"shared\"", Duration::from_millis(2)).await;
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let expected = hex::encode(Sha256::digest(&payload));
    let mut request = create_request(primary.url(), Some(expected));
    request.options.mirrors.push(mirror.url());
    let job = app.manager.create(request).await.unwrap();
    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(30),
    )
    .await;

    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    assert!(primary.range_requests() > 0);
    assert!(mirror.range_requests() > 0);
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        payload
    );
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_verified_piece_is_completed_by_one_bounded_hedge() {
    let temp = tempfile::tempdir().unwrap();
    let payload = vec![0x4c; 16 * 1024 * 1024];
    let slow = TestServer::start(payload.clone(), "\"shared\"", Duration::from_millis(400)).await;
    let fast = TestServer::start(payload.clone(), "\"shared\"", Duration::ZERO).await;
    let piece_hash = hex::encode(Sha256::digest(&payload));
    let expected = piece_hash.clone();
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let mut request = create_request(slow.url(), Some(expected));
    request.options.mirrors.push(fast.url());
    request.options.metalink = Some(MetalinkMetadata {
        size: payload.len() as u64,
        piece_length: Some(payload.len() as u64),
        piece_sha256: vec![piece_hash],
    });
    let started = tokio::time::Instant::now();
    let job = app.manager.create(request).await.unwrap();
    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(15),
    )
    .await;

    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    assert!(started.elapsed() < Duration::from_secs(6));
    assert!(slow.range_requests() > 0);
    assert!(fast.range_requests() > 0);
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        payload
    );
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn incremental_single_stream_checksum_failure_cleans_up_before_mirror_retry() {
    let temp = tempfile::tempdir().unwrap();
    let body = vec![0x31; 512 * 1024];
    let corrupt = vec![0x32; body.len()];
    let bad = TestServer::start(corrupt, r#""bad""#, Duration::ZERO).await;
    let good = TestServer::start(body.clone(), r#""good""#, Duration::ZERO).await;
    let expected = hex::encode(Sha256::digest(&body));
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let mut request = create_request(bad.url(), Some(expected));
    request.options.mirrors.push(good.url());
    let job = app.manager.create(request).await.unwrap();
    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(15),
    )
    .await;

    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        body
    );
    assert!(
        !tokio::fs::try_exists(temp.path().join("downloads/payload.bin.ravyn.part"))
            .await
            .unwrap()
    );
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resumed_single_stream_rebuilds_incremental_checksum_state() {
    let temp = tempfile::tempdir().unwrap();
    let body = (0..8 * 1024 * 1024)
        .map(|index| (index % 239) as u8)
        .collect::<Vec<_>>();
    let expected = hex::encode(Sha256::digest(&body));
    let server = TestServer::start(body.clone(), r#""resume""#, Duration::from_millis(20)).await;
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let job = app
        .manager
        .create(create_request(server.url(), Some(expected)))
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let current = app.repository.get_job(job.id).await.unwrap();
        if current.downloaded_bytes > 0 {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    app.manager.pause(job.id).await.unwrap();
    app.manager.resume(job.id).await.unwrap();

    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(15),
    )
    .await;
    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        body
    );
    app.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retrying_a_failed_job_resets_progress_and_creates_no_duplicates() {
    let temp = tempfile::tempdir().unwrap();
    let body = vec![0x7c; 4 * 1024 * 1024];
    // The per-chunk delay keeps each attempt alive across the 250ms durable
    // progress flush so the failure leaves persisted partial progress behind,
    // mirroring a real dropped transfer.
    let server = TestServer::start(body.clone(), "\"retry-v1\"", Duration::from_millis(30)).await;
    server.fail_after_bytes(768 * 1024);
    let app = Ravyn::bootstrap(test_config(temp.path())).await.unwrap();
    app.manager.clone().start_workers().await.unwrap();

    let mut request = create_request(server.url(), None);
    request.options.overwrite = false;
    let job = app.manager.create(request).await.unwrap();
    let failed = wait_for_status(&app, job.id, &[JobStatus::Failed], Duration::from_secs(60)).await;
    assert!(
        failed.downloaded_bytes > 0,
        "test setup: the failure should leave partial progress behind"
    );

    server.fail_after_bytes(0);
    app.manager.retry(job.id).await.unwrap();
    let retried = app.repository.get_job(job.id).await.unwrap();
    assert_eq!(retried.status, JobStatus::Queued);
    assert_eq!(
        retried.downloaded_bytes, 0,
        "retry must clear stale progress so the UI does not show a frozen bar"
    );

    let completed = wait_for_status(
        &app,
        job.id,
        &[JobStatus::Completed, JobStatus::Failed],
        Duration::from_secs(60),
    )
    .await;
    assert_eq!(
        completed.status,
        JobStatus::Completed,
        "{:?}",
        completed.error
    );
    let jobs = app.repository.list_jobs().await.unwrap();
    assert_eq!(jobs.len(), 1, "retry must reuse the job, not duplicate it");
    let outputs = app.repository.list_job_outputs(job.id).await.unwrap();
    assert_eq!(
        outputs.len(),
        1,
        "retry must not register duplicate outputs"
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("downloads/payload.bin"))
            .await
            .unwrap(),
        body
    );
    assert!(
        !tokio::fs::try_exists(temp.path().join("downloads/payload (1).bin"))
            .await
            .unwrap(),
        "retry must not auto-rename against its own previous attempt"
    );
    app.manager.shutdown().await;
}
