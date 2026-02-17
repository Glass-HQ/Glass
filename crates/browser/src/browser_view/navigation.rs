use gpui::{Context, Window};

use super::{BrowserView, CopyUrl, FocusOmnibox, GoBack, GoForward, OpenDevTools, Reload};

impl BrowserView {
    pub(super) fn handle_focus_omnibox(
        &mut self,
        _: &FocusOmnibox,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(toolbar) = self.toolbar.clone() {
            toolbar.update(cx, |toolbar, cx| {
                toolbar.focus_omnibox(window, cx);
            });
            cx.stop_propagation();
        }
    }

    pub(super) fn handle_reload(
        &mut self,
        _: &Reload,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            tab.update(cx, |tab, _| {
                tab.reload();
            });
        }
    }

    pub(super) fn handle_go_back(
        &mut self,
        _: &GoBack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            tab.update(cx, |tab, _| {
                tab.go_back();
            });
        }
    }

    pub(super) fn handle_go_forward(
        &mut self,
        _: &GoForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            tab.update(cx, |tab, _| {
                tab.go_forward();
            });
        }
    }

    pub(super) fn handle_open_devtools(
        &mut self,
        _: &OpenDevTools,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).open_devtools();
        }
    }

    pub(super) fn handle_copy_url(
        &mut self,
        _: &CopyUrl,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            let url = tab.read(cx).url().to_string();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));

            let status_toast = toast::StatusToast::new("URL copied to clipboard", cx, |this, _| {
                this.icon(toast::ToastIcon::new(ui::IconName::Check).color(ui::Color::Success))
            });
            self.toast_layer.update(cx, |layer, cx| {
                layer.toggle_toast(cx, status_toast);
                layer.start_dismiss_timer(std::time::Duration::from_secs(2), cx);
            });
        }
    }
}
