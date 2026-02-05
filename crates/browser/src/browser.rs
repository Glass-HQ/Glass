//! Browser Mode for Glass
//!
//! This crate provides the browser mode functionality, integrating
//! Chromium Embedded Framework (CEF) for a full browser experience within Glass.

mod browser_view;
mod cef_instance;
mod client;
mod display_handler;
mod events;
mod input;
mod keycodes;
mod life_span_handler;
mod load_handler;
#[cfg(target_os = "macos")]
mod macos_protocol;
mod render_handler;
mod tab;
mod toolbar;

pub use browser_view::BrowserView;
pub use cef_instance::CefInstance;
pub use tab::BrowserTab;

/// Handle CEF subprocess execution. This MUST be called very early in main(),
/// before any GUI initialization. See CefInstance::handle_subprocess() for details.
pub fn handle_cef_subprocess() -> anyhow::Result<()> {
    CefInstance::handle_subprocess()
}

use gpui::{App, AppContext, Focusable};
use workspace_modes::{ModeId, ModeViewRegistry, RegisteredModeView};

pub fn init(cx: &mut App) {
    match CefInstance::initialize(cx) {
        Ok(_) => {
            log::info!("CEF browser mode initialized");
        }
        Err(e) => {
            log::error!("Failed to initialize CEF: {}. Browser mode will show placeholder.", e);
        }
    }

    let browser_view = cx.new(|cx| BrowserView::new(cx));
    let focus_handle = browser_view.focus_handle(cx);

    ModeViewRegistry::global_mut(cx).register(
        ModeId::BROWSER,
        RegisteredModeView {
            view: browser_view.into(),
            focus_handle,
        },
    );
}
