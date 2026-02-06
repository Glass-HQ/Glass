use crate::{BuildConfiguration, BuildError, BuildResult, BuildWarning, Device};
use anyhow::{Context, Result};
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use smol::io::{AsyncBufReadExt, BufReader};
use smol::process::Command;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

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
    pub active_pid: Arc<AtomicU32>,
}

#[derive(Debug, Clone)]
pub enum BuildOutput {
    Line(String),
    Verbose(String),
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
    // Don't pass -configuration; let the scheme's build action decide.
    // This allows schemes like "MyApp - RELEASE" to use their configured Release settings.

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

    let active_pid = Arc::new(AtomicU32::new(child.id()));

    let (mut tx, rx) = mpsc::unbounded();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let pid_handle = active_pid.clone();
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

                    let build_output = parse_build_line(&line, &mut errors, &mut warnings);
                    let _ = tx.send(build_output).await;
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

        pid_handle.store(0, Ordering::Release);

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
        active_pid,
    })
}

fn parse_build_line(
    line: &str,
    errors: &mut Vec<BuildError>,
    warnings: &mut Vec<BuildWarning>,
) -> BuildOutput {
    // Errors and warnings always shown in full
    if line.contains(": error:") {
        let error = parse_error_line(line);
        errors.push(error.clone());
        return BuildOutput::Error(error);
    }

    if line.contains(": warning:") {
        let warning = parse_warning_line(line);
        warnings.push(warning.clone());
        return BuildOutput::Warning(warning);
    }

    // xcpretty-style summaries for known xcodebuild action lines
    if line.starts_with("CompileSwift ") || line.starts_with("CompileSwiftSources ") {
        return BuildOutput::Progress {
            phase: format_compile_summary(line),
            percent: None,
        };
    }

    if line.starts_with("CompileC ") {
        return BuildOutput::Progress {
            phase: format_compile_summary(line),
            percent: None,
        };
    }

    if line.starts_with("Ld ") {
        return BuildOutput::Progress {
            phase: format_link_summary(line),
            percent: None,
        };
    }

    if line.starts_with("CodeSign ") {
        return BuildOutput::Progress {
            phase: format_codesign_summary(line),
            percent: None,
        };
    }

    if line.starts_with("PhaseScriptExecution ") {
        return BuildOutput::Progress {
            phase: format_script_summary(line),
            percent: None,
        };
    }

    if line.starts_with("CpResource ") || line.starts_with("CopyPNGFile ")
        || line.starts_with("CpHeader ") || line.starts_with("Copy ")
    {
        return BuildOutput::Progress {
            phase: format_copy_summary(line),
            percent: None,
        };
    }

    if line.starts_with("ProcessInfoPlistFile ") || line.starts_with("ProcessProductPackaging ") {
        return BuildOutput::Progress {
            phase: format_process_summary(line),
            percent: None,
        };
    }

    if line.starts_with("MergeSwiftModule ") {
        let target = extract_target_name(line).unwrap_or("unknown");
        return BuildOutput::Progress {
            phase: format!("Merging modules {} \u{203a} Swift", target),
            percent: None,
        };
    }

    if line.starts_with("GenerateDSYMFile ") {
        let target = extract_target_name(line).unwrap_or("unknown");
        return BuildOutput::Progress {
            phase: format!("Generating dSYM {}", target),
            percent: None,
        };
    }

    if line.starts_with("CreateBuildDirectory ")
        || line.starts_with("RegisterExecutionPolicyException ")
        || line.starts_with("Validate ")
        || line.starts_with("Touch ")
        || line.starts_with("RegisterWithLaunchServices ")
        || line.starts_with("EmitSwiftModule ")
        || line.starts_with("SwiftDriver ")
        || line.starts_with("SwiftCompile ")
        || line.starts_with("SwiftEmitModule ")
        || line.starts_with("SwiftMergeGeneratedHeaders ")
        || line.starts_with("WriteAuxiliaryFile ")
        || line.starts_with("CreateUniversalBinary ")
        || line.starts_with("Ditto ")
        || line.starts_with("LinkStoryboards ")
        || line.starts_with("CompileStoryboard ")
        || line.starts_with("CompileXIB ")
        || line.starts_with("CompileAssetCatalog ")
    {
        return BuildOutput::Verbose(line.to_string());
    }

    // Build succeeded/failed lines are important
    if line.starts_with("Build ") || line.starts_with("** BUILD ") {
        return BuildOutput::Progress {
            phase: line.to_string(),
            percent: None,
        };
    }

    // Verbose noise detection
    let trimmed = line.trim();

    // Empty or whitespace-only lines
    if trimmed.is_empty() {
        return BuildOutput::Verbose(line.to_string());
    }

    // Indented lines (compiler/linker flags)
    if line.starts_with("    ") || line.starts_with('\t') {
        return BuildOutput::Verbose(line.to_string());
    }

    // Full-path tool invocations
    if trimmed.starts_with('/') {
        return BuildOutput::Verbose(line.to_string());
    }

    // Shell commands in build output
    if trimmed.starts_with("cd ") || trimmed.starts_with("export ") || trimmed.starts_with("setenv ") {
        return BuildOutput::Verbose(line.to_string());
    }

    // write-file and note: lines
    if trimmed.starts_with("write-file ") || trimmed.starts_with("note: ") {
        return BuildOutput::Verbose(line.to_string());
    }

    // Anything else shows as a normal line
    BuildOutput::Line(line.to_string())
}

