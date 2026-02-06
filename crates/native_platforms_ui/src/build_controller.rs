use crate::build_logs::{AnyOutput, BuildLogsView};
use futures::StreamExt;
use gpui::{AppContext, AsyncWindowContext, Context, Task, WeakEntity, Window};
use native_platforms::apple::run::RunOutput;
use native_platforms::apple::{build, run, simulator, xcode};
use native_platforms::{Device, DeviceType};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use workspace::Workspace;

pub enum PipelineKind {
    Build,
    Run,
}

enum ControllerState {
    Idle,
    Active {
        // Held for drop semantics: dropping the task cancels the pipeline.
        _task: Task<()>,
        active_pid: Arc<AtomicU32>,
    },
}

pub struct LaunchedApp {
    pub bundle_id: String,
    pub device: Device,
    pub pid: Option<u32>,
}

pub struct BuildController {
    state: ControllerState,
    last_launched: Option<LaunchedApp>,
    launched_slot: Arc<Mutex<Option<LaunchedApp>>>,
    completed: Arc<AtomicBool>,
}

impl BuildController {
    pub fn new() -> Self {
        Self {
            state: ControllerState::Idle,
            last_launched: None,
            launched_slot: Arc::new(Mutex::new(None)),
            completed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, ControllerState::Active { .. })
    }

    pub fn last_launched(&self) -> Option<&LaunchedApp> {
        self.last_launched.as_ref()
    }

    /// Called when the panel gets notified — drains the shared slot into `last_launched`
    /// and transitions to Idle if the task has finished.
    pub fn poll_completion(&mut self) {
        if let Ok(mut slot) = self.launched_slot.lock() {
            if let Some(app) = slot.take() {
                self.last_launched = Some(app);
            }
        }

        if matches!(self.state, ControllerState::Active { .. })
            && self.completed.load(Ordering::Acquire)
        {
            self.state = ControllerState::Idle;
        }
    }

    pub fn stop(&mut self) {
        if let ControllerState::Active { active_pid, .. } = &self.state {
            let pid = active_pid.load(Ordering::Acquire);
            if pid != 0 {
                let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .status();
            }
        }
        self.state = ControllerState::Idle;
        self.completed.store(false, Ordering::Release);
    }

