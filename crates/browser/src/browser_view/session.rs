use crate::session::{self, SerializedBrowserTabs, SerializedTab};
use crate::tab::BrowserTab;
use gpui::{App, AppContext as _, Context, Task};
use std::time::Duration;
use util::ResultExt as _;

use super::{BrowserView, DownloadItemState, TabBarMode};

impl BrowserView {
    pub(super) fn restore_tabs(&mut self, cx: &mut Context<Self>) -> bool {
        if self.is_incognito_window {
            return false;
        }

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
            self.configure_tab_request_context(&tab, cx);
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

    pub(super) fn restore_pinned_tabs(&mut self, cx: &mut Context<Self>) -> bool {
        if self.is_incognito_window {
            return false;
        }

        let pinned = match session::restore_pinned_tabs() {
            Some(pinned) if !pinned.is_empty() => pinned,
            _ => return false,
        };

        for serialized_tab in &pinned {
            let url = serialized_tab.url.clone();
            let title = serialized_tab.title.clone();
            let is_new_tab_page = serialized_tab.is_new_tab_page;
            let favicon_url = serialized_tab.favicon_url.clone();
            let tab = cx.new(|cx| {
                let mut tab =
                    BrowserTab::new_with_state(url, title, is_new_tab_page, favicon_url, cx);
                tab.set_pinned(true);
                tab
            });
            self.configure_tab_request_context(&tab, cx);
            let subscription = cx.subscribe(&tab, Self::handle_tab_event);
            self._subscriptions.push(subscription);
            self.tabs.push(tab);
        }

        self.sort_tabs_pinned_first(cx);
        self.add_tab(cx);
        self.sync_bookmark_bar_visibility(cx);
        true
    }

    pub(super) fn restore_downloads(&mut self) {
        if self.is_incognito_window {
            self.downloads.clear();
            return;
        }

        self.downloads = session::restore_downloads()
            .unwrap_or_default()
            .into_iter()
            .map(DownloadItemState::from_serialized)
            .collect();
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

    pub(super) fn serialize_pinned_tabs(&self, cx: &App) -> String {
        let pinned: Vec<SerializedTab> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let tab = tab.read(cx);
                if tab.is_pinned() {
                    Some(SerializedTab {
                        url: tab.url().to_string(),
                        title: tab.title().to_string(),
                        is_new_tab_page: tab.is_new_tab_page(),
                        is_pinned: true,
                        favicon_url: tab.favicon_url().map(|s| s.to_string()),
                    })
                } else {
                    None
                }
            })
            .collect();

        serde_json::to_string(&pinned).unwrap_or_else(|_| "[]".to_string())
    }

    pub(super) fn serialize_downloads(&self) -> Option<String> {
        let downloads = self
            .downloads
            .iter()
            .filter(|download| !download.is_incognito)
            .map(DownloadItemState::to_serialized)
            .collect::<Vec<_>>();
        serde_json::to_string(&downloads).log_err()
    }

    pub(super) fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if self.is_incognito_window {
            return;
        }

        let is_tab_owner = self.is_tab_owner;
        self._schedule_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;

            let (tabs_json, pinned_json, history_json, downloads_json) = this
                .read_with(cx, |this, cx| {
                    (
                        if is_tab_owner {
                            this.serialize_tabs(cx)
                        } else {
                            None
                        },
                        Some(this.serialize_pinned_tabs(cx)),
                        if is_tab_owner {
                            this.history.read(cx).serialize()
                        } else {
                            None
                        },
                        if is_tab_owner {
                            this.serialize_downloads()
                        } else {
                            None
                        },
                    )
                })
                .ok()
                .unwrap_or((None, None, None, None));

            if let Some(json) = tabs_json {
                session::save(json).await.log_err();
            }
            if let Some(json) = pinned_json {
                session::save_pinned_tabs(json).await.log_err();
            }
            if let Some(json) = history_json {
                session::save_history(json).await.log_err();
            }
            if let Some(json) = downloads_json {
                session::save_downloads(json).await.log_err();
            }

            this.update(cx, |this, _| {
                this._schedule_save.take();
            })
            .ok();
        }));
    }

    pub(super) fn save_tabs_on_quit(&mut self, cx: &mut Context<Self>) -> Task<()> {
        if self.is_incognito_window {
            return Task::ready(());
        }

        let tabs_json = if self.is_tab_owner {
            self.serialize_tabs(cx)
        } else {
            None
        };
        let pinned_json = self.serialize_pinned_tabs(cx);
        let history_json = if self.is_tab_owner {
            self.history.read(cx).serialize()
        } else {
            None
        };
        let downloads_json = if self.is_tab_owner {
            self.serialize_downloads()
        } else {
            None
        };

        cx.background_spawn(async move {
            if let Some(json) = tabs_json {
                session::save(json).await.log_err();
            }
            session::save_pinned_tabs(pinned_json).await.log_err();
            if let Some(json) = history_json {
                session::save_history(json).await.log_err();
            }
            if let Some(json) = downloads_json {
                session::save_downloads(json).await.log_err();
            }
        })
    }
}
