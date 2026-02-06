use crate::DeviceType;
use anyhow::Result;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

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
    Completed,
    Failed { phase: RunPhase, message: String },
}

pub struct RunProcess {
    pub output_receiver: mpsc::UnboundedReceiver<RunOutput>,
}

pub async fn run(
    project: &XcodeProject,
    options: &BuildOptions,
) -> Result<RunProcess> {
    let (tx, rx) = mpsc::unbounded();

    let project = project.clone();
    let options = options.clone();

    smol::spawn(async move {
        run_pipeline(tx, project, options).await;
    })
    .detach();

    Ok(RunProcess {
        output_receiver: rx,
    })
}

async fn run_pipeline(
    mut tx: mpsc::UnboundedSender<RunOutput>,
    project: XcodeProject,
    options: BuildOptions,
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

    let mut build_receiver = build_process.output_receiver;
    let mut build_success = false;

    while let Some(output) = build_receiver.next().await {
        if let BuildOutput::Completed(ref result) = output {
            build_success = result.success;
        }
        let _ = tx.send(RunOutput::Build(output)).await;
    }

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

        while let Some(output) = launch_receiver.next().await {
            match &output {
                LaunchOutput::Completed { .. } => {
                    launch_success = true;
                }
                LaunchOutput::Failed(_) => {
                    launch_success = false;
                }
                _ => {}
            }
            let _ = tx.send(RunOutput::Launch(output)).await;
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
