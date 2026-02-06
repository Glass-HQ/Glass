//! Browser Tab Entity
//!
//! GPUI Entity wrapping a CEF Browser instance. Owns all navigation state
//! and drains the event channel from CEF handlers to emit GPUI events.

use crate::client::{ClientBuilder, MANUAL_KEY_EVENT};
use crate::context_menu_handler::ContextMenuContext;
use crate::events::{self, BrowserEvent, EventReceiver};
use crate::render_handler::RenderState;
use anyhow::{Context as _, Result};
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame, MouseButtonType};
use core_video::pixel_buffer::CVPixelBuffer;
use gpui::{Context, EventEmitter};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Events emitted by BrowserTab to subscribers (toolbar, browser_view).
pub enum TabEvent {
    AddressChanged(String),
    TitleChanged(String),
    LoadingStateChanged,
    FrameReady,
    NavigateToUrl(String),
    OpenNewTab(String),
    FaviconChanged(Option<String>),
    LoadError {
        url: String,
        error_code: i32,
        error_text: String,
    },
    ContextMenuOpen {
        context: ContextMenuContext,
    },
}

pub struct BrowserTab {
    browser: Option<cef::Browser>,
    client: cef::Client,
    render_state: Arc<Mutex<RenderState>>,
    event_receiver: EventReceiver,
    url: String,
    title: String,
    is_loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    loading_progress: f64,
    is_new_tab_page: bool,
    is_pinned: bool,
    favicon_url: Option<String>,
    pending_url: Option<String>,
}

impl EventEmitter<TabEvent> for BrowserTab {}

impl BrowserTab {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let render_state = Arc::new(Mutex::new(RenderState::default()));
        let (sender, receiver) = events::event_channel();
        let client = ClientBuilder::build(render_state.clone(), sender);

