use crate::history::BrowserHistory;
use crate::omnibox::{Omnibox, OmniboxEvent};
use crate::tab::{BrowserTab, TabEvent};
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, Subscription, Window,
    native_icon_button,
};
use ui::{h_flex, prelude::*};

pub struct BrowserToolbar {
    tab: Entity<BrowserTab>,
    omnibox: Entity<Omnibox>,
    _subscriptions: Vec<Subscription>,
}

impl BrowserToolbar {
    pub fn new(
        tab: Entity<BrowserTab>,
        history: Entity<BrowserHistory>,
        browser_focus_handle: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let omnibox = cx.new(|cx| Omnibox::new(history, browser_focus_handle, window, cx));

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
                    tab.update(cx, |tab, cx| {
                        tab.navigate(&url, cx);
                        tab.set_focus(true);
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
                    tab.update(cx, |tab, cx| {
                        tab.navigate(&url, cx);
                        tab.set_focus(true);
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

    pub fn focus_omnibox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.omnibox.update(cx, |omnibox, cx| {
            omnibox.focus_and_select_all(window, cx);
        });
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
        let is_new_tab_page = self.tab.read(cx).is_new_tab_page();
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
            .when(!is_new_tab_page, |this| {
                this.child(
                    native_icon_button("back", "chevron.left")
                        .disabled(!can_go_back)
                        .tooltip("Go Back")
                        .on_click(cx.listener(Self::go_back)),
                )
                .child(
                    native_icon_button("forward", "chevron.right")
                        .disabled(!can_go_forward)
                        .tooltip("Go Forward")
                        .on_click(cx.listener(Self::go_forward)),
                )
                .child(if is_loading {
                    native_icon_button("stop", "xmark.circle")
                        .on_click(cx.listener(Self::stop))
                        .tooltip("Stop")
                } else {
                    native_icon_button("reload", "arrow.clockwise")
                        .on_click(cx.listener(Self::reload))
                        .tooltip("Reload")
                })
                .child(self.omnibox.clone())
            })
    }
}