fn extract_target_name(line: &str) -> Option<&str> {
    // Pattern: "(in target 'NAME' from project 'PROJ')"
    let marker = "in target '";
    let start = line.find(marker)? + marker.len();
    let end = start + line[start..].find('\'')?;
    Some(&line[start..end])
}

fn extract_filename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn format_compile_summary(line: &str) -> String {
    let target = extract_target_name(line).unwrap_or("unknown");
    // Find the source file path — usually the last path-like token before "(in target"
    let file = line
        .split_whitespace()
        .filter(|s| s.contains('/') || s.ends_with(".swift") || s.ends_with(".c") || s.ends_with(".m") || s.ends_with(".mm") || s.ends_with(".cpp"))
        .last()
        .map(extract_filename)
        .unwrap_or("sources");
    format!("Compiling {} \u{203a} {}", target, file)
}

fn format_link_summary(line: &str) -> String {
    let target = extract_target_name(line).unwrap_or("unknown");
    // Second token is typically the output path
    let binary = line
        .split_whitespace()
        .nth(1)
        .map(extract_filename)
        .unwrap_or("binary");
    format!("Linking {} \u{203a} {}", target, binary)
}

fn format_codesign_summary(line: &str) -> String {
    let target = extract_target_name(line).unwrap_or("unknown");
    // "CodeSign /path/to/App.app ..."
    let artifact = line
        .split_whitespace()
        .nth(1)
        .map(extract_filename)
        .unwrap_or("artifact");
    format!("Signing {} \u{203a} {}", target, artifact)
}

fn format_script_summary(line: &str) -> String {
    let target = extract_target_name(line).unwrap_or("unknown");
    // "PhaseScriptExecution Script\ Name /path..."
    // Script name is between PhaseScriptExecution and the path (may have escaped spaces)
    let rest = line.strip_prefix("PhaseScriptExecution ").unwrap_or(line);
    let script_name = rest
        .split('/')
        .next()
        .unwrap_or("script")
        .replace("\\ ", " ")
        .trim()
        .to_string();
    let script_name = if script_name.is_empty() { "script".to_string() } else { script_name };
    format!("Running script {} \u{203a} {}", target, script_name)
}

fn format_copy_summary(line: &str) -> String {
    let target = extract_target_name(line).unwrap_or("unknown");
    let file = line
        .split_whitespace()
        .nth(1)
        .map(extract_filename)
        .unwrap_or("resource");
    format!("Copying {} \u{203a} {}", target, file)
}

fn format_process_summary(line: &str) -> String {
    let target = extract_target_name(line).unwrap_or("unknown");
    let file = line
        .split_whitespace()
        .nth(1)
        .map(extract_filename)
        .unwrap_or("file");
    format!("Processing {} \u{203a} {}", target, file)
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

pub fn find_built_app(project: &XcodeProject, options: &BuildOptions) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let derived_data = Path::new(&home).join("Library/Developer/Xcode/DerivedData");

    if !derived_data.exists() {
        return None;
    }

    let project_name = project.path.file_stem()?.to_str()?;

    // Find the project's DerivedData folder
    let mut project_derived_data: Option<std::path::PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(&derived_data) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_str()?;
            if name_str.starts_with(project_name) {
                project_derived_data = Some(entry.path());
                break;
            }
        }
    }

    let project_derived_data = project_derived_data?;

    // Determine the platform suffix based on destination
    let platform_suffix = if let Some(device) = &options.destination {
        match device.device_type {
            crate::DeviceType::Simulator => "iphonesimulator",
            crate::DeviceType::PhysicalDevice => "iphoneos",
        }
    } else {
        "iphoneos"
    };

    // Infer configuration from scheme name (e.g., "MyApp - RELEASE" -> Release)
    // This matches how xcodebuild works when -configuration is not specified
    let config = if options.scheme.to_lowercase().contains("release") {
        "Release"
    } else {
        "Debug"
    };

    let build_dir = project_derived_data
        .join("Build/Products")
        .join(format!("{}-{}", config, platform_suffix));

    // If the inferred directory doesn't exist, try the other configuration
    let build_dir = if build_dir.exists() {
        build_dir
    } else {
        let alt_config = if config == "Release" { "Debug" } else { "Release" };
        let alt_dir = project_derived_data
            .join("Build/Products")
            .join(format!("{}-{}", alt_config, platform_suffix));
        if alt_dir.exists() {
            alt_dir
        } else {
            return None;
        }
    };

    // Find .app bundle
    if let Ok(entries) = std::fs::read_dir(&build_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "app").unwrap_or(false) {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    None
}

pub fn get_bundle_identifier(app_path: &str) -> Option<String> {
    let info_plist = Path::new(app_path).join("Info.plist");
    if !info_plist.exists() {
        return None;
    }

    // Use /usr/libexec/PlistBuddy to read the bundle identifier
    let output = std::process::Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleIdentifier", info_plist.to_str()?])
        .output()
        .ok()?;

    if output.status.success() {
        let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !bundle_id.is_empty() {
            return Some(bundle_id);
        }
    }

    None
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