        Self {
            browser: None,
            client,
            render_state,
            event_receiver: receiver,
            url: String::from("glass://newtab"),
            title: String::from("New Tab"),
            is_loading: false,
            can_go_back: false,
            can_go_forward: false,
            loading_progress: 0.0,
            is_new_tab_page: true,
            is_pinned: false,
            favicon_url: None,
            pending_url: None,
        }
    }

    pub fn new_with_state(
        url: String,
        title: String,
        is_new_tab_page: bool,
        favicon_url: Option<String>,
        _cx: &mut Context<Self>,
    ) -> Self {
        let render_state = Arc::new(Mutex::new(RenderState::default()));
        let (sender, receiver) = events::event_channel();
        let client = ClientBuilder::build(render_state.clone(), sender);

        Self {
            browser: None,
            client,
            render_state,
            event_receiver: receiver,
            url,
            title,
            is_loading: false,
            can_go_back: false,
            can_go_forward: false,
            loading_progress: 0.0,
            is_new_tab_page,
            is_pinned: false,
            favicon_url,
            pending_url: None,
        }
    }

    pub fn drain_events(&mut self, cx: &mut Context<Self>) {
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                BrowserEvent::AddressChanged(url) => {
                    self.url.clone_from(&url);
                    cx.emit(TabEvent::AddressChanged(url));
                }
                BrowserEvent::TitleChanged(title) => {
                    self.title.clone_from(&title);
                    cx.emit(TabEvent::TitleChanged(title));
                }
                BrowserEvent::LoadingStateChanged {
                    is_loading,
                    can_go_back,
                    can_go_forward,
                } => {
                    self.is_loading = is_loading;
                    self.can_go_back = can_go_back;
                    self.can_go_forward = can_go_forward;
                    cx.emit(TabEvent::LoadingStateChanged);
                }
                BrowserEvent::LoadingProgress(progress) => {
                    self.loading_progress = progress;
                }
                BrowserEvent::FrameReady => {
                    cx.emit(TabEvent::FrameReady);
                }
                BrowserEvent::BrowserCreated => {}
                BrowserEvent::PopupRequested(url) => {
                    cx.emit(TabEvent::OpenNewTab(url));
                }
                BrowserEvent::LoadError {
                    url,
                    error_code,
                    error_text,
                } => {
                    cx.emit(TabEvent::LoadError {
                        url,
                        error_code,
                        error_text,
                    });
                }
                BrowserEvent::ContextMenuRequested { context } => {
                    cx.emit(TabEvent::ContextMenuOpen { context });
                }
                BrowserEvent::FaviconUrlChanged(urls) => {
                    self.favicon_url = urls.into_iter().next();
                    cx.emit(TabEvent::FaviconChanged(self.favicon_url.clone()));
                }
            }
        }
    }

    pub fn create_browser(&mut self, initial_url: &str) -> Result<()> {
        if self.browser.is_some() {
            return Ok(());
        }

        let window_info = cef::WindowInfo {
            windowless_rendering_enabled: 1,
            shared_texture_enabled: 1,
            ..Default::default()
        };

        let browser_settings = cef::BrowserSettings {
            windowless_frame_rate: 60,
            ..Default::default()
        };

        let url = cef::CefString::from(initial_url);

        let browser = cef::browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut self.client.clone()),
            Some(&url),
            Some(&browser_settings),
            None,
            None,
        )
        .context("Failed to create CEF browser")?;

        self.url = initial_url.to_string();
        self.browser = Some(browser);

        self.with_host(|host| {
            host.was_resized();
        });

        Ok(())
    }

    pub fn navigate(&mut self, url: &str, cx: &mut Context<Self>) {
        self.favicon_url = None;
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.main_frame() {
                let url_string = cef::CefString::from(url);
                frame.load_url(Some(&url_string));
                self.url = url.to_string();
                self.is_loading = true;
            }
        } else {
            self.url = url.to_string();
            cx.emit(TabEvent::NavigateToUrl(url.to_string()));
        }
    }

    pub fn reload(&mut self) {
        if let Some(browser) = &self.browser {
            browser.reload();
            self.is_loading = true;
        }
    }

    pub fn stop(&mut self) {
        if let Some(browser) = &self.browser {
            browser.stop_load();
            self.is_loading = false;
        }
    }

    pub fn go_back(&mut self) {
        if let Some(browser) = &self.browser {
            if self.can_go_back {
                browser.go_back();
            }
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(browser) = &self.browser {
            if self.can_go_forward {
                browser.go_forward();
            }
        }
    }

    pub fn copy(&self) {
        self.with_focused_frame(|frame| frame.copy());
    }

    pub fn cut(&self) {
        self.with_focused_frame(|frame| frame.cut());
    }

    pub fn paste(&self) {
        self.with_focused_frame(|frame| frame.paste());
    }

    pub fn undo(&self) {
        self.with_focused_frame(|frame| frame.undo());
    }

    pub fn redo(&self) {
        self.with_focused_frame(|frame| frame.redo());
    }

    pub fn select_all(&self) {
        self.with_focused_frame(|frame| frame.select_all());
    }

    pub fn delete(&self) {
        self.with_focused_frame(|frame| frame.del());
    }

    pub fn open_devtools(&self) {
        self.with_host(|host| {
            let window_info = cef::WindowInfo::default();
            let settings = cef::BrowserSettings::default();
            let point = cef::Point { x: 0, y: 0 };
            host.show_dev_tools(Some(&window_info), None, Some(&settings), Some(&point));
        });
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        {
            let mut state = self.render_state.lock();
            state.width = width;
            state.height = height;
        }
        self.with_host(|host| {
            host.was_resized();
        });
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        self.render_state.lock().scale_factor = scale;
    }

    pub fn invalidate(&self) {
        self.with_host(|host| {
            host.invalidate(cef::PaintElementType::default());
        });
    }

    pub fn set_focus(&self, focus: bool) {
        self.with_host(|host| {
            host.set_focus(if focus { 1 } else { 0 });
        });
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.with_host(|host| {
            host.was_hidden(if hidden { 1 } else { 0 });
        });
    }

    pub fn set_audio_muted(&self, muted: bool) {
        self.with_host(|host| {
            host.set_audio_muted(if muted { 1 } else { 0 });
        });
    }

    pub fn send_mouse_click(
        &self,
        x: i32,
        y: i32,
        button: MouseButtonType,
        is_down: bool,
        click_count: i32,
        modifiers: u32,
    ) {
        self.with_host(|host| {
            let event = cef::MouseEvent { x, y, modifiers };
            host.send_mouse_click_event(
                Some(&event),
                button,
                if is_down { 0 } else { 1 },
                click_count,
            );
        });
    }

    pub fn send_mouse_move(&self, x: i32, y: i32, mouse_leave: bool, modifiers: u32) {
        self.with_host(|host| {
            let event = cef::MouseEvent { x, y, modifiers };
            host.send_mouse_move_event(Some(&event), if mouse_leave { 1 } else { 0 });
        });
    }

    pub fn send_mouse_wheel(&self, x: i32, y: i32, delta_x: i32, delta_y: i32, modifiers: u32) {
        self.with_host(|host| {
            let event = cef::MouseEvent { x, y, modifiers };
            host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
        });
    }

    pub fn send_key_event(&self, event: &cef::KeyEvent) {
        log::info!("[browser::tab] send_key_event(type={:?}, wkc={}, native={}, char={})",
            event.type_, event.windows_key_code, event.native_key_code, event.character);
        let needs_flag = matches!(
            event.type_,
            cef::KeyEventType::RAWKEYDOWN | cef::KeyEventType::CHAR
        );
        self.with_host(|host| {
            if needs_flag {
                MANUAL_KEY_EVENT.store(true, Ordering::Relaxed);
            }
            host.send_key_event(Some(event));
            if needs_flag {
                MANUAL_KEY_EVENT.store(false, Ordering::Relaxed);
            }
        });
    }

    pub fn current_frame(&self) -> Option<CVPixelBuffer> {
        self.render_state.lock().current_frame.clone()
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn can_go_back(&self) -> bool {
        self.can_go_back
    }

    pub fn can_go_forward(&self) -> bool {
        self.can_go_forward
    }

    pub fn favicon_url(&self) -> Option<&str> {
        self.favicon_url.as_deref()
    }

    pub fn is_new_tab_page(&self) -> bool {
        self.is_new_tab_page
    }

    pub fn set_new_tab_page(&mut self, value: bool) {
        self.is_new_tab_page = value;
    }

    pub fn set_pending_url(&mut self, url: String) {
        self.url = url.clone();
        self.title = url.clone();
        self.pending_url = Some(url);
    }

    pub fn take_pending_url(&mut self) -> Option<String> {
        self.pending_url.take()
    }

    pub fn has_pending_url(&self) -> bool {
        self.pending_url.is_some()
    }

    pub fn is_pinned(&self) -> bool {
        self.is_pinned
    }

    pub fn set_pinned(&mut self, value: bool) {
        self.is_pinned = value;
    }

    pub fn close_browser(&mut self) {
        if let Some(browser) = self.browser.take() {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        }
    }

    fn with_host(&self, callback: impl FnOnce(&cef::BrowserHost)) {
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                callback(&host);
            }
        }
    }

    fn with_focused_frame(&self, callback: impl FnOnce(&cef::Frame)) {
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.focused_frame() {
                callback(&frame);
            }
        }
    }
}

impl Drop for BrowserTab {
    fn drop(&mut self) {
        if let Some(browser) = self.browser.take() {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        }
    }
}
