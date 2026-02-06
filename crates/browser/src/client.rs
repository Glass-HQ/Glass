//! CEF Client Implementation
//!
//! Provides the Client that CEF uses to communicate with the browser.
//! Ties together the render, load, display, life span, and keyboard handlers.

use cef::{
    rc::Rc as _, wrap_client, wrap_keyboard_handler, Browser, Client, ContextMenuHandler,
    DisplayHandler, ImplClient, ImplKeyboardHandler, KeyEvent, KeyboardHandler, LifeSpanHandler,
    LoadHandler, PermissionHandler, RenderHandler, WrapClient, WrapKeyboardHandler,
};

use crate::context_menu_handler::{ContextMenuHandlerBuilder, OsrContextMenuHandler};
use crate::display_handler::{DisplayHandlerBuilder, OsrDisplayHandler};
use crate::events::EventSender;
use crate::life_span_handler::{LifeSpanHandlerBuilder, OsrLifeSpanHandler};
use crate::load_handler::{LoadHandlerBuilder, OsrLoadHandler};
use crate::permission_handler::{OsrPermissionHandler, PermissionHandlerBuilder};
use crate::render_handler::{OsrRenderHandler, RenderHandlerBuilder, RenderState};
use crate::request_handler::{OsrRequestHandler, RequestHandlerBuilder};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
            let event_type = _event.map(|e| format!("{:?}", e.type_)).unwrap_or_default();
            let wkc = _event.map(|e| e.windows_key_code).unwrap_or(0);
            log::info!("[browser::keyboard] on_pre_key_event(manual={}, type={}, wkc={}, suppress={})",
                is_manual, event_type, wkc, !is_manual);
            if is_manual {
                0
            } else {
                1
            }
        }
    }
}

impl KeyboardHandlerBuilder {
    fn build() -> cef::KeyboardHandler {
        Self::new(OsrKeyboardHandler)
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
    pub fn build(
        render_state: Arc<Mutex<RenderState>>,
        event_sender: EventSender,
    ) -> cef::Client {
        let render_handler = OsrRenderHandler::new(render_state, event_sender.clone());
        let load_handler = OsrLoadHandler::new(event_sender.clone());
        let display_handler = OsrDisplayHandler::new(event_sender.clone());
        let life_span_handler = OsrLifeSpanHandler::new(event_sender.clone());
        let request_handler = OsrRequestHandler::new(event_sender.clone());
        let context_menu_handler = OsrContextMenuHandler::new(event_sender);
        let permission_handler = OsrPermissionHandler::new();
        Self::new(
            RenderHandlerBuilder::build(render_handler),
            LoadHandlerBuilder::build(load_handler),
            DisplayHandlerBuilder::build(display_handler),
            LifeSpanHandlerBuilder::build(life_span_handler),
            KeyboardHandlerBuilder::build(),
            RequestHandlerBuilder::build(request_handler),
            ContextMenuHandlerBuilder::build(context_menu_handler),
            PermissionHandlerBuilder::build(permission_handler),
        )
    }
}
