//! CEF Client Implementation
//!
//! Provides the Client that CEF uses to communicate with the browser.
//! Ties together the render, load, display, life span, and keyboard handlers.

use cef::{
    Browser, Client, ContextMenuHandler, DisplayHandler, DownloadHandler, FindHandler, ImplClient,
    ImplKeyboardHandler, KeyEvent, KeyboardHandler, LifeSpanHandler, LoadHandler,
    PermissionHandler, RenderHandler, WrapClient, WrapKeyboardHandler, rc::Rc as _, wrap_client,
    wrap_keyboard_handler,
};

use crate::context_menu_handler::{ContextMenuHandlerBuilder, OsrContextMenuHandler};
use crate::display_handler::{DisplayHandlerBuilder, OsrDisplayHandler};
use crate::download_handler::{DownloadHandlerBuilder, OsrDownloadHandler};
use crate::events::EventSender;
use crate::find_handler::{FindHandlerBuilder, OsrFindHandler};
use crate::life_span_handler::{LifeSpanHandlerBuilder, OsrLifeSpanHandler};
use crate::load_handler::{LoadHandlerBuilder, OsrLoadHandler};
use crate::permission_handler::{OsrPermissionHandler, PermissionHandlerBuilder};
use crate::render_handler::{OsrRenderHandler, RenderHandlerBuilder, RenderState};
use crate::request_handler::{OsrRequestHandler, RequestHandlerBuilder};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// ── Keyboard Handler ─────────────────────────────────────────────────
// Suppresses native macOS key events that CEF picks up through the
// application's sendEvent: override. We send all key input explicitly
// via BrowserHost::send_key_event, so native events are duplicates.
//
// We use a flag to distinguish our manual events from native ones:
// tab.rs sets MANUAL_KEY_EVENT=true before calling send_key_event,
// and on_pre_key_event checks it. Since on_pre_key_event is called
// synchronously from within send_key_event, this is safe.

pub(crate) static MANUAL_KEY_EVENT: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
struct OsrKeyboardHandler;

wrap_keyboard_handler! {
    struct KeyboardHandlerBuilder {
        handler: OsrKeyboardHandler,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            _event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let is_manual = MANUAL_KEY_EVENT.load(Ordering::Relaxed);
            let suppress = if is_manual { 0 } else { 1 };
            log::trace!(
                "[browser::client] on_pre_key_event: is_manual={} suppress={}",
                is_manual,
                suppress == 1,
            );
            suppress
        }
    }
}

impl KeyboardHandlerBuilder {
    fn build() -> cef::KeyboardHandler {
        Self::new(OsrKeyboardHandler)
    }
}

// Popup windows are real native windows, so their key events come through
// the OS natively and should NOT be suppressed. This handler allows all
// events through.
#[derive(Clone)]
struct PopupKeyboardHandler;

wrap_keyboard_handler! {
    struct PopupKeyboardHandlerBuilder {
        handler: PopupKeyboardHandler,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            _event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            0 // Allow all native key events through.
        }
    }
}

impl PopupKeyboardHandlerBuilder {
    fn build() -> cef::KeyboardHandler {
        Self::new(PopupKeyboardHandler)
    }
}

// ── Client ───────────────────────────────────────────────────────────

wrap_client! {
    pub struct ClientBuilder {
        render_handler: RenderHandler,
        load_handler: LoadHandler,
        display_handler: DisplayHandler,
        life_span_handler: LifeSpanHandler,
        keyboard_handler: KeyboardHandler,
        download_handler: DownloadHandler,
        find_handler: FindHandler,
        request_handler: cef::RequestHandler,
        context_menu_handler: ContextMenuHandler,
        permission_handler: PermissionHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<cef::RenderHandler> {
            Some(self.render_handler.clone())
        }

        fn load_handler(&self) -> Option<cef::LoadHandler> {
            Some(self.load_handler.clone())
        }

        fn display_handler(&self) -> Option<cef::DisplayHandler> {
            Some(self.display_handler.clone())
        }

        fn life_span_handler(&self) -> Option<cef::LifeSpanHandler> {
            Some(self.life_span_handler.clone())
        }

        fn keyboard_handler(&self) -> Option<cef::KeyboardHandler> {
            Some(self.keyboard_handler.clone())
        }

        fn download_handler(&self) -> Option<cef::DownloadHandler> {
            Some(self.download_handler.clone())
        }

        fn find_handler(&self) -> Option<cef::FindHandler> {
            Some(self.find_handler.clone())
        }

        fn request_handler(&self) -> Option<cef::RequestHandler> {
            Some(self.request_handler.clone())
        }

        fn context_menu_handler(&self) -> Option<cef::ContextMenuHandler> {
            Some(self.context_menu_handler.clone())
        }

        fn permission_handler(&self) -> Option<cef::PermissionHandler> {
            Some(self.permission_handler.clone())
        }
    }
}

impl ClientBuilder {
    pub fn build(render_state: Arc<Mutex<RenderState>>, event_sender: EventSender) -> cef::Client {
        Self::build_inner(render_state, event_sender, KeyboardHandlerBuilder::build())
    }

    pub fn build_for_popup(
        render_state: Arc<Mutex<RenderState>>,
        event_sender: EventSender,
    ) -> cef::Client {
        Self::build_inner(render_state, event_sender, PopupKeyboardHandlerBuilder::build())
    }

    fn build_inner(
        render_state: Arc<Mutex<RenderState>>,
        event_sender: EventSender,
        keyboard_handler: cef::KeyboardHandler,
    ) -> cef::Client {
        let render_handler = OsrRenderHandler::new(render_state, event_sender.clone());
        let load_handler = OsrLoadHandler::new(event_sender.clone());
        let display_handler = OsrDisplayHandler::new(event_sender.clone());
        let life_span_handler = OsrLifeSpanHandler::new(event_sender.clone());
        let request_handler = OsrRequestHandler::new();
        let download_handler = OsrDownloadHandler::new(event_sender.clone());
        let find_handler = OsrFindHandler::new(event_sender.clone());
        let context_menu_handler = OsrContextMenuHandler::new(event_sender);
        let permission_handler = OsrPermissionHandler::new();
        Self::new(
            RenderHandlerBuilder::build(render_handler),
            LoadHandlerBuilder::build(load_handler),
            DisplayHandlerBuilder::build(display_handler),
            LifeSpanHandlerBuilder::build(life_span_handler),
            keyboard_handler,
            DownloadHandlerBuilder::build(download_handler),
            FindHandlerBuilder::build(find_handler),
            RequestHandlerBuilder::build(request_handler),
            ContextMenuHandlerBuilder::build(context_menu_handler),
            PermissionHandlerBuilder::build(permission_handler),
        )
    }
}
