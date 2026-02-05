pub mod apple;

pub use apple::*;

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    Apple,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    Simulator,
    PhysicalDevice,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub state: DeviceState,
    pub os_version: Option<String>,
    pub platform: Platform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    Booted,
    Shutdown,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Scheme {
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum BuildConfiguration {
    Debug,
    Release,
    Custom(String),
}

impl BuildConfiguration {
    pub fn as_str(&self) -> &str {
        match self {
            BuildConfiguration::Debug => "Debug",
            BuildConfiguration::Release => "Release",
            BuildConfiguration::Custom(name) => name.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub success: bool,
    pub output: String,
    pub errors: Vec<BuildError>,
    pub warnings: Vec<BuildWarning>,
}

#[derive(Debug, Clone)]
pub struct BuildError {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct BuildWarning {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

pub fn detect_platform(workspace_root: &Path) -> Option<Platform> {
    if apple::xcode::detect_xcode_project(workspace_root).is_some() {
        return Some(Platform::Apple);
    }
    None
}
