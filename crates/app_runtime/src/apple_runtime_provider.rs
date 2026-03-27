use std::path::{Path, PathBuf};

use serde::Deserialize;
use walkdir::WalkDir;

use crate::{
    CapabilityState, CommandRunner, DetectedProject, ProjectKind, RuntimeCapabilitySet,
    RuntimeDevice, RuntimeDeviceKind, RuntimeDeviceState, RuntimeTarget,
    runtime_provider::RuntimeProvider,
};

pub struct AppleRuntimeProvider<'a> {
    runner: &'a dyn CommandRunner,
}

impl<'a> AppleRuntimeProvider<'a> {
    pub fn new(runner: &'a dyn CommandRunner) -> Self {
        Self { runner }
    }

    fn detect_project(&self, workspace_root: &Path) -> Option<DetectedProject> {
        let project_path = match detect_xcode_project(workspace_root) {
            Some(project_path) => project_path,
            None => return None,
        };

        let targets = list_targets(&project_path);
        if targets.is_empty() {
            return None;
        }

        let toolchain_ready = self
            .runner
            .run("xcodebuild", &["-version"])
            .map(|output| output.success)
            .unwrap_or(false);

        let project_kind =
            if project_path.extension().and_then(|ext| ext.to_str()) == Some("xcworkspace") {
                ProjectKind::AppleWorkspace
            } else {
                ProjectKind::AppleProject
            };

        let devices = if toolchain_ready {
            list_supported_devices(&project_path, &project_kind, &targets, self.runner)
        } else {
            Vec::new()
        };

        let capabilities = RuntimeCapabilitySet {
            build: if toolchain_ready {
                CapabilityState::Available
            } else {
                CapabilityState::RequiresSetup {
                    reason: "Install Xcode and its command line tools on this Mac.".to_string(),
                }
            },
            run: if !toolchain_ready {
                CapabilityState::RequiresSetup {
                    reason: "Install Xcode and its command line tools on this Mac.".to_string(),
                }
            } else if devices.is_empty() {
                CapabilityState::RequiresSetup {
                    reason:
                        "No supported Apple simulator or macOS destination is available for the detected schemes."
                            .to_string(),
                }
            } else {
                CapabilityState::Available
            },
        };

        let label = project_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Apple App")
            .to_string();

        Some(DetectedProject {
            id: project_path.to_string_lossy().into_owned(),
            label,
            kind: project_kind,
            workspace_root: workspace_root.to_path_buf(),
            project_path,
            targets,
            devices,
            capabilities,
        })
    }
}

impl RuntimeProvider for AppleRuntimeProvider<'_> {
    fn detect(&self, workspace_root: &Path) -> Vec<DetectedProject> {
        self.detect_project(workspace_root).into_iter().collect()
    }
}

fn detect_xcode_project(workspace_root: &Path) -> Option<PathBuf> {
    let mut workspaces = Vec::new();
    let mut projects = Vec::new();

    let walker = WalkDir::new(workspace_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !((entry.depth() > 0 && name.starts_with('.'))
                || name == "node_modules"
                || name == "Pods"
                || name == "build"
                || name == "DerivedData"
                || name == "vendor")
        });

    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("xcworkspace") => {
                let parent_is_project = path
                    .parent()
                    .and_then(|parent| parent.extension())
                    .and_then(|extension| extension.to_str())
                    == Some("xcodeproj");
                if !parent_is_project {
                    workspaces.push(path.to_path_buf());
                }
            }
            Some("xcodeproj") => projects.push(path.to_path_buf()),
            _ => {}
        }
    }

    workspaces.sort_by_key(|path| path.components().count());
    projects.sort_by_key(|path| path.components().count());

    workspaces
        .into_iter()
        .next()
        .or_else(|| projects.into_iter().next())
}

fn list_targets(project_path: &Path) -> Vec<RuntimeTarget> {
    let mut scheme_directories = vec![project_path.join("xcshareddata").join("xcschemes")];

    let user_data_dir = project_path.join("xcuserdata");
    if let Ok(entries) = std::fs::read_dir(&user_data_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let is_user_dir =
                path.extension().and_then(|extension| extension.to_str()) == Some("xcuserdatad");
            if is_user_dir {
                scheme_directories.push(path.join("xcschemes"));
            }
        }
    }

    let is_workspace = project_path
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("xcworkspace");
    if is_workspace {
        scheme_directories.extend(referenced_workspace_scheme_directories(project_path));
    }

    let mut targets = Vec::new();
    for scheme_directory in scheme_directories {
        let Ok(entries) = std::fs::read_dir(&scheme_directory) else {
            continue;
        };

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let is_scheme =
                path.extension().and_then(|extension| extension.to_str()) == Some("xcscheme");
            if !is_scheme {
                continue;
            }

            let Some(label) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };

            if targets
                .iter()
                .any(|existing: &RuntimeTarget| existing.id == label)
            {
                continue;
            }

            targets.push(RuntimeTarget {
                id: label.to_string(),
                label: label.to_string(),
            });
        }
    }

    targets.sort_by(|left, right| left.label.cmp(&right.label));
    targets
}

