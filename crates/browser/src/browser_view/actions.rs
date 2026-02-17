use std::path::Path;

use editor::{Editor, actions::SelectAll as EditorSelectAll};
use gpui::{App, AppContext, Context, Entity, Focusable, Window};

use super::{
    BrowserView, CloseFindInPage, Copy, Cut, FindInPage, FindNextInPage, FindPreviousInPage, Paste,
    Redo, SelectAll, ToggleDownloadCenter, Undo,
};
use crate::tab::BrowserTab;

impl BrowserView {
    pub(super) fn handle_copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).copy();
        }
    }

    pub(super) fn handle_cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).cut();
        }
    }

    pub(super) fn handle_paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).paste();
        }
    }

    pub(super) fn handle_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).undo();
        }
    }

    pub(super) fn handle_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).redo();
        }
    }

    pub(super) fn handle_select_all(
        &mut self,
        _: &SelectAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).select_all();
        }
    }

    pub(super) fn find_editor_is_focused(&self, window: &Window, cx: &App) -> bool {
        self.find_editor
            .as_ref()
            .is_some_and(|editor| editor.focus_handle(cx).contains_focused(window, cx))
    }

    pub(super) fn ensure_find_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Editor> {
        if let Some(editor) = self.find_editor.clone() {
            return editor;
        }

        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Find in page", window, cx);
            editor
        });
        let subscription = cx.subscribe(&editor, Self::handle_find_editor_event);
        self._subscriptions.push(subscription);
        self.find_editor = Some(editor.clone());
        editor
    }

    pub(super) fn focus_find_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.ensure_find_editor(window, cx);
        window.focus(&editor.focus_handle(cx), cx);
        editor.update(cx, |editor, cx| {
            editor.select_all(&EditorSelectAll, window, cx);
        });
    }

    pub(super) fn set_find_editor_text(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.find_editor.clone() {
            self.suppress_find_editor_event = true;
            editor.update(cx, |editor, cx| {
                editor.set_text(text.to_string(), window, cx);
            });
            self.suppress_find_editor_event = false;
        }
    }

    pub(super) fn run_find(&mut self, forward: bool, find_next: bool, cx: &mut Context<Self>) {
        if self.find_query.is_empty() {
            if let Some(tab) = self.active_tab() {
                tab.read(cx).stop_finding(true);
            }
            self.find_match_count = 0;
            self.find_active_match_ordinal = 0;
            return;
        }

        if let Some(tab) = self.active_tab() {
            tab.read(cx)
                .find_in_page(&self.find_query, forward, false, find_next);
        }
    }

    pub(super) fn clear_find_for_tab_switch(
        &mut self,
        previous_tab: &Entity<BrowserTab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.find_visible && self.find_query.is_empty() {
            return;
        }

        previous_tab.read(cx).stop_finding(true);
        self.find_visible = false;
        self.find_query.clear();
        self.find_match_count = 0;
        self.find_active_match_ordinal = 0;
        self.set_find_editor_text("", window, cx);
        cx.notify();
    }

    fn handle_find_editor_event(
        &mut self,
        _editor: Entity<Editor>,
        event: &editor::EditorEvent,
        cx: &mut Context<Self>,
    ) {
        if self.suppress_find_editor_event || !matches!(event, editor::EditorEvent::BufferEdited) {
            return;
        }

        let Some(editor) = self.find_editor.clone() else {
            return;
        };

        self.find_query = editor.read(cx).text(cx);
        self.find_match_count = 0;
        self.find_active_match_ordinal = 0;
        self.run_find(true, false, cx);
        cx.notify();
    }

    pub(super) fn handle_find_in_page(
        &mut self,
        _: &FindInPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.find_visible = true;
        self.focus_find_editor(window, cx);
        if !self.find_query.is_empty() {
            self.run_find(true, false, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn handle_find_next_in_page(
        &mut self,
        _: &FindNextInPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.find_visible {
            self.find_visible = true;
            self.focus_find_editor(window, cx);
        }
        self.run_find(true, true, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn handle_find_previous_in_page(
        &mut self,
        _: &FindPreviousInPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.find_visible {
            self.find_visible = true;
            self.focus_find_editor(window, cx);
        }
        self.run_find(false, true, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn handle_close_find_in_page(
        &mut self,
        _: &CloseFindInPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).stop_finding(true);
        }
        self.find_visible = false;
        self.find_query.clear();
        self.find_match_count = 0;
        self.find_active_match_ordinal = 0;
        self.set_find_editor_text("", window, cx);
        window.focus(&self.focus_handle, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn handle_toggle_download_center(
        &mut self,
        _: &ToggleDownloadCenter,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.download_center_visible = !self.download_center_visible;
        cx.notify();
    }

    pub(super) fn open_download_with_system(&mut self, id: u32, cx: &mut Context<Self>) {
        let path = self
            .downloads
            .iter()
            .find(|download| download.item.id == id)
            .and_then(|download| download.item.full_path.clone());
        if let Some(path) = path {
            cx.open_with_system(Path::new(&path));
        }
    }

    pub(super) fn reveal_download_in_finder(&mut self, id: u32, cx: &mut Context<Self>) {
        let path = self
            .downloads
            .iter()
            .find(|download| download.item.id == id)
            .and_then(|download| download.item.full_path.clone());
        if let Some(path) = path {
            cx.reveal_path(Path::new(&path));
        }
    }
}
