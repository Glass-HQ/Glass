//! Browser View
//!
//! The main view for Browser Mode. Renders CEF browser content and handles
//! user input for navigation and interaction. Supports multiple tabs.

use crate::cef_instance::CefInstance;
use crate::context_menu_handler::ContextMenuContext;
use crate::history::BrowserHistory;
use crate::input;
use crate::session::{self, SerializedBrowserTabs, SerializedTab};
use crate::tab::{BrowserTab, TabEvent};
use crate::toolbar::BrowserToolbar;
use gpui::{
    actions, anchored, canvas, deferred, div, point, prelude::*, surface, App, Bounds, Context,
    Corner, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, MouseButton, ObjectFit, ParentElement, Pixels, Point, Render, Styled,
    Subscription, Task, Window,
};
use std::time::Duration;
use ui::{prelude::*, Icon, IconButton, IconName, IconSize, Tooltip};
use util::ResultExt as _;
use workspace_modes::{ModeId, ModeViewRegistry};

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
        NextTab,
        PreviousTab,
    ]
);

const DEFAULT_URL: &str = "https://www.google.com";

struct BrowserContextMenu {
    menu: Entity<ui::ContextMenu>,
    position: Point<Pixels>,
    _dismiss_subscription: Subscription,
}

struct PendingContextMenu {
    context: ContextMenuContext,
}

pub struct BrowserView {
    focus_handle: FocusHandle,
    tabs: Vec<Entity<BrowserTab>>,
    active_tab_index: usize,
    toolbar: Option<Entity<BrowserToolbar>>,
    history: Entity<BrowserHistory>,
    content_bounds: Bounds<Pixels>,
    cef_available: bool,
    message_pump_started: bool,
    last_viewport: Option<(u32, u32, u32)>,
    pending_new_tab_urls: Vec<String>,
    context_menu: Option<BrowserContextMenu>,
    pending_context_menu: Option<PendingContextMenu>,
    _message_pump_task: Option<Task<()>>,
    _schedule_save: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl BrowserView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let cef_available = CefInstance::global().is_some();