fn referenced_workspace_scheme_directories(project_path: &Path) -> Vec<PathBuf> {
    let contents_path = project_path.join("contents.xcworkspacedata");
    let Ok(contents) = std::fs::read_to_string(&contents_path) else {
        return Vec::new();
    };

    let mut directories = Vec::new();
    for line in contents.lines() {
        let Some(start) = line.find("location = \"group:") else {
            continue;
        };
        let rest = &line[start + 18..];
        let Some(end) = rest.find('"') else {
            continue;
        };
        let relative_path = &rest[..end];
        if !relative_path.ends_with(".xcodeproj") {
            continue;
        }

        let Some(workspace_parent) = project_path.parent() else {
            continue;
        };
        let project_reference = workspace_parent.join(relative_path);
        directories.push(project_reference.join("xcshareddata").join("xcschemes"));

        let user_data_dir = project_reference.join("xcuserdata");
        if let Ok(entries) = std::fs::read_dir(user_data_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let is_user_dir = path.extension().and_then(|extension| extension.to_str())
                    == Some("xcuserdatad");
                if is_user_dir {
                    directories.push(path.join("xcschemes"));
                }
            }
        }
    }

    directories
}

fn list_supported_devices(
    project_path: &Path,
    project_kind: &ProjectKind,
    targets: &[RuntimeTarget],
    runner: &dyn CommandRunner,
) -> Vec<RuntimeDevice> {
    let mut devices = Vec::new();
    for target in targets {
        let output = match runner.run(
            "xcodebuild",
            &show_destinations_args(project_path, project_kind, target.label.as_str()),
        ) {
            Ok(output) if output.success => output,
            _ => continue,
        };

        for destination in parse_xcode_destinations(&output.stdout) {
            if let Some(device) = coerce_destination(destination) {
                if devices
                    .iter()
                    .all(|existing: &RuntimeDevice| existing.id != device.id)
                {
                    devices.push(device);
                }
            }
        }
    }

    devices.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    devices
}

fn show_destinations_args<'a>(
    project_path: &'a Path,
    project_kind: &'a ProjectKind,
    scheme: &'a str,
) -> Vec<&'a str> {
    let selector_flag = match project_kind {
        ProjectKind::AppleWorkspace => "-workspace",
        ProjectKind::AppleProject => "-project",
        ProjectKind::GpuiApplication => unreachable!("Apple provider cannot receive GPUI projects"),
    };

    vec![
        selector_flag,
        project_path.to_str().unwrap_or_default(),
        "-scheme",
        scheme,
        "-showdestinations",
        "-quiet",
    ]
}

fn parse_xcode_destinations(stdout: &str) -> Vec<XcodeDestination> {
    stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.starts_with('{') || !line.ends_with('}') {
                return None;
            }

            let inner = &line[1..line.len() - 1];
            let mut platform = None;
            let mut arch = None;
            let mut id = None;
            let mut name = None;
            let mut os = None;

            for part in inner.split(", ") {
                let (key, value) = part.split_once(':')?;
                match key.trim() {
                    "platform" => platform = Some(value.trim().to_string()),
                    "arch" => arch = Some(value.trim().to_string()),
                    "id" => id = Some(value.trim().to_string()),
                    "name" => name = Some(value.trim().to_string()),
                    "OS" => os = Some(value.trim().to_string()),
                    "variant" => {}
                    _ => {}
                }
            }

            Some(XcodeDestination {
                platform: platform?,
                arch,
                id: id?,
                name: name?,
                os,
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct XcodeDestination {
    platform: String,
    arch: Option<String>,
    id: String,
    name: String,
    #[serde(rename = "OS")]
    os: Option<String>,
}

fn coerce_destination(destination: XcodeDestination) -> Option<RuntimeDevice> {
    let platform = destination.platform.as_str();

    if platform.ends_with("Simulator") {
        let os_version = destination
            .os
            .as_ref()
            .map(|os| format!("{} {}", platform.trim_end_matches(" Simulator"), os));
        return Some(RuntimeDevice {
            id: destination.id,
            name: destination.name,
            kind: RuntimeDeviceKind::Simulator,
            state: RuntimeDeviceState::Unknown,
            os_version,
        });
    }

    if platform == "macOS" && destination.arch.is_some() {
        return Some(RuntimeDevice {
            id: destination.id,
            name: destination.name,
            kind: RuntimeDeviceKind::Desktop,
            state: RuntimeDeviceState::Unknown,
            os_version: None,
        });
    }

    None
}
