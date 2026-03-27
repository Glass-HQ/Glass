use std::path::{Path, PathBuf};

use crate::{
    CapabilityState, CommandRunner, DetectedProject, ExecutionPlan, ExecutionRequest, ProjectKind,
    RuntimeAction, RuntimeDeviceKind, RuntimeError, apple_runtime_provider::AppleRuntimeProvider,
    gpui_runtime_provider::GpuiRuntimeProvider, runtime_provider::RuntimeProvider,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeCatalog {
    pub projects: Vec<DetectedProject>,
}

impl RuntimeCatalog {
    pub fn discover(workspace_roots: &[PathBuf], runner: &dyn CommandRunner) -> Self {
        let apple_provider = AppleRuntimeProvider::new(runner);
        let gpui_provider = GpuiRuntimeProvider::new(runner);
        let providers: [&dyn RuntimeProvider; 2] = [&apple_provider, &gpui_provider];
        let mut projects = Vec::new();

        for workspace_root in workspace_roots {
            for provider in providers {
                projects.extend(provider.detect(workspace_root));
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
                    Ok(match project.kind {
                        ProjectKind::AppleWorkspace | ProjectKind::AppleProject => {
                            build_apple_plan(
                                project.workspace_root.as_path(),
                                project,
                                target.label.as_str(),
                            )
                        }
                        ProjectKind::GpuiApplication => {
                            build_gpui_plan(project, target.label.as_str())
                        }
                    })
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

                    Ok(match project.kind {
                        ProjectKind::AppleWorkspace | ProjectKind::AppleProject => {
                            match device.kind {
                                RuntimeDeviceKind::Simulator => run_apple_simulator_plan(
                                    project.workspace_root.as_path(),
                                    project,
                                    target.label.as_str(),
                                    device.id.as_str(),
                                ),
                                RuntimeDeviceKind::Desktop => run_apple_desktop_plan(
                                    project.workspace_root.as_path(),
                                    project,
                                    target.label.as_str(),
                                ),
                            }
                        }
                        ProjectKind::GpuiApplication => {
                            if !matches!(device.kind, RuntimeDeviceKind::Desktop) {
                                return Err(RuntimeError::UnsupportedDeviceKind);
                            }

                            run_gpui_plan(project, target.label.as_str())
                        }
                    })
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

fn build_apple_plan(
    workspace_root: &Path,
    project: &DetectedProject,
    target: &str,
) -> ExecutionPlan {
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

fn run_apple_simulator_plan(
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

fn run_apple_desktop_plan(
    workspace_root: &Path,
    project: &DetectedProject,
    target: &str,
) -> ExecutionPlan {
    let derived_data_path = workspace_root
        .join(".glass")
        .join("app_runtime")
        .join(target.replace('/', "-"));
    let derived_data = derived_data_path.to_string_lossy();
    let selector = xcode_selector(project);
    let target = shell_escape(target);

    ExecutionPlan {
        label: format!("Run {}", project.label),
        command_label: format!("xcodebuild {} build and open", target),
        command: "zsh".to_string(),
        args: vec![
            "-lc".to_string(),
            format!(
                "set -euo pipefail\n\
                mkdir -p {derived_data}\n\
                {selector} -scheme {target} -destination 'platform=macOS' -derivedDataPath {derived_data} build\n\
                app_path=\"$(find {derived_data}/Build/Products -maxdepth 2 -name '*.app' -print -quit)\"\n\
                if [ -z \"$app_path\" ]; then\n\
                  echo 'Glass could not find the built .app bundle.' >&2\n\
                  exit 1\n\
                fi\n\
                open -n \"$app_path\"\n",
                derived_data = shell_escape(derived_data.as_ref()),
            ),
        ],
        cwd: workspace_root.to_path_buf(),
    }
}

fn build_gpui_plan(project: &DetectedProject, target: &str) -> ExecutionPlan {
    ExecutionPlan {
        label: format!("Build {}", project.label),
        command_label: format!("cargo build --bin {}", target),
        command: "cargo".to_string(),
        args: vec![
            "build".to_string(),
            "--manifest-path".to_string(),
            project.project_path.to_string_lossy().into_owned(),
            "--bin".to_string(),
            target.to_string(),
        ],
        cwd: project.workspace_root.clone(),
    }
}

fn run_gpui_plan(project: &DetectedProject, target: &str) -> ExecutionPlan {
    ExecutionPlan {
        label: format!("Run {}", project.label),
        command_label: format!("cargo run --bin {}", target),
        command: "cargo".to_string(),
        args: vec![
            "run".to_string(),
            "--manifest-path".to_string(),
            project.project_path.to_string_lossy().into_owned(),
            "--bin".to_string(),
            target.to_string(),
        ],
        cwd: project.workspace_root.clone(),
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
        ProjectKind::GpuiApplication => {
            unreachable!("xcode selector is only valid for Apple runtime projects")
        }
    }
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::Path, sync::Mutex};

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
    fn detects_gpui_application_and_marks_missing_cargo_as_setup_required() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_gpui_fixture(temp_dir.path(), "mini-gpui", &["mini-gpui"]);

        let runner = FakeCommandRunner::new(BTreeMap::from([(
            "cargo --version".to_string(),
            CommandOutput {
                success: false,
                stdout: String::new(),
                stderr: "cargo missing".to_string(),
            },
        )]));

        let catalog = RuntimeCatalog::discover(&[temp_dir.path().to_path_buf()], &runner);
        let project = catalog
            .projects
            .iter()
            .find(|project| project.label == "mini-gpui")
            .unwrap();

        assert_eq!(project.kind, crate::ProjectKind::GpuiApplication);
        assert!(!project.capabilities.build.is_available());
        assert!(!project.capabilities.run.is_available());
        assert_eq!(project.targets.len(), 1);
        assert_eq!(project.devices.len(), 1);
    }

    #[test]
    fn detects_schemes_and_simulators_before_ui_integration() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_workspace_fixture(temp_dir.path());
        let workspace_path = temp_dir.path().join("Demo.xcworkspace");

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
                format!(
                    "xcodebuild -workspace {} -scheme Demo -showdestinations -quiet",
                    workspace_path.display()
                ),
                CommandOutput {
                    success: true,
                    stdout: "{ platform:iOS Simulator, id:SIM-1, OS:18.2, name:iPhone 16 Pro }"
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
        let workspace_path = temp_dir.path().join("Demo.xcworkspace");

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
                format!(
                    "xcodebuild -workspace {} -scheme Demo -showdestinations -quiet",
                    workspace_path.display()
                ),
                CommandOutput {
                    success: true,
                    stdout: "{ platform:iOS Simulator, id:SIM-1, OS:18.2, name:iPhone 16 }"
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
        let workspace_path = temp_dir.path().join("Demo.xcworkspace");

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
                format!(
                    "xcodebuild -workspace {} -scheme Demo -showdestinations -quiet",
                    workspace_path.display()
                ),
                CommandOutput {
                    success: true,
                    stdout: "{ platform:iOS Simulator, id:SIM-1, OS:18.2, name:iPhone 16 }"
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

    #[test]
    fn builds_a_macos_run_plan_when_a_mac_destination_is_available() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_workspace_fixture(temp_dir.path());
        let workspace_path = temp_dir.path().join("Demo.xcworkspace");

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
                format!(
                    "xcodebuild -workspace {} -scheme Demo -showdestinations -quiet",
                    workspace_path.display()
                ),
                CommandOutput {
                    success: true,
                    stdout: "{ platform:macOS, arch:arm64, id:MAC-1, name:My Mac }\n".to_string(),
                    stderr: String::new(),
                },
            ),
        ]));

        let catalog = RuntimeCatalog::discover(&[temp_dir.path().to_path_buf()], &runner);
        let project = catalog.projects.first().unwrap();
        let desktop = project
            .devices
            .iter()
            .find(|device| device.kind == crate::RuntimeDeviceKind::Desktop)
            .unwrap();

        let run_plan = catalog
            .build_execution_plan(&ExecutionRequest {
                project_id: project.id.clone(),
                target_id: project.targets[0].id.clone(),
                device_id: Some(desktop.id.clone()),
                action: RuntimeAction::Run,
            })
            .unwrap();

        assert!(run_plan.args[1].contains("-destination 'platform=macOS'"));
        assert!(run_plan.args[1].contains("open -n \"$app_path\""));
    }

    #[test]
    fn builds_cargo_plans_for_gpui_targets() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_gpui_fixture(
            temp_dir.path(),
            "mini-gpui",
            &["mini-gpui", "mini-gpui-debug"],
        );

        let runner = FakeCommandRunner::new(BTreeMap::from([(
            "cargo --version".to_string(),
            CommandOutput {
                success: true,
                stdout: "cargo 1.87.0".to_string(),
                stderr: String::new(),
            },
        )]));

        let catalog = RuntimeCatalog::discover(&[temp_dir.path().to_path_buf()], &runner);
        let project = catalog
            .projects
            .iter()
            .find(|project| project.label == "mini-gpui")
            .unwrap();

        let build_plan = catalog
            .build_execution_plan(&ExecutionRequest {
                project_id: project.id.clone(),
                target_id: project.targets[0].id.clone(),
                device_id: None,
                action: RuntimeAction::Build,
            })
            .unwrap();
        assert_eq!(build_plan.command, "cargo");
        assert_eq!(build_plan.args[0], "build");
        assert!(build_plan.args.contains(&"--manifest-path".to_string()));

        let run_plan = catalog
            .build_execution_plan(&ExecutionRequest {
                project_id: project.id.clone(),
                target_id: project.targets[0].id.clone(),
                device_id: Some(project.devices[0].id.clone()),
                action: RuntimeAction::Run,
            })
            .unwrap();
        assert_eq!(run_plan.command, "cargo");
        assert_eq!(run_plan.args[0], "run");
        assert!(run_plan.args.contains(&"--bin".to_string()));
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

    fn create_gpui_fixture(root: &Path, package_name: &str, bins: &[&str]) {
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();

        let manifest = if bins.len() <= 1 {
            format!(
                r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
gpui = "0.1"
"#,
            )
        } else {
            let bin_sections = bins
                .iter()
                .map(|bin| {
                    format!(
                        r#"[[bin]]
name = "{bin}"
path = "src/main.rs"
"#
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
gpui = "0.1"

{bin_sections}
"#,
            )
        };

        fs::write(root.join("Cargo.toml"), manifest).unwrap();
    }
}
