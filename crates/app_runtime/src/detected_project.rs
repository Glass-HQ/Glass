use std::path::PathBuf;

use crate::{RuntimeCapabilitySet, RuntimeDevice, RuntimeTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedProject {
    pub id: String,
    pub label: String,
    pub kind: ProjectKind,
    pub workspace_root: PathBuf,
    pub project_path: PathBuf,
    pub targets: Vec<RuntimeTarget>,
    pub devices: Vec<RuntimeDevice>,
    pub capabilities: RuntimeCapabilitySet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectKind {
    AppleWorkspace,
    AppleProject,
}
