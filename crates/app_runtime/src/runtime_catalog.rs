use std::path::{Path, PathBuf};

use crate::{
    CapabilityState, CommandRunner, DetectedProject, ExecutionPlan, ExecutionRequest,
    ProjectKind, RuntimeAction, RuntimeDeviceKind, RuntimeError,
    apple_runtime_provider::AppleRuntimeProvider,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeCatalog {
    pub projects: Vec<DetectedProject>,
}

impl RuntimeCatalog {
    pub fn discover(workspace_roots: &[PathBuf], runner: &dyn CommandRunner) -> Self {
        let provider = AppleRuntimeProvider::new(runner);
        let mut projects = Vec::new();

        for workspace_root in workspace_roots {
            if let Some(project) = provider.detect(workspace_root) {
                projects.push(project);
            }
        }

        Self { projects }
    }

    pub fn build_execution_plan(
        &self,
        request: &ExecutionRequest,
    ) -> Result<ExecutionPlan, RuntimeError> {
        let project = self
            .projects
            .iter()
            .find(|project| project.id == request.project_id)
            .ok_or_else(|| RuntimeError::ProjectNotFound(request.project_id.clone()))?;

        let target = project
            .targets
            .iter()
            .find(|target| target.id == request.target_id)
            .ok_or_else(|| RuntimeError::TargetNotFound(request.target_id.clone()))?;

        match request.action {
            RuntimeAction::Build => {
                if let CapabilityState::Available = project.capabilities.build {
                    Ok(build_plan(project.workspace_root.as_path(), project, target.label.as_str()))
                } else {
                    Err(RuntimeError::ActionUnavailable(
                        request.action.into(),
                        capability_reason(&project.capabilities.build),
                    ))
                }
            }
            RuntimeAction::Run => {
                if let CapabilityState::Available = project.capabilities.run {
                    let device_id = request
                        .device_id
                        .as_ref()
                        .ok_or(RuntimeError::DeviceRequired)?;
                    let device = project
                        .devices
                        .iter()
                        .find(|device| &device.id == device_id)
                        .ok_or_else(|| RuntimeError::DeviceNotFound(device_id.clone()))?;

                    if !matches!(device.kind, RuntimeDeviceKind::Simulator) {
                        return Err(RuntimeError::UnsupportedDeviceKind);
                    }

                    Ok(run_plan(
                        project.workspace_root.as_path(),
                        project,
                        target.label.as_str(),
                        device.id.as_str(),
                    ))
                } else {
                    Err(RuntimeError::ActionUnavailable(
                        request.action.into(),
                        capability_reason(&project.capabilities.run),
                    ))
                }
            }
        }
    }
}

fn build_plan(workspace_root: &Path, project: &DetectedProject, target: &str) -> ExecutionPlan {
    let command = "zsh".to_string();
    let args = vec![
        "-lc".to_string(),
        format!(
            "set -euo pipefail\n{} -scheme {} build",
            xcode_selector(project),
            shell_escape(target),
        ),
    ];

    ExecutionPlan {
        label: format!("Build {}", project.label),
        command_label: format!("xcodebuild {} build", target),
        command,
        args,
        cwd: workspace_root.to_path_buf(),
    }
}

fn run_plan(
    workspace_root: &Path,
    project: &DetectedProject,
    target: &str,
    simulator_id: &str,
) -> ExecutionPlan {
    let derived_data_path = workspace_root
        .join(".glass")
        .join("app_runtime")
        .join(target.replace('/', "-"));
    let derived_data = derived_data_path.to_string_lossy();
    let selector = xcode_selector(project);
    let target = shell_escape(target);
    let simulator_id = shell_escape(simulator_id);
    let command = "zsh".to_string();
    let args = vec![
        "-lc".to_string(),
        format!(
            "set -euo pipefail\n\
            mkdir -p {derived_data}\n\
            open -a Simulator\n\
            xcrun simctl boot {simulator_id} >/dev/null 2>&1 || true\n\
            xcrun simctl bootstatus {simulator_id} -b\n\
            {selector} -scheme {target} -destination id={simulator_id} -derivedDataPath {derived_data} build\n\
            app_path=\"$(find {derived_data}/Build/Products -maxdepth 2 -name '*.app' -print -quit)\"\n\
            if [ -z \"$app_path\" ]; then\n\
              echo 'Glass could not find the built .app bundle.' >&2\n\
              exit 1\n\
            fi\n\
            bundle_id=\"$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \"$app_path/Info.plist\")\"\n\
            xcrun simctl install {simulator_id} \"$app_path\"\n\
            xcrun simctl launch {simulator_id} \"$bundle_id\"\n",
            derived_data = shell_escape(derived_data.as_ref()),
        ),
    ];

    ExecutionPlan {
        label: format!("Run {}", project.label),
        command_label: format!("xcodebuild {} build and launch", target),
        command,
        args,
        cwd: workspace_root.to_path_buf(),
    }
}

fn capability_reason(capability: &CapabilityState) -> String {
    match capability {
        CapabilityState::Available => "available".to_string(),
        CapabilityState::RequiresSetup { reason } | CapabilityState::Unavailable { reason } => {
            reason.clone()
        }
    }
}

fn xcode_selector(project: &DetectedProject) -> String {
    match project.kind {
        ProjectKind::AppleWorkspace => format!(
            "xcodebuild -workspace {}",
            shell_escape(project.project_path.to_string_lossy().as_ref())
        ),
        ProjectKind::AppleProject => format!(
            "xcodebuild -project {}",
            shell_escape(project.project_path.to_string_lossy().as_ref())
        ),
    }
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::Path,
        sync::Mutex,
    };

    use crate::{
        CommandOutput, ExecutionRequest, RuntimeAction, RuntimeCatalog, RuntimeError,
        command_runner::CommandRunner,
    };

    struct FakeCommandRunner {
        outputs: BTreeMap<String, CommandOutput>,
        invocations: Mutex<Vec<String>>,
    }

    impl FakeCommandRunner {
        fn new(outputs: BTreeMap<String, CommandOutput>) -> Self {
            Self {
                outputs,
                invocations: Mutex::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for FakeCommandRunner {
        fn run(&self, program: &str, args: &[&str]) -> anyhow::Result<CommandOutput> {
            let key = std::iter::once(program)
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            self.invocations.lock().unwrap().push(key.clone());
            self.outputs
                .get(&key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unexpected command: {key}"))
        }
    }

    #[test]
    fn detects_apple_workspace_and_marks_missing_toolchain_as_setup_required() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_workspace_fixture(temp_dir.path());

        let runner = FakeCommandRunner::new(BTreeMap::from([(
            "xcodebuild -version".to_string(),
            CommandOutput {
                success: false,
                stdout: String::new(),
                stderr: "xcodebuild missing".to_string(),
            },
        )]));

        let catalog = RuntimeCatalog::discover(&[temp_dir.path().to_path_buf()], &runner);
        let project = catalog.projects.first().unwrap();

        assert_eq!(project.label, "Demo");
        assert!(!project.capabilities.build.is_available());
        assert!(!project.capabilities.run.is_available());
        assert_eq!(project.targets.len(), 1);
    }

    #[test]
    fn detects_schemes_and_simulators_before_ui_integration() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_workspace_fixture(temp_dir.path());

        let runner = FakeCommandRunner::new(BTreeMap::from([
            (
                "xcodebuild -version".to_string(),
                CommandOutput {
                    success: true,
                    stdout: "Xcode 16.2".to_string(),
                    stderr: String::new(),
                },
            ),
            (
                "xcrun simctl list devices --json".to_string(),
                CommandOutput {
                    success: true,
                    stdout: serde_json::json!({
                        "devices": {
                            "com.apple.CoreSimulator.SimRuntime.iOS-18-2": [
                                {
                                    "udid": "SIM-1",
                                    "isAvailable": true,
                                    "state": "Booted",
                                    "name": "iPhone 16 Pro"
                                }
                            ]
                        }
                    })
                    .to_string(),
                    stderr: String::new(),
                },
            ),
        ]));

        let catalog = RuntimeCatalog::discover(&[temp_dir.path().to_path_buf()], &runner);
        let project = catalog.projects.first().unwrap();

        assert!(project.capabilities.build.is_available());
        assert!(project.capabilities.run.is_available());
        assert_eq!(project.devices.len(), 1);
        assert_eq!(project.targets[0].label, "Demo");
    }

    #[test]
    fn validates_that_run_requires_a_device() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_workspace_fixture(temp_dir.path());

        let runner = FakeCommandRunner::new(BTreeMap::from([
            (
                "xcodebuild -version".to_string(),
                CommandOutput {
                    success: true,
                    stdout: "Xcode 16.2".to_string(),
                    stderr: String::new(),
                },
            ),
            (
                "xcrun simctl list devices --json".to_string(),
                CommandOutput {
                    success: true,
                    stdout: serde_json::json!({
                        "devices": {
                            "com.apple.CoreSimulator.SimRuntime.iOS-18-2": [
                                {
                                    "udid": "SIM-1",
                                    "isAvailable": true,
                                    "state": "Shutdown",
                                    "name": "iPhone 16"
                                }
                            ]
                        }
                    })
                    .to_string(),
                    stderr: String::new(),
                },
            ),
        ]));

        let catalog = RuntimeCatalog::discover(&[temp_dir.path().to_path_buf()], &runner);
        let project = catalog.projects.first().unwrap();

        let result = catalog.build_execution_plan(&ExecutionRequest {
            project_id: project.id.clone(),
            target_id: project.targets[0].id.clone(),
            device_id: None,
            action: RuntimeAction::Run,
        });

        assert_eq!(result, Err(RuntimeError::DeviceRequired));
    }

    #[test]
    fn builds_an_xcodebuild_plan_for_build_and_run() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_workspace_fixture(temp_dir.path());

        let runner = FakeCommandRunner::new(BTreeMap::from([
            (
                "xcodebuild -version".to_string(),
                CommandOutput {
                    success: true,
                    stdout: "Xcode 16.2".to_string(),
                    stderr: String::new(),
                },
            ),
            (
                "xcrun simctl list devices --json".to_string(),
                CommandOutput {
                    success: true,
                    stdout: serde_json::json!({
                        "devices": {
                            "com.apple.CoreSimulator.SimRuntime.iOS-18-2": [
                                {
                                    "udid": "SIM-1",
                                    "isAvailable": true,
                                    "state": "Shutdown",
                                    "name": "iPhone 16"
                                }
                            ]
                        }
                    })
                    .to_string(),
                    stderr: String::new(),
                },
            ),
        ]));

        let catalog = RuntimeCatalog::discover(&[temp_dir.path().to_path_buf()], &runner);
        let project = catalog.projects.first().unwrap();

        let build_plan = catalog
            .build_execution_plan(&ExecutionRequest {
                project_id: project.id.clone(),
                target_id: project.targets[0].id.clone(),
                device_id: None,
                action: RuntimeAction::Build,
            })
            .unwrap();
        assert_eq!(build_plan.command, "zsh");
        assert!(build_plan.args[1].contains("xcodebuild -workspace"));

        let run_plan = catalog
            .build_execution_plan(&ExecutionRequest {
                project_id: project.id.clone(),
                target_id: project.targets[0].id.clone(),
                device_id: Some(project.devices[0].id.clone()),
                action: RuntimeAction::Run,
            })
            .unwrap();
        assert!(run_plan.args[1].contains("xcrun simctl install"));
        assert!(run_plan.args[1].contains("xcrun simctl launch"));
    }

    fn create_workspace_fixture(root: &Path) {
        let workspace = root.join("Demo.xcworkspace");
        let scheme_dir = workspace.join("xcshareddata").join("xcschemes");
        fs::create_dir_all(&scheme_dir).unwrap();
        fs::write(
            workspace.join("contents.xcworkspacedata"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Workspace version = "1.0"></Workspace>"#,
        )
        .unwrap();
        fs::write(scheme_dir.join("Demo.xcscheme"), "<Scheme />").unwrap();
    }
}
