use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct XcodeProject {
    pub path: PathBuf,
    pub project_type: XcodeProjectType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XcodeProjectType {
    Project,
    Workspace,
}

#[derive(Debug, Deserialize)]
struct XcodebuildListOutput {
    project: Option<ProjectInfo>,
    workspace: Option<WorkspaceInfo>,
}

#[derive(Debug, Deserialize)]
struct ProjectInfo {
    configurations: Vec<String>,
    name: String,
    schemes: Vec<String>,
    targets: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceInfo {
    name: String,
    schemes: Vec<String>,
}

pub fn detect_xcode_project(workspace_root: &Path) -> Option<XcodeProject> {
    log::info!("detect_xcode_project: searching recursively in {:?}", workspace_root);

    let mut workspaces: Vec<PathBuf> = Vec::new();
    let mut projects: Vec<PathBuf> = Vec::new();

    let walker = WalkDir::new(workspace_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            // Skip hidden directories, common non-project directories, and inside .xcodeproj
            if name.starts_with('.')
                || name == "node_modules"
                || name == "Pods"
                || name == "build"
                || name == "DerivedData"
                || name == "vendor"
                || name == "Carthage"
            {
                return false;
            }
            true
        });

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();

        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy();

            if ext_str == "xcworkspace" {
                // Skip the internal project.xcworkspace inside .xcodeproj directories
                let name = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
                let parent_is_xcodeproj = path
                    .parent()
                    .and_then(|p| p.extension())
                    .map(|e| e == "xcodeproj")
                    .unwrap_or(false);

                if name != "project" && !parent_is_xcodeproj && !name.starts_with('.') {
                    log::info!("detect_xcode_project: found workspace at {:?}", path);
                    workspaces.push(path.to_path_buf());
                }
            } else if ext_str == "xcodeproj" {
                log::info!("detect_xcode_project: found project at {:?}", path);
                projects.push(path.to_path_buf());
            }
        }
    }

    // Prioritize workspaces over projects, and prefer ones closer to the root
    workspaces.sort_by_key(|p| p.components().count());
    projects.sort_by_key(|p| p.components().count());

    if let Some(workspace) = workspaces.into_iter().next() {
        log::info!("detect_xcode_project: selected workspace {:?}", workspace);
        return Some(XcodeProject {
            path: workspace,
            project_type: XcodeProjectType::Workspace,
        });
    }

    if let Some(project) = projects.into_iter().next() {
        log::info!("detect_xcode_project: selected project {:?}", project);
        return Some(XcodeProject {
            path: project,
            project_type: XcodeProjectType::Project,
        });
    }

    log::info!("detect_xcode_project: no Xcode project found");
    None
}


pub fn list_schemes(project: &XcodeProject) -> Result<Vec<String>> {
    log::info!("list_schemes: starting for {:?}", project.path);

    // Parse schemes directly from filesystem - much faster than xcodebuild -list
    let mut schemes = Vec::new();

    // Collect scheme directories to search
    let mut scheme_dirs = Vec::new();

    match project.project_type {
        XcodeProjectType::Project => {
            // Shared schemes in .xcodeproj/xcshareddata/xcschemes/
            let shared_dir = project.path.join("xcshareddata").join("xcschemes");
            scheme_dirs.push(shared_dir);

            // User schemes in .xcodeproj/xcuserdata/<user>.xcuserdatad/xcschemes/
            let userdata_dir = project.path.join("xcuserdata");
            if userdata_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&userdata_dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.extension().map(|e| e == "xcuserdatad").unwrap_or(false) {
                            scheme_dirs.push(path.join("xcschemes"));
                        }
                    }
                }
            }
        }
        XcodeProjectType::Workspace => {
            // Shared schemes in .xcworkspace/xcshareddata/xcschemes/
            let shared_dir = project.path.join("xcshareddata").join("xcschemes");
            scheme_dirs.push(shared_dir);

            // User schemes in .xcworkspace/xcuserdata/<user>.xcuserdatad/xcschemes/
            let userdata_dir = project.path.join("xcuserdata");
            if userdata_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&userdata_dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.extension().map(|e| e == "xcuserdatad").unwrap_or(false) {
                            scheme_dirs.push(path.join("xcschemes"));
                        }
                    }
                }
            }

            // Also check schemes in referenced .xcodeproj files within workspace
            let contents_path = project.path.join("contents.xcworkspacedata");
            if contents_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&contents_path) {
                    // Parse workspace to find referenced projects
                    for line in contents.lines() {
                        if let Some(start) = line.find("location = \"group:") {
                            let rest = &line[start + 18..];
                            if let Some(end) = rest.find('"') {
                                let relative_path = &rest[..end];
                                if relative_path.ends_with(".xcodeproj") {
                                    let proj_path = project.path.parent()
                                        .map(|p| p.join(relative_path));
                                    if let Some(proj_path) = proj_path {
                                        let proj_shared = proj_path.join("xcshareddata").join("xcschemes");
                                        scheme_dirs.push(proj_shared);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Collect .xcscheme files from all directories
    for dir in scheme_dirs {
        if dir.exists() {
            log::debug!("list_schemes: scanning {:?}", dir);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().map(|e| e == "xcscheme").unwrap_or(false) {
                        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                            if !schemes.contains(&name.to_string()) {
                                schemes.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    schemes.sort();
    log::info!("list_schemes: found {} schemes by parsing files", schemes.len());

    Ok(schemes)
}

pub fn list_configurations(_project: &XcodeProject) -> Result<Vec<String>> {
    // Return standard configurations - parsing project.pbxproj is complex
    // and Debug/Release are the universal standard
    Ok(vec!["Debug".to_string(), "Release".to_string()])
}

pub fn list_targets(project: &XcodeProject) -> Result<Vec<String>> {
    if project.project_type != XcodeProjectType::Project {
        return Ok(Vec::new());
    }

    let mut cmd = Command::new("xcodebuild");
    cmd.arg("-list")
        .arg("-json")
        .arg("-project")
        .arg(&project.path);

    let output = cmd.output().context("Failed to run xcodebuild -list")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: XcodebuildListOutput =
        serde_json::from_str(&stdout).context("Failed to parse xcodebuild output")?;

    if let Some(project_info) = parsed.project {
        Ok(project_info.targets)
    } else {
        Ok(Vec::new())
    }
}
