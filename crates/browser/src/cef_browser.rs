//! CEF Browser Entity
//!
//! GPUI Entity wrapping a CEF Browser instance with navigation
//! and rendering capabilities.

use crate::cef_client::ClientBuilder;
use crate::cef_load_handler::LoadState;
use crate::cef_render_handler::RenderState;
use anyhow::{Context as _, Result};
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame};
use gpui::{Context, RenderImage};
use parking_lot::Mutex;
use std::sync::Arc;

pub struct CefBrowser {
    browser: Option<cef::Browser>,
    client: cef::Client,
    render_state: Arc<Mutex<RenderState>>,
    load_state: Arc<Mutex<LoadState>>,
}

impl CefBrowser {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let render_state = Arc::new(Mutex::new(RenderState::default()));
        let load_state = Arc::new(Mutex::new(LoadState::default()));
        let client = ClientBuilder::build(render_state.clone(), load_state.clone());

        Self {
            browser: None,
            client,
            render_state,
            load_state,
        }
    }

    pub fn create_browser(&mut self, initial_url: &str) -> Result<()> {
        if self.browser.is_some() {
            return Ok(());
        }

        let window_info = cef::WindowInfo {
            windowless_rendering_enabled: 1,
            ..Default::default()
        };

        let browser_settings = cef::BrowserSettings {
            windowless_frame_rate: 60,
            ..Default::default()
        };

        let url = cef::CefString::from(initial_url);

        log::info!("Creating CEF browser for URL: {}", initial_url);

        let browser = cef::browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut self.client.clone()),
            Some(&url),
            Some(&browser_settings),
            None,
            None,
        ).context("Failed to create CEF browser")?;

        {
            let mut state = self.load_state.lock();
            state.url = initial_url.to_string();
        }
        self.browser = Some(browser);

        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                log::info!("Browser created, triggering initial resize");
                host.was_resized();
            }
        }

        Ok(())
    }

    pub fn navigate(&mut self, url: &str) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.main_frame() {
                let url_string = cef::CefString::from(url);
                frame.load_url(Some(&url_string));
                {
                    let mut state = self.load_state.lock();
                    state.url = url.to_string();
                    state.is_loading = true;
                }
            }
        }
    }

    pub fn reload(&mut self) {
        if let Some(browser) = &self.browser {
            browser.reload();
            {
                let mut state = self.load_state.lock();
                state.is_loading = true;
            }
        }
    }

    pub fn reload_ignore_cache(&mut self) {
        if let Some(browser) = &self.browser {
            browser.reload_ignore_cache();
            {
                let mut state = self.load_state.lock();
                state.is_loading = true;
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(browser) = &self.browser {
            browser.stop_load();
            {
                let mut state = self.load_state.lock();
                state.is_loading = false;
            }
        }
    }

    pub fn go_back(&mut self) {
        if let Some(browser) = &self.browser {
            let can_go = self.load_state.lock().can_go_back;
            if can_go {
                browser.go_back();
                log::info!("Going back");
            } else {
                log::debug!("Cannot go back - no history");
            }
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(browser) = &self.browser {
            let can_go = self.load_state.lock().can_go_forward;
            if can_go {
                browser.go_forward();
                log::info!("Going forward");
            } else {
                log::debug!("Cannot go forward - no forward history");
            }
        }
    }

    pub fn execute_javascript(&self, code: &str) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.main_frame() {
                let script = cef::CefString::from(code);
                let url = cef::CefString::from("");
                frame.execute_java_script(Some(&script), Some(&url), 0);
            }
        }
    }

    pub fn copy(&self) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.focused_frame() {
                frame.copy();
            }
        }
    }

    pub fn cut(&self) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.focused_frame() {
                frame.cut();
            }
        }
    }

    pub fn paste(&self) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.focused_frame() {
                frame.paste();
            }
        }
    }

    pub fn undo(&self) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.focused_frame() {
                frame.undo();
            }
        }
    }

    pub fn redo(&self) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.focused_frame() {
                frame.redo();
            }
        }
    }

    pub fn select_all(&self) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.focused_frame() {
                frame.select_all();
            }
        }
    }

    pub fn open_devtools(&self) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                let window_info = cef::WindowInfo::default();
                let settings = cef::BrowserSettings::default();
                let point = cef::Point { x: 0, y: 0 };
                host.show_dev_tools(Some(&window_info), None, Some(&settings), Some(&point));
            }
        }
    }

    pub fn close_devtools(&self) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                host.close_dev_tools();
            }
        }
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        {
            let mut state = self.render_state.lock();
            state.width = width;
            state.height = height;
        }

        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                host.was_resized();
            }
        }
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        let mut state = self.render_state.lock();
        state.scale_factor = scale;
    }

    pub fn scale_factor(&self) -> f32 {
        self.render_state.lock().scale_factor
    }

    pub fn invalidate(&self) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                host.invalidate(cef::PaintElementType::default());
            }
        }
    }

    pub fn set_focus(&self, focus: bool) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                host.set_focus(if focus { 1 } else { 0 });
            }
        }
    }

    pub fn send_mouse_click(
        &self,
        x: i32,
        y: i32,
        button: MouseButton,
        is_down: bool,
        click_count: i32,
    ) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                let event = cef::MouseEvent {
                    x,
                    y,
                    modifiers: 0,
                };
                let button_type = match button {
                    MouseButton::Left => cef::sys::cef_mouse_button_type_t::MBT_LEFT,
                    MouseButton::Middle => cef::sys::cef_mouse_button_type_t::MBT_MIDDLE,
                    MouseButton::Right => cef::sys::cef_mouse_button_type_t::MBT_RIGHT,
                };
                host.send_mouse_click_event(
                    Some(&event),
                    button_type.into(),
                    if is_down { 0 } else { 1 },
                    click_count,
                );
            }
        }
    }

    pub fn send_mouse_move(&self, x: i32, y: i32, mouse_leave: bool) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                let event = cef::MouseEvent {
                    x,
                    y,
                    modifiers: 0,
                };
                host.send_mouse_move_event(Some(&event), if mouse_leave { 1 } else { 0 });
            }
        }
    }

    pub fn send_mouse_wheel(&self, x: i32, y: i32, delta_x: i32, delta_y: i32) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                let event = cef::MouseEvent {
                    x,
                    y,
                    modifiers: 0,
                };
                host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
            }
        }
    }

    pub fn send_key_event(&self, event: &CefKeyEvent) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                let cef_event = cef::KeyEvent {
                    size: std::mem::size_of::<cef::sys::_cef_key_event_t>(),
                    type_: event.event_type,
                    modifiers: event.modifiers,
                    windows_key_code: event.windows_key_code,
                    native_key_code: event.native_key_code,
                    is_system_key: event.is_system_key,
                    character: event.character,
                    unmodified_character: event.unmodified_character,
                    focus_on_editable_field: event.focus_on_editable_field,
                };
                host.send_key_event(Some(&cef_event));
            } else {
                log::warn!("[CEF] send_key_event: no host available");
            }
        } else {
            log::warn!("[CEF] send_key_event: no browser available");
        }
    }

    pub fn ime_commit_text(&self, text: &str) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                let cef_text = cef::CefString::from(text);
                host.ime_commit_text(Some(&cef_text), None, 0);
            }
        }
    }

    pub fn current_frame(&self) -> Option<Arc<RenderImage>> {
        self.render_state.lock().current_frame.clone()
    }

    pub fn url(&self) -> String {
        self.load_state.lock().url.clone()
    }

    pub fn title(&self) -> String {
        self.load_state.lock().title.clone()
    }

    pub fn is_loading(&self) -> bool {
        self.load_state.lock().is_loading
    }

    pub fn can_go_back(&self) -> bool {
        self.load_state.lock().can_go_back
    }

    pub fn can_go_forward(&self) -> bool {
        self.load_state.lock().can_go_forward
    }

    pub fn host(&self) -> Option<cef::BrowserHost> {
        self.browser.as_ref().and_then(|b| b.host())
    }
}

impl Drop for CefBrowser {
    fn drop(&mut self) {
        if let Some(browser) = self.browser.take() {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

pub struct CefKeyEvent {
    pub event_type: cef::KeyEventType,
    pub modifiers: u32,
    pub windows_key_code: i32,
    pub native_key_code: i32,
    pub is_system_key: i32,
    pub character: u16,
    pub unmodified_character: u16,
    pub focus_on_editable_field: i32,
}

impl Default for CefKeyEvent {
    fn default() -> Self {
        Self {
            event_type: cef::KeyEventType::RAWKEYDOWN,
            modifiers: 0,
            windows_key_code: 0,
            native_key_code: 0,
            is_system_key: 0,
            character: 0,
            unmodified_character: 0,
            focus_on_editable_field: 1,
        }
    }
}
