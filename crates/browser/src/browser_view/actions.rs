use gpui::{Context, Window};

use super::{BrowserView, Copy, Cut, Paste, Redo, SelectAll, Undo};

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

    pub(super) fn handle_paste(
        &mut self,
        _: &Paste,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
}
