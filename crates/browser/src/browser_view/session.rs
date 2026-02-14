use crate::session::{self, SerializedBrowserTabs, SerializedTab};
use crate::tab::BrowserTab;
use gpui::{App, AppContext as _, Context, Task};
use std::time::Duration;
use util::ResultExt as _;

use super::{BrowserView, TabBarMode};

impl BrowserView {
    pub(super) fn restore_tabs(&mut self, cx: &mut Context<Self>) -> bool {
        let saved = match session::restore() {
            Some(saved) if !saved.tabs.is_empty() => saved,
            _ => return false,
        };

        for serialized_tab in &saved.tabs {
            let url = serialized_tab.url.clone();
            let title = serialized_tab.title.clone();
            let is_new_tab_page = serialized_tab.is_new_tab_page;
            let is_pinned = serialized_tab.is_pinned;
            let favicon_url = serialized_tab.favicon_url.clone();
            let tab = cx.new(|cx| {
                let mut tab =
                    BrowserTab::new_with_state(url, title, is_new_tab_page, favicon_url, cx);
                tab.set_pinned(is_pinned);
                tab
            });
            let subscription = cx.subscribe(&tab, Self::handle_tab_event);
            self._subscriptions.push(subscription);
            self.tabs.push(tab);
        }

        self.sort_tabs_pinned_first(cx);
        self.active_tab_index = saved.active_index.min(self.tabs.len().saturating_sub(1));
        self.tab_bar_mode = if saved.sidebar {
            TabBarMode::Sidebar
        } else {
            TabBarMode::Horizontal
        };
        self.sync_bookmark_bar_visibility(cx);
        true
    }

    pub(super) fn serialize_tabs(&self, cx: &App) -> Option<String> {
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
                    is_new_tab_page: tab.is_new_tab_page(),
                    is_pinned: tab.is_pinned(),
                    favicon_url: tab.favicon_url().map(|s| s.to_string()),
                }
            })
            .collect();

        let data = SerializedBrowserTabs {
            tabs,
            active_index: self.active_tab_index,
            sidebar: self.tab_bar_mode == TabBarMode::Sidebar,
        };

        serde_json::to_string(&data).log_err()
    }

    pub(super) fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self._schedule_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;

            let (tabs_json, history_json) = this
                .read_with(cx, |this, cx| {
                    (this.serialize_tabs(cx), this.history.read(cx).serialize())
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

    pub(super) fn save_tabs_on_quit(&mut self, cx: &mut Context<Self>) -> Task<()> {
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
}
