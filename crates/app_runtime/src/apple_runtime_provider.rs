use std::path::{Path, PathBuf};

use serde::Deserialize;
use walkdir::WalkDir;

use crate::{
    CapabilityState, CommandRunner, DetectedProject, ProjectKind, RuntimeCapabilitySet,
    RuntimeDevice, RuntimeDeviceKind, RuntimeDeviceState, RuntimeTarget,
};

pub struct AppleRuntimeProvider<'a> {
    runner: &'a dyn CommandRunner,
}

impl<'a> AppleRuntimeProvider<'a> {
    pub fn new(runner: &'a dyn CommandRunner) -> Self {
        Self { runner }
    }

    pub fn detect(&self, workspace_root: &Path) -> Option<DetectedProject> {
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

        let devices = if toolchain_ready {
            list_simulators(self.runner)
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
                    reason: "Boot an iOS simulator in Xcode before running.".to_string(),
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
            kind: if project_path.extension().and_then(|ext| ext.to_str()) == Some("xcworkspace") {
                ProjectKind::AppleWorkspace
            } else {
                ProjectKind::AppleProject
            },
            workspace_root: workspace_root.to_path_buf(),
            project_path,
            targets,
            devices,
            capabilities,
        })
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

    workspaces.into_iter().next().or_else(|| projects.into_iter().next())
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

    let is_workspace =
        project_path.extension().and_then(|extension| extension.to_str()) == Some("xcworkspace");
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
                let is_user_dir = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    == Some("xcuserdatad");
                if is_user_dir {
                    directories.push(path.join("xcschemes"));
                }
            }
        }
    }

    directories
}

fn list_simulators(runner: &dyn CommandRunner) -> Vec<RuntimeDevice> {
    let Ok(output) = runner.run("xcrun", &["simctl", "list", "devices", "--json"]) else {
        return Vec::new();
    };
    if !output.success {
        return Vec::new();
    }

    let Ok(parsed) = serde_json::from_str::<SimctlListOutput>(&output.stdout) else {
        return Vec::new();
    };

    let mut devices = Vec::new();
    for (runtime, runtime_devices) in parsed.devices {
        let os_version = runtime_to_os_version(runtime.as_str());
        for device in runtime_devices {
            if !device.is_available {
                continue;
            }

            let state = match device.state.as_str() {
                "Booted" => RuntimeDeviceState::Booted,
                "Shutdown" => RuntimeDeviceState::Shutdown,
                _ => RuntimeDeviceState::Unknown,
            };

            devices.push(RuntimeDevice {
                id: device.udid,
                name: device.name,
                kind: RuntimeDeviceKind::Simulator,
                state,
                os_version: os_version.clone(),
            });
        }
    }

    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices
}

fn runtime_to_os_version(runtime: &str) -> Option<String> {
    let trimmed = runtime.strip_prefix("com.apple.CoreSimulator.SimRuntime.")?;
    let trimmed = trimmed.replace('-', ".");
    trimmed
        .strip_prefix("iOS.")
        .map(|version| format!("iOS {version}"))
}

#[derive(Deserialize)]
struct SimctlListOutput {
    devices: std::collections::HashMap<String, Vec<SimctlDevice>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SimctlDevice {
    udid: String,
    is_available: bool,
    state: String,
    name: String,
}
