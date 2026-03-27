use std::path::Path;

use crate::DetectedProject;

pub trait RuntimeProvider {
    fn detect(&self, workspace_root: &Path) -> Vec<DetectedProject>;
}
