use crate::BrowserView;
use crate::omnibox::{Omnibox, OmniboxEvent};
use crate::tab::{BrowserTab, TabEvent};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Render, Subscription,
    WeakEntity, Window, native_icon_button,
};
use ui::{h_flex, prelude::*};
use workspace::{
    ItemHandle, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView, WorkspaceItemKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserToolbarStyle {
    TitleBar,
    Pane,
}

pub struct BrowserToolbar {
    browser_view: WeakEntity<BrowserView>,
    tab: Option<Entity<BrowserTab>>,
    omnibox: Entity<Omnibox>,
    style: BrowserToolbarStyle,
    _browser_view_subscription: Subscription,
    tab_subscription: Option<Subscription>,
    _omnibox_subscription: Subscription,
}

impl BrowserToolbar {
    pub fn new(
        browser_view: WeakEntity<BrowserView>,
        style: BrowserToolbarStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let browser_view_entity = browser_view
            .upgrade()
            .expect("browser toolbar should be created with a live browser view");
        let (history, browser_focus_handle) =
            browser_view_entity.read_with(cx, |browser_view, cx| {
                (
                    browser_view.history().clone(),
                    browser_view.focus_handle(cx),
                )
            });
        let omnibox = cx.new(|cx| Omnibox::new(history, browser_focus_handle, window, cx));

        let omnibox_subscription = cx.subscribe(&omnibox, {
            move |_this, _omnibox, event: &OmniboxEvent, cx| match event {
                OmniboxEvent::Navigate(url) => {
                    let url = url.clone();
                    if let Some(tab) = _this.tab.clone() {
                        tab.update(cx, |tab, cx| {
                            tab.navigate(&url, cx);
                            tab.set_focus(true);
                        });
                    }
                }
            }
        });

        let browser_view_subscription = cx.observe_in(
            &browser_view_entity,
            window,
            |this, browser_view, window, cx| {
                this.sync_active_tab(&browser_view, window, cx);
            },
        );

        let mut this = Self {
            browser_view,
            tab: None,
            omnibox,
            style,
            _browser_view_subscription: browser_view_subscription,
            tab_subscription: None,
            _omnibox_subscription: omnibox_subscription,
        };
        this.sync_active_tab(&browser_view_entity, window, cx);
        this
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_active_tab(
        &mut self,
        tab: Entity<BrowserTab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.bind_active_tab(Some(tab), window, cx);
    }

    #[cfg(not(target_os = "macos"))]
    pub fn focus_omnibox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.omnibox.update(cx, |omnibox, cx| {
            omnibox.focus_and_select_all(window, cx);
        });
    }

    fn sync_active_tab(
        &mut self,
        browser_view: &Entity<BrowserView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_tab = browser_view.read(cx).active_tab().cloned();
        let current_tab_id = self.tab.as_ref().map(Entity::entity_id);
        let next_tab_id = active_tab.as_ref().map(Entity::entity_id);
        if current_tab_id == next_tab_id {
            cx.notify();
            return;
        }

        self.bind_active_tab(active_tab, window, cx);
    }

    fn bind_active_tab(
        &mut self,
        tab: Option<Entity<BrowserTab>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab_subscription = None;
        self.tab = tab;

        if let Some(tab) = self.tab.clone() {
            self.tab_subscription = Some(cx.subscribe_in(&tab, window, {
                let omnibox = self.omnibox.clone();
                move |_this, _tab, event, window, cx| match event {
                    TabEvent::AddressChanged(url) => {
                        let url = url.clone();
                        omnibox.update(cx, |omnibox, cx| {
                            omnibox.set_url(&url, window, cx);
                        });
                    }
                    TabEvent::LoadingStateChanged | TabEvent::TitleChanged => {
                        cx.notify();
                    }
                    _ => {}
                }
            }));

            let url = tab.read(cx).url().to_string();
            self.omnibox.update(cx, |omnibox, cx| {
                omnibox.set_url(&url, window, cx);
            });
        }

        cx.notify();
    }

    fn toggle_download_center(
        &mut self,
        _: &gpui::ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(browser_view) = self.browser_view.upgrade() {
            browser_view.update(cx, |browser_view, cx| {
                browser_view.toggle_download_center(cx);
            });
        }
    }

    fn go_back(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab.clone() {
            tab.update(cx, |tab, _| {
                tab.go_back();
            });
        }
    }

    fn go_forward(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab.clone() {
            tab.update(cx, |tab, _| {
                tab.go_forward();
            });
        }
    }

    fn reload(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab.clone() {
            tab.update(cx, |tab, _| {
                tab.reload();
            });
        }
    }

    fn stop(&mut self, _: &gpui::ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab.clone() {
            tab.update(cx, |tab, _| {
                tab.stop();
            });
        }
    }
}

impl EventEmitter<ToolbarItemEvent> for BrowserToolbar {}

impl Focusable for BrowserToolbar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.omnibox.focus_handle(cx)
    }
}

impl ToolbarItemView for BrowserToolbar {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        let is_browser_item = active_pane_item
            .is_some_and(|item| item.workspace_item_kind(cx) == Some(WorkspaceItemKind::Browser));

        if is_browser_item {
            ToolbarItemLocation::PrimaryLeft
        } else {
            ToolbarItemLocation::Hidden
        }
    }
}

impl Render for BrowserToolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_new_tab_page = self
            .tab
            .as_ref()
            .is_some_and(|tab| tab.read(cx).is_new_tab_page());
        let can_go_back = self
            .tab
            .as_ref()
            .is_some_and(|tab| tab.read(cx).can_go_back());
        let can_go_forward = self
            .tab
            .as_ref()
            .is_some_and(|tab| tab.read(cx).can_go_forward());
        let is_loading = self
            .tab
            .as_ref()
            .is_some_and(|tab| tab.read(cx).is_loading());
        let shows_downloads_button = self.style == BrowserToolbarStyle::Pane;
        let show_navigation = self.style == BrowserToolbarStyle::Pane || !is_new_tab_page;

        h_flex()
            .w_full()
            .min_w_0()
            .when(self.style == BrowserToolbarStyle::TitleBar, |this| {
                this.max_w(px(680.)).h_full().px_2()
            })
            .when(self.style == BrowserToolbarStyle::Pane, |this| {
                this.min_h_8()
            })
            .items_center()
            .gap_1()
            .key_context("BrowserToolbar")
            .when(show_navigation, |this| {
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
                .when(shows_downloads_button, |this| {
                    this.child(
                        native_icon_button("downloads", "arrow.down.circle")
                            .on_click(cx.listener(Self::toggle_download_center))
                            .tooltip("Downloads"),
                    )
                })
            })
    }
}
