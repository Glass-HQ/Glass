//! Workspace Modes for Glass
//!
//! This crate provides the mode switching functionality for Glass.
//! Modes allow switching between different full-screen interfaces:
//! - Browser Mode: A full-screen browser experience (default on first launch)
//! - Editor Mode: The full code editing experience
//! - Terminal Mode: A full-screen terminal experience
//!
//! ## Architecture
//!
//! The mode system uses a registry pattern to avoid cyclic dependencies:
//! - `ModeViewRegistry` holds registered mode views as `AnyView`
//! - Mode-specific crates (like `browser`) register their views during init
//! - `workspace` queries the registry to get views for rendering
//!
//! This allows workspace to render mode views without depending on the specific
//! crate that implements them.

mod mode_switcher;
mod mode_view_registry;

pub use mode_switcher::ModeSwitcher;
pub use mode_view_registry::{ModeViewFactory, ModeViewRegistry, RegisteredModeView};

use gpui::{App, actions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

actions!(
    workspace_modes,
    [
        /// Switch to Browser Mode
        SwitchToBrowserMode,
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
    pub const BROWSER: ModeId = ModeId("browser");
    pub const EDITOR: ModeId = ModeId("editor");
    pub const TERMINAL: ModeId = ModeId("terminal");

    /// Parse a mode ID from a string (for persistence)
    pub fn from_str(s: &str) -> Self {
        match s {
            "browser" => Self::BROWSER,
            "terminal" => Self::TERMINAL,
            "editor" => Self::EDITOR,
            _ => Self::BROWSER, // Default to browser for first launch
        }
    }
}

impl std::fmt::Display for ModeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Initialize the workspace_modes crate
pub fn init(cx: &mut App) {
    ModeViewRegistry::init(cx);
}
