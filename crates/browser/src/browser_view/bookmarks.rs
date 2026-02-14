use crate::bookmarks::{BookmarkBar, BookmarkBarEvent};
use crate::tab::BrowserTab;
use gpui::{Context, Entity, Window};

use super::{BookmarkCurrentPage, BrowserView};

impl BrowserView {
    pub(super) fn handle_bookmark_bar_event(
        &mut self,
        _bookmark_bar: Entity<BookmarkBar>,
        event: &BookmarkBarEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            BookmarkBarEvent::NavigateToUrl(url) => {
                if let Some(tab) = self.active_tab().cloned() {
                    let url = url.clone();
                    self.create_browser_and_navigate(&tab, &url, cx);
                }
            }
            BookmarkBarEvent::OpenInNewTab(url) => {
                self.pending_new_tab_urls.push(url.clone());
                cx.notify();
            }
        }
    }

    pub(super) fn handle_bookmark_current_page(
        &mut self,
        _: &BookmarkCurrentPage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_bookmark_active_tab(cx);
    }

    fn toggle_bookmark_active_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab().cloned() {
            self.toggle_bookmark_for_tab(&tab, cx);
        }
    }

    pub(super) fn toggle_bookmark_at(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(index).cloned() {
            self.toggle_bookmark_for_tab(&tab, cx);
        }
    }

    fn toggle_bookmark_for_tab(&mut self, tab: &Entity<BrowserTab>, cx: &mut Context<Self>) {
        let tab = tab.read(cx);
        let url = tab.url().to_string();
        if url == "glass://newtab" || url.is_empty() {
            return;
        }
        let title = tab.title().to_string();
        let favicon_url = tab.favicon_url().map(|s| s.to_string());
        let stripped = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(&url);
        let title = if title.is_empty() || title == "New Tab" {
            stripped
                .strip_prefix("www.")
                .unwrap_or(stripped)
                .to_string()
        } else {
            title
        };
        self.bookmark_bar.update(cx, |bar, cx| {
            if bar.is_bookmarked(&url) {
                bar.remove_bookmark(&url, cx);
            } else {
                bar.add_bookmark(url, title, favicon_url, cx);
            }
        });
    }
}
