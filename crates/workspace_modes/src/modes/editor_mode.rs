//! Editor Mode - the default code editing experience

use crate::ModeEvent;
use gpui::{App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render};
use ui::Window;

/// Editor Mode wraps the existing workspace center + docks
pub struct EditorMode {
    // Wraps existing workspace layout
    // Implementation will be added during Phase 3
    focus_handle: FocusHandle,
}

impl EditorMode {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<ModeEvent> for EditorMode {}

impl Focusable for EditorMode {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorMode {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Will delegate to existing workspace rendering
        gpui::Empty
    }
}
