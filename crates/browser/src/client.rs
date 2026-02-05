//! CEF Client Implementation
//!
//! Provides the Client that CEF uses to communicate with the browser.
//! Ties together the render, load, display, and life span handlers.

use cef::{
    rc::Rc as _, wrap_client, Client, DisplayHandler, ImplClient, LifeSpanHandler, LoadHandler,
    RenderHandler, WrapClient,
};

use crate::display_handler::{DisplayHandlerBuilder, OsrDisplayHandler};
use crate::events::EventSender;
use crate::life_span_handler::{LifeSpanHandlerBuilder, OsrLifeSpanHandler};
use crate::load_handler::{LoadHandlerBuilder, OsrLoadHandler};
use crate::render_handler::{OsrRenderHandler, RenderHandlerBuilder, RenderState};
use parking_lot::Mutex;
use std::sync::Arc;

wrap_client! {
    pub struct ClientBuilder {
        render_handler: RenderHandler,
        load_handler: LoadHandler,
        display_handler: DisplayHandler,
        life_span_handler: LifeSpanHandler,
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
        let life_span_handler = OsrLifeSpanHandler::new(event_sender);
        Self::new(
            RenderHandlerBuilder::build(render_handler),
            LoadHandlerBuilder::build(load_handler),
            DisplayHandlerBuilder::build(display_handler),
            LifeSpanHandlerBuilder::build(life_span_handler),
        )
    }
}
