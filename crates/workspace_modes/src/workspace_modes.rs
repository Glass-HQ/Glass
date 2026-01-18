//! Workspace Modes for Glass
//!
//! This crate provides a system for switching between different full-screen interfaces
//! within Glass. Rather than being just an IDE with embedded panels, Glass becomes a
//! complete development environment with first-class experiences for editing code,
//! using the terminal, and (in the future) browsing the web.
//!
//! See `docs/design.md` for the full architecture and design documentation.

mod mode_container;
mod mode_registry;
mod mode_switcher;
mod modes;
mod persistence;

pub use mode_container::ModeContainer;
pub use mode_registry::ModeRegistry;
pub use mode_switcher::ModeSwitcher;
pub use modes::{EditorMode, TerminalMode};

use gpui::{App, Context, EventEmitter, Focusable, KeyContext, Render, actions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ui::Window;

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
}

impl std::fmt::Display for ModeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Events that can be emitted by workspace modes
#[derive(Clone, Debug)]
pub enum ModeEvent {
    /// Request to switch to another mode (e.g., when clicking a file link in terminal)
    RequestSwitchTo(ModeId),
}

/// Trait that all workspace modes must implement
pub trait WorkspaceMode: Render + Focusable + EventEmitter<ModeEvent> {
    /// Unique identifier for this mode
    fn id(&self) -> ModeId;

    /// Display name shown in the mode switcher
    fn name(&self) -> &'static str;

    /// Key context for mode-specific keybindings
    fn key_context(&self) -> KeyContext;

    /// Called when switching TO this mode
    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>);

    /// Called when switching AWAY from this mode
    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>);

    /// Whether this mode can be activated (e.g., Terminal Mode requires terminal support)
    fn can_activate(&self, cx: &App) -> bool;
}

/// Initialize the workspace_modes crate
pub fn init(_cx: &mut App) {
    // Register actions
    // Mode switching actions will be registered here
}