    pub fn terminate_app<T: 'static>(&mut self, cx: &mut Context<T>) {
        if let Some(launched) = self.last_launched.take() {
            cx.background_spawn(async move {
                if launched.device.device_type == DeviceType::Simulator {
                    if let Err(error) =
                        simulator::terminate_app(&launched.device.id, &launched.bundle_id)
                    {
                        log::error!("Failed to terminate app on simulator: {}", error);
                    }
                } else {
                    match launched.pid {
                        Some(pid) => {
                            log::info!(
                                "Terminating app on device {} with pid {}",
                                launched.device.id,
                                pid
                            );
                            let output = std::process::Command::new("xcrun")
                                .args([
                                    "devicectl",
                                    "device",
                                    "process",
                                    "terminate",
                                    "--device",
                                    &launched.device.id,
                                    "--pid",
                                    &pid.to_string(),
                                ])
                                .output();

                            match output {
                                Ok(result) if !result.status.success() => {
                                    let stderr = String::from_utf8_lossy(&result.stderr);
                                    log::error!(
                                        "Failed to terminate app on device: {}",
                                        stderr
                                    );
                                }
                                Err(error) => {
                                    log::error!(
                                        "Failed to run devicectl terminate: {}",
                                        error
                                    );
                                }
                                _ => {
                                    log::info!("App terminated successfully");
                                }
                            }
                        }
                        None => {
                            log::error!(
                                "Cannot terminate app: no PID was captured during launch"
                            );
                        }
                    }
                }
            })
            .detach();
        }
    }

    pub fn start_pipeline<T: 'static>(
        &mut self,
        kind: PipelineKind,
        project: &xcode::XcodeProject,
        options: build::BuildOptions,
        workspace: WeakEntity<Workspace>,
        panel: WeakEntity<T>,
        window: &mut Window,
        cx: &mut Context<T>,
    ) {
        self.stop();
        self.last_launched = None;
        self.completed.store(false, Ordering::Release);

        let active_pid = Arc::new(AtomicU32::new(0));
        let active_pid_for_task = active_pid.clone();
        let project = project.clone();
        let device = options.destination.clone();
        let launched_slot = self.launched_slot.clone();
        let completed = self.completed.clone();

        let task = cx.spawn_in(window, async move |_this, cx| {
            match kind {
                PipelineKind::Build => {
                    Self::run_build_pipeline(
                        project,
                        options,
                        active_pid_for_task,
                        workspace,
                        panel,
                        completed,
                        cx,
                    )
                    .await;
                }
                PipelineKind::Run => {
                    Self::run_run_pipeline(
                        project,
                        options,
                        device,
                        active_pid_for_task,
                        workspace,
                        panel,
                        launched_slot,
                        completed,
                        cx,
                    )
                    .await;
                }
            }
        });

        self.state = ControllerState::Active { _task: task, active_pid };
    }

    async fn run_build_pipeline<T: 'static>(
        project: xcode::XcodeProject,
        options: build::BuildOptions,
        active_pid: Arc<AtomicU32>,
        workspace: WeakEntity<Workspace>,
        panel: WeakEntity<T>,
        completed: Arc<AtomicBool>,
        cx: &mut AsyncWindowContext,
    ) {
        let build_result = build::build(&project, &options).await;

        match build_result {
            Ok(process) => {
                active_pid.store(
                    process.active_pid.load(Ordering::Acquire),
                    Ordering::Release,
                );

                let (forwarder_tx, forwarder_rx) = futures::channel::mpsc::unbounded();

                if let Some(workspace) = workspace.upgrade() {
                    workspace
                        .update_in(cx, |workspace, window, cx| {
                            let view = cx.new(|cx| {
                                BuildLogsView::new(forwarder_rx, "Build Output", window, cx)
                            });
                            workspace.add_item_to_active_pane(
                                Box::new(view),
                                None,
                                true,
                                window,
                                cx,
                            );
                        })
                        .ok();
                }

                let mut receiver = process.output_receiver;
                while let Some(output) = receiver.next().await {
                    let _ = forwarder_tx.unbounded_send(AnyOutput::Build(output));
                }
                drop(forwarder_tx);
            }
            Err(error) => {
                log::error!("Build failed to start: {}", error);
            }
        }

        completed.store(true, Ordering::Release);
        panel
            .update(cx, |_this, cx| {
                cx.notify();
            })
            .ok();
    }

    async fn run_run_pipeline<T: 'static>(
        project: xcode::XcodeProject,
        options: build::BuildOptions,
        device: Option<Device>,
        active_pid: Arc<AtomicU32>,
        workspace: WeakEntity<Workspace>,
        panel: WeakEntity<T>,
        launched_slot: Arc<Mutex<Option<LaunchedApp>>>,
        completed: Arc<AtomicBool>,
        cx: &mut AsyncWindowContext,
    ) {
        let run_result = run::run(&project, &options).await;

        match run_result {
            Ok(process) => {
                active_pid.store(
                    process.active_pid.load(Ordering::Acquire),
                    Ordering::Release,
                );

                let (forwarder_tx, forwarder_rx) = futures::channel::mpsc::unbounded();

                if let Some(workspace) = workspace.upgrade() {
                    workspace
                        .update_in(cx, |workspace, window, cx| {
                            let view = cx.new(|cx| {
                                BuildLogsView::new(forwarder_rx, "Run Output", window, cx)
                            });
                            workspace.add_item_to_active_pane(
                                Box::new(view),
                                None,
                                true,
                                window,
                                cx,
                            );
                        })
                        .ok();
                }

                let mut receiver = process.output_receiver;
                while let Some(output) = receiver.next().await {
                    if let RunOutput::AppLaunched {
                        ref bundle_id,
                        pid,
                    } = output
                    {
                        if let Some(device) = &device {
                            if let Ok(mut slot) = launched_slot.lock() {
                                *slot = Some(LaunchedApp {
                                    bundle_id: bundle_id.clone(),
                                    device: device.clone(),
                                    pid,
                                });
                            }
                        }
                    }
                    let _ = forwarder_tx.unbounded_send(AnyOutput::Run(output));
                }
                drop(forwarder_tx);
            }
            Err(error) => {
                log::error!("Run failed to start: {}", error);
            }
        }

        completed.store(true, Ordering::Release);
        panel
            .update(cx, |_this, cx| {
                cx.notify();
            })
            .ok();
    }
}
