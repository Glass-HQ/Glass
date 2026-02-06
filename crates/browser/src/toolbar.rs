use crate::history::BrowserHistory;
use crate::omnibox::{Omnibox, OmniboxEvent};
use crate::tab::{BrowserTab, TabEvent};
use gpui::{App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, Subscription, Window};
use ui::{h_flex, prelude::*, IconButton, IconName, Tooltip};

pub struct BrowserToolbar {
    tab: Entity<BrowserTab>,
    omnibox: Entity<Omnibox>,
    _subscriptions: Vec<Subscription>,
}

impl BrowserToolbar {
    pub fn new(
        tab: Entity<BrowserTab>,
        history: Entity<BrowserHistory>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let omnibox = cx.new(|cx| Omnibox::new(history, window, cx));

        let tab_subscription = cx.subscribe_in(&tab, window, {
            let omnibox = omnibox.clone();
            move |_this, _tab, event, window, cx| match event {
                TabEvent::AddressChanged(url) => {
                    let url = url.clone();
                    omnibox.update(cx, |omnibox, cx| {
                        omnibox.set_url(&url, window, cx);
                    });
                }
                TabEvent::LoadingStateChanged | TabEvent::TitleChanged(_) => {
                    cx.notify();
                }
                _ => {}
            }
        });

        let omnibox_subscription = cx.subscribe(&omnibox, {
            let tab = tab.clone();
            move |_this, _omnibox, event: &OmniboxEvent, cx| match event {
                OmniboxEvent::Navigate(url) => {
                    let url = url.clone();
                    tab.update(cx, |tab, _| {
                        tab.navigate(&url);
                    });
                }
            }
        });

        Self {
            tab,
            omnibox,
            _subscriptions: vec![tab_subscription, omnibox_subscription],
        }
    }

    pub fn set_active_tab(
        &mut self,
        tab: Entity<BrowserTab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab = tab;
        self._subscriptions.clear();

        let tab_subscription = cx.subscribe_in(&self.tab, window, {
            let omnibox = self.omnibox.clone();
            move |_this, _tab, event, window, cx| match event {
                TabEvent::AddressChanged(url) => {
                    let url = url.clone();
                    omnibox.update(cx, |omnibox, cx| {
                        omnibox.set_url(&url, window, cx);
                    });
                }
                TabEvent::LoadingStateChanged | TabEvent::TitleChanged(_) => {
                    cx.notify();
                }
                _ => {}
            }
        });

        let omnibox_subscription = cx.subscribe(&self.omnibox, {
            let tab = self.tab.clone();
            move |_this, _omnibox, event: &OmniboxEvent, cx| match event {
                OmniboxEvent::Navigate(url) => {
                    let url = url.clone();
                    tab.update(cx, |tab, _| {
                        tab.navigate(&url);
                    });
                }
            }
        });

        self._subscriptions
            .extend([tab_subscription, omnibox_subscription]);

        let url = self.tab.read(cx).url().to_string();
        self.omnibox.update(cx, |omnibox, cx| {
            omnibox.set_url(&url, window, cx);
        });
        cx.notify();
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

}

impl Focusable for BrowserToolbar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.omnibox.focus_handle(cx)
    }
}

impl Render for BrowserToolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let can_go_back = self.tab.read(cx).can_go_back();
        let can_go_forward = self.tab.read(cx).can_go_forward();
        let is_loading = self.tab.read(cx).is_loading();

        h_flex()
            .w_full()
            .max_w(px(680.))
            .h_full()
            .items_center()
            .px_2()
            .gap_1()
            .key_context("BrowserToolbar")
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
            .child(if is_loading {
                IconButton::new("stop", IconName::XCircle)
                    .on_click(cx.listener(Self::stop))
                    .tooltip(Tooltip::text("Stop"))
            } else {
                IconButton::new("reload", IconName::RotateCw)
                    .on_click(cx.listener(Self::reload))
                    .tooltip(Tooltip::text("Reload"))
            })
            .child(self.omnibox.clone())
    }
}
