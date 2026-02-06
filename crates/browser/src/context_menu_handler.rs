//! CEF Context Menu Handler
//!
//! In windowless (OSR) mode CEF cannot display its own context menu, so we
//! intercept the request and forward context information to the GPUI layer
//! which builds and renders a native-looking popup.
//!
//! CEF skips `run_context_menu` when the menu model is empty, so we must
//! leave at least one item in the model from `on_before_context_menu`.

use crate::events::{BrowserEvent, EventSender};
use cef::{
    rc::Rc as _, wrap_context_menu_handler, Browser, ContextMenuHandler, ContextMenuParams, Frame,
    ImplContextMenuHandler, ImplContextMenuParams, ImplRunContextMenuCallback, MenuModel,
    RunContextMenuCallback, WrapContextMenuHandler,
};

/// Context information extracted from CEF's ContextMenuParams, sent to the UI.
#[derive(Debug, Clone)]
pub struct ContextMenuContext {
    pub link_url: Option<String>,
    pub selection_text: Option<String>,
    pub is_editable: bool,
    pub page_url: String,
    pub can_undo: bool,
    pub can_redo: bool,
    pub can_cut: bool,
    pub can_copy: bool,
    pub can_paste: bool,
    pub can_delete: bool,
    pub can_select_all: bool,
}

#[derive(Clone)]
pub struct OsrContextMenuHandler {
    sender: EventSender,
}

impl OsrContextMenuHandler {
    pub fn new(sender: EventSender) -> Self {
        Self { sender }
    }

    fn extract_context(params: &ContextMenuParams) -> ContextMenuContext {
        let link_url = {
            let userfree = params.link_url();
            let s = cef::CefString::from(&userfree).to_string();
            if s.is_empty() { None } else { Some(s) }
        };

        let selection_text = {
            let userfree = params.selection_text();
            let s = cef::CefString::from(&userfree).to_string();
            if s.is_empty() { None } else { Some(s) }
        };

        let page_url = {
            let userfree = params.page_url();
            cef::CefString::from(&userfree).to_string()
        };

        let is_editable = params.is_editable() != 0;

        let edit_flags = params.edit_state_flags();
        let edit_flags_raw: cef::sys::cef_context_menu_edit_state_flags_t = edit_flags.into();

        ContextMenuContext {
            link_url,
            selection_text,
            is_editable,
            page_url,
            can_undo: (edit_flags_raw.0 & 1) != 0,
            can_redo: (edit_flags_raw.0 & 2) != 0,
            can_cut: (edit_flags_raw.0 & 4) != 0,
            can_copy: (edit_flags_raw.0 & 8) != 0,
            can_paste: (edit_flags_raw.0 & 16) != 0,
            can_delete: (edit_flags_raw.0 & 32) != 0,
            can_select_all: (edit_flags_raw.0 & 64) != 0,
        }
    }
}

wrap_context_menu_handler! {
    pub struct ContextMenuHandlerBuilder {
        handler: OsrContextMenuHandler,
    }

    impl ContextMenuHandler {
        // NOTE: we intentionally do NOT override on_before_context_menu.
        // CEF's default populates the model with standard items. If the model
        // is empty, CEF skips run_context_menu entirely.  We need at least
        // one item so run_context_menu gets called.

        fn run_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            _model: Option<&mut MenuModel>,
            callback: Option<&mut RunContextMenuCallback>,
        ) -> ::std::os::raw::c_int {
            let context = params
                .map(|p| OsrContextMenuHandler::extract_context(p))
                .unwrap_or(ContextMenuContext {
                    link_url: None,
                    selection_text: None,
                    is_editable: false,
                    page_url: String::new(),
                    can_undo: false,
                    can_redo: false,
                    can_cut: false,
                    can_copy: false,
                    can_paste: false,
                    can_delete: false,
                    can_select_all: false,
                });

            // Cancel the CEF callback — we handle everything via GPUI and
            // direct browser commands, not through CEF's menu dispatch.
            if let Some(callback) = callback {
                callback.cancel();
            }

            let _ = self
                .handler
                .sender
                .send(BrowserEvent::ContextMenuRequested { context });

            // Return 1 = we handle the menu display ourselves
            1
        }
    }
}

impl ContextMenuHandlerBuilder {
    pub fn build(handler: OsrContextMenuHandler) -> cef::ContextMenuHandler {
        Self::new(handler)
    }
}
