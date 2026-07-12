use std::{path::PathBuf, process::ExitStatus, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
};
use tokio_util::sync::CancellationToken;

use crate::error::{RavynError, Result};

pub fn redact_sensitive_output(input: &str) -> String {
    input
        .lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if [
                "authorization",
                "proxy-authorization",
                "cookie",
                "password",
                "api_key",
                "api-key",
            ]
            .iter()
            .any(|marker| lower.contains(marker))
            {
                "[redacted]".to_owned()
            } else {
                line.split_whitespace()
                    .map(|token| {
                        url::Url::parse(token)
                            .ok()
                            .and_then(|mut url| {
                                if url.username().is_empty() && url.password().is_none() {
                                    return None;
                                }
                                let _ = url.set_username("redacted");
                                let _ = url.set_password(Some("redacted"));
                                Some(url.to_string())
                            })
                            .unwrap_or_else(|| token.to_owned())
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone)]
pub struct ProcessLimits {
    pub wall_time: Duration,
    pub cpu_time: Duration,
    pub memory_bytes: u64,
    pub output_file_bytes: Option<u64>,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            wall_time: Duration::from_secs(6 * 60 * 60),
            cpu_time: Duration::from_secs(2 * 60 * 60),
            memory_bytes: 2 * 1024 * 1024 * 1024,
            output_file_bytes: None,
            stdout_bytes: 1024 * 1024,
            stderr_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

pub async fn run(
    command: &mut Command,
    limits: &ProcessLimits,
    output_path: Option<PathBuf>,
    cancellation: CancellationToken,
) -> Result<ProcessOutput> {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    configure(command, limits);
    let mut child = command.spawn()?;
    let tree = ProcessTree::attach(&child, limits)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RavynError::Internal("child stdout pipe is missing".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| RavynError::Internal("child stderr pipe is missing".into()))?;
    let stdout_task = tokio::spawn(read_bounded(stdout, limits.stdout_bytes));
    let stderr_task = tokio::spawn(read_bounded(stderr, limits.stderr_bytes));
    let deadline = tokio::time::sleep(limits.wall_time);
    tokio::pin!(deadline);
    let mut output_check = tokio::time::interval(Duration::from_millis(250));
    output_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let status = loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                tree.terminate(&mut child).await;
                return Err(RavynError::Cancelled);
            }
            _ = &mut deadline => {
                tree.terminate(&mut child).await;
                return Err(RavynError::Process("external process exceeded its wall-clock limit".into()));
            }
            _ = output_check.tick(), if output_path.is_some() && limits.output_file_bytes.is_some() => {
                let exceeded = match (&output_path, limits.output_file_bytes) {
                    (Some(path), Some(limit)) => tokio::fs::metadata(path)
                        .await
                        .ok()
                        .is_some_and(|metadata| metadata.len() > limit),
                    _ => false,
                };
                if exceeded {
                    tree.terminate(&mut child).await;
                    return Err(RavynError::Process("external process exceeded its output file-size limit".into()));
                }
            }
            status = child.wait() => break status?,
        }
    };
    drop(tree);
    let (stdout, stdout_truncated) = stdout_task
        .await
        .map_err(|error| RavynError::Internal(error.to_string()))??;
    let (stderr, stderr_truncated) = stderr_task
        .await
        .map_err(|error| RavynError::Internal(error.to_string()))??;
    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
    })
}

pub fn configure(command: &mut Command, limits: &ProcessLimits) {
    configure_process_group(command, limits);
}

pub struct ProcessGuard(ProcessTree);

impl ProcessGuard {
    pub fn attach(child: &Child, limits: &ProcessLimits) -> Result<Self> {
        ProcessTree::attach(child, limits).map(Self)
    }

    pub async fn terminate(&self, child: &mut Child) {
        self.0.terminate(child).await;
    }
}

