use crate::bookmarks::BookmarkStore;
use crate::history::HistoryEntry;
use db::kvp::KEY_VALUE_STORE;
use serde::{Deserialize, Serialize};
use util::ResultExt as _;

const BROWSER_TABS_KEY: &str = "browser_tabs";
const BROWSER_PINNED_TABS_KEY: &str = "browser_pinned_tabs";
const BROWSER_HISTORY_KEY: &str = "browser_history";
const BROWSER_BOOKMARKS_KEY: &str = "browser_bookmarks";
const BROWSER_DOWNLOADS_KEY: &str = "browser_downloads";

#[derive(Serialize, Deserialize)]
pub struct SerializedBrowserTabs {
    pub tabs: Vec<SerializedTab>,
    pub active_index: usize,
    #[serde(default)]
    pub sidebar: bool,
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

#[derive(Clone, Serialize, Deserialize)]
pub struct SerializedDownloadItem {
    pub id: u32,
    pub url: String,
    pub original_url: String,
    pub suggested_file_name: String,
    pub full_path: Option<String>,
    pub current_speed: i64,
    pub percent_complete: i32,
    pub total_bytes: i64,
    pub received_bytes: i64,
    pub is_in_progress: bool,
    pub is_complete: bool,
    pub is_canceled: bool,
    pub is_interrupted: bool,
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

pub fn restore_pinned_tabs() -> Option<Vec<SerializedTab>> {
    let json = KEY_VALUE_STORE
        .read_kvp(BROWSER_PINNED_TABS_KEY)
        .log_err()??;
    serde_json::from_str(&json).log_err()
}

pub async fn save_pinned_tabs(json: String) -> anyhow::Result<()> {
    KEY_VALUE_STORE
        .write_kvp(BROWSER_PINNED_TABS_KEY.to_string(), json)
        .await
}

pub fn restore_history() -> Option<Vec<HistoryEntry>> {
    let json = KEY_VALUE_STORE.read_kvp(BROWSER_HISTORY_KEY).log_err()??;
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

pub fn restore_downloads() -> Option<Vec<SerializedDownloadItem>> {
    let json = KEY_VALUE_STORE
        .read_kvp(BROWSER_DOWNLOADS_KEY)
        .log_err()??;
    serde_json::from_str(&json).log_err()
}

pub async fn save_downloads(json: String) -> anyhow::Result<()> {
    KEY_VALUE_STORE
        .write_kvp(BROWSER_DOWNLOADS_KEY.to_string(), json)
        .await
}
