use crate::Device;
use anyhow::Result;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use smol::io::{AsyncBufReadExt, BufReader};
use smol::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum InstallPhase {
    CreatingStagingDirectory,
    ExtractingPackage,
    PreflightingApplication,
    VerifyingApplication,
    StagingApplication,
    RegisteringApplication,
    GeneratingApplicationMap,
    Complete,
}

impl InstallPhase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CreatingStagingDirectory => "Creating staging directory",
            Self::ExtractingPackage => "Extracting package",
            Self::PreflightingApplication => "Preflighting application",
            Self::VerifyingApplication => "Verifying application",
            Self::StagingApplication => "Staging application",
            Self::RegisteringApplication => "Registering application",
            Self::GeneratingApplicationMap => "Generating application map",
            Self::Complete => "Installation complete",
        }
    }

    pub fn progress_range(&self) -> (f32, f32) {
        match self {
            Self::CreatingStagingDirectory => (0.0, 5.0),
            Self::ExtractingPackage => (5.0, 25.0),
            Self::PreflightingApplication => (25.0, 35.0),
            Self::VerifyingApplication => (35.0, 50.0),
            Self::StagingApplication => (50.0, 75.0),
            Self::RegisteringApplication => (75.0, 90.0),
            Self::GeneratingApplicationMap => (90.0, 99.0),
            Self::Complete => (100.0, 100.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub phase: InstallPhase,
    pub percent: f32,
    pub raw_line: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InstallErrorKind {
    DeviceLocked,
    UsbConnectionFailed,
    DeviceBusy,
    AppNotFound,
    CodeSigningFailed,
    ProvisioningFailed,
    DiskFull,
    Timeout,
    ProcessSpawnFailed,
    Unknown(String),
}

impl InstallErrorKind {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::DeviceBusy | Self::UsbConnectionFailed | Self::Timeout
        )
    }

    pub fn user_suggestion(&self) -> &'static str {
        match self {
            Self::DeviceLocked => "Unlock your device and try again",
            Self::UsbConnectionFailed => "Check the USB cable and reconnect your device",
            Self::DeviceBusy => "The device is busy, will retry automatically",
            Self::AppNotFound => "The app bundle was not found at the expected path",
            Self::CodeSigningFailed => "Check your code signing settings in Xcode",
            Self::ProvisioningFailed => "Check your provisioning profile in Xcode",
            Self::DiskFull => "Free up space on your device",
            Self::Timeout => "Installation timed out, will retry automatically",
            Self::ProcessSpawnFailed => "Failed to start devicectl - check Xcode installation",
            Self::Unknown(_) => "An unexpected error occurred during installation",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallError {
    pub kind: InstallErrorKind,
    pub message: String,
    pub raw_output: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InstallOutput {
    Line(String),
    Progress(InstallProgress),
    Error(InstallError),
    Retrying {
        attempt: u32,
        max_retries: u32,
        reason: String,
    },
    Completed,
    Failed(InstallError),
}

#[derive(Debug, Clone)]
pub struct InstallConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub retry_base_delay: Duration,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_retries: 3,
            retry_base_delay: Duration::from_millis(500),
        }
    }
}

pub struct InstallProcess {
    pub output_receiver: mpsc::UnboundedReceiver<InstallOutput>,
}

pub async fn install(
    app_path: &str,
    device: &Device,
    config: &InstallConfig,
) -> Result<InstallProcess> {
    let (tx, rx) = mpsc::unbounded();

    let app_path = app_path.to_string();
    let device_id = device.id.clone();
    let config = config.clone();

    smol::spawn(async move {
        let mut last_error = None;

        for attempt in 0..=config.max_retries {
            if attempt > 0 {
                let delay = config.retry_base_delay * 2u32.pow(attempt - 1);
                let reason = last_error
                    .as_ref()
                    .map(|e: &InstallError| e.message.clone())
                    .unwrap_or_else(|| "Unknown error".to_string());

                let _ = tx
                    .clone()
                    .send(InstallOutput::Retrying {
                        attempt,
                        max_retries: config.max_retries,
                        reason,
                    })
                    .await;

                smol::Timer::after(delay).await;
            }

            match install_once(&app_path, &device_id, &config.timeout, tx.clone()).await {
                Ok(()) => {
                    let _ = tx.clone().send(InstallOutput::Completed).await;
                    return;
                }
                Err(err) => {
                    if !err.kind.is_retryable() || attempt == config.max_retries {
                        let _ = tx.clone().send(InstallOutput::Failed(err)).await;
                        return;
                    }
                    last_error = Some(err);
                }
            }
        }
    })
    .detach();

    Ok(InstallProcess {
        output_receiver: rx,
    })
}

async fn install_once(
    app_path: &str,
    device_id: &str,
    timeout: &Duration,
    mut tx: mpsc::UnboundedSender<InstallOutput>,
) -> std::result::Result<(), InstallError> {
    let mut cmd = Command::new("xcrun");
    cmd.args([
        "devicectl",
        "device",
        "install",
        "app",
        "--device",
        device_id,
        app_path,
    ]);
    cmd.stdout(smol::process::Stdio::piped());
    cmd.stderr(smol::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| InstallError {
        kind: InstallErrorKind::ProcessSpawnFailed,
        message: format!("Failed to spawn devicectl: {}", e),
        raw_output: None,
    })?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Spawn stderr collector
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

    // Stream stdout with timeout
    let stream_future = async {
        let mut current_phase = InstallPhase::CreatingStagingDirectory;

        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Some(Ok(line)) = lines.next().await {
                let _ = tx.send(InstallOutput::Line(line.clone())).await;

                if let Some(phase) = detect_phase(&line) {
                    current_phase = phase.clone();
                    let (start, _) = phase.progress_range();
                    let _ = tx
                        .send(InstallOutput::Progress(InstallProgress {
                            phase,
                            percent: start,
                            raw_line: Some(line.clone()),
                        }))
                        .await;
                }

                if let Some(pct) = parse_progress_line(&line) {
                    let _ = tx
                        .send(InstallOutput::Progress(InstallProgress {
                            phase: current_phase.clone(),
                            percent: pct,
                            raw_line: Some(line.clone()),
                        }))
                        .await;
                }
            }
        }
    };

    let timeout_future = smol::Timer::after(*timeout);
    futures::pin_mut!(stream_future);
    futures::pin_mut!(timeout_future);

    match futures::future::select(stream_future, timeout_future).await {
        futures::future::Either::Left(((), _)) => {
            // Stream completed normally, check exit status
        }
        futures::future::Either::Right((_, _)) => {
            // Timeout - kill the process
            let _ = child.kill();
            return Err(InstallError {
                kind: InstallErrorKind::Timeout,
                message: format!("Installation timed out after {:?}", timeout),
                raw_output: None,
            });
        }
    }

    let stderr_output = stderr_handle.await;
    let status = child.status().await.map_err(|e| InstallError {
        kind: InstallErrorKind::Unknown(e.to_string()),
        message: format!("Failed to get process status: {}", e),
        raw_output: Some(stderr_output.clone()),
    })?;

    if !status.success() {
        let kind = classify_error(&stderr_output);
        return Err(InstallError {
            kind,
            message: stderr_output.trim().to_string(),
            raw_output: Some(stderr_output),
        });
    }

    Ok(())
}

