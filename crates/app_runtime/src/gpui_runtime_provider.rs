use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use walkdir::WalkDir;

use crate::{
    CapabilityState, CommandRunner, DetectedProject, ProjectKind, RuntimeCapabilitySet,
    RuntimeDevice, RuntimeDeviceKind, RuntimeDeviceState, RuntimeTarget,
    runtime_provider::RuntimeProvider,
};

pub struct GpuiRuntimeProvider<'a> {
    runner: &'a dyn CommandRunner,
}

impl<'a> GpuiRuntimeProvider<'a> {
    pub fn new(runner: &'a dyn CommandRunner) -> Self {
        Self { runner }
    }
}

impl RuntimeProvider for GpuiRuntimeProvider<'_> {
    fn detect(&self, workspace_root: &Path) -> Vec<DetectedProject> {
        let cargo_ready = self
            .runner
            .run("cargo", &["--version"])
            .map(|output| output.success)
            .unwrap_or(false);

        discover_gpui_projects(workspace_root)
            .into_iter()
            .map(|project| {
                let capabilities = if cargo_ready {
                    RuntimeCapabilitySet {
                        build: CapabilityState::Available,
                        run: CapabilityState::Available,
                    }
                } else {
                    RuntimeCapabilitySet {
                        build: CapabilityState::RequiresSetup {
                            reason: "Install Rust and Cargo on this machine.".to_string(),
                        },
                        run: CapabilityState::RequiresSetup {
                            reason: "Install Rust and Cargo on this machine.".to_string(),
                        },
                    }
                };

                DetectedProject {
                    id: project.manifest_path.to_string_lossy().into_owned(),
                    label: project.name.clone(),
                    kind: ProjectKind::GpuiApplication,
                    workspace_root: project.package_root,
                    project_path: project.manifest_path,
                    targets: project.targets,
                    devices: vec![RuntimeDevice {
                        id: "local-desktop".to_string(),
                        name: "This Mac".to_string(),
                        kind: RuntimeDeviceKind::Desktop,
                        state: RuntimeDeviceState::Unknown,
                        os_version: None,
                    }],
                    capabilities,
                }
            })
            .collect()
    }
}

#[derive(Deserialize)]
struct CargoManifest {
    package: Option<CargoPackage>,
    dependencies: Option<BTreeMap<String, toml::Value>>,
    bin: Option<Vec<CargoBinary>>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
    #[serde(rename = "default-run")]
    default_run: Option<String>,
}

#[derive(Deserialize)]
struct CargoBinary {
    name: String,
}

struct GpuiProjectCandidate {
    name: String,
    package_root: PathBuf,
    manifest_path: PathBuf,
    targets: Vec<RuntimeTarget>,
}

fn discover_gpui_projects(workspace_root: &Path) -> Vec<GpuiProjectCandidate> {
    let walker = WalkDir::new(workspace_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !((entry.depth() > 0 && name.starts_with('.'))
                || name == "node_modules"
                || name == "target"
                || name == "vendor")
        });

    walker
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() == "Cargo.toml")
        .filter_map(|entry| parse_gpui_project(entry.path()))
        .collect()
}

fn parse_gpui_project(manifest_path: &Path) -> Option<GpuiProjectCandidate> {
    let contents = std::fs::read_to_string(manifest_path).ok()?;
    let manifest: CargoManifest = toml::from_str(&contents).ok()?;
    let package = manifest.package?;
    let dependencies = manifest.dependencies?;
    if !dependencies.contains_key("gpui") {
        return None;
    }

    let package_root = manifest_path.parent()?.to_path_buf();
    let mut targets = manifest
        .bin
        .unwrap_or_default()
        .into_iter()
        .map(|binary| RuntimeTarget {
            id: binary.name.clone(),
            label: binary.name,
        })
        .collect::<Vec<_>>();

    if targets.is_empty()
        && (package.default_run.is_some() || package_root.join("src").join("main.rs").exists())
    {
        let binary_name = package
            .default_run
            .clone()
            .unwrap_or_else(|| package.name.clone());
        targets.push(RuntimeTarget {
            id: binary_name.clone(),
            label: binary_name,
        });
    }

    if targets.is_empty() {
        return None;
    }

    targets.sort_by(|left, right| left.label.cmp(&right.label));
    targets.dedup_by(|left, right| left.id == right.id);

    Some(GpuiProjectCandidate {
        name: package.name,
        package_root,
        manifest_path: manifest_path.to_path_buf(),
        targets,
    })
}
