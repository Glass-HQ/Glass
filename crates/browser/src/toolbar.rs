//! Browser Toolbar
//!
//! Navigation toolbar with back/forward buttons, URL bar, reload, and devtools.

use crate::tab::{BrowserTab, TabEvent};
use editor::Editor;
use gpui::{
    div, px, App, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Subscription, Window,
};
use ui::{h_flex, prelude::*, IconButton, IconName, Tooltip};

pub struct BrowserToolbar {
    tab: Entity<BrowserTab>,
    url_editor: Entity<Editor>,
    _subscriptions: Vec<Subscription>,
}

impl BrowserToolbar {
    pub fn new(tab: Entity<BrowserTab>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Enter URL or search...", window, cx);
            editor
        });

        let subscription = cx.subscribe_in(&tab, window, {
            let url_editor = url_editor.clone();
            move |_this, _tab, event, window, cx| {
                match event {
                    TabEvent::AddressChanged(url) => {
                        let url = url.clone();
                        url_editor.update(cx, |editor, cx| {
                            editor.set_text(url, window, cx);
                        });
                    }
                    TabEvent::LoadingStateChanged | TabEvent::TitleChanged(_) => {
                        cx.notify();
                    }
                    _ => {}
                }
            }
        });

        Self {
            tab,
            url_editor,
            _subscriptions: vec![subscription],
        }
    }

    fn go_back(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.tab.update(cx, |tab, _| {
            tab.go_back();
        });
    }

    fn go_forward(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.tab.update(cx, |tab, _| {
            tab.go_forward();
        });
    }

    fn reload(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.tab.update(cx, |tab, _| {
            tab.reload();
        });
    }

    fn stop(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.tab.update(cx, |tab, _| {
            tab.stop();
        });
    }

    fn open_devtools(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.tab.update(cx, |tab, _| {
            tab.open_devtools();
        });
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.url_editor.read(cx).text(cx);
        if url.is_empty() {
            return;
        }

        let url = if url.starts_with("http://") || url.starts_with("https://") {
            url
        } else if url.contains('.') {
            format!("https://{}", url)
        } else {
            let encoded: String = url::form_urlencoded::byte_serialize(url.as_bytes()).collect();
            format!("https://www.google.com/search?q={}", encoded)
        };

        self.tab.update(cx, |tab, _| {
            tab.navigate(&url);
        });

        window.blur();
    }
}

impl Focusable for BrowserToolbar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.url_editor.focus_handle(cx)
    }
}

impl Render for BrowserToolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let can_go_back = self.tab.read(cx).can_go_back();
        let can_go_forward = self.tab.read(cx).can_go_forward();
        let is_loading = self.tab.read(cx).is_loading();

        h_flex()
            .w_full()
            .h_full()
            .items_center()
            .px_2()
            .gap_1()
            .key_context("BrowserToolbar")
            .on_action(cx.listener(Self::confirm))
            .child(
                IconButton::new("back", IconName::ArrowLeft)
                    .disabled(!can_go_back)
                    .on_click(cx.listener(Self::go_back))
                    .tooltip(Tooltip::text("Go Back")),
            )
            .child(
                IconButton::new("forward", IconName::ArrowRight)
                    .disabled(!can_go_forward)
                    .on_click(cx.listener(Self::go_forward))
                    .tooltip(Tooltip::text("Go Forward")),
            )
            .child(
                if is_loading {
                    IconButton::new("stop", IconName::XCircle)
                        .on_click(cx.listener(Self::stop))
                        .tooltip(Tooltip::text("Stop"))
                } else {
                    IconButton::new("reload", IconName::RotateCw)
                        .on_click(cx.listener(Self::reload))
                        .tooltip(Tooltip::text("Reload"))
                },
            )
            .child(
                div()
                    .flex_1()
                    .h(px(24.))
                    .mx_2()
                    .px_2()
                    .rounded_md()
                    .bg(theme.colors().editor_background)
                    .border_1()
                    .border_color(theme.colors().border)
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(self.url_editor.clone()),
            )
            .child(
                IconButton::new("devtools", IconName::Code)
                    .on_click(cx.listener(Self::open_devtools))
                    .tooltip(Tooltip::text("Open DevTools")),
            )
    }
}
