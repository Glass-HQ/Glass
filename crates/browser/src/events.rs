//! Browser Event System
//!
//! Defines events sent from CEF handler threads to the BrowserTab entity
//! on the main/foreground thread via a channel.

use crate::context_menu_handler::ContextMenuContext;
use crate::page_chrome::PageChrome;
use crate::text_input::BrowserTextInputState;
use cef::WindowOpenDisposition;
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub struct FindResultEvent {
    pub count: i32,
    pub active_match_ordinal: i32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserTabOpenTarget {
    Foreground,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenDisposition {
    Unknown,
    CurrentTab,
    SingletonTab,
    NewForegroundTab,
    NewBackgroundTab,
    NewPopup,
    NewWindow,
    SaveToDisk,
    OffTheRecord,
    IgnoreAction,
    SwitchToTab,
    NewPictureInPicture,
}

impl From<WindowOpenDisposition> for OpenDisposition {
    fn from(value: WindowOpenDisposition) -> Self {
        if value == WindowOpenDisposition::CURRENT_TAB {
            Self::CurrentTab
        } else if value == WindowOpenDisposition::SINGLETON_TAB {
            Self::SingletonTab
        } else if value == WindowOpenDisposition::NEW_FOREGROUND_TAB {
            Self::NewForegroundTab
        } else if value == WindowOpenDisposition::NEW_BACKGROUND_TAB {
            Self::NewBackgroundTab
        } else if value == WindowOpenDisposition::NEW_POPUP {
            Self::NewPopup
        } else if value == WindowOpenDisposition::NEW_WINDOW {
            Self::NewWindow
        } else if value == WindowOpenDisposition::SAVE_TO_DISK {
            Self::SaveToDisk
        } else if value == WindowOpenDisposition::OFF_THE_RECORD {
            Self::OffTheRecord
        } else if value == WindowOpenDisposition::IGNORE_ACTION {
            Self::IgnoreAction
        } else if value == WindowOpenDisposition::SWITCH_TO_TAB {
            Self::SwitchToTab
        } else if value == WindowOpenDisposition::NEW_PICTURE_IN_PICTURE {
            Self::NewPictureInPicture
        } else {
            Self::Unknown
        }
    }
}

impl OpenDisposition {
    pub fn app_tab_target(self) -> Option<BrowserTabOpenTarget> {
        match self {
            Self::NewForegroundTab | Self::NewWindow | Self::OffTheRecord | Self::SwitchToTab => {
                Some(BrowserTabOpenTarget::Foreground)
            }
            Self::NewBackgroundTab => Some(BrowserTabOpenTarget::Background),
            Self::Unknown
            | Self::CurrentTab
            | Self::SingletonTab
            | Self::NewPopup
            | Self::SaveToDisk
            | Self::IgnoreAction
            | Self::NewPictureInPicture => None,
        }
    }

    pub fn allow_native_popup(self) -> bool {
        matches!(self, Self::NewPopup)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTargetRequest {
    pub url: String,
    pub disposition: OpenDisposition,
    pub user_gesture: bool,
    pub is_popup_request: bool,
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
    LoadError {
        url: String,
        error_text: String,
    },
    ContextMenuRequested {
        context: ContextMenuContext,
    },
    OpenTargetRequested(OpenTargetRequest),
    FaviconUrlChanged(Vec<String>),
    PageChromeChanged(Option<PageChrome>),
    TextInputStateChanged(BrowserTextInputState),
    FindResult(FindResultEvent),
    DownloadUpdated(DownloadUpdatedEvent),
}

pub type EventSender = mpsc::Sender<BrowserEvent>;
pub type EventReceiver = mpsc::Receiver<BrowserEvent>;

pub fn event_channel() -> (EventSender, EventReceiver) {
    mpsc::channel()
}

#[cfg(test)]
mod tests {
    use super::{BrowserTabOpenTarget, OpenDisposition};

    #[test]
    fn tab_like_dispositions_are_app_managed() {
        assert_eq!(
            OpenDisposition::NewForegroundTab.app_tab_target(),
            Some(BrowserTabOpenTarget::Foreground),
        );
        assert_eq!(
            OpenDisposition::NewBackgroundTab.app_tab_target(),
            Some(BrowserTabOpenTarget::Background),
        );
        assert_eq!(
            OpenDisposition::NewWindow.app_tab_target(),
            Some(BrowserTabOpenTarget::Foreground),
        );
        assert_eq!(
            OpenDisposition::SwitchToTab.app_tab_target(),
            Some(BrowserTabOpenTarget::Foreground),
        );
    }

    #[test]
    fn popup_disposition_is_left_to_native_popup_handling() {
        assert_eq!(OpenDisposition::NewPopup.app_tab_target(), None);
        assert!(OpenDisposition::NewPopup.allow_native_popup());
    }
}
