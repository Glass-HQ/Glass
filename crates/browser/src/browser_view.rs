mod actions;
mod bookmarks;
mod content;
mod context_menu;
mod input;
mod navigation;
mod session;
mod swipe;
mod tab_strip;
mod tabs;

use self::context_menu::{BrowserContextMenu, PendingContextMenu};
use self::swipe::SwipeNavigationState;

use crate::bookmarks::BookmarkBar;
use crate::cef_instance::CefInstance;
use crate::history::BrowserHistory;
use crate::session::SerializedTab;
use crate::tab::{BrowserTab, TabEvent};
use crate::toolbar::BrowserToolbar;
use gpui::{
    actions, div, prelude::*, App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, Styled, Subscription, Task,
    Window,
};
use std::sync::atomic::{AtomicBool, Ordering};
use workspace_modes::{ModeId, ModeViewRegistry};

const MAX_CLOSED_TABS: usize = 20;

static TABS_RESTORED: AtomicBool = AtomicBool::new(false);

actions!(
    browser,
    [
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        SelectAll,
        NewTab,
        CloseTab,
        ReopenClosedTab,
        NextTab,
        PreviousTab,
        FocusOmnibox,
        Reload,
        GoBack,
        GoForward,
        OpenDevTools,
        PinTab,
        UnpinTab,
        BookmarkCurrentPage,
        CopyUrl,
        ToggleSidebar,
    ]
);

#[derive(Default, Clone, Copy, PartialEq)]
enum TabBarMode {
    #[default]
    Horizontal,
    Sidebar,
}

pub struct BrowserView {
    focus_handle: FocusHandle,
    tabs: Vec<Entity<BrowserTab>>,
    active_tab_index: usize,
    closed_tabs: Vec<SerializedTab>,
    toolbar: Option<Entity<BrowserToolbar>>,
    bookmark_bar: Entity<BookmarkBar>,
    history: Entity<BrowserHistory>,
    content_bounds: Bounds<Pixels>,
    cef_available: bool,
    message_pump_started: bool,
    last_viewport: Option<(u32, u32, u32)>,
    pending_new_tab_urls: Vec<String>,
    context_menu: Option<BrowserContextMenu>,
    pending_context_menu: Option<PendingContextMenu>,
    tab_bar_mode: TabBarMode,
    toast_layer: Entity<toast::ToastLayer>,
    swipe_state: SwipeNavigationState,
    _swipe_dismiss_task: Option<Task<()>>,
    _message_pump_task: Option<Task<()>>,
    _schedule_save: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl BrowserView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let cef_available = CefInstance::global().is_some();

        let quit_subscription = cx.on_app_quit(Self::save_tabs_on_quit);
        let history = cx.new(|cx| BrowserHistory::new(cx));
        let bookmark_bar = cx.new(|cx| BookmarkBar::new(cx));
        let bookmark_subscription = cx.subscribe(&bookmark_bar, Self::handle_bookmark_bar_event);
        let toast_layer = cx.new(|_| toast::ToastLayer::new());

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            tabs: Vec::new(),
            active_tab_index: 0,
            closed_tabs: Vec::new(),
            toolbar: None,
            bookmark_bar,
            history,
            content_bounds: Bounds::default(),
            cef_available,
            message_pump_started: false,
            last_viewport: None,
            pending_new_tab_urls: Vec::new(),
            context_menu: None,
            pending_context_menu: None,
            tab_bar_mode: TabBarMode::default(),
            toast_layer,
            swipe_state: SwipeNavigationState::default(),
            _swipe_dismiss_task: None,
            _message_pump_task: None,
            _schedule_save: None,
            _subscriptions: vec![quit_subscription, bookmark_subscription],
        };

        if cef_available {
            let already_restored = TABS_RESTORED.swap(true, Ordering::SeqCst);
            let restored = if !already_restored {
                this.restore_tabs(cx)
            } else {
                false
            };
            if !restored {
                this.add_tab(cx);
            }
        }

        this
    }

    fn active_tab(&self) -> Option<&Entity<BrowserTab>> {
        self.tabs.get(self.active_tab_index)
    }

    fn update_toolbar_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(toolbar), Some(tab)) = (self.toolbar.clone(), self.active_tab().cloned()) {
            toolbar.update(cx, |toolbar, cx| {
                toolbar.set_active_tab(tab, window, cx);
            });
        }
        self.sync_bookmark_bar_visibility(cx);
    }

    fn focus_omnibox_if_new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_new_tab = self
            .active_tab()
            .map(|t| t.read(cx).is_new_tab_page())
            .unwrap_or(false);
        if is_new_tab {
            if let Some(toolbar) = self.toolbar.clone() {
                toolbar.update(cx, |toolbar, cx| {
                    toolbar.focus_omnibox(window, cx);
                });
            }
        }
    }

    fn sync_bookmark_bar_visibility(&self, cx: &mut Context<Self>) {
        let is_new_tab_page = self
            .active_tab()
            .map(|t| t.read(cx).is_new_tab_page())
            .unwrap_or(true);
        self.bookmark_bar.update(cx, |bar, _| {
            bar.set_active_tab_is_new_tab_page(is_new_tab_page);
        });
    }

    fn handle_tab_event(
        &mut self,
        tab_entity: Entity<BrowserTab>,
        event: &TabEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TabEvent::FrameReady => {
                cx.notify();
            }
            TabEvent::NavigateToUrl(url) => {
                let url = url.clone();
                self.create_browser_and_navigate(&tab_entity, &url, cx);
            }
            TabEvent::OpenNewTab(url) => {
                self.pending_new_tab_urls.push(url.clone());
                cx.notify();
            }
            TabEvent::AddressChanged(_) | TabEvent::TitleChanged(_) => {
                let tab_handle = tab_entity;
                let history = self.history.clone();
                cx.defer(move |cx| {
                    let (url, title) = {
                        let tab = tab_handle.read(cx);
                        (tab.url().to_string(), tab.title().to_string())
                    };
                    history.update(cx, |history, _| {
                        history.record_visit(&url, &title);
                    });
                });
                self.schedule_save(cx);
                cx.notify();
            }
            TabEvent::FaviconChanged(_) => {
                self.schedule_save(cx);
                cx.notify();
            }
            TabEvent::LoadingStateChanged => {
                cx.notify();
            }
            TabEvent::LoadError {
                url, error_text, ..
            } => {
                log::warn!("[browser] load error: url={} err={}", url, error_text);
                cx.notify();
            }
            TabEvent::ContextMenuOpen { context } => {
                self.pending_context_menu = Some(PendingContextMenu {
                    context: context.clone(),
                });
                cx.notify();
            }
        }
    }

    fn create_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab().cloned() {
            let history = self.history.clone();
            let browser_focus_handle = self.focus_handle.clone();
            let toolbar =
                cx.new(|cx| BrowserToolbar::new(tab, history, browser_focus_handle, window, cx));
            self.toolbar = Some(toolbar.clone());

            ModeViewRegistry::global_mut(cx)
                .set_titlebar_center_view(ModeId::BROWSER, toolbar.into());
            self.focus_omnibox_if_new_tab(window, cx);
            cx.notify();
        }
    }
}

