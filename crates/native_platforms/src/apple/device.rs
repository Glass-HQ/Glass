use crate::{Device, DeviceState, DeviceType, Platform};
use serde::Deserialize;
use std::process::Command;

pub fn list_physical_devices() -> Vec<Device> {
    log::info!("list_physical_devices: detecting connected devices");

    // Try devicectl first (iOS 17+, Xcode 15+)
    let devicectl_devices = list_devices_via_devicectl();
    if !devicectl_devices.is_empty() {
        log::info!(
            "list_physical_devices: found {} devices via devicectl",
            devicectl_devices.len()
        );
        return devicectl_devices;
    }

    // Fall back to xctrace for older devices
    log::info!("list_physical_devices: falling back to xctrace");
    let xctrace_devices = list_devices_via_xctrace();
    log::info!(
        "list_physical_devices: found {} devices via xctrace",
        xctrace_devices.len()
    );
    xctrace_devices
}

#[derive(Debug, Deserialize)]
struct DevicectlOutput {
    result: DevicectlResult,
}

#[derive(Debug, Deserialize)]
struct DevicectlResult {
    devices: Vec<DevicectlDevice>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicectlDevice {
    #[serde(default)]
    connection_properties: Option<ConnectionProperties>,
    #[serde(default)]
    device_properties: Option<DeviceProperties>,
    #[serde(default)]
    hardware_properties: Option<HardwareProperties>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionProperties {
    #[serde(default)]
    tunnel_state: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceProperties {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    os_version_number: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HardwareProperties {
    #[serde(default)]
    udid: Option<String>,
}

fn list_devices_via_devicectl() -> Vec<Device> {
    let output = Command::new("xcrun")
        .args(["devicectl", "list", "devices", "--json-output", "-"])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            log::debug!("devicectl not available: {}", e);
            return Vec::new();
        }
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Result<DevicectlOutput, _> = serde_json::from_str(&stdout);

    match parsed {
        Ok(data) => data
            .result
            .devices
            .into_iter()
            .filter_map(|d| {
                let udid = d
                    .hardware_properties
                    .as_ref()
                    .and_then(|hp| hp.udid.clone())?;

                let name = d
                    .device_properties
                    .as_ref()
                    .and_then(|dp| dp.name.clone())
                    .unwrap_or_else(|| "Unknown Device".to_string());

                let os_version = d
                    .device_properties
                    .as_ref()
                    .and_then(|dp| dp.os_version_number.clone());

                let state = d
                    .connection_properties
                    .as_ref()
                    .and_then(|cp| cp.tunnel_state.as_ref())
                    .map(|s| {
                        if s == "connected" {
                            DeviceState::Booted
                        } else {
                            DeviceState::Unknown
                        }
                    })
                    .unwrap_or(DeviceState::Booted);

                Some(Device {
                    id: udid,
                    name,
                    device_type: DeviceType::PhysicalDevice,
                    state,
                    os_version,
                    platform: Platform::Apple,
                })
            })
            .collect(),
        Err(e) => {
            log::debug!("Failed to parse devicectl output: {}", e);
            Vec::new()
        }
    }
}

fn list_devices_via_xctrace() -> Vec<Device> {
    let output = Command::new("xcrun")
        .args(["xctrace", "list", "devices"])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            log::debug!("xctrace not available: {}", e);
            return Vec::new();
        }
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_xctrace_output(&stdout)
}

fn parse_xctrace_output(output: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    let mut in_devices_section = false;

    for line in output.lines() {
        let line = line.trim();

        if line == "== Devices ==" {
            in_devices_section = true;
            continue;
        }
        if line == "== Simulators ==" {
            in_devices_section = false;
            continue;
        }

        if in_devices_section && !line.is_empty() {
            if let Some(device) = parse_device_line(line) {
                devices.push(device);
            }
        }
    }

    devices
}

fn parse_device_line(line: &str) -> Option<Device> {
    let open_paren = line.rfind('(')?;
    let close_paren = line.rfind(')')?;

    if open_paren >= close_paren {
        return None;
    }

    let udid = line[open_paren + 1..close_paren].trim().to_string();
    let name_part = line[..open_paren].trim();

    if udid.is_empty() || name_part.is_empty() {
        return None;
    }

    if udid.contains("Simulator") || name_part.to_lowercase().contains("simulator") {
        return None;
    }

    let (name, os_version) = if let Some(bracket_start) = name_part.rfind('[') {
        let bracket_end = name_part.rfind(']').unwrap_or(name_part.len());
        let version = name_part[bracket_start + 1..bracket_end].trim().to_string();
        let name = name_part[..bracket_start].trim().to_string();
        (name, Some(version))
    } else {
        (name_part.to_string(), None)
    };

    Some(Device {
        id: udid,
        name,
        device_type: DeviceType::PhysicalDevice,
        state: DeviceState::Booted,
        os_version,
        platform: Platform::Apple,
    })
}

pub fn get_device_destination(device: &Device) -> String {
    match device.device_type {
        DeviceType::Simulator => {
            if let Some(os_version) = &device.os_version {
                let os = os_version.replace("iOS ", "");
                format!("platform=iOS Simulator,name={},OS={}", device.name, os)
            } else {
                format!("platform=iOS Simulator,name={}", device.name)
            }
        }
        DeviceType::PhysicalDevice => {
            format!("platform=iOS,id={}", device.id)
        }
    }
}
