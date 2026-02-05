//! CEF Instance Management
//!
//! Manages the CEF (Chromium Embedded Framework) lifecycle as a singleton.
//! Handles initialization, message loop pumping, and shutdown.
//!
//! CEF initialization is split into two phases:
//! 1. `handle_subprocess()` - Must be called very early in main(), before any GUI
//!    initialization. This handles CEF subprocess execution.
//! 2. `initialize()` - Called later to complete CEF setup for the browser process.

use anyhow::{anyhow, Result};
use cef::{
    api_hash, rc::Rc as _, sys, wrap_app, wrap_browser_process_handler, App,
    BrowserProcessHandler, ImplApp, ImplBrowserProcessHandler, ImplCommandLine, WrapApp,
    WrapBrowserProcessHandler,
};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

static CEF_SUBPROCESS_HANDLED: AtomicBool = AtomicBool::new(false);
static CEF_INITIALIZED: AtomicBool = AtomicBool::new(false);
static CEF_CONTEXT_READY: AtomicBool = AtomicBool::new(false);
static CEF_INSTANCE: Mutex<Option<Arc<CefInstance>>> = Mutex::new(None);
static CEF_APP: Mutex<Option<cef::App>> = Mutex::new(None);

#[cfg(target_os = "macos")]
static CEF_LIBRARY_LOADER: Mutex<Option<cef::library_loader::LibraryLoader>> = Mutex::new(None);

// Pump scheduling: absolute time (microseconds since PUMP_EPOCH) when
// the next do_message_loop_work() call should happen. u64::MAX = idle.
static PUMP_EPOCH: OnceLock<Instant> = OnceLock::new();
static NEXT_PUMP_AT_US: AtomicU64 = AtomicU64::new(u64::MAX);

fn elapsed_us() -> u64 {
    PUMP_EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64
}

// ── Browser Process Handler ──────────────────────────────────────────
// Defined before GlassApp so a cached instance can be stored in it.

#[derive(Clone)]
struct GlassBrowserProcessHandler {}

impl GlassBrowserProcessHandler {
    fn new() -> Self {
        Self {}
    }
}

wrap_browser_process_handler! {
    struct GlassBrowserProcessHandlerBuilder {
        handler: GlassBrowserProcessHandler,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            CEF_CONTEXT_READY.store(true, Ordering::SeqCst);
        }

        fn on_before_child_process_launch(&self, command_line: Option<&mut cef::CommandLine>) {
            let Some(command_line) = command_line else {
                return;
            };
            command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
        }

        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            let target_us = elapsed_us() + (delay_ms.max(0) as u64) * 1000;
            NEXT_PUMP_AT_US.fetch_min(target_us, Ordering::Release);
        }
    }
}

impl GlassBrowserProcessHandlerBuilder {
    fn build(handler: GlassBrowserProcessHandler) -> BrowserProcessHandler {
        Self::new(handler)
    }
}

// ── CEF App ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct GlassApp {
    browser_process_handler: cef::BrowserProcessHandler,
}

impl GlassApp {
    fn new() -> Self {
        let handler =
            GlassBrowserProcessHandlerBuilder::build(GlassBrowserProcessHandler::new());
        Self {
            browser_process_handler: handler,
        }
    }
}

wrap_app! {
    struct GlassAppBuilder {
        app: GlassApp,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&cef::CefStringUtf16>,
            command_line: Option<&mut cef::CommandLine>,
        ) {
            let Some(command_line) = command_line else {
                return;
            };

            command_line.append_switch(Some(&"no-startup-window".into()));
            command_line.append_switch(Some(&"noerrdialogs".into()));
            command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
            command_line.append_switch(Some(&"use-mock-keychain".into()));
            command_line.append_switch(Some(&"disable-gpu-sandbox".into()));

            #[cfg(debug_assertions)]
            {
                command_line.append_switch(Some(&"enable-logging=stderr".into()));
                command_line.append_switch_with_value(
                    Some(&"remote-debugging-port".into()),
                    Some(&"9222".into()),
                );
            }
        }

        fn browser_process_handler(&self) -> Option<cef::BrowserProcessHandler> {
            Some(self.app.browser_process_handler.clone())
        }
    }
}

impl GlassAppBuilder {
    fn build(app: GlassApp) -> cef::App {
        Self::new(app)
    }
}

// ── CefInstance ──────────────────────────────────────────────────────

pub struct CefInstance {}

impl CefInstance {
    pub fn global() -> Option<Arc<CefInstance>> {
        CEF_INSTANCE.lock().clone()
    }

    /// Check if CEF context is initialized and ready for browser creation
    pub fn is_context_ready() -> bool {
        CEF_CONTEXT_READY.load(Ordering::SeqCst)
    }