async fn read_bounded(mut reader: impl AsyncRead + Unpin, limit: usize) -> Result<(Vec<u8>, bool)> {
    let mut captured = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..read.min(remaining)]);
        truncated |= read > remaining;
    }
    Ok((captured, truncated))
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command, limits: &ProcessLimits) {
    use std::os::unix::process::CommandExt;
    let command = command.as_std_mut();
    command.process_group(0);
    let cpu_seconds = limits.cpu_time.as_secs().max(1);
    let memory_bytes = limits.memory_bytes;
    // SAFETY: the closure only calls async-signal-safe setrlimit between fork
    // and exec and captures plain integer values.
    unsafe {
        command.pre_exec(move || {
            let cpu = libc::rlimit {
                rlim_cur: cpu_seconds,
                rlim_max: cpu_seconds,
            };
            let memory = libc::rlimit {
                rlim_cur: memory_bytes,
                rlim_max: memory_bytes,
            };
            if libc::setrlimit(libc::RLIMIT_CPU, &cpu) != 0
                || libc::setrlimit(libc::RLIMIT_AS, &memory) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_process_group(_: &mut Command, _: &ProcessLimits) {}

#[cfg(windows)]
struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);

// SAFETY: the value is an owned kernel handle. Job Object operations used by
// this type are thread-safe, and ownership is released exactly once in Drop.
#[cfg(windows)]
unsafe impl Send for ProcessTree {}
// SAFETY: see the Send implementation; shared operations do not mutate the
// Rust representation and the kernel synchronizes access to the object.
#[cfg(windows)]
unsafe impl Sync for ProcessTree {}

#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &Child, limits: &ProcessLimits) -> Result<Self> {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
        };
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_PROCESS_TIME;
        info.BasicLimitInformation.PerProcessUserTimeLimit =
            i64::try_from(limits.cpu_time.as_nanos() / 100).unwrap_or(i64::MAX);
        info.ProcessMemoryLimit = usize::try_from(limits.memory_bytes).unwrap_or(usize::MAX);
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                u32::try_from(std::mem::size_of_val(&info)).unwrap_or(u32::MAX),
            )
        };
        let assigned = child.raw_handle().is_some_and(|process| unsafe {
            AssignProcessToJobObject(handle, process.cast()) != 0
        });
        if configured == 0 || !assigned {
            unsafe { CloseHandle(handle) };
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(handle))
    }

    async fn terminate(&self, child: &mut Child) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
        let _ = child.wait().await;
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    fn shell(script: &str) -> Command {
        #[cfg(windows)]
        {
            let mut command = Command::new("powershell.exe");
            command.args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ]);
            command
        }
        #[cfg(unix)]
        {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            command
        }
    }

    #[test]
    fn redacts_headers_and_url_credentials_from_child_output() {
        let output = redact_sensitive_output(
            "Authorization: Bearer secret\nfetch https://user:pass@example.test/file\nnormal text",
        );
        assert!(!output.contains("secret"));
        assert!(!output.contains("user:pass"));
        assert!(output.contains("redacted:redacted"));
        assert!(output.contains("normal text"));
    }

    #[tokio::test]
    async fn bounds_captured_output_while_draining_the_pipe() {
        #[cfg(windows)]
        let script = "[Console]::Out.Write(('x' * 10000))";
        #[cfg(unix)]
        let script = "head -c 10000 /dev/zero | tr '\\0' x";
        let limits = ProcessLimits {
            stdout_bytes: 128,
            ..ProcessLimits::default()
        };
        let output = run(&mut shell(script), &limits, None, CancellationToken::new())
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 128);
        assert!(output.stdout_truncated);
    }

    #[tokio::test]
    async fn terminates_a_process_tree_at_the_wall_clock_limit() {
        #[cfg(windows)]
        let script = "Start-Sleep -Seconds 30";
        #[cfg(unix)]
        let script = "sleep 30";
        let limits = ProcessLimits {
            wall_time: Duration::from_millis(500),
            ..ProcessLimits::default()
        };
        let error = run(&mut shell(script), &limits, None, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("wall-clock limit"));
    }

    #[tokio::test]
    async fn cancellation_terminates_the_process_tree() {
        #[cfg(windows)]
        let script = "Start-Sleep -Seconds 30";
        #[cfg(unix)]
        let script = "sleep 30";
        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            trigger.cancel();
        });
        let error = run(
            &mut shell(script),
            &ProcessLimits::default(),
            None,
            cancellation,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, RavynError::Cancelled));
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(unix)]
struct ProcessTree(i32);

#[cfg(unix)]
impl ProcessTree {
    fn attach(child: &Child, _: &ProcessLimits) -> Result<Self> {
        let pid = child
            .id()
            .ok_or_else(|| RavynError::Process("child process has no id".into()))?;
        Ok(Self(i32::try_from(pid).map_err(|_| {
            RavynError::Process("child process id exceeds platform range".into())
        })?))
    }

    async fn terminate(&self, child: &mut Child) {
        unsafe { libc::kill(-self.0, libc::SIGKILL) };
        let _ = child.wait().await;
    }
}

#[cfg(not(any(unix, windows)))]
struct ProcessTree;

#[cfg(not(any(unix, windows)))]
impl ProcessTree {
    fn attach(_: &Child, _: &ProcessLimits) -> Result<Self> {
        Ok(Self)
    }
    async fn terminate(&self, child: &mut Child) {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}
