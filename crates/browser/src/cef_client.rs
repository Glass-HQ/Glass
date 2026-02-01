//! CEF Client Implementation
//!
//! Provides the Client that CEF uses to communicate with the browser.
//! Ties together the render handler, load handler, and other handlers.

use cef::{
    rc::Rc as _, wrap_client, Client, ImplClient, KeyboardHandler, LoadHandler, RenderHandler,
    WrapClient,
};

use crate::cef_keyboard_handler::{KeyboardHandlerBuilder, OsrKeyboardHandler};
use crate::cef_load_handler::{LoadHandlerBuilder, LoadState, OsrLoadHandler};
use crate::cef_render_handler::{OsrRenderHandler, RenderHandlerBuilder, RenderState};
use parking_lot::Mutex;
use std::sync::Arc;

wrap_client! {
    pub struct ClientBuilder {
        render_handler: RenderHandler,
        load_handler: LoadHandler,
        keyboard_handler: KeyboardHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<cef::RenderHandler> {
            Some(self.render_handler.clone())
        }

        fn load_handler(&self) -> Option<cef::LoadHandler> {
            Some(self.load_handler.clone())
        }

        fn keyboard_handler(&self) -> Option<cef::KeyboardHandler> {
            Some(self.keyboard_handler.clone())
        }
    }
}

impl ClientBuilder {
    pub fn build(
        render_state: Arc<Mutex<RenderState>>,
        load_state: Arc<Mutex<LoadState>>,
    ) -> cef::Client {
        let render_handler = OsrRenderHandler::new(render_state);
        let load_handler = OsrLoadHandler::new(load_state);
        let keyboard_handler = OsrKeyboardHandler::new();
        Self::new(
            RenderHandlerBuilder::build(render_handler),
            LoadHandlerBuilder::build(load_handler),
            KeyboardHandlerBuilder::build(keyboard_handler),
        )
    }
}
