use crate::bookmarks::BookmarkStore;
use crate::history::HistoryEntry;
use db::kvp::KEY_VALUE_STORE;
use serde::{Deserialize, Serialize};
use util::ResultExt as _;

const BROWSER_TABS_KEY: &str = "browser_tabs";
const BROWSER_HISTORY_KEY: &str = "browser_history";
const BROWSER_BOOKMARKS_KEY: &str = "browser_bookmarks";

#[derive(Serialize, Deserialize)]
pub struct SerializedBrowserTabs {
    pub tabs: Vec<SerializedTab>,
    pub active_index: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SerializedTab {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub is_new_tab_page: bool,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub favicon_url: Option<String>,
}

pub fn restore() -> Option<SerializedBrowserTabs> {
    let json = KEY_VALUE_STORE.read_kvp(BROWSER_TABS_KEY).log_err()??;
    serde_json::from_str(&json).log_err()
}

pub async fn save(json: String) -> anyhow::Result<()> {
    KEY_VALUE_STORE
        .write_kvp(BROWSER_TABS_KEY.to_string(), json)
        .await
}

pub fn restore_history() -> Option<Vec<HistoryEntry>> {
    let json = KEY_VALUE_STORE
        .read_kvp(BROWSER_HISTORY_KEY)
        .log_err()??;
    serde_json::from_str(&json).log_err()
}

pub async fn save_history(json: String) -> anyhow::Result<()> {
    KEY_VALUE_STORE
        .write_kvp(BROWSER_HISTORY_KEY.to_string(), json)
        .await
}

pub fn restore_bookmarks() -> Option<BookmarkStore> {
    let json = KEY_VALUE_STORE
        .read_kvp(BROWSER_BOOKMARKS_KEY)
        .log_err()??;
    serde_json::from_str(&json).log_err()
}

pub async fn save_bookmarks(json: String) -> anyhow::Result<()> {
    KEY_VALUE_STORE
        .write_kvp(BROWSER_BOOKMARKS_KEY.to_string(), json)
        .await
}
