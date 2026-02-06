use crate::DeviceType;
use anyhow::Result;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::build::{self, BuildOptions, BuildOutput};
use super::install::{self, InstallConfig, InstallOutput};
use super::launch::{self, LaunchConfig, LaunchOutput};
use super::xcode::XcodeProject;

#[derive(Debug, Clone)]
pub enum RunPhase {
    Building,
    FindingApp,
    Installing,
    Launching,
}

impl RunPhase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Building => "Building",
            Self::FindingApp => "Finding app",
            Self::Installing => "Installing",
            Self::Launching => "Launching",
        }
    }
}

#[derive(Debug, Clone)]
pub enum RunOutput {
    PhaseChanged(RunPhase),
    Build(BuildOutput),
    Install(InstallOutput),
    Launch(LaunchOutput),
    AppLaunched { bundle_id: String, pid: Option<u32> },
    Completed,
    Failed { phase: RunPhase, message: String },
}

pub struct RunProcess {
    pub output_receiver: mpsc::UnboundedReceiver<RunOutput>,
    pub active_pid: Arc<AtomicU32>,
}

pub async fn run(
    project: &XcodeProject,
    options: &BuildOptions,
) -> Result<RunProcess> {
    let (tx, rx) = mpsc::unbounded();
    let active_pid = Arc::new(AtomicU32::new(0));

    let project = project.clone();
    let options = options.clone();

    let pid_handle = active_pid.clone();
    smol::spawn(async move {
        run_pipeline(tx, project, options, pid_handle).await;
    })
    .detach();

    Ok(RunProcess {
        output_receiver: rx,
        active_pid,
    })
}

async fn run_pipeline(
    mut tx: mpsc::UnboundedSender<RunOutput>,
    project: XcodeProject,
    options: BuildOptions,
    active_pid: Arc<AtomicU32>,
) {
    // Phase 1: Build
    let _ = tx.send(RunOutput::PhaseChanged(RunPhase::Building)).await;

    let build_process = match build::build(&project, &options).await {
        Ok(p) => p,
        Err(e) => {
            let _ = tx
                .send(RunOutput::Failed {
                    phase: RunPhase::Building,
                    message: format!("Failed to start build: {}", e),
                })
                .await;
            return;
        }
    };

    // Propagate the build child PID so the panel can kill it
    let build_pid = build_process.active_pid.load(Ordering::Acquire);
    if build_pid != 0 {
        active_pid.store(build_pid, Ordering::Release);
    }

    let mut build_receiver = build_process.output_receiver;
    let mut build_success = false;

    while let Some(output) = build_receiver.next().await {
        if let BuildOutput::Completed(ref result) = output {
            build_success = result.success;
        }
        let _ = tx.send(RunOutput::Build(output)).await;
    }

    active_pid.store(0, Ordering::Release);

    if !build_success {
        let _ = tx
            .send(RunOutput::Failed {
                phase: RunPhase::Building,
                message: "Build failed".to_string(),
            })
            .await;
        return;
    }

    // Phase 2: Find .app
    let _ = tx
        .send(RunOutput::PhaseChanged(RunPhase::FindingApp))
        .await;

    let app_path = match build::find_built_app(&project, &options) {
        Some(p) => p,
        None => {
            let _ = tx
                .send(RunOutput::Failed {
                    phase: RunPhase::FindingApp,
                    message: "Could not find built .app bundle in DerivedData".to_string(),
                })
                .await;
            return;
        }
    };

    let _ = tx
        .send(RunOutput::Build(BuildOutput::Line(format!(
            "Found app at: {}",
            app_path
        ))))
        .await;

    // Phase 3: Install (physical device only)
    if let Some(device) = &options.destination {
        if device.device_type == DeviceType::PhysicalDevice {
            let _ = tx
                .send(RunOutput::PhaseChanged(RunPhase::Installing))
                .await;

            let install_config = InstallConfig::default();
            let install_process =
                match install::install(&app_path, device, &install_config).await {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = tx
                            .send(RunOutput::Failed {
                                phase: RunPhase::Installing,
                                message: format!("Failed to start install: {}", e),
                            })
                            .await;
                        return;
                    }
                };

            let mut install_receiver = install_process.output_receiver;
            let mut install_success = false;

            while let Some(output) = install_receiver.next().await {
                match &output {
                    InstallOutput::Completed => {
                        install_success = true;
                    }
                    InstallOutput::Failed(_) => {
                        install_success = false;
                    }
                    _ => {}
                }
                let _ = tx.send(RunOutput::Install(output)).await;
            }

            if !install_success {
                let _ = tx
                    .send(RunOutput::Failed {
                        phase: RunPhase::Installing,
                        message: "Installation failed".to_string(),
                    })
                    .await;
                return;
            }
        }
    }

    // Phase 4: Launch
    if let Some(device) = &options.destination {
        let _ = tx
            .send(RunOutput::PhaseChanged(RunPhase::Launching))
            .await;

        let bundle_id = match build::get_bundle_identifier(&app_path) {
            Some(id) => id,
            None => {
                let _ = tx
                    .send(RunOutput::Failed {
                        phase: RunPhase::Launching,
                        message: "Could not read bundle identifier from app".to_string(),
                    })
                    .await;
                return;
            }
        };

        let launch_config = LaunchConfig {
            // Post-install delay already handled by launch module
            post_install_delay: if device.device_type == DeviceType::PhysicalDevice {
                std::time::Duration::from_millis(500)
            } else {
                std::time::Duration::ZERO
            },
            ..LaunchConfig::default()
        };

        let launch_process =
            match launch::launch(&bundle_id, device, &launch_config).await {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx
                        .send(RunOutput::Failed {
                            phase: RunPhase::Launching,
                            message: format!("Failed to start launch: {}", e),
                        })
                        .await;
                    return;
                }
            };

        let mut launch_receiver = launch_process.output_receiver;
        let mut launch_success = false;
        let mut launched_pid = None;

        while let Some(output) = launch_receiver.next().await {
            match &output {
                LaunchOutput::Completed { pid } => {
                    launch_success = true;
                    launched_pid = *pid;
                }
                LaunchOutput::Failed(_) => {
                    launch_success = false;
                }
                _ => {}
            }
            let _ = tx.send(RunOutput::Launch(output)).await;
        }

        if launch_success {
            let _ = tx
                .send(RunOutput::AppLaunched {
                    bundle_id: bundle_id.clone(),
                    pid: launched_pid,
                })
                .await;
        }

        if !launch_success {
            let _ = tx
                .send(RunOutput::Failed {
                    phase: RunPhase::Launching,
                    message: "Launch failed".to_string(),
                })
                .await;
            return;
        }
    }

    let _ = tx.send(RunOutput::Completed).await;
}