    /// Handle CEF subprocess execution. This MUST be called very early in main(),
    /// before any GUI initialization.
    ///
    /// If this process is a CEF subprocess (renderer, GPU, etc.), this function
    /// will NOT return - it will call std::process::exit().
    ///
    /// If this is the main browser process, it returns Ok(()) and normal
    /// initialization should continue.
    pub fn handle_subprocess() -> Result<()> {
        if CEF_SUBPROCESS_HANDLED.load(Ordering::SeqCst) {
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            let exe_path = std::env::current_exe()
                .map_err(|e| anyhow!("Failed to get current executable path: {}", e))?;

            let framework_path = exe_path
                .parent()
                .map(|p| p.join("../Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework"));

            match framework_path {
                Some(path) if path.exists() => {
                    let loader = cef::library_loader::LibraryLoader::new(&exe_path, false);
                    if !loader.load() {
                        log::warn!("[browser::cef_instance] LibraryLoader::load() failed");
                        return Ok(());
                    }
                    *CEF_LIBRARY_LOADER.lock() = Some(loader);
                }
                _ => {
                    return Ok(());
                }
            }
        }

        let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

        let args = cef::args::Args::new();
        let mut app = GlassAppBuilder::build(GlassApp::new());

        let ret = cef::execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());

        if ret >= 0 {
            std::process::exit(ret);
        }

        *CEF_APP.lock() = Some(app);
        CEF_SUBPROCESS_HANDLED.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Initialize CEF for the browser process. Call this after handle_subprocess()
    /// has returned successfully and after GPUI is set up.
    pub fn initialize(_cx: &mut gpui::App) -> Result<Arc<CefInstance>> {
        if CEF_INITIALIZED.load(Ordering::SeqCst) {
            if let Some(instance) = Self::global() {
                return Ok(instance);
            }
        }

        if !CEF_SUBPROCESS_HANDLED.load(Ordering::SeqCst) {
            return Err(anyhow!(
                "CEF subprocess handling was not done. Call CefInstance::handle_subprocess() early in main()."
            ));
        }

        Self::initialize_cef()?;

        CEF_INITIALIZED.store(true, Ordering::SeqCst);
        let instance = Arc::new(CefInstance {});
        *CEF_INSTANCE.lock() = Some(instance.clone());

        Ok(instance)
    }

    fn initialize_cef() -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            crate::macos_protocol::add_cef_protocols_to_nsapp();
        }

        let args = cef::args::Args::new();

        let mut app_guard = CEF_APP.lock();
        let app = app_guard.as_mut().ok_or_else(|| {
            anyhow!("CEF App not found. handle_subprocess() must be called first.")
        })?;

        let mut settings = cef::Settings::default();

        settings.windowless_rendering_enabled = 1;
        settings.external_message_pump = 1;
        settings.no_sandbox = 1;
        settings.log_severity = cef::sys::cef_log_severity_t::LOGSEVERITY_WARNING.into();

        #[cfg(target_os = "macos")]
        {
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(macos_dir) = exe_path.parent() {
                    let helper_path = macos_dir
                        .join("../Frameworks/Glass Helper.app/Contents/MacOS/Glass Helper");
                    if let Ok(canonical) = helper_path.canonicalize() {
                        if let Some(path_str) = canonical.to_str() {
                            settings.browser_subprocess_path =
                                cef::CefString::from(path_str);
                        }
                    }
                }
            }
        }

        let cache_dir = paths::data_dir().join("browser_cache");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            log::warn!("[browser::cef_instance] Failed to create browser cache directory: {}", e);
        }
        if let Some(cache_path_str) = cache_dir.to_str() {
            settings.cache_path = cef::CefString::from(cache_path_str);
            settings.root_cache_path = cef::CefString::from(cache_path_str);
        }
        settings.persist_session_cookies = 1;

        #[cfg(debug_assertions)]
        {
            settings.remote_debugging_port = 9222;
        }

        let result = cef::initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(app),
            std::ptr::null_mut(),
        );

        if result != 1 {
            return Err(anyhow!("Failed to initialize CEF (error code: {})", result));
        }

        Ok(())
    }

    /// Returns true when CEF has requested a pump and the delay has elapsed.
    pub fn should_pump() -> bool {
        if !CEF_CONTEXT_READY.load(Ordering::SeqCst) {
            return false;
        }
        elapsed_us() >= NEXT_PUMP_AT_US.load(Ordering::Acquire)
    }

    /// Microseconds until the next scheduled pump, or 0 if overdue.
    pub fn time_until_next_pump_us() -> u64 {
        NEXT_PUMP_AT_US
            .load(Ordering::Acquire)
            .saturating_sub(elapsed_us())
    }

    /// Pump CEF message loop. Only call when `should_pump()` returns true.
    pub fn pump_messages() {
        if !CEF_CONTEXT_READY.load(Ordering::SeqCst) {
            return;
        }
        // Clear schedule before pumping. CEF will call
        // on_schedule_message_pump_work during do_message_loop_work
        // if more work is needed.
        NEXT_PUMP_AT_US.store(u64::MAX, Ordering::Release);
        cef::do_message_loop_work();
        // If CEF didn't schedule new work during the pump, set a
        // fallback so we check back periodically (~30 Hz idle).
        let _ = NEXT_PUMP_AT_US.compare_exchange(
            u64::MAX,
            elapsed_us() + 33_000,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );
    }

    pub fn shutdown() {
        if !CEF_INITIALIZED.load(Ordering::SeqCst) {
            return;
        }

        CEF_INITIALIZED.store(false, Ordering::SeqCst);
        CEF_CONTEXT_READY.store(false, Ordering::SeqCst);
        *CEF_INSTANCE.lock() = None;

        cef::shutdown();

        *CEF_APP.lock() = None;
    }
}

impl Drop for CefInstance {
    fn drop(&mut self) {
        Self::shutdown();
    }
}