impl EventEmitter<()> for BrowserView {}

impl Focusable for BrowserView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BrowserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.cef_available {
            return div()
                .id("browser-view")
                .track_focus(&self.focus_handle)
                .size_full()
                .child(self.render_placeholder(cx))
                .into_any_element();
        }

        if self.toolbar.is_none() && !self.tabs.is_empty() {
            cx.defer_in(window, |this, window, cx| {
                this.create_toolbar(window, cx);
            });
        }

        if let Some(toolbar) = self.toolbar.clone() {
            ModeViewRegistry::global_mut(cx)
                .set_titlebar_center_view(ModeId::BROWSER, toolbar.into());
        }

        if !self.pending_new_tab_urls.is_empty() {
            let urls: Vec<String> = std::mem::take(&mut self.pending_new_tab_urls);
            for url in urls {
                self.add_tab_in_background(&url, cx);
            }
        }

        if let Some(pending) = self.pending_context_menu.take() {
            self.open_context_menu(pending.context, window, cx);
        }

        let scale_factor = window.scale_factor();

        let actual_width = f32::from(self.content_bounds.size.width);
        let actual_height = f32::from(self.content_bounds.size.height);
        let has_actual_bounds = actual_width > 0.0 && actual_height > 0.0;

        let (content_width, content_height) = if has_actual_bounds {
            (actual_width as u32, actual_height as u32)
        } else {
            let viewport_size = window.viewport_size();
            (
                f32::from(viewport_size.width) as u32,
                f32::from(viewport_size.height) as u32,
            )
        };

        if content_width > 0 && content_height > 0 {
            if !self.message_pump_started {
                self.ensure_browser_created(content_width, content_height, scale_factor, cx);
                if !self.message_pump_started {
                    cx.notify();
                }
            } else {
                let scale_key = (scale_factor * 1000.0) as u32;
                let new_viewport = (content_width, content_height, scale_key);
                if self.last_viewport != Some(new_viewport) {
                    self.last_viewport = Some(new_viewport);
                    if let Some(tab) = self.active_tab() {
                        tab.update(cx, |tab, _| {
                            tab.set_scale_factor(scale_factor);
                            tab.set_size(content_width, content_height);
                        });
                    }
                }
            }
        }

        let element = div()
            .id("browser-view")
            .track_focus(&self.focus_handle)
            .key_context("BrowserView")
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_key_up(cx.listener(Self::handle_key_up))
            .on_action(cx.listener(Self::handle_copy))
            .on_action(cx.listener(Self::handle_cut))
            .on_action(cx.listener(Self::handle_paste))
            .on_action(cx.listener(Self::handle_undo))
            .on_action(cx.listener(Self::handle_redo))
            .on_action(cx.listener(Self::handle_select_all))
            .on_action(cx.listener(Self::handle_new_tab))
            .on_action(cx.listener(Self::handle_close_tab))
            .on_action(cx.listener(Self::handle_reopen_closed_tab))
            .on_action(cx.listener(Self::handle_next_tab))
            .on_action(cx.listener(Self::handle_previous_tab))
            .on_action(cx.listener(Self::handle_focus_omnibox))
            .on_action(cx.listener(Self::handle_reload))
            .on_action(cx.listener(Self::handle_go_back))
            .on_action(cx.listener(Self::handle_go_forward))
            .on_action(cx.listener(Self::handle_open_devtools))
            .on_action(cx.listener(Self::handle_bookmark_current_page))
            .on_action(cx.listener(Self::handle_copy_url))
            .on_action(cx.listener(Self::handle_toggle_sidebar))
            .size_full()
            .flex();

        let element = match self.tab_bar_mode {
            TabBarMode::Horizontal => element
                .flex_col()
                .child(self.render_tab_strip(cx))
                .child(self.bookmark_bar.clone())
                .child(self.render_browser_content(cx))
                .into_any_element(),
            TabBarMode::Sidebar => element
                .flex_row()
                .child(self.render_sidebar(cx))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(self.bookmark_bar.clone())
                        .child(self.render_browser_content(cx)),
                )
                .into_any_element(),
        };

        div()
            .size_full()
            .relative()
            .child(element)
            .child(self.toast_layer.clone())
            .into_any_element()
    }
}
