//! Supervision of Ravyn-owned rqbit daemon processes.

use std::{path::{Path, PathBuf}, sync::Arc, time::Duration};

use serde::Serialize;
use tokio::{process::{Child, Command}, sync::Mutex, time::Instant};

use crate::error::{RavynError, Result};

/// Observable lifecycle state for a Ravyn-managed rqbit daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RqbitProcessState {
    Stopped,
    Starting,
    Ready,
    Degraded,
    Restarting,
    Failed,
    Stopping,
}

struct Inner {
    state: RqbitProcessState,
    child: Option<Child>,
    api_url: Option<String>,
}

/// Owns exactly one loopback-bound rqbit server for the current Ravyn process.
#[derive(Clone)]
pub struct RqbitProcessManager {
    data_dir: PathBuf,
    inner: Arc<Mutex<Inner>>,
}

impl RqbitProcessManager {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into().join("rqbit"),
            inner: Arc::new(Mutex::new(Inner {
                state: RqbitProcessState::Stopped,
                child: None,
                api_url: None,
            })),
        }
    }

    pub async fn state(&self) -> RqbitProcessState {
        self.inner.lock().await.state
    }

    pub async fn api_url(&self) -> Option<String> {
        self.inner.lock().await.api_url.clone()
    }

    /// Start rqbit on a newly selected loopback port and wait for the API that
    /// Ravyn uses. The child is configured to die when this owner is dropped.
    pub async fn start(&self, executable: &Path, config: &mut crate::config::Config) -> Result<()> {
        self.stop().await?;
        tokio::fs::create_dir_all(&self.data_dir).await?;
        {
            let mut inner = self.inner.lock().await;
            inner.state = RqbitProcessState::Starting;
        }

        let mut last_error = None;
        for attempt in 0..3 {
            if attempt > 0 {
                self.inner.lock().await.state = RqbitProcessState::Restarting;
            }
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            drop(listener);
            let api_url = format!("http://{address}");
            let mut command = Command::new(executable);
            command
                .arg("server")
                .arg("start")
                .arg(&self.data_dir)
                .env("RQBIT_HTTP_API_LISTEN_ADDR", address.to_string())
                .kill_on_drop(true)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            if let (Some(username), Some(password)) = (&config.rqbit_username, &config.rqbit_password) {
                command.env("RQBIT_HTTP_BASIC_AUTH_USERPASS", format!("{username}:{password}"));
            }
            let child = command.spawn().map_err(|error| {
                RavynError::Unavailable(format!("failed to start rqbit: {error}"))
            })?;
            {
                let mut inner = self.inner.lock().await;
                inner.child = Some(child);
                inner.api_url = Some(api_url.clone());
            }
            match self.wait_for_ready(config, &api_url).await {
                Ok(()) => {
                    config.rqbit_api = api_url;
                    self.inner.lock().await.state = RqbitProcessState::Ready;
                    return Ok(());
                }
                Err(error) => {
                    last_error = Some(error);
                    self.stop_child().await;
                }
            }
        }
        self.inner.lock().await.state = RqbitProcessState::Failed;
        Err(last_error.unwrap_or_else(|| RavynError::Unavailable("rqbit did not become ready".into())))
    }

    pub async fn stop(&self) -> Result<()> {
        self.inner.lock().await.state = RqbitProcessState::Stopping;
        self.stop_child().await;
        let mut inner = self.inner.lock().await;
        inner.state = RqbitProcessState::Stopped;
        inner.api_url = None;
        Ok(())
    }

    async fn stop_child(&self) {
        let child = self.inner.lock().await.child.take();
        if let Some(mut child) = child {
            let _ = child.kill().await;
        }
    }

    async fn wait_for_ready(&self, config: &crate::config::Config, api_url: &str) -> Result<()> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build()?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            {
                let mut inner = self.inner.lock().await;
                if let Some(child) = inner.child.as_mut() {
                    if let Some(status) = child.try_wait()? {
                        return Err(RavynError::Unavailable(format!(
                            "rqbit exited before API readiness with {status}"
                        )));
                    }
                }
            }
            let mut request = client.get(format!("{api_url}/"));
            if let (Some(username), Some(password)) = (&config.rqbit_username, &config.rqbit_password) {
                request = request.basic_auth(username, Some(password));
            }
            if let Ok(response) = request.send().await {
                if response.status().is_success() {
                    let body = response.bytes().await?;
                    if body.len() <= 4 * 1024 * 1024 {
                        let value: serde_json::Value = serde_json::from_slice(&body)?;
                        if value.get("server").and_then(serde_json::Value::as_str) == Some("rqbit") {
                            return Ok(());
                        }
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(RavynError::Unavailable("timed out waiting for rqbit HTTP readiness".into()));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}
