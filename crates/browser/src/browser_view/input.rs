use crate::input;
use gpui::{point, Context, MouseButton, Window};

use super::BrowserView;

impl BrowserView {
    pub(super) fn handle_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.context_menu.is_some() && event.button != MouseButton::Right {
            self.dismiss_context_menu();
        }

        if let Some(tab) = self.active_tab() {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_mouse_down(&tab.read(cx), event, offset);

            tab.update(cx, |tab, _| {
                tab.set_focus(true);
            });
        }
        window.focus(&self.focus_handle, cx);
    }

    pub(super) fn handle_mouse_up(
        &mut self,
        event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_mouse_up(&tab.read(cx), event, offset);
        }
    }

    pub(super) fn handle_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_mouse_move(&tab.read(cx), event, offset);
        }
    }

    pub(super) fn handle_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            tab.update(cx, |tab, _| {
                tab.set_focus(true);
            });

            let keystroke = event.keystroke.clone();
            let is_held = event.is_held;
            let tab = tab.clone();

            cx.defer(move |cx| {
                tab.update(cx, |tab, _| {
                    input::handle_key_down_deferred(tab, &keystroke, is_held);
                });
            });
        }
    }

    pub(super) fn handle_key_up(
        &mut self,
        event: &gpui::KeyUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            let keystroke = event.keystroke.clone();
            let tab = tab.clone();

            cx.defer(move |cx| {
                tab.update(cx, |tab, _| {
                    input::handle_key_up_deferred(tab, &keystroke);
                });
            });
        }
    }
}
