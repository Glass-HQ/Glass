use gpui::{Context, NativeSearchFieldTarget, Window};

use super::{BrowserView, CopyUrl, FocusOmnibox, GoBack, GoForward, OpenDevTools, Reload};

impl BrowserView {
    pub(super) fn handle_focus_omnibox(
        &mut self,
        _: &FocusOmnibox,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_new_tab = self
            .active_tab()
            .map(|tab| tab.read(cx).is_new_tab_page())
            .unwrap_or(false);

        if is_new_tab {
            window.focus_native_search_field(
                NativeSearchFieldTarget::ContentElement("new-tab-search".into()),
                true,
            );
            cx.stop_propagation();
            return;
        }

        window.focus_native_search_field(
            NativeSearchFieldTarget::ToolbarItem("glass.omnibox".into()),
            true,
        );

        if let Some(toolbar) = self.toolbar.clone() {
            toolbar.update(cx, |toolbar, cx| {
                toolbar.focus_omnibox(window, cx);
            });
        }

        cx.stop_propagation();
    }

    pub(super) fn handle_reload(
        &mut self,
        _: &Reload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab().cloned() {
            let is_suspended = tab.read(cx).is_suspended();
            if is_suspended {
                tab.update(cx, |tab, _| {
                    tab.resume();
                });
                let (width, height, scale_factor) = self.current_dimensions(window);
                let url = tab.read(cx).url().to_string();
                tab.update(cx, |tab, _| {
                    if !tab.has_browser() && width > 0 && height > 0 {
                        tab.set_scale_factor(scale_factor);
                        tab.set_size(width, height);
                        if let Err(e) = tab.create_browser(&url) {
                            log::error!("[browser] Failed to create browser on reload: {}", e);
                            return;
                        }
                    }
                    tab.set_hidden(false);
                    tab.set_focus(true);
                });
            } else {
                tab.update(cx, |tab, _| {
                    tab.reload();
                });
            }
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
