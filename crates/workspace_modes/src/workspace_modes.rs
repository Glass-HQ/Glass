//! Workspace Modes for Glass
//!
//! This crate provides the mode switching functionality for Glass.
//! Modes allow switching between different full-screen interfaces:
//! - Editor Mode: The full code editing experience
//! - Terminal Mode: A full-screen terminal experience
//!
//! See `docs/design.md` for the full architecture and design documentation.

mod mode_switcher;

pub use mode_switcher::ModeSwitcher;

use gpui::{App, actions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

actions!(
    workspace_modes,
    [
        /// Switch to Editor Mode
        SwitchToEditorMode,
        /// Switch to Terminal Mode
        SwitchToTerminalMode,
    ]
);

/// Unique identifier for a workspace mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ModeId(pub &'static str);

impl ModeId {
    pub const EDITOR: ModeId = ModeId("editor");
    pub const TERMINAL: ModeId = ModeId("terminal");

    /// Parse a mode ID from a string (for persistence)
    pub fn from_str(s: &str) -> Self {
        match s {
            "terminal" => Self::TERMINAL,
            _ => Self::EDITOR, // Default to editor
        }
    }
}

impl std::fmt::Display for ModeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Initialize the workspace_modes crate
pub fn init(_cx: &mut App) {
    // Nothing to initialize for now - modes are simple
}
