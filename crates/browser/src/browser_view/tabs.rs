use crate::cef_instance::CefInstance;
use crate::session::SerializedTab;
use crate::tab::BrowserTab;
use gpui::{App, AppContext as _, Context, Entity, Task, Window};
use std::time::Duration;

use super::{
    BrowserView, CloseTab, MAX_CLOSED_TABS, NewTab, NextTab, PreviousTab, ReopenClosedTab,
    TabBarMode, ToggleSidebar,
};

impl BrowserView {
    pub(super) fn add_tab(&mut self, cx: &mut Context<Self>) {
        let tab = cx.new(|cx| BrowserTab::new(cx));
        self.configure_tab_request_context(&tab, cx);

        let subscription = cx.subscribe(&tab, Self::handle_tab_event);
        self._subscriptions.push(subscription);

        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        self.schedule_save(cx);
    }

    pub fn open_url(&mut self, url: &str, cx: &mut Context<Self>) {
        log::info!(
            "[default-browser] BrowserView::open_url called with: {}, message_pump_started: {}, last_viewport: {:?}",
            url,
            self.message_pump_started,
            self.last_viewport
        );
        let tab = cx.new(|cx| {
            let mut tab = BrowserTab::new(cx);
            tab.set_new_tab_page(false);
            tab.set_pending_url(url.to_string());
            tab
        });
        self.configure_tab_request_context(&tab, cx);
        let subscription = cx.subscribe(&tab, Self::handle_tab_event);
        self._subscriptions.push(subscription);

        let tab_ref = tab.clone();
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;

        if self.message_pump_started {
            self.create_browser_and_navigate(&tab_ref, url, cx);
            tab_ref.update(cx, |tab: &mut BrowserTab, _| {
                tab.take_pending_url();
            });
        }

        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn add_tab_in_background(&mut self, url: &str, cx: &mut Context<Self>) {
        let tab = cx.new(|cx| {
            let mut tab = BrowserTab::new(cx);
            tab.set_new_tab_page(false);
            tab.set_pending_url(url.to_string());
            tab
        });
        self.configure_tab_request_context(&tab, cx);
        let subscription = cx.subscribe(&tab, Self::handle_tab_event);
        self._subscriptions.push(subscription);
        self.tabs.push(tab);

        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn create_browser_and_navigate(
        &mut self,
        tab_entity: &Entity<BrowserTab>,
        url: &str,
        cx: &mut Context<Self>,
    ) {
        let (width, height, scale_factor) = if let Some(vp) = self.last_viewport {
            (vp.0, vp.1, vp.2 as f32 / 1000.0)
        } else {
            return;
        };

        if width == 0 || height == 0 {
            return;
        }

        let url = url.to_string();
        let request_context = self.request_context_for_new_tab();
        tab_entity.update(cx, |tab, _| {
            tab.set_request_context(request_context);
            tab.set_new_tab_page(false);
            tab.set_scale_factor(scale_factor);
            tab.set_size(width, height);
            if let Err(e) = tab.create_browser(&url) {
                log::error!("[browser] Failed to create browser for tab: {}", e);
                return;
            }
            tab.set_focus(true);
            tab.invalidate();
        });

        if !self.message_pump_started {
            self._message_pump_task = Some(Self::start_message_pump(cx));
            self.message_pump_started = true;
        }

        self.sync_bookmark_bar_visibility(cx);
        cx.notify();
    }

    pub(super) fn activate_tab_for_close(&mut self, cx: &mut Context<Self>) {
        let Some(new_tab) = self.active_tab().cloned() else {
            return;
        };

        if new_tab.read(cx).is_new_tab_page() {
            return;
        }

        if new_tab.read(cx).has_pending_url() {
            let url = new_tab.read(cx).url().to_string();
            self.create_browser_and_navigate(&new_tab, &url, cx);
            new_tab.update(cx, |tab, _| {
                tab.take_pending_url();
                tab.set_hidden(false);
                tab.set_focus(true);
            });
        } else {
            let viewport = self.last_viewport;
            new_tab.update(cx, |tab, _| {
                if tab.current_frame().is_none() {
                    if let Some((width, height, scale_key)) = viewport {
                        if width > 0 && height > 0 {
                            let scale_factor = scale_key as f32 / 1000.0;
                            tab.set_scale_factor(scale_factor);
                            tab.set_size(width, height);
                            let url = tab.url().to_string();
                            if let Err(e) = tab.create_browser(&url) {
                                log::error!(
                                    "[browser] Failed to create browser after tab close: {}",
                                    e
                                );
                                return;
                            }
                        }
                    }
                }
                tab.set_hidden(false);
                tab.set_focus(true);
            });
        }
    }

    pub(super) fn current_dimensions(&self, window: &mut Window) -> (u32, u32, f32) {
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

    pub(super) fn switch_to_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return;
        }

        if let Some(old_tab) = self.active_tab().cloned() {
            self.clear_find_for_tab_switch(&old_tab, window, cx);
            old_tab.update(cx, |tab, _| {
                tab.set_focus(false);
                tab.set_hidden(true);
            });
        }

        self.active_tab_index = index;

        if let Some(new_tab) = self.active_tab() {
            let is_new_tab_page = new_tab.read(cx).is_new_tab_page();
            if !is_new_tab_page {
                let has_pending = new_tab.read(cx).has_pending_url();
                if has_pending {
                    let new_tab = new_tab.clone();
                    let url = new_tab.read(cx).url().to_string();
                    self.create_browser_and_navigate(&new_tab, &url, cx);
                    new_tab.update(cx, |tab, _| {
                        tab.take_pending_url();
                        tab.set_hidden(false);
                        tab.set_focus(true);
                    });
                } else {
                    let (width, height, scale_factor) = self.current_dimensions(window);
                    new_tab.update(cx, |tab, _| {
                        if tab.current_frame().is_none() && width > 0 && height > 0 {
                            tab.set_scale_factor(scale_factor);
                            tab.set_size(width, height);
                            let url = tab.url().to_string();
                            if let Err(e) = tab.create_browser(&url) {
                                log::error!(
                                    "[browser] Failed to create browser on tab switch: {}",
                                    e
                                );
                                return;
                            }
                        }
                        tab.set_hidden(false);
                        tab.set_focus(true);
                    });
                }
            }
        }

        self.update_toolbar_active_tab(window, cx);
        self.focus_omnibox_if_new_tab(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn ensure_browser_created(
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
            let is_new_tab_page = tab.read(cx).is_new_tab_page();
            if !is_new_tab_page {
                tab.update(cx, |tab, _| {
                    tab.set_scale_factor(scale_factor);
                    tab.set_size(width, height);
                    let url = tab.url().to_string();
                    if let Err(e) = tab.create_browser(&url) {
                        log::error!("[browser] Failed to create browser: {}", e);
                        return;
                    }
                    tab.set_focus(true);
                    tab.invalidate();
                });
            }
            self.last_viewport = Some((width, height, (scale_factor * 1000.0) as u32));
            if !self.message_pump_started {
                self._message_pump_task = Some(Self::start_message_pump(cx));
                self.message_pump_started = true;
            }
        }
    }

    pub(super) fn start_message_pump(cx: &mut Context<Self>) -> Task<()> {
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

    // --- Tab action handlers ---

    pub(super) fn handle_new_tab(
        &mut self,
        _: &NewTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_tab(cx);
        self.update_toolbar_active_tab(window, cx);
        self.focus_omnibox_if_new_tab(window, cx);
        cx.notify();
    }

    fn push_closed_tab(&mut self, tab: &Entity<BrowserTab>, cx: &App) {
        let tab = tab.read(cx);
        let url = tab.url().to_string();
        if url == "glass://newtab" || url.is_empty() {
            return;
        }
        self.closed_tabs.push(SerializedTab {
            url,
            title: tab.title().to_string(),
            is_new_tab_page: tab.is_new_tab_page(),
            is_pinned: tab.is_pinned(),
            favicon_url: tab.favicon_url().map(|s| s.to_string()),
        });
        if self.closed_tabs.len() > MAX_CLOSED_TABS {
            self.closed_tabs.remove(0);
        }
    }

    pub(super) fn handle_close_tab(
        &mut self,
        _: &CloseTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab() {
            if tab.read(cx).is_pinned() {
                return;
            }
        }

        if let Some(tab) = self.active_tab().cloned() {
            self.clear_find_for_tab_switch(&tab, window, cx);
        }

        if let Some(tab) = self.active_tab().cloned() {
            self.push_closed_tab(&tab, cx);
        }

        if let Some(tab) = self.active_tab().cloned() {
            tab.update(cx, |tab, _| {
                tab.close_browser();
            });
        }

        if self.tabs.len() <= 1 {
            self.tabs.pop();
            self.active_tab_index = 0;
            self.add_tab(cx);

            self.update_toolbar_active_tab(window, cx);
            self.focus_omnibox_if_new_tab(window, cx);
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

        self.activate_tab_for_close(cx);

        self.update_toolbar_active_tab(window, cx);
        self.focus_omnibox_if_new_tab(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn handle_reopen_closed_tab(
        &mut self,
        _: &ReopenClosedTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let closed = match self.closed_tabs.pop() {
            Some(closed) => closed,
            None => return,
        };

        let url = closed.url.clone();
        let title = closed.title.clone();
        let favicon_url = closed.favicon_url.clone();

        let tab = cx.new(|cx| BrowserTab::new_with_state(url, title, false, favicon_url, cx));
        self.configure_tab_request_context(&tab, cx);
        let subscription = cx.subscribe(&tab, Self::handle_tab_event);
        self._subscriptions.push(subscription);
        self.tabs.push(tab.clone());
        self.active_tab_index = self.tabs.len() - 1;

        let url = closed.url;
        self.create_browser_and_navigate(&tab, &url, cx);
        self.update_toolbar_active_tab(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn handle_next_tab(
        &mut self,
        _: &NextTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.len() <= 1 {
            return;
        }
        let next_index = (self.active_tab_index + 1) % self.tabs.len();
        self.switch_to_tab(next_index, window, cx);
    }

    pub(super) fn handle_previous_tab(
        &mut self,
        _: &PreviousTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn handle_toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.tab_bar_mode {
            TabBarMode::Horizontal => {
                self.tab_bar_mode = TabBarMode::Sidebar;
                self.sidebar_collapsed = false;
            }
            TabBarMode::Sidebar => {
                self.tab_bar_mode = TabBarMode::Horizontal;
                self.sidebar_collapsed = false;
            }
        }
        self.hovered_top_tab_index = None;
        self.hovered_top_tab_close_index = None;
        self.hovered_top_new_tab_button = false;
        self.hovered_sidebar_tab_index = None;
        self.hovered_sidebar_tab_close_index = None;
        self.hovered_sidebar_new_tab_button = false;
        if let Some(native_sidebar_panel) = &self.native_sidebar_panel {
            native_sidebar_panel.update(cx, |sidebar_panel, cx| {
                sidebar_panel.clear_hover_state(cx);
            });
        }
        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn close_tab_at(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index == self.active_tab_index {
            if let Some(tab) = self.active_tab().cloned() {
                self.clear_find_for_tab_switch(&tab, window, cx);
            }
        }

        self.close_tab_at_inner(index, cx);
        self.update_toolbar_active_tab(window, cx);
    }

    pub(super) fn close_tab_at_inner(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }

        let was_active = index == self.active_tab_index;

        self.push_closed_tab(&self.tabs[index].clone(), cx);

        self.tabs[index].update(cx, |tab, _| {
            tab.close_browser();
        });

        if self.tabs.len() <= 1 {
            self.tabs.pop();
            self.active_tab_index = 0;
            self.add_tab(cx);

            self.sync_bookmark_bar_visibility(cx);
            self.schedule_save(cx);
            cx.notify();
            return;
        }

        self.tabs.remove(index);

        if index < self.active_tab_index {
            self.active_tab_index -= 1;
        } else if was_active {
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len() - 1;
            }
            self.activate_tab_for_close(cx);
        }

        if self.hovered_top_tab_index == Some(index) {
            self.hovered_top_tab_index = None;
        }
        if self.hovered_top_tab_close_index == Some(index) {
            self.hovered_top_tab_close_index = None;
        }
        if self.hovered_sidebar_tab_index == Some(index) {
            self.hovered_sidebar_tab_index = None;
        }
        if self.hovered_sidebar_tab_close_index == Some(index) {
            self.hovered_sidebar_tab_close_index = None;
        }
        if let Some(native_sidebar_panel) = &self.native_sidebar_panel {
            native_sidebar_panel.update(cx, |sidebar_panel, cx| {
                sidebar_panel.clear_hover_state(cx);
            });
        }

        self.sync_bookmark_bar_visibility(cx);
        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn sort_tabs_pinned_first(&mut self, cx: &App) {
        self.tabs.sort_by_key(|tab| !tab.read(cx).is_pinned());
    }

    pub(super) fn pin_tab_at(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs[index].update(cx, |tab, _| {
            tab.set_pinned(true);
        });

        let active_tab = self.active_tab().cloned();
        self.sort_tabs_pinned_first(cx);

        if let Some(active) = active_tab {
            if let Some(new_index) = self.tabs.iter().position(|t| t == &active) {
                self.active_tab_index = new_index;
            }
        }

        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn unpin_tab_at(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs[index].update(cx, |tab, _| {
            tab.set_pinned(false);
        });

        let active_tab = self.active_tab().cloned();
        self.sort_tabs_pinned_first(cx);

        if let Some(active) = active_tab {
            if let Some(new_index) = self.tabs.iter().position(|t| t == &active) {
                self.active_tab_index = new_index;
            }
        }

        self.schedule_save(cx);
        cx.notify();
    }

    pub(super) fn close_other_tabs_at(&mut self, keep_index: usize, cx: &mut Context<Self>) {
        if keep_index >= self.tabs.len() {
            return;
        }
        let keep_tab = self.tabs[keep_index].clone();
        for tab in &self.tabs {
            if tab != &keep_tab && !tab.read(cx).is_pinned() {
                tab.update(cx, |tab, _| {
                    tab.close_browser();
                });
            }
        }
        self.tabs
            .retain(|tab| tab == &keep_tab || tab.read(cx).is_pinned());
        if let Some(new_index) = self.tabs.iter().position(|t| t == &keep_tab) {
            self.active_tab_index = new_index;
        } else {
            self.active_tab_index = 0;
        }
        self.activate_tab_for_close(cx);
        self.sync_bookmark_bar_visibility(cx);
        self.schedule_save(cx);
        cx.notify();
    }
}
