use crate::{BuildConfiguration, BuildError, BuildResult, BuildWarning, Device};
use anyhow::{Context, Result};
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use smol::io::{AsyncBufReadExt, BufReader};
use smol::process::Command;
use std::path::Path;

use super::device::get_device_destination;
use super::xcode::{XcodeProject, XcodeProjectType};

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub scheme: String,
    pub configuration: BuildConfiguration,
    pub destination: Option<Device>,
    pub clean: bool,
    pub derived_data_path: Option<String>,
}

pub struct BuildProcess {
    pub output_receiver: mpsc::UnboundedReceiver<BuildOutput>,
}

#[derive(Debug, Clone)]
pub enum BuildOutput {
    Line(String),
    Error(BuildError),
    Warning(BuildWarning),
    Progress { phase: String, percent: Option<f32> },
    Completed(BuildResult),
}

pub async fn build(
    project: &XcodeProject,
    options: &BuildOptions,
) -> Result<BuildProcess> {
    let mut cmd = Command::new("xcodebuild");

    match project.project_type {
        XcodeProjectType::Project => {
            cmd.arg("-project").arg(&project.path);
        }
        XcodeProjectType::Workspace => {
            cmd.arg("-workspace").arg(&project.path);
        }
    }

    cmd.arg("-scheme").arg(&options.scheme);
    cmd.arg("-configuration").arg(options.configuration.as_str());

    if let Some(device) = &options.destination {
        cmd.arg("-destination").arg(get_device_destination(device));
    }

    if let Some(derived_data) = &options.derived_data_path {
        cmd.arg("-derivedDataPath").arg(derived_data);
    }

    if options.clean {
        cmd.arg("clean");
    }

    cmd.arg("build");

    cmd.stdout(smol::process::Stdio::piped());
    cmd.stderr(smol::process::Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn xcodebuild")?;

    let (mut tx, rx) = mpsc::unbounded();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    smol::spawn(async move {
        let mut all_output = String::new();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Some(line_result) = lines.next().await {
                if let Ok(line) = line_result {
                    all_output.push_str(&line);
                    all_output.push('\n');

                    if let Some(build_output) = parse_build_line(&line, &mut errors, &mut warnings) {
                        let _ = tx.send(build_output).await;
                    } else {
                        let _ = tx.send(BuildOutput::Line(line)).await;
                    }
                }
            }
        }

        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Some(line_result) = lines.next().await {
                if let Ok(line) = line_result {
                    all_output.push_str(&line);
                    all_output.push('\n');
                    let _ = tx.send(BuildOutput::Line(line)).await;
                }
            }
        }

        let status = child.status().await;
        let success = status.map(|s| s.success()).unwrap_or(false);

        let _ = tx
            .send(BuildOutput::Completed(BuildResult {
                success,
                output: all_output,
                errors,
                warnings,
            }))
            .await;
    })
    .detach();

    Ok(BuildProcess {
        output_receiver: rx,
    })
}

fn parse_build_line(
    line: &str,
    errors: &mut Vec<BuildError>,
    warnings: &mut Vec<BuildWarning>,
) -> Option<BuildOutput> {
    if line.contains(": error:") {
        let error = parse_error_line(line);
        errors.push(error.clone());
        return Some(BuildOutput::Error(error));
    }

    if line.contains(": warning:") {
        let warning = parse_warning_line(line);
        warnings.push(warning.clone());
        return Some(BuildOutput::Warning(warning));
    }

    if line.starts_with("Compiling") || line.starts_with("Linking") || line.starts_with("Build ") {
        return Some(BuildOutput::Progress {
            phase: line.to_string(),
            percent: None,
        });
    }

    None
}

fn parse_error_line(line: &str) -> BuildError {
    if let Some(error_idx) = line.find(": error:") {
        let file_info = &line[..error_idx];
        let message = line[error_idx + 8..].trim().to_string();

        let (file, line_num) = parse_file_location(file_info);

        BuildError {
            message,
            file,
            line: line_num,
        }
    } else {
        BuildError {
            message: line.to_string(),
            file: None,
            line: None,
        }
    }
}

fn parse_warning_line(line: &str) -> BuildWarning {
    if let Some(warning_idx) = line.find(": warning:") {
        let file_info = &line[..warning_idx];
        let message = line[warning_idx + 10..].trim().to_string();

        let (file, line_num) = parse_file_location(file_info);

        BuildWarning {
            message,
            file,
            line: line_num,
        }
    } else {
        BuildWarning {
            message: line.to_string(),
            file: None,
            line: None,
        }
    }
}

fn parse_file_location(file_info: &str) -> (Option<String>, Option<u32>) {
    let parts: Vec<&str> = file_info.rsplitn(3, ':').collect();
    match parts.len() {
        3 => {
            let file = parts[2].to_string();
            let line = parts[1].parse().ok();
            (Some(file), line)
        }
        2 => {
            let file = parts[1].to_string();
            let line = parts[0].parse().ok();
            (Some(file), line)
        }
        _ => (Some(file_info.to_string()), None),
    }
}

pub async fn run(
    project: &XcodeProject,
    options: &BuildOptions,
) -> Result<BuildProcess> {
    let mut cmd = Command::new("xcodebuild");

    match project.project_type {
        XcodeProjectType::Project => {
            cmd.arg("-project").arg(&project.path);
        }
        XcodeProjectType::Workspace => {
            cmd.arg("-workspace").arg(&project.path);
        }
    }

    cmd.arg("-scheme").arg(&options.scheme);
    cmd.arg("-configuration").arg(options.configuration.as_str());

    if let Some(device) = &options.destination {
        cmd.arg("-destination").arg(get_device_destination(device));
    }

    if let Some(derived_data) = &options.derived_data_path {
        cmd.arg("-derivedDataPath").arg(derived_data);
    }

    cmd.arg("build");
    cmd.arg("-allowProvisioningUpdates");

    cmd.stdout(smol::process::Stdio::piped());
    cmd.stderr(smol::process::Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn xcodebuild")?;

    let (mut tx, rx) = mpsc::unbounded();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    smol::spawn(async move {
        let mut all_output = String::new();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Some(line_result) = lines.next().await {
                if let Ok(line) = line_result {
                    all_output.push_str(&line);
                    all_output.push('\n');

                    if let Some(build_output) = parse_build_line(&line, &mut errors, &mut warnings) {
                        let _ = tx.send(build_output).await;
                    } else {
                        let _ = tx.send(BuildOutput::Line(line)).await;
                    }
                }
            }
        }

        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Some(line_result) = lines.next().await {
                if let Ok(line) = line_result {
                    all_output.push_str(&line);
                    all_output.push('\n');
                    let _ = tx.send(BuildOutput::Line(line)).await;
                }
            }
        }

        let status = child.status().await;
        let success = status.map(|s| s.success()).unwrap_or(false);

        let _ = tx
            .send(BuildOutput::Completed(BuildResult {
                success,
                output: all_output,
                errors,
                warnings,
            }))
            .await;
    })
    .detach();

    Ok(BuildProcess {
        output_receiver: rx,
    })
}

pub fn find_derived_data_path(project: &XcodeProject) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let derived_data = Path::new(&home).join("Library/Developer/Xcode/DerivedData");

    if !derived_data.exists() {
        return None;
    }

    let project_name = project.path.file_stem()?.to_str()?;

    if let Ok(entries) = std::fs::read_dir(&derived_data) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_str()?;
            if name_str.starts_with(project_name) {
                return Some(entry.path().to_string_lossy().to_string());
            }
        }
    }

    None
}
