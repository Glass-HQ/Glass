//! Mode switcher UI component (dropdown menu in title bar)

use crate::ModeId;
use gpui::RenderOnce;
use std::sync::Arc;
use ui::{ContextMenu, ContextMenuEntry, DropdownMenu, DropdownStyle, IconPosition, prelude::*};

/// Callback type for when a mode is selected
pub type OnModeSelect = Arc<dyn Fn(ModeId, &mut Window, &mut App) + Send + Sync>;

const MODES: [(ModeId, &str, IconName); 3] = [
    (ModeId::BROWSER, "Browser", IconName::Globe),
    (ModeId::EDITOR, "Editor", IconName::FileCode),
    (ModeId::TERMINAL, "Terminal", IconName::Terminal),
];

/// Dropdown menu UI component for switching between modes.
///
/// This component is designed to be placed in the title bar and provides
/// a dropdown to switch between Browser, Editor and Terminal modes.
#[derive(IntoElement)]
pub struct ModeSwitcher {
    active_mode_id: ModeId,
    on_mode_select: Option<OnModeSelect>,
}

impl ModeSwitcher {
    pub fn new(active_mode_id: ModeId) -> Self {
        Self {
            active_mode_id,
            on_mode_select: None,
        }
    }

    pub fn on_mode_select(
        mut self,
        callback: impl Fn(ModeId, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_mode_select = Some(Arc::new(callback));
        self
    }

    fn active_mode_label(&self) -> &'static str {
        MODES
            .iter()
            .find(|(id, _, _)| *id == self.active_mode_id)
            .map(|(_, label, _)| *label)
            .unwrap_or("Browser")
    }
}

impl RenderOnce for ModeSwitcher {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let active_mode_id = self.active_mode_id;
        let active_label = self.active_mode_label();
        let on_mode_select = self.on_mode_select;

        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for (mode_id, label, icon) in MODES {
                let is_selected = mode_id == active_mode_id;
                let callback = on_mode_select.clone();

                menu.push_item(
                    ContextMenuEntry::new(label)
                        .icon(icon)
                        .toggleable(IconPosition::End, is_selected)
                        .handler(move |window, cx| {
                            if let Some(ref callback) = callback {
                                callback(mode_id, window, cx);
                            }
                        }),
                );
            }
            menu
        });

        DropdownMenu::new("mode-switcher", active_label, menu)
            .style(DropdownStyle::Ghost)
            .trigger_size(ButtonSize::Compact)
    }
}
