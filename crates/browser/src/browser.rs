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

pub use browser_view::{
    BrowserDownloadItem, BrowserPaneItem, BrowserSidebarPanel, BrowserSurfaceState, BrowserView,
    OpenBrowserPane,
};
pub use cef_instance::CefInstance;
pub use tab::BrowserTab;

/// Handle CEF subprocess execution. This MUST be called very early in main(),
/// before any GUI initialization. See CefInstance::handle_subprocess() for details.
pub fn handle_cef_subprocess() -> anyhow::Result<()> {
    CefInstance::handle_subprocess()
}

use gpui::{AnyView, App, AppContext as _, Entity, Focusable, Window};
use std::sync::Arc;
use util::ResultExt;
use workspace::{register_browser_mode_url_opener, register_embedded_browser_item_factory};
use workspace_modes::{ModeId, ModeNavigationHost, ModeViewRegistry, RegisteredModeView};

fn browser_navigation_entries(
    view: &AnyView,
    _window: &Window,
    cx: &App,
) -> Vec<workspace_modes::ModeNavigationEntry> {
    view.clone()
        .downcast::<BrowserView>()
        .ok()
        .map(|browser_view| browser_view.read(cx).navigation_entries(cx))
        .unwrap_or_default()
}

fn activate_browser_navigation_entry(
    view: &AnyView,
    entry_id: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let Ok(tab_id) = entry_id.parse::<u64>() else {
        return;
    };
    if let Ok(browser_view) = view.clone().downcast::<BrowserView>() {
        let _ = browser_view.update(cx, |browser_view, cx| {
            browser_view.activate_navigation_entry(tab_id, window, cx);
        });
    }
}

fn close_browser_navigation_entry(
    view: &AnyView,
    entry_id: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let Ok(tab_id) = entry_id.parse::<u64>() else {
        return;
    };
    if let Ok(browser_view) = view.clone().downcast::<BrowserView>() {
        let _ = browser_view.update(cx, |browser_view, cx| {
            browser_view.close_navigation_entry(tab_id, window, cx);
        });
    }
}

fn create_browser_navigation_entry(view: &AnyView, window: &mut Window, cx: &mut App) {
    if let Ok(browser_view) = view.clone().downcast::<BrowserView>() {
        let _ = browser_view.update(cx, |browser_view, cx| {
            browser_view.add_tab(cx);
            browser_view.update_toolbar_active_tab(window, cx);
            cx.notify();
        });
    }
}

fn open_browser_mode_url(view: &AnyView, url: &str, _window: &mut Window, cx: &mut App) {
    if let Ok(browser_view) = view.clone().downcast::<BrowserView>() {
        let url = url.to_string();
        let _ = browser_view.update(cx, |browser_view, cx| {
            browser_view.open_url(&url, cx);
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

    register_browser_mode_url_opener(Arc::new(open_browser_mode_url), cx);
    register_embedded_browser_item_factory(
        Arc::new(|workspace, browser_view, cx: &mut App| {
            let browser_view = browser_view
                .clone()
                .downcast::<BrowserView>()
                .expect("shared browser view should downcast to BrowserView");
            Box::new(BrowserPaneItem::new(&browser_view, workspace, cx))
                as Box<dyn workspace::ItemHandle>
        }),
        cx,
    );

    cx.observe_new(
        |workspace: &mut workspace::Workspace,
         _window: Option<&mut Window>,
         _cx: &mut gpui::Context<workspace::Workspace>| {
            workspace.register_action(
                |workspace, _: &browser_view::OpenBrowserPane, window, cx| {
                    workspace.show_browser_surface(true, window, cx).log_err();
                },
            );
        },
    )
    .detach();

    ModeViewRegistry::global_mut(cx).register_factory(
        ModeId::BROWSER,
        Arc::new(|cx: &mut App| {
            let browser_view: Entity<BrowserView> = cx.new(|cx| BrowserView::new(cx));
            let focus_handle = browser_view.focus_handle(cx);

            #[cfg(target_os = "macos")]
            let sidebar_view = {
                let panel = browser_view.update(cx, |bv, cx| bv.ensure_native_sidebar_panel(cx));
                Some(gpui::AnyView::from(panel))
            };
            #[cfg(not(target_os = "macos"))]
            let sidebar_view = None;

            let deactivate_view = browser_view.downgrade();
            let on_deactivate: Arc<dyn Fn(&mut App) + Send + Sync> =
                Arc::new(move |cx: &mut App| {
                    if let Some(browser_view) = deactivate_view.upgrade() {
                        browser_view.update(cx, |bv, cx| {
                            bv.release_cef_focus(cx);
                            bv.park_surface(cx);
                        });
                    }
                });

            let activate_view = browser_view.downgrade();
            let on_activate: Arc<dyn Fn(&mut Window, &mut App) + Send + Sync> =
                Arc::new(move |window: &mut Window, cx: &mut App| {
                    if let Some(browser_view) = activate_view.upgrade() {
                        browser_view.update(cx, |bv, cx| {
                            bv.set_surface_state(BrowserSurfaceState::Visible, window, cx);
                        });
                    }
                });

            RegisteredModeView {
                view: browser_view.into(),
                focus_handle,
                titlebar_center_view: None,
                sidebar_view,
                navigation_host: Some(ModeNavigationHost {
                    entries: browser_navigation_entries,
                    activate: activate_browser_navigation_entry,
                    close: close_browser_navigation_entry,
                    create: create_browser_navigation_entry,
                }),
                on_activate: Some(on_activate),
                on_deactivate: Some(on_deactivate),
            }
        }),
    );
}
