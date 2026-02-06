use db::kvp::KEY_VALUE_STORE;
use serde::{Deserialize, Serialize};
use util::ResultExt as _;

const BROWSER_TABS_KEY: &str = "browser_tabs";

#[derive(Serialize, Deserialize)]
pub struct SerializedBrowserTabs {
    pub tabs: Vec<SerializedTab>,
    pub active_index: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SerializedTab {
    pub url: String,
    pub title: String,
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
