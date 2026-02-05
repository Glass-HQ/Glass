//! macOS CEF Protocol Patching
//!
//! CEF on macOS requires the NSApplication instance to conform to CrAppProtocol,
//! CrAppControlProtocol, and CefAppProtocol. These protocols provide
//! `isHandlingSendEvent` / `setHandlingSendEvent:` methods that CEF uses internally
//! to detect re-entrant event handling. GPUI's GPUIApplication class does not
//! implement these protocols, so we patch them in at runtime using the Objective-C
//! runtime after the CEF library has been loaded (which registers the protocol
//! definitions).

use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, Ordering};

type ObjcClass = *mut std::ffi::c_void;
type ObjcProtocol = *mut std::ffi::c_void;
type ObjcSel = *mut std::ffi::c_void;
type ObjcImp = *const std::ffi::c_void;
type ObjcId = *mut std::ffi::c_void;

#[repr(C)]
struct ObjcSuper {
    receiver: ObjcId,
    super_class: ObjcClass,
}

unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> ObjcClass;
    fn class_getSuperclass(cls: ObjcClass) -> ObjcClass;
    fn objc_getProtocol(name: *const c_char) -> ObjcProtocol;
    fn class_addProtocol(cls: ObjcClass, protocol: ObjcProtocol) -> bool;
    fn class_addMethod(
        cls: ObjcClass,
        name: ObjcSel,
        imp: ObjcImp,
        types: *const c_char,
    ) -> bool;
    fn sel_registerName(name: *const c_char) -> ObjcSel;
    // Declared with concrete signature to avoid ARM64 variadic ABI issues.
    // On ARM64, variadic function args go on the stack but ObjC messages
    // expect args in registers.
    fn objc_msgSendSuper(sup: *mut ObjcSuper, sel: ObjcSel, event: ObjcId);
}

static HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);
static mut GPUI_APPLICATION_CLASS: ObjcClass = std::ptr::null_mut();

extern "C" fn is_handling_send_event(_this: ObjcId, _sel: ObjcSel) -> i8 {
    if HANDLING_SEND_EVENT.load(Ordering::Relaxed) {
        1
    } else {
        0
    }
}

extern "C" fn set_handling_send_event(_this: ObjcId, _sel: ObjcSel, value: i8) {
    HANDLING_SEND_EVENT.store(value != 0, Ordering::Relaxed);
}

extern "C" fn send_event_override(this: ObjcId, _sel: ObjcSel, event: ObjcId) {
    let was_handling = HANDLING_SEND_EVENT.load(Ordering::Relaxed);
    if !was_handling {
        HANDLING_SEND_EVENT.store(true, Ordering::Relaxed);
    }

    unsafe {
        let super_class = class_getSuperclass(GPUI_APPLICATION_CLASS);
        let mut sup = ObjcSuper {
            receiver: this,
            super_class,
        };
        let sel = sel_registerName(c"sendEvent:".as_ptr());
        objc_msgSendSuper(&mut sup, sel, event);
    }

    if !was_handling {
        HANDLING_SEND_EVENT.store(false, Ordering::Relaxed);
    }
}

/// Patch GPUIApplication with CEF protocol conformance. Must be called after
/// the CEF library is loaded so the protocol definitions are available.
pub fn add_cef_protocols_to_nsapp() {
    unsafe {
        let cls = objc_getClass(c"GPUIApplication".as_ptr());
        if cls.is_null() {
            log::warn!("GPUIApplication class not found, cannot add CEF protocol conformance");
            return;
        }

        GPUI_APPLICATION_CLASS = cls;

        let sel_is = sel_registerName(c"isHandlingSendEvent".as_ptr());
        class_addMethod(
            cls,
            sel_is,
            is_handling_send_event as ObjcImp,
            c"c@:".as_ptr(),
        );

        let sel_set = sel_registerName(c"setHandlingSendEvent:".as_ptr());
        class_addMethod(
            cls,
            sel_set,
            set_handling_send_event as ObjcImp,
            c"v@:c".as_ptr(),
        );

        // Uses class_addMethod (not method_setImplementation) so only
        // GPUIApplication is affected, not NSApplication globally. Calls
        // through to super via objc_msgSendSuper.
        let sel_send = sel_registerName(c"sendEvent:".as_ptr());
        class_addMethod(
            cls,
            sel_send,
            send_event_override as ObjcImp,
            c"v@:@".as_ptr(),
        );

        for proto_name in [
            c"CrAppProtocol",
            c"CrAppControlProtocol",
            c"CefAppProtocol",
        ] {
            let proto = objc_getProtocol(proto_name.as_ptr());
            if !proto.is_null() {
                class_addProtocol(cls, proto);
            } else {
                log::warn!(
                    "CEF protocol {} not found in loaded libraries",
                    proto_name.to_str().unwrap_or("?")
                );
            }
        }

        log::info!("Added CEF protocol conformance to GPUIApplication");
    }
}
