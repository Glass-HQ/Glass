//! CEF Permission Handler
//!
//! Handles permission requests from web content. Grants protected media
//! identifier permissions required for Widevine DRM playback on streaming
//! platforms (HBO Max, Netflix, Disney+, etc).

use cef::{
    rc::Rc as _, wrap_permission_handler, Browser, Frame, ImplMediaAccessCallback,
    ImplPermissionHandler, ImplPermissionPromptCallback, MediaAccessCallback,
    PermissionHandler, PermissionPromptCallback, PermissionRequestResult,
    WrapPermissionHandler,
};

const PROTECTED_MEDIA_IDENTIFIER: u32 = 262144;

#[derive(Clone)]
pub struct OsrPermissionHandler;

impl OsrPermissionHandler {
    pub fn new() -> Self {
        Self
    }
}

wrap_permission_handler! {
    pub struct PermissionHandlerBuilder {
        handler: OsrPermissionHandler,
    }

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&cef::CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> ::std::os::raw::c_int {
            let origin = requesting_origin
                .map(|o| o.to_string())
                .unwrap_or_default();
            log::info!(
                "[browser::permission] media access request from '{}' (permissions=0x{:x})",
                origin,
                requested_permissions,
            );
            if let Some(callback) = callback {
                callback.cont(requested_permissions);
            }
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&cef::CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> ::std::os::raw::c_int {
            let origin = requesting_origin
                .map(|o| o.to_string())
                .unwrap_or_default();

            if requested_permissions & PROTECTED_MEDIA_IDENTIFIER != 0 {
                log::info!(
                    "[browser::permission] granting protected media identifier for '{}' (prompt_id={})",
                    origin,
                    prompt_id,
                );
                if let Some(callback) = callback {
                    callback.cont(PermissionRequestResult::ACCEPT);
                }
                return 1;
            }

            log::info!(
                "[browser::permission] permission prompt from '{}' (permissions=0x{:x}, prompt_id={})",
                origin,
                requested_permissions,
                prompt_id,
            );
            0
        }

        fn on_dismiss_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            result: PermissionRequestResult,
        ) {
            log::debug!(
                "[browser::permission] permission prompt dismissed (prompt_id={}, result={:?})",
                prompt_id,
                result,
            );
        }
    }
}

impl PermissionHandlerBuilder {
    pub fn build(handler: OsrPermissionHandler) -> cef::PermissionHandler {
        Self::new(handler)
    }
}
