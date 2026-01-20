//! Mode switcher UI component (segmented control in title bar)

use crate::ModeId;
use gpui::{ClickEvent, RenderOnce};
use std::sync::Arc;
use ui::{
    ToggleButtonGroup, ToggleButtonGroupSize, ToggleButtonGroupStyle, ToggleButtonSimple, Tooltip,
    prelude::*,
};

/// Callback type for when a mode is selected
pub type OnModeSelect = Arc<dyn Fn(ModeId, &ClickEvent, &mut Window, &mut App) + Send + Sync>;

/// Segmented control UI component for switching between modes.
///
/// This component is designed to be placed in the title bar and provides
/// a visual way to switch between Browser, Editor and Terminal modes.
#[derive(IntoElement)]
pub struct ModeSwitcher {
    active_mode_id: ModeId,
    on_mode_select: Option<OnModeSelect>,
}

impl ModeSwitcher {
    /// Create a new ModeSwitcher with the given active mode
    pub fn new(active_mode_id: ModeId) -> Self {
        Self {
            active_mode_id,
            on_mode_select: None,
        }
    }

    /// Set the callback for when a mode is selected
    pub fn on_mode_select(
        mut self,
        callback: impl Fn(ModeId, &ClickEvent, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_mode_select = Some(Arc::new(callback));
        self
    }

    fn selected_index(&self) -> usize {
        match self.active_mode_id {
            ModeId::BROWSER => 0,
            ModeId::EDITOR => 1,
            ModeId::TERMINAL => 2,
            _ => 0,
        }
    }
}

impl RenderOnce for ModeSwitcher {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let selected_index = self.selected_index();
        let active_mode_id = self.active_mode_id;

        let on_browser_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App)> =
            if let Some(ref callback) = self.on_mode_select {
                let callback = callback.clone();
                Box::new(move |event, window, cx| {
                    callback(ModeId::BROWSER, event, window, cx);
                })
            } else {
                Box::new(|_, _, _| {})
            };

        let on_editor_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App)> =
            if let Some(ref callback) = self.on_mode_select {
                let callback = callback.clone();
                Box::new(move |event, window, cx| {
                    callback(ModeId::EDITOR, event, window, cx);
                })
            } else {
                Box::new(|_, _, _| {})
            };

        let on_terminal_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App)> =
            if let Some(callback) = self.on_mode_select {
                Box::new(move |event, window, cx| {
                    callback(ModeId::TERMINAL, event, window, cx);
                })
            } else {
                Box::new(|_, _, _| {})
            };

        ToggleButtonGroup::single_row(
            "mode-switcher",
            [
                ToggleButtonSimple::new("Browser", on_browser_click)
                    .selected(active_mode_id == ModeId::BROWSER)
                    .tooltip(Tooltip::text("Switch to Browser Mode")),
                ToggleButtonSimple::new("Editor", on_editor_click)
                    .selected(active_mode_id == ModeId::EDITOR)
                    .tooltip(Tooltip::text("Switch to Editor Mode")),
                ToggleButtonSimple::new("Terminal", on_terminal_click)
                    .selected(active_mode_id == ModeId::TERMINAL)
                    .tooltip(Tooltip::text("Switch to Terminal Mode")),
            ],
        )
        .style(ToggleButtonGroupStyle::Outlined)
        .size(ToggleButtonGroupSize::Default)
        .selected_index(selected_index)
        .auto_width()
    }
}
