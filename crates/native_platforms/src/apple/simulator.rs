use crate::{Device, DeviceState, DeviceType, Platform};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct SimctlListOutput {
    devices: HashMap<String, Vec<SimctlDevice>>,
}

#[derive(Debug, Deserialize)]
struct SimctlDevice {
    udid: String,
    #[serde(rename = "isAvailable")]
    is_available: bool,
    state: String,
    name: String,
}

pub fn list_simulators() -> Result<Vec<Device>> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "--json"])
        .output()
        .context("Failed to run xcrun simctl list devices")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("simctl list devices failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: SimctlListOutput =
        serde_json::from_str(&stdout).context("Failed to parse simctl output")?;

    let mut devices = Vec::new();

    for (runtime, simulators) in parsed.devices {
        let os_version = extract_os_version(&runtime);

        for sim in simulators {
            if !sim.is_available {
                continue;
            }

            let state = match sim.state.as_str() {
                "Booted" => DeviceState::Booted,
                "Shutdown" => DeviceState::Shutdown,
                _ => DeviceState::Unknown,
            };

            devices.push(Device {
                id: sim.udid,
                name: sim.name,
                device_type: DeviceType::Simulator,
                state,
                os_version: os_version.clone(),
                platform: Platform::Apple,
            });
        }
    }

    devices.sort_by(|a, b| {
        let state_order = |s: &DeviceState| match s {
            DeviceState::Booted => 0,
            DeviceState::Shutdown => 1,
            DeviceState::Unknown => 2,
        };
        let state_cmp = state_order(&a.state).cmp(&state_order(&b.state));
        if state_cmp != std::cmp::Ordering::Equal {
            return state_cmp;
        }
        a.name.cmp(&b.name)
    });

    Ok(devices)
}

fn extract_os_version(runtime: &str) -> Option<String> {
    if runtime.contains("iOS") {
        let parts: Vec<&str> = runtime.split('.').collect();
        for (i, part) in parts.iter().enumerate() {
            if part.contains("iOS") {
                let version_parts: Vec<&str> = parts[i..].iter().copied().collect();
                if let Some(first) = version_parts.first() {
                    if let Some(ios_idx) = first.find("iOS") {
                        let version_str = &first[ios_idx + 3..];
                        let rest: String = version_parts[1..]
                            .iter()
                            .filter(|p| !p.contains("SimRuntime"))
                            .map(|p| format!(".{}", p.trim_end_matches('-')))
                            .collect();
                        return Some(format!("iOS {}{}", version_str.trim_start_matches('-'), rest));
                    }
                }
            }
        }
    }

    if runtime.contains("watchOS") {
        return Some("watchOS".to_string());
    }
    if runtime.contains("tvOS") {
        return Some("tvOS".to_string());
    }
    if runtime.contains("visionOS") {
        return Some("visionOS".to_string());
    }

    None
}

pub fn boot_simulator(udid: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "boot", udid])
        .output()
        .context("Failed to run xcrun simctl boot")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("current state: Booted") {
            anyhow::bail!("simctl boot failed: {}", stderr);
        }
    }

    Ok(())
}

pub fn shutdown_simulator(udid: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "shutdown", udid])
        .output()
        .context("Failed to run xcrun simctl shutdown")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("current state: Shutdown") {
            anyhow::bail!("simctl shutdown failed: {}", stderr);
        }
    }

    Ok(())
}

pub fn open_simulator_app() -> Result<()> {
    Command::new("open")
        .arg("-a")
        .arg("Simulator")
        .spawn()
        .context("Failed to open Simulator app")?;
    Ok(())
}

pub fn install_app(udid: &str, app_path: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "install", udid, app_path])
        .output()
        .context("Failed to run xcrun simctl install")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("simctl install failed: {}", stderr);
    }

    Ok(())
}

pub fn terminate_app(udid: &str, bundle_id: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "terminate", udid, bundle_id])
        .output()
        .context("Failed to run xcrun simctl terminate")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("simctl terminate failed: {}", stderr);
    }

    Ok(())
}

pub fn launch_app(udid: &str, bundle_id: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "launch", udid, bundle_id])
        .output()
        .context("Failed to run xcrun simctl launch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("simctl launch failed: {}", stderr);
    }

    Ok(())
}