        let quit_subscription = cx.on_app_quit(Self::save_tabs_on_quit);
        let history = cx.new(|cx| BrowserHistory::new(cx));

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            tabs: Vec::new(),
            active_tab_index: 0,
            toolbar: None,
            history,
            content_bounds: Bounds::default(),
            cef_available,
            message_pump_started: false,
            last_viewport: None,
            pending_new_tab_urls: Vec::new(),
            context_menu: None,
            pending_context_menu: None,
            _message_pump_task: None,
            _schedule_save: None,
            _subscriptions: vec![quit_subscription],
        };

        if cef_available {
            let restored = this.restore_tabs(cx);
            if !restored {
                this.add_tab(cx);
            }
        }

        this
    }

    fn active_tab(&self) -> Option<&Entity<BrowserTab>> {
        self.tabs.get(self.active_tab_index)
    }

    fn restore_tabs(&mut self, cx: &mut Context<Self>) -> bool {
        let saved = match session::restore() {
            Some(saved) if !saved.tabs.is_empty() => saved,
            _ => return false,
        };

        for serialized_tab in &saved.tabs {
            let url = serialized_tab.url.clone();
            let title = serialized_tab.title.clone();
            let tab = cx.new(|cx| BrowserTab::new_with_state(url, title, cx));
            let subscription = cx.subscribe(&tab, Self::handle_tab_event);
            self._subscriptions.push(subscription);
            self.tabs.push(tab);
        }

        self.active_tab_index = saved.active_index.min(self.tabs.len().saturating_sub(1));
        true
    }

    fn serialize_tabs(&self, cx: &App) -> Option<String> {
        if self.tabs.is_empty() {
            return None;
        }

        let tabs: Vec<SerializedTab> = self
            .tabs
            .iter()
            .map(|tab| {
                let tab = tab.read(cx);
                SerializedTab {
                    url: tab.url().to_string(),
                    title: tab.title().to_string(),
                }
            })
            .collect();

        let data = SerializedBrowserTabs {
            tabs,
            active_index: self.active_tab_index,
        };

        serde_json::to_string(&data).log_err()
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self._schedule_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;

            let (tabs_json, history_json) = this
                .read_with(cx, |this, cx| {
                    (
                        this.serialize_tabs(cx),
                        this.history.read(cx).serialize(),
                    )
                })
                .ok()
                .unwrap_or((None, None));

            if let Some(json) = tabs_json {
                session::save(json).await.log_err();
            }
            if let Some(json) = history_json {
                session::save_history(json).await.log_err();
            }

            this.update(cx, |this, _| {
                this._schedule_save.take();
            })
            .ok();
        }));
    }

    fn save_tabs_on_quit(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let tabs_json = self.serialize_tabs(cx);
        let history_json = self.history.read(cx).serialize();
        cx.background_spawn(async move {
            if let Some(json) = tabs_json {
                session::save(json).await.log_err();
            }
            if let Some(json) = history_json {
                session::save_history(json).await.log_err();
            }
        })
    }

    fn add_tab(&mut self, cx: &mut Context<Self>) {
        let tab = cx.new(|cx| BrowserTab::new(cx));

        let subscription = cx.subscribe(&tab, Self::handle_tab_event);
        self._subscriptions.push(subscription);

        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        self.schedule_save(cx);
    }

    fn add_tab_with_url(
        &mut self,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_tab(cx);

        if let Some(tab) = self.active_tab() {
            let (width, height, scale_factor) = self.current_dimensions(window);
            if width > 0 && height > 0 {
                tab.update(cx, |tab, _| {
                    tab.set_scale_factor(scale_factor);
                    tab.set_size(width, height);
                    if let Err(e) = tab.create_browser(url) {
                        log::error!("[browser] Failed to create browser for new tab: {}", e);
                        return;
                    }
                    tab.set_focus(true);
                    tab.invalidate();
                });
            }
        }

        self.update_toolbar_active_tab(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    fn current_dimensions(&self, window: &mut Window) -> (u32, u32, f32) {
        let scale_factor = window.scale_factor();
        let actual_width = f32::from(self.content_bounds.size.width);
        let actual_height = f32::from(self.content_bounds.size.height);

        if actual_width > 0.0 && actual_height > 0.0 {
            (actual_width as u32, actual_height as u32, scale_factor)
        } else {
            let viewport_size = window.viewport_size();
            (
                f32::from(viewport_size.width) as u32,
                f32::from(viewport_size.height) as u32,
                scale_factor,
            )
        }
    }

    fn switch_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return;
        }

        // Unfocus old tab
        if let Some(old_tab) = self.active_tab() {
            old_tab.update(cx, |tab, _| {
                tab.set_focus(false);
            });
        }

        self.active_tab_index = index;

        // Focus new tab, ensure browser is created
        if let Some(new_tab) = self.active_tab() {
            let (width, height, scale_factor) = self.current_dimensions(window);
            new_tab.update(cx, |tab, _| {
                if tab.current_frame().is_none() && width > 0 && height > 0 {
                    tab.set_scale_factor(scale_factor);
                    tab.set_size(width, height);
                    let url = if tab.url() != "about:blank" {
                        tab.url().to_string()
                    } else {
                        DEFAULT_URL.to_string()
                    };
                    if let Err(e) = tab.create_browser(&url) {
                        log::error!("[browser] Failed to create browser on tab switch: {}", e);
                        return;
                    }
                }
                tab.set_focus(true);
            });
        }

        self.update_toolbar_active_tab(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    fn update_toolbar_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(toolbar), Some(tab)) = (self.toolbar.clone(), self.active_tab().cloned()) {
            toolbar.update(cx, |toolbar, cx| {
                toolbar.set_active_tab(tab, window, cx);
            });
        }
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
                if let Some(tab) = self.active_tab() {
                    let url = url.clone();
                    tab.update(cx, |tab, _| {
                        tab.navigate(&url);
                    });
                }
            }
            TabEvent::OpenNewTab(url) => {
                self.pending_new_tab_urls.push(url.clone());
                cx.notify();
            }
            TabEvent::AddressChanged(_) | TabEvent::TitleChanged(_) => {
                // Defer history recording: the tab entity is still mutably borrowed
                // by drain_events() when this handler fires, so we can't read it here.
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
            TabEvent::LoadingStateChanged => {
                cx.notify();
            }
            TabEvent::LoadError { url, error_text, .. } => {
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

    fn open_context_menu(
        &mut self,
        context: ContextMenuContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = window.mouse_position();
        let tab = match self.active_tab().cloned() {
            Some(tab) => tab,
            None => return,
        };

        let menu = ui::ContextMenu::build(window, cx, move |mut menu, _window, _cx| {
            let has_link = context.link_url.is_some();
            let has_selection = context.selection_text.is_some();

            // Link actions
            if let Some(link_url) = &context.link_url {
                let url = link_url.clone();
                let tab = tab.clone();
                menu = menu.entry("Open Link in New Tab", None, move |_window, cx| {
                    tab.update(cx, |_, cx| {
                        cx.emit(TabEvent::OpenNewTab(url.clone()));
                    });
                });

                let url = link_url.clone();
                menu = menu.entry("Copy Link Address", None, move |_window, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(url.clone()));
                });

                menu = menu.separator();
            }

            // Edit actions for editable fields
            if context.is_editable {
                if context.can_undo {
                    let tab = tab.clone();
                    menu = menu.entry("Undo", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.undo());
                    });
                }
                if context.can_redo {
                    let tab = tab.clone();
                    menu = menu.entry("Redo", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.redo());
                    });
                }
                if context.can_undo || context.can_redo {
                    menu = menu.separator();
                }
                if context.can_cut {
                    let tab = tab.clone();
                    menu = menu.entry("Cut", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.cut());
                    });
                }
                if context.can_copy {
                    let tab = tab.clone();
                    menu = menu.entry("Copy", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.copy());
                    });
                }
                if context.can_paste {
                    let tab = tab.clone();
                    menu = menu.entry("Paste", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.paste());
                    });
                }
                if context.can_delete {
                    let tab = tab.clone();
                    menu = menu.entry("Delete", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.delete());
                    });
                }
                menu = menu.separator();
                if context.can_select_all {
                    let tab = tab.clone();
                    menu = menu.entry("Select All", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.select_all());
                    });
                }
            } else {
                // Non-editable: just copy if there's a selection
                if has_selection {
                    let tab = tab.clone();
                    menu = menu.entry("Copy", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.copy());
                    });
                    menu = menu.separator();
                }
            }

            // Navigation (only when not on a link or selection)
            if !has_link && !has_selection && !context.is_editable {
                {
                    let tab = tab.clone();
                    menu = menu.entry("Back", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.go_back());
                    });
                }
                {
                    let tab = tab.clone();
                    menu = menu.entry("Forward", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.go_forward());
                    });
                }
                {
                    let tab = tab.clone();
                    menu = menu.entry("Reload", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.reload());
                    });
                }
                menu = menu.separator();
            }

            // Always show Inspect
            {
                menu = menu.entry("Inspect", None, move |_window, cx| {
                    tab.update(cx, |tab, _| tab.open_devtools());
                });
            }

            menu
        });

        let dismiss_subscription = cx.subscribe(&menu, {
            move |this, _, _event: &DismissEvent, cx| {
                this.context_menu.take();
                cx.notify();
            }
        });

        self.context_menu = Some(BrowserContextMenu {
            menu,
            position,
            _dismiss_subscription: dismiss_subscription,
        });

        cx.notify();
    }

    fn start_message_pump(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                if this.upgrade().is_none() {
                    break;
                }

                if CefInstance::should_pump() {
                    CefInstance::pump_messages();

                    let _ = cx.update(|cx| {
                        if let Some(this) = this.upgrade() {
                            this.update(cx, |view, cx| {
                                for tab in &view.tabs {
                                    tab.update(cx, |tab, cx| {
                                        tab.drain_events(cx);
                                    });
                                }
                            });
                        }
                    });
                }

                let wait_us = CefInstance::time_until_next_pump_us();
                let sleep_us = wait_us.clamp(500, 4_000);
                cx.background_executor()
                    .timer(Duration::from_micros(sleep_us))
                    .await;
            }
        })
    }

    fn create_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab().cloned() {
            let history = self.history.clone();
            let toolbar = cx.new(|cx| BrowserToolbar::new(tab, history, window, cx));
            self.toolbar = Some(toolbar.clone());

            ModeViewRegistry::global_mut(cx)
                .set_titlebar_center_view(ModeId::BROWSER, toolbar.into());
            cx.notify();
        }
    }

    fn ensure_browser_created(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        cx: &mut Context<Self>,
    ) {
        if !CefInstance::is_context_ready() {
            return;
        }

        if let Some(tab) = self.active_tab() {
            tab.update(cx, |tab, _| {
                tab.set_scale_factor(scale_factor);
                tab.set_size(width, height);
                let url = if tab.url() != "about:blank" {
                    tab.url().to_string()
                } else {
                    DEFAULT_URL.to_string()
                };
                if let Err(e) = tab.create_browser(&url) {
                    log::error!("[browser] Failed to create browser: {}", e);
                    return;
                }
                tab.set_focus(true);
                tab.invalidate();
            });
            self.last_viewport = Some((width, height, (scale_factor * 1000.0) as u32));
            if !self.message_pump_started {
                self._message_pump_task = Some(Self::start_message_pump(cx));
                self.message_pump_started = true;
            }
        }
    }

    fn handle_mouse_down(
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

    fn dismiss_context_menu(&mut self) {
        if let Some(cm) = self.context_menu.take() {
            drop(cm);
        }
    }

    fn handle_mouse_up(
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

    fn handle_mouse_move(
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

    fn handle_scroll(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_scroll_wheel(&tab.read(cx), event, offset);
        }
    }

    fn handle_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!("[browser::view] handle_key_down called (key={}, is_held={})",
            event.keystroke.key, event.is_held);
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

    fn handle_key_up(
        &mut self,
        event: &gpui::KeyUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!("[browser::view] handle_key_up called (key={})", event.keystroke.key);
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

    fn handle_copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).copy();
        }
    }

    fn handle_cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).cut();
        }
    }

    fn handle_paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).paste();
        }
    }

    fn handle_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).undo();
        }
    }

    fn handle_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).redo();
        }
    }

    fn handle_select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            tab.read(cx).select_all();
        }
    }

    fn handle_new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.add_tab_with_url(DEFAULT_URL, window, cx);
    }

    fn handle_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() <= 1 {
            // Always keep at least one tab — replace with a fresh one
            if let Some(tab) = self.tabs.pop() {
                drop(tab);
            }
            self.active_tab_index = 0;
            self.add_tab(cx);

            if let Some(tab) = self.active_tab() {
                let (width, height, scale_factor) = self.current_dimensions(window);
                if width > 0 && height > 0 {
                    tab.update(cx, |tab, _| {
                        tab.set_scale_factor(scale_factor);
                        tab.set_size(width, height);
                        if let Err(e) = tab.create_browser(DEFAULT_URL) {
                            log::error!("[browser] Failed to create replacement tab: {}", e);
                            return;
                        }
                        tab.set_focus(true);
                        tab.invalidate();
                    });
                }
            }

            self.update_toolbar_active_tab(window, cx);
            self.schedule_save(cx);
            cx.notify();
            return;
        }

        let closed_index = self.active_tab_index;
        self.tabs.remove(closed_index);

        if closed_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        } else {
            self.active_tab_index = closed_index;
        }

        // Focus the new active tab
        if let Some(tab) = self.active_tab() {
            tab.update(cx, |tab, _| {
                tab.set_focus(true);
            });
        }

        self.update_toolbar_active_tab(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    fn handle_next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() <= 1 {
            return;
        }
        let next_index = (self.active_tab_index + 1) % self.tabs.len();
        self.switch_to_tab(next_index, window, cx);
    }

    fn handle_previous_tab(&mut self, _: &PreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() <= 1 {
            return;
        }
        let previous_index = if self.active_tab_index == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab_index - 1
        };
        self.switch_to_tab(previous_index, window, cx);
    }

    fn close_tab_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() <= 1 {
            // Last tab — replace with fresh
            if let Some(tab) = self.tabs.pop() {
                drop(tab);
            }
            self.active_tab_index = 0;
            self.add_tab(cx);

            if let Some(tab) = self.active_tab() {
                let (width, height, scale_factor) = self.current_dimensions(window);
                if width > 0 && height > 0 {
                    tab.update(cx, |tab, _| {
                        tab.set_scale_factor(scale_factor);
                        tab.set_size(width, height);
                        if let Err(e) = tab.create_browser(DEFAULT_URL) {
                            log::error!("[browser] Failed to create replacement tab: {}", e);
                            return;
                        }
                        tab.set_focus(true);
                        tab.invalidate();
                    });
                }
            }

            self.update_toolbar_active_tab(window, cx);
            self.schedule_save(cx);
            cx.notify();
            return;
        }

        self.tabs.remove(index);

        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        } else if index < self.active_tab_index {
            self.active_tab_index -= 1;
        } else if index == self.active_tab_index {
            // Active tab was closed; clamp and focus replacement
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len() - 1;
            }
            if let Some(tab) = self.active_tab() {
                tab.update(cx, |tab, _| {
                    tab.set_focus(true);
                });
            }
        }

        self.update_toolbar_active_tab(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    fn render_placeholder(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(theme.colors().editor_background)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        Icon::new(IconName::Globe)
                            .size(IconSize::Custom(rems(6.0)))
                            .color(Color::Muted),
                    )
                    .child(
                        div()
                            .text_color(theme.colors().text_muted)
                            .text_size(rems(1.0))
                            .child("Browser"),
                    )
                    .child(
                        div()
                            .text_color(theme.colors().text_muted)
                            .text_size(rems(0.875))
                            .max_w(px(400.))
                            .text_center()
                            .child("CEF is not initialized. Set CEF_PATH environment variable and restart."),
                    ),
            )
    }

    fn render_tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active_index = self.active_tab_index;

        h_flex()
            .w_full()
            .h(px(30.))
            .flex_shrink_0()
            .bg(theme.colors().title_bar_background)
            .border_b_1()
            .border_color(theme.colors().border)
            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                let title = tab.read(cx).title().to_string();
                let is_active = index == active_index;
                let display_title = if title.len() > 24 {
                    let truncated = match title.char_indices().nth(21) {
                        Some((byte_index, _)) => &title[..byte_index],
                        None => &title,
                    };
                    format!("{truncated}...")
                } else {
                    title
                };

                div()
                    .id(("browser-tab", index))
                    .flex()
                    .items_center()
                    .h_full()
                    .px_2()
                    .gap_1()
                    .min_w(px(80.))
                    .max_w(px(200.))
                    .border_r_1()
                    .border_color(theme.colors().border)
                    .cursor_pointer()
                    .when(is_active, |this| {
                        this.bg(theme.colors().editor_background)
                    })
                    .when(!is_active, |this| {
                        this.hover(|style| style.bg(theme.colors().ghost_element_hover))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.switch_to_tab(index, window, cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_size(rems(0.75))
                            .text_color(if is_active {
                                theme.colors().text
                            } else {
                                theme.colors().text_muted
                            })
                            .child(display_title)
                    )
                    .child(
                        IconButton::new(("close-tab", index), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_tab_at(index, window, cx);
                            }))
                            .tooltip(Tooltip::text("Close Tab")),
                    )
            }))
            .child(
                IconButton::new("new-tab-button", IconName::Plus)
                    .icon_size(IconSize::XSmall)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_tab_with_url(DEFAULT_URL, window, cx);
                    }))
                    .tooltip(Tooltip::text("New Tab")),
            )
    }

    fn render_browser_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let current_frame = self
            .active_tab()
            .and_then(|t| t.read(cx).current_frame());

        let has_frame = current_frame.is_some();

        let this = cx.entity();
        let bounds_tracker = canvas(
            move |bounds, _window, cx| {
                this.update(cx, |view, _| {
                    view.content_bounds = bounds;
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let context_menu_overlay = self.context_menu.as_ref().map(|cm| {
            deferred(
                anchored()
                    .position(cm.position)
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(cm.menu.clone()),
            )
            .with_priority(1)
        });

        let element = div()
            .id("browser-content")
            .relative()
            .flex_1()
            .w_full()
            .bg(theme.colors().editor_background)
            .child(bounds_tracker)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::handle_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::handle_mouse_up))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_scroll_wheel(cx.listener(Self::handle_scroll))
            .when_some(current_frame, |this, frame| {
                this.child(surface(frame).size_full().object_fit(ObjectFit::Fill))
            })
            .when(!has_frame, |this| {
                this.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_color(theme.colors().text_muted)
                                .child("Loading..."),
                        ),
                )
            })
            .when_some(context_menu_overlay, |this, overlay| {
                this.child(overlay)
            });

        element
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

        // Process any pending new tab URLs queued from OpenNewTab events
        if !self.pending_new_tab_urls.is_empty() {
            let urls: Vec<String> = std::mem::take(&mut self.pending_new_tab_urls);
            for url in urls {
                self.add_tab_with_url(&url, window, cx);
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
            .on_action(cx.listener(Self::handle_next_tab))
            .on_action(cx.listener(Self::handle_previous_tab))
            .size_full()
            .flex()
            .flex_col()
            .child(self.render_tab_strip(cx))
            .child(self.render_browser_content(cx))
            .into_any_element();

        element
    }
}
