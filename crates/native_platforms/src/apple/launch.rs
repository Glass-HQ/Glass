use crate::Device;
use anyhow::Result;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use smol::io::{AsyncBufReadExt, BufReader};
use smol::process::Command;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum LaunchOutput {
    Line(String),
    Progress(String),
    Completed { pid: Option<u32> },
    Failed(LaunchError),
}

#[derive(Debug, Clone)]
pub enum LaunchErrorKind {
    DeviceBusy,
    AppNotFound,
    DeviceLocked,
    ProcessSpawnFailed,
    Timeout,
    Unknown(String),
}

impl LaunchErrorKind {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::DeviceBusy | Self::AppNotFound)
    }
}

#[derive(Debug, Clone)]
pub struct LaunchError {
    pub kind: LaunchErrorKind,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct LaunchConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub retry_base_delay: Duration,
    pub post_install_delay: Duration,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_base_delay: Duration::from_millis(500),
            post_install_delay: Duration::from_millis(500),
        }
    }
}

pub struct LaunchProcess {
    pub output_receiver: mpsc::UnboundedReceiver<LaunchOutput>,
}

pub async fn launch(
    bundle_id: &str,
    device: &Device,
    config: &LaunchConfig,
) -> Result<LaunchProcess> {
    let (tx, rx) = mpsc::unbounded();

    let bundle_id = bundle_id.to_string();
    let device_id = device.id.clone();
    let config = config.clone();

    smol::spawn(async move {
        // Post-install delay to prevent EBusy
        if !config.post_install_delay.is_zero() {
            smol::Timer::after(config.post_install_delay).await;
        }

        let mut last_error: Option<LaunchError> = None;

        for attempt in 0..=config.max_retries {
            if attempt > 0 {
                let delay = config.retry_base_delay * 2u32.pow(attempt - 1);
                let _ = tx
                    .clone()
                    .send(LaunchOutput::Progress(format!(
                        "Retrying launch (attempt {}/{})",
                        attempt + 1,
                        config.max_retries + 1
                    )))
                    .await;
                smol::Timer::after(delay).await;
            }

            match launch_once(&bundle_id, &device_id, &config.timeout, tx.clone()).await {
                Ok(pid) => {
                    let _ = tx.clone().send(LaunchOutput::Completed { pid }).await;
                    return;
                }
                Err(err) => {
                    if !err.kind.is_retryable() || attempt == config.max_retries {
                        let _ = tx.clone().send(LaunchOutput::Failed(err)).await;
                        return;
                    }
                    last_error = Some(err);
                }
            }
        }

        // Should not reach here, but handle gracefully
        if let Some(err) = last_error {
            let _ = tx.clone().send(LaunchOutput::Failed(err)).await;
        }
    })
    .detach();

    Ok(LaunchProcess {
        output_receiver: rx,
    })
}

async fn launch_once(
    bundle_id: &str,
    device_id: &str,
    timeout: &Duration,
    tx: mpsc::UnboundedSender<LaunchOutput>,
) -> std::result::Result<Option<u32>, LaunchError> {
    let json_output_path = json_output_path();

    let mut cmd = Command::new("xcrun");
    cmd.args([
        "devicectl",
        "device",
        "process",
        "launch",
        "--device",
        device_id,
        bundle_id,
        "--terminate-existing",
        "--activate",
        "--json-output",
        json_output_path.to_str().unwrap_or("/tmp/glass_devicectl.json"),
    ]);
    cmd.stdout(smol::process::Stdio::piped());
    cmd.stderr(smol::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| LaunchError {
        kind: LaunchErrorKind::ProcessSpawnFailed,
        message: format!("Failed to spawn devicectl: {}", e),
    })?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Collect stderr in background
    let stderr_handle = smol::spawn(async move {
        let mut collected = String::new();
        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Some(Ok(line)) = lines.next().await {
                collected.push_str(&line);
                collected.push('\n');
            }
        }
        collected
    });

    // Stream stdout lines to the UI
    let stdout_handle = {
        let mut tx = tx.clone();
        smol::spawn(async move {
            if let Some(stdout) = stdout {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Some(Ok(line)) = lines.next().await {
                    let _ = tx.send(LaunchOutput::Line(line)).await;
                }
            }
        })
    };

    // Wait for stdout/stderr streams to complete, with timeout
    let streams_future = async {
        stdout_handle.await;
        let stderr_output = stderr_handle.await;
        stderr_output
    };

    let timeout_future = smol::Timer::after(*timeout);
    futures::pin_mut!(streams_future);
    futures::pin_mut!(timeout_future);

    match futures::future::select(streams_future, timeout_future).await {
        futures::future::Either::Left((stderr_output, _)) => {
            let status = child.status().await;
            let success = status.map(|s| s.success()).unwrap_or(false);
            if !success {
                let kind = classify_launch_error(&stderr_output);
                let _ = std::fs::remove_file(&json_output_path);
                return Err(LaunchError {
                    kind,
                    message: stderr_output.trim().to_string(),
                });
            }

            let pid = extract_pid_from_json(&json_output_path);
            let _ = std::fs::remove_file(&json_output_path);
            Ok(pid)
        }
        futures::future::Either::Right((_, _)) => {
            let _ = child.kill();
            let _ = std::fs::remove_file(&json_output_path);
            Err(LaunchError {
                kind: LaunchErrorKind::Timeout,
                message: format!("Launch timed out after {:?}", timeout),
            })
        }
    }
}

fn json_output_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "glass_devicectl_{}.json",
        std::process::id()
    ));
    path
}

fn extract_pid_from_json(path: &PathBuf) -> Option<u32> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("result")?
        .get("process")?
        .get("processIdentifier")?
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
}

fn classify_launch_error(stderr: &str) -> LaunchErrorKind {
    let lower = stderr.to_lowercase();
    if lower.contains("ebusy") || lower.contains("device is busy") {
        LaunchErrorKind::DeviceBusy
    } else if lower.contains("not found") || lower.contains("no such") {
        LaunchErrorKind::AppNotFound
    } else if lower.contains("locked") || lower.contains("passcode") {
        LaunchErrorKind::DeviceLocked
    } else {
        LaunchErrorKind::Unknown(stderr.to_string())
    }
}
