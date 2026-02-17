//! CEF Download Handler
//!
//! Handles destination selection and progress updates for browser downloads.

use crate::events::{BrowserEvent, DownloadUpdatedEvent, EventSender};
use cef::{
    Browser, DownloadHandler, DownloadItem, ImplBeforeDownloadCallback, ImplDownloadHandler,
    ImplDownloadItem, WrapDownloadHandler, rc::Rc as _, wrap_download_handler,
};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct OsrDownloadHandler {
    sender: EventSender,
}

impl OsrDownloadHandler {
    pub fn new(sender: EventSender) -> Self {
        Self { sender }
    }

    fn default_download_directory() -> PathBuf {
        let preferred = paths::home_dir().join("Downloads");
        if std::fs::create_dir_all(&preferred).is_ok() {
            return preferred;
        }

        let fallback = paths::data_dir().join("browser_downloads");
        if let Err(error) = std::fs::create_dir_all(&fallback) {
            log::warn!(
                "[browser] failed to create fallback download directory {}: {}",
                fallback.display(),
                error
            );
        }
        fallback
    }

    fn file_name_for_download(
        suggested_name: Option<&cef::CefString>,
        download_item: Option<&mut DownloadItem>,
    ) -> String {
        if let Some(suggested_name) = suggested_name {
            let suggested = suggested_name.to_string();
            if !suggested.is_empty() {
                return suggested;
            }
        }

        if let Some(download_item) = download_item {
            let suggested_userfree = download_item.suggested_file_name();
            let suggested = cef::CefString::from(&suggested_userfree).to_string();
            if !suggested.is_empty() {
                return suggested;
            }

            let url_userfree = download_item.url();
            let url_text = cef::CefString::from(&url_userfree).to_string();
            if let Ok(parsed) = url::Url::parse(&url_text) {
                if let Some(segment) = parsed
                    .path_segments()
                    .and_then(|segments| segments.last())
                    .filter(|segment| !segment.is_empty())
                {
                    return segment.to_string();
                }
            }
        }

        String::from("download")
    }

    fn unique_download_path(directory: &Path, file_name: &str) -> PathBuf {
        let file_name = Path::new(file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("download");

        let original_path = directory.join(file_name);
        if !original_path.exists() {
            return original_path;
        }

        let file_path = Path::new(file_name);
        let stem = file_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty())
            .unwrap_or("download");
        let extension = file_path.extension().and_then(|ext| ext.to_str());

        let mut attempt = 1u32;
        loop {
            let candidate_file_name = if let Some(extension) = extension {
                format!("{stem} ({attempt}).{extension}")
            } else {
                format!("{stem} ({attempt})")
            };

            let candidate_path = directory.join(candidate_file_name);
            if !candidate_path.exists() {
                return candidate_path;
            }

            attempt += 1;
        }
    }
}

wrap_download_handler! {
    pub struct DownloadHandlerBuilder {
        handler: OsrDownloadHandler,
    }

    impl DownloadHandler {
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&cef::CefString>,
            _request_method: Option<&cef::CefString>,
        ) -> ::std::os::raw::c_int {
            1
        }

        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&cef::CefString>,
            callback: Option<&mut cef::BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback else {
                return 0;
            };

            let download_directory = OsrDownloadHandler::default_download_directory();
            let file_name = OsrDownloadHandler::file_name_for_download(
                suggested_name,
                download_item,
            );
            let target_path =
                OsrDownloadHandler::unique_download_path(&download_directory, &file_name);
            let target_path_text = target_path.to_string_lossy().to_string();
            let cef_target_path = cef::CefString::from(target_path_text.as_str());

            callback.cont(Some(&cef_target_path), 0);
            1
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            _callback: Option<&mut cef::DownloadItemCallback>,
        ) {
            let Some(download_item) = download_item else {
                return;
            };

            let full_path_userfree = download_item.full_path();
            let full_path_text = cef::CefString::from(&full_path_userfree).to_string();
            let full_path = if full_path_text.is_empty() {
                None
            } else {
                Some(full_path_text)
            };

            let url_userfree = download_item.url();
            let original_url_userfree = download_item.original_url();
            let suggested_file_name_userfree = download_item.suggested_file_name();

            let event = DownloadUpdatedEvent {
                id: download_item.id(),
                url: cef::CefString::from(&url_userfree).to_string(),
                original_url: cef::CefString::from(&original_url_userfree).to_string(),
                suggested_file_name: cef::CefString::from(&suggested_file_name_userfree)
                    .to_string(),
                full_path,
                current_speed: download_item.current_speed(),
                percent_complete: download_item.percent_complete(),
                total_bytes: download_item.total_bytes(),
                received_bytes: download_item.received_bytes(),
                is_in_progress: download_item.is_in_progress() != 0,
                is_complete: download_item.is_complete() != 0,
                is_canceled: download_item.is_canceled() != 0,
                is_interrupted: download_item.is_interrupted() != 0,
            };

            if let Err(error) = self.handler.sender.send(BrowserEvent::DownloadUpdated(event)) {
                log::debug!("[browser] failed to send download updated event: {}", error);
            }
        }
    }
}

impl DownloadHandlerBuilder {
    pub fn build(handler: OsrDownloadHandler) -> cef::DownloadHandler {
        Self::new(handler)
    }
}