fn parse_progress_line(line: &str) -> Option<f32> {
    for segment in line.trim().split_whitespace() {
        if let Some(pct_str) = segment.trim_end_matches("...").strip_suffix('%') {
            return pct_str.parse::<f32>().ok();
        }
    }
    None
}

fn detect_phase(line: &str) -> Option<InstallPhase> {
    let lower = line.to_lowercase();
    if lower.contains("creating staging") {
        Some(InstallPhase::CreatingStagingDirectory)
    } else if lower.contains("extracting") {
        Some(InstallPhase::ExtractingPackage)
    } else if lower.contains("preflight") {
        Some(InstallPhase::PreflightingApplication)
    } else if lower.contains("verifying") {
        Some(InstallPhase::VerifyingApplication)
    } else if lower.contains("staging") {
        Some(InstallPhase::StagingApplication)
    } else if lower.contains("registering") {
        Some(InstallPhase::RegisteringApplication)
    } else if lower.contains("generating") && lower.contains("map") {
        Some(InstallPhase::GeneratingApplicationMap)
    } else if lower.contains("complete") || lower.contains("installed") {
        Some(InstallPhase::Complete)
    } else {
        None
    }
}

fn classify_error(stderr: &str) -> InstallErrorKind {
    let lower = stderr.to_lowercase();
    if lower.contains("locked") || lower.contains("passcode") {
        InstallErrorKind::DeviceLocked
    } else if lower.contains("ebusy") || lower.contains("device is busy") {
        InstallErrorKind::DeviceBusy
    } else if lower.contains("usb") || lower.contains("connection") {
        InstallErrorKind::UsbConnectionFailed
    } else if lower.contains("not found") || lower.contains("no such file") {
        InstallErrorKind::AppNotFound
    } else if lower.contains("code sign") || lower.contains("signature") || lower.contains("codesign") {
        InstallErrorKind::CodeSigningFailed
    } else if lower.contains("provisioning") || lower.contains("provision") {
        InstallErrorKind::ProvisioningFailed
    } else if lower.contains("no space") || lower.contains("disk full") || lower.contains("storage") {
        InstallErrorKind::DiskFull
    } else {
        InstallErrorKind::Unknown(stderr.to_string())
    }
}
