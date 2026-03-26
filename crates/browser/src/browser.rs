//! Browser Mode for Glass
//!
//! This crate provides the browser mode functionality, integrating
//! Chromium Embedded Framework (CEF) for a full browser experience within Glass.

mod bookmarks;
mod browser_view;
mod cef_instance;
mod client;
mod context_menu_handler;
mod display_handler;
mod download_handler;
mod events;
mod find_handler;
pub mod history;
mod input;
mod keycodes;
mod life_span_handler;
mod load_handler;
#[cfg(target_os = "macos")]
mod macos_protocol;
mod new_tab_page;
#[cfg(not(target_os = "macos"))]
mod omnibox;
mod permission_handler;
mod render_handler;
mod request_handler;
mod session;
mod tab;
#[cfg(not(target_os = "macos"))]
mod toolbar;

pub use browser_view::{BrowserDownloadItem, BrowserSidebarPanel, BrowserView};
pub use cef_instance::CefInstance;
pub use tab::BrowserTab;

/// Handle CEF subprocess execution. This MUST be called very early in main(),
/// before any GUI initialization. See CefInstance::handle_subprocess() for details.
pub fn handle_cef_subprocess() -> anyhow::Result<()> {
    CefInstance::handle_subprocess()
}

use gpui::{AnyView, App, AppContext as _, Entity, Focusable};
use std::sync::Arc;
use workspace_modes::{ModeId, ModeSidebarController, ModeViewRegistry, RegisteredModeView};

fn browser_sidebar_visible(view: &AnyView, cx: &App) -> bool {
    view.clone()
        .downcast::<BrowserView>()
        .ok()
        .is_some_and(|browser_view| browser_view.read(cx).sidebar_visible())
}

fn toggle_browser_sidebar(view: &AnyView, cx: &mut App) {
    if let Ok(browser_view) = view.clone().downcast::<BrowserView>() {
        let _ = browser_view.update(cx, |browser_view, cx| {
            browser_view.toggle_sidebar_visibility(cx);
        });
    }
}

pub fn init(cx: &mut App) {
    match CefInstance::initialize(cx) {
        Ok(_) => {
            // Ensure CEF is shut down before the process exits. Without this,
            // exit() triggers CEF's static CefShutdownChecker destructor which
            // asserts that CefShutdown() was already called.
            //
            // CefInstance::shutdown() handles everything: it takes all browser
            // handles from the global registry, force-closes them, drops the
            // Rust refs (so CEF's BrowserContext ref counts reach zero), pumps
            // the message loop, then calls cef::shutdown().
            std::mem::forget(cx.on_app_quit(|_| async {
                CefInstance::shutdown();
            }));
        }
        Err(e) => {
            log::error!(
                "[browser] init() Failed to initialize CEF: {}. Browser mode will show placeholder.",
                e
            );
        }
    }

    ModeViewRegistry::global_mut(cx).register_factory(
        ModeId::BROWSER,
        Arc::new(|cx: &mut App| {
            let browser_view: Entity<BrowserView> = cx.new(|cx| BrowserView::new(cx));
            let focus_handle = browser_view.focus_handle(cx);

            #[cfg(target_os = "macos")]
            let sidebar = {
                let panel = browser_view.update(cx, |bv, cx| bv.ensure_native_sidebar_panel(cx));
                Some(ModeSidebarController {
                    sidebar_view: gpui::AnyView::from(panel),
                    is_visible: browser_sidebar_visible,
                    toggle: toggle_browser_sidebar,
                })
            };
            #[cfg(not(target_os = "macos"))]
            let sidebar = None;

            let deactivate_view = browser_view.downgrade();
            let on_deactivate: Arc<dyn Fn(&mut App) + Send + Sync> =
                Arc::new(move |cx: &mut App| {
                    if let Some(browser_view) = deactivate_view.upgrade() {
                        browser_view.update(cx, |bv, cx| {
                            bv.release_cef_focus(cx);
                        });
                    }
                });

            RegisteredModeView {
                view: browser_view.into(),
                focus_handle,
                titlebar_center_view: None,
                sidebar,
                on_deactivate: Some(on_deactivate),
            }
        }),
    );
}
