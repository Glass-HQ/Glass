//! Browser Mode for Glass
//!
//! This crate provides the browser mode functionality, which will eventually
//! integrate Chromium for a full browser experience within Glass.
//!
//! Currently displays a placeholder view that will be replaced with the
//! actual browser implementation.

mod browser_view;

pub use browser_view::BrowserView;

use gpui::{App, AppContext, Focusable};
use workspace_modes::{ModeId, ModeViewRegistry, RegisteredModeView};

/// Initialize the browser crate and register the browser view with the mode registry
pub fn init(cx: &mut App) {
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
