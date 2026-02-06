//! Browser Event System
//!
//! Defines events sent from CEF handler threads to the BrowserTab entity
//! on the main/foreground thread via a channel.

use crate::context_menu_handler::ContextMenuContext;
use std::sync::mpsc;

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
}

pub type EventSender = mpsc::Sender<BrowserEvent>;
pub type EventReceiver = mpsc::Receiver<BrowserEvent>;

pub fn event_channel() -> (EventSender, EventReceiver) {
    mpsc::channel()
}
