//! Browser Event System
//!
//! Defines events sent from CEF handler threads to the BrowserTab entity
//! on the main/foreground thread via a channel.

use crate::context_menu_handler::ContextMenuContext;
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub struct FindResultEvent {
    pub identifier: i32,
    pub count: i32,
    pub active_match_ordinal: i32,
    pub final_update: bool,
}

#[derive(Debug, Clone)]
pub struct DownloadUpdatedEvent {
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

pub enum BrowserEvent {
    AddressChanged(String),
    TitleChanged(String),
    LoadingStateChanged {
        is_loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    LoadingProgress(f64),
    FrameReady,
    BrowserCreated,
    PopupRequested(String),
    LoadError {
        url: String,
        error_code: i32,
        error_text: String,
    },
    ContextMenuRequested {
        context: ContextMenuContext,
    },
    FaviconUrlChanged(Vec<String>),
    FindResult(FindResultEvent),
    DownloadUpdated(DownloadUpdatedEvent),
}

pub type EventSender = mpsc::Sender<BrowserEvent>;
pub type EventReceiver = mpsc::Receiver<BrowserEvent>;

pub fn event_channel() -> (EventSender, EventReceiver) {
    mpsc::channel()
}
