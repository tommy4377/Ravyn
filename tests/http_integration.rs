use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use clap::Parser;
use ravyn::{
    Ravyn,
    config::Config,
    core::models::{CreateJob, DownloadOptions, DuplicatePolicy, JobKind, JobStatus},
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
        for chunk in body[start..=end].chunks(64 * 1024) {
            stream.write_all(chunk).await?;
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
