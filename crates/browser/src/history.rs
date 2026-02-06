use crate::session;
use fuzzy::StringMatchCandidate;
use gpui::{BackgroundExecutor, Context, EventEmitter};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 2000;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visit_count: u32,
    pub last_visited_ms: u64,
}

pub struct HistoryMatch {
    pub url: String,
    pub title: String,
    pub score: f64,
}

pub struct BrowserHistory {
    entries: Vec<HistoryEntry>,
}

impl EventEmitter<()> for BrowserHistory {}

impl BrowserHistory {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            entries: Vec::new(),
        };
        if let Some(entries) = session::restore_history() {
            this.entries = entries;
        }
        cx.notify();
        this
    }

    pub fn record_visit(&mut self, url: &str, title: &str) {
        if url.is_empty() || url == "about:blank" {
            return;
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if let Some(entry) = self.entries.iter_mut().find(|e| e.url == url) {
            entry.visit_count += 1;
            entry.last_visited_ms = now_ms;
            if !title.is_empty() {
                entry.title = title.to_string();
            }
        } else {
            self.entries.push(HistoryEntry {
                url: url.to_string(),
                title: title.to_string(),
                visit_count: 1,
                last_visited_ms: now_ms,
            });

            if self.entries.len() > MAX_ENTRIES {
                // Evict the least recently visited entry
                if let Some(oldest_index) = self
                    .entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, e)| e.last_visited_ms)
                    .map(|(i, _)| i)
                {
                    self.entries.swap_remove(oldest_index);
                }
            }
        }
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn serialize(&self) -> Option<String> {
        serde_json::to_string(&self.entries).ok()
    }

    pub async fn search(
        entries: Vec<HistoryEntry>,
        query: String,
        max_results: usize,
        executor: BackgroundExecutor,
    ) -> Vec<HistoryMatch> {
        if query.is_empty() {
            return Vec::new();
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let candidates: Vec<StringMatchCandidate> = entries
            .iter()
            .enumerate()
            .map(|(id, entry)| {
                StringMatchCandidate::new(id, &format!("{} {}", entry.title, entry.url))
            })
            .collect();

        let cancel_flag = AtomicBool::new(false);
        let matches = fuzzy::match_strings(
            &candidates,
            &query,
            false,
            true,
            max_results * 3, // over-fetch so we can re-rank
            &cancel_flag,
            executor,
        )
        .await;

        let query_lower = query.to_lowercase();

        let mut results: Vec<HistoryMatch> = matches
            .into_iter()
            .filter_map(|m| {
                let entry = entries.get(m.candidate_id)?;
                let fuzzy_score = m.score;

                // Recency bonus: 0.0-0.3 based on age
                let age_ms = now_ms.saturating_sub(entry.last_visited_ms);
                let age_hours = age_ms as f64 / 3_600_000.0;
                let recency_bonus = 0.3 * (1.0 / (1.0 + age_hours / 24.0));

                // Frequency bonus: 0.0-0.2 on log scale
                let frequency_bonus = 0.2 * (entry.visit_count as f64).ln_1p() / 10.0_f64.ln_1p();

                // Prefix bonus: 0.5 if URL starts with query
                let prefix_bonus = if entry.url.to_lowercase().starts_with(&query_lower)
                    || entry
                        .url
                        .to_lowercase()
                        .strip_prefix("https://")
                        .is_some_and(|u| u.starts_with(&query_lower))
                    || entry
                        .url
                        .to_lowercase()
                        .strip_prefix("http://")
                        .is_some_and(|u| u.starts_with(&query_lower))
                {
                    0.5
                } else {
                    0.0
                };

                let final_score = fuzzy_score + recency_bonus + frequency_bonus + prefix_bonus;

                Some(HistoryMatch {
                    url: entry.url.clone(),
                    title: entry.title.clone(),
                    score: final_score,
                })
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(max_results);
        results
    }
}
