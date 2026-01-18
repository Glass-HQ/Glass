//! Mode registry for tracking available workspace modes

/// Registry that tracks all available workspace modes
pub struct ModeRegistry {
    // Mode factories indexed by ModeId
    // Implementation will be added during Phase 1
}

impl ModeRegistry {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ModeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
