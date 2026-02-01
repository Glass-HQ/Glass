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
    api_hash, rc::Rc as _, sys, wrap_app, wrap_browser_process_handler, App, BrowserProcessHandler,
    ImplApp, ImplBrowserProcessHandler, ImplCommandLine, WrapApp, WrapBrowserProcessHandler,
};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static CEF_SUBPROCESS_HANDLED: AtomicBool = AtomicBool::new(false);
static CEF_INITIALIZED: AtomicBool = AtomicBool::new(false);
static CEF_CONTEXT_READY: AtomicBool = AtomicBool::new(false);
static CEF_INSTANCE: Mutex<Option<Arc<CefInstance>>> = Mutex::new(None);
static CEF_APP: Mutex<Option<cef::App>> = Mutex::new(None);

#[cfg(target_os = "macos")]
static CEF_LIBRARY_LOADER: Mutex<Option<cef::library_loader::LibraryLoader>> = Mutex::new(None);

// CEF App implementation
#[derive(Clone)]
struct GlassApp {}

impl GlassApp {
    fn new() -> Self {
        Self {}
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
            command_line.append_switch(Some(&"enable-logging=stderr".into()));

            #[cfg(debug_assertions)]
            {
                command_line.append_switch_with_value(
                    Some(&"remote-debugging-port".into()),
                    Some(&"9222".into()),
                );
            }
        }

        fn browser_process_handler(&self) -> Option<cef::BrowserProcessHandler> {
            Some(GlassBrowserProcessHandlerBuilder::build(
                GlassBrowserProcessHandler::new(),
            ))
        }
    }
}

impl GlassAppBuilder {
    fn build(app: GlassApp) -> cef::App {
        Self::new(app)
    }
}

// Browser Process Handler implementation
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
            log::info!("CEF context initialized - browser creation is now safe");
            CEF_CONTEXT_READY.store(true, Ordering::SeqCst);
        }

        fn on_before_child_process_launch(&self, command_line: Option<&mut cef::CommandLine>) {
            let Some(command_line) = command_line else {
                return;
            };

            command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
        }
    }
}

impl GlassBrowserProcessHandlerBuilder {
    fn build(handler: GlassBrowserProcessHandler) -> BrowserProcessHandler {
        Self::new(handler)
    }
}

pub struct CefInstance {
    _message_loop_task: Option<gpui::Task<()>>,
}

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
            // Load CEF library first on macOS
            let exe_path = std::env::current_exe()
                .map_err(|e| anyhow!("Failed to get current executable path: {}", e))?;

            // Check if framework exists before trying to load it
            // LibraryLoader::new() will panic if the path doesn't exist
            let framework_path = exe_path
                .parent()
                .and_then(|p| Some(p.join("../Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework")));

            match framework_path {
                Some(path) if path.exists() => {
                    let loader = cef::library_loader::LibraryLoader::new(&exe_path, false);

                    if !loader.load() {
                        // Library loading failed
                        return Ok(());
                    }

                    *CEF_LIBRARY_LOADER.lock() = Some(loader);
                }
                _ => {
                    // Framework not found - this is not an error for the main app,
                    // it just means CEF is not available (not running from bundle)
                    return Ok(());
                }
            }
        }

        // Initialize CEF API version check
        let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

        let args = cef::args::Args::new();
        let mut app = GlassAppBuilder::build(GlassApp::new());

        // execute_process returns:
        // - >= 0 for subprocesses (the exit code to use)
        // - -1 for the browser process (continue with initialization)
        let ret = cef::execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());

        if ret >= 0 {
            // This is a subprocess - exit immediately
            std::process::exit(ret);
        }

        // Store the app for reuse in initialize_cef()
        // CEF requires the same App instance for both execute_process and initialize
        *CEF_APP.lock() = Some(app);

        CEF_SUBPROCESS_HANDLED.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Initialize CEF for the browser process. Call this after handle_subprocess()
    /// has returned successfully and after GPUI is set up.
    pub fn initialize(cx: &mut gpui::App) -> Result<Arc<CefInstance>> {
        if CEF_INITIALIZED.load(Ordering::SeqCst) {
            if let Some(instance) = Self::global() {
                return Ok(instance);
            }
        }

        // Ensure subprocess handling was done
        if !CEF_SUBPROCESS_HANDLED.load(Ordering::SeqCst) {
            return Err(anyhow!(
                "CEF subprocess handling was not done. Call CefInstance::handle_subprocess() early in main()."
            ));
        }

        log::info!("Initializing CEF...");

        Self::initialize_cef()?;

        CEF_INITIALIZED.store(true, Ordering::SeqCst);

        let message_loop_task = Self::start_message_loop(cx);

        let instance = Arc::new(CefInstance {
            _message_loop_task: Some(message_loop_task),
        });

        *CEF_INSTANCE.lock() = Some(instance.clone());

        log::info!("CEF initialized successfully");

        Ok(instance)
    }

    fn initialize_cef() -> Result<()> {
        let args = cef::args::Args::new();

        // Reuse the same App instance from handle_subprocess()
        // CEF requires the same App for both execute_process and initialize
        let mut app_guard = CEF_APP.lock();
        let app = app_guard.as_mut().ok_or_else(|| {
            anyhow!("CEF App not found. handle_subprocess() must be called first.")
        })?;

        let mut settings = cef::Settings::default();

        settings.windowless_rendering_enabled = 1;
        settings.external_message_pump = 1;
        settings.no_sandbox = 1;
        settings.log_severity = cef::sys::cef_log_severity_t::LOGSEVERITY_WARNING.into();

        #[cfg(debug_assertions)]
        {
            settings.remote_debugging_port = 9222;
        }

        // Initialize CEF with the same app used in execute_process
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

    fn start_message_loop(_cx: &mut gpui::App) -> gpui::Task<()> {
        // Message loop pumping is done manually via pump_messages()
        // Called from BrowserView rendering to avoid conflicts with GPUI's event loop.
        gpui::Task::ready(())
    }

    /// Pump CEF message loop. Call this periodically from the main thread.
    /// Only pumps messages after CEF context is fully initialized.
    pub fn pump_messages() {
        // Only pump messages after on_context_initialized has fired
        if CEF_CONTEXT_READY.load(Ordering::SeqCst) {
            cef::do_message_loop_work();
        }
    }

    pub fn shutdown() {
        if !CEF_INITIALIZED.load(Ordering::SeqCst) {
            return;
        }

        log::info!("Shutting down CEF...");

        CEF_INITIALIZED.store(false, Ordering::SeqCst);
        CEF_CONTEXT_READY.store(false, Ordering::SeqCst);
        *CEF_INSTANCE.lock() = None;

        cef::shutdown();

        // Clear the app after shutdown
        *CEF_APP.lock() = None;

        log::info!("CEF shutdown complete");
    }
}

impl Drop for CefInstance {
    fn drop(&mut self) {
        Self::shutdown();
    }
}
