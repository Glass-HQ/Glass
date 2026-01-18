//! Terminal Mode - full-screen terminal experience

use crate::ModeEvent;
use gpui::{App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render};
use ui::Window;

/// Terminal Mode provides a full-window terminal experience
pub struct TerminalMode {
    // Full terminal interface with its own PaneGroup
    // Implementation will be added during Phase 4
    focus_handle: FocusHandle,
}

impl TerminalMode {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<ModeEvent> for TerminalMode {}

impl Focusable for TerminalMode {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalMode {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Will render full-screen terminal PaneGroup
        gpui::Empty
    }
}
