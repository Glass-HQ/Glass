//! Browser Tab Entity
//!
//! GPUI Entity wrapping a CEF Browser instance. Owns all navigation state
//! and drains the event channel from CEF handlers to emit GPUI events.

use crate::client::ClientBuilder;
use crate::events::{self, BrowserEvent, EventReceiver};
use crate::render_handler::RenderState;
use anyhow::{Context as _, Result};
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame, MouseButtonType};
use core_video::pixel_buffer::CVPixelBuffer;
use gpui::{Context, EventEmitter};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;

/// Events emitted by BrowserTab to subscribers (toolbar, browser_view).
pub enum TabEvent {
    AddressChanged(String),
    TitleChanged(String),
    LoadingStateChanged,
    FrameReady,
    NavigateToUrl(String),
    LoadError {
        url: String,
        error_code: i32,
        error_text: String,
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
}

impl EventEmitter<TabEvent> for BrowserTab {}

impl BrowserTab {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        log::info!("[browser::tab] BrowserTab::new()");
        let render_state = Arc::new(Mutex::new(RenderState::default()));
        let (sender, receiver) = events::event_channel();
        let client = ClientBuilder::build(render_state.clone(), sender);

        Self {
            browser: None,
            client,
            render_state,
            event_receiver: receiver,
            url: String::from("about:blank"),
            title: String::from("New Tab"),
            is_loading: false,
            can_go_back: false,
            can_go_forward: false,
            loading_progress: 0.0,
        }
    }

    pub fn drain_events(&mut self, cx: &mut Context<Self>) {
        let start = Instant::now();
        let mut count = 0u32;
        while let Ok(event) = self.event_receiver.try_recv() {
            count += 1;
            match event {
                BrowserEvent::AddressChanged(url) => {
                    log::info!("[browser::tab] drain_events: AddressChanged({})", url);
                    self.url.clone_from(&url);
                    cx.emit(TabEvent::AddressChanged(url));
                }
                BrowserEvent::TitleChanged(title) => {
                    log::info!("[browser::tab] drain_events: TitleChanged({})", title);
                    self.title.clone_from(&title);
                    cx.emit(TabEvent::TitleChanged(title));
                }
                BrowserEvent::LoadingStateChanged {
                    is_loading,
                    can_go_back,
                    can_go_forward,
                } => {
                    log::info!("[browser::tab] drain_events: LoadingStateChanged(loading={}, back={}, fwd={})",
                        is_loading, can_go_back, can_go_forward);
                    self.is_loading = is_loading;
                    self.can_go_back = can_go_back;
                    self.can_go_forward = can_go_forward;
                    cx.emit(TabEvent::LoadingStateChanged);
                }
                BrowserEvent::LoadingProgress(progress) => {
                    log::info!("[browser::tab] drain_events: LoadingProgress({:.2})", progress);
                    self.loading_progress = progress;
                }
                BrowserEvent::FrameReady => {
                    log::info!("[browser::tab] drain_events: FrameReady");
                    cx.emit(TabEvent::FrameReady);
                }
                BrowserEvent::BrowserCreated => {
                    log::info!("[browser::tab] drain_events: BrowserCreated");
                }
                BrowserEvent::PopupRequested(url) => {
                    log::info!("[browser::tab] drain_events: PopupRequested({})", url);
                    cx.emit(TabEvent::NavigateToUrl(url));
                }
                BrowserEvent::LoadError {
                    url,
                    error_code,
                    error_text,
                } => {
                    log::info!("[browser::tab] drain_events: LoadError(url={}, code={}, text={})", url, error_code, error_text);
                    cx.emit(TabEvent::LoadError {
                        url,
                        error_code,
                        error_text,
                    });
                }
            }
        }
        if count > 0 {
            log::info!("[browser::tab] drain_events() drained {} events ({:?})", count, start.elapsed());
        }
    }

    pub fn create_browser(&mut self, initial_url: &str) -> Result<()> {
        log::info!("[browser::tab] create_browser({})", initial_url);
        let start = Instant::now();

        if self.browser.is_some() {
            log::info!("[browser::tab] create_browser() already exists");
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

        let t0 = Instant::now();
        let browser = cef::browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut self.client.clone()),
            Some(&url),
            Some(&browser_settings),
            None,
            None,
        )
        .context("Failed to create CEF browser")?;
        let create_time = t0.elapsed();

        self.url = initial_url.to_string();
        self.browser = Some(browser);

        let t1 = Instant::now();
        self.with_host(|host| {
            host.was_resized();
        });
        let resize_time = t1.elapsed();

        log::info!("[browser::tab] create_browser() DONE create={:?} resize={:?} total={:?}",
            create_time, resize_time, start.elapsed());

        Ok(())
    }

    pub fn navigate(&mut self, url: &str) {
        log::info!("[browser::tab] navigate({})", url);
        if let Some(browser) = &self.browser {
            if let Some(frame) = browser.main_frame() {
                let url_string = cef::CefString::from(url);
                frame.load_url(Some(&url_string));
                self.url = url.to_string();
                self.is_loading = true;
            }
        }
    }

    pub fn reload(&mut self) {
        log::info!("[browser::tab] reload()");
        if let Some(browser) = &self.browser {
            browser.reload();
            self.is_loading = true;
        }
    }

    pub fn stop(&mut self) {
        log::info!("[browser::tab] stop()");
        if let Some(browser) = &self.browser {
            browser.stop_load();
            self.is_loading = false;
        }
    }

    pub fn go_back(&mut self) {
        log::info!("[browser::tab] go_back()");
        if let Some(browser) = &self.browser {
            if self.can_go_back {
                browser.go_back();
            }
        }
    }

    pub fn go_forward(&mut self) {
        log::info!("[browser::tab] go_forward()");
        if let Some(browser) = &self.browser {
            if self.can_go_forward {
                browser.go_forward();
            }
        }
    }

    pub fn copy(&self) {
        log::info!("[browser::tab] copy()");
        self.with_focused_frame(|frame| frame.copy());
    }

    pub fn cut(&self) {
        log::info!("[browser::tab] cut()");
        self.with_focused_frame(|frame| frame.cut());
    }

    pub fn paste(&self) {
        log::info!("[browser::tab] paste()");
        self.with_focused_frame(|frame| frame.paste());
    }

    pub fn undo(&self) {
        log::info!("[browser::tab] undo()");
        self.with_focused_frame(|frame| frame.undo());
    }

    pub fn redo(&self) {
        log::info!("[browser::tab] redo()");
        self.with_focused_frame(|frame| frame.redo());
    }

    pub fn select_all(&self) {
        log::info!("[browser::tab] select_all()");
        self.with_focused_frame(|frame| frame.select_all());
    }

    pub fn open_devtools(&self) {
        log::info!("[browser::tab] open_devtools()");
        self.with_host(|host| {
            let window_info = cef::WindowInfo::default();
            let settings = cef::BrowserSettings::default();
            let point = cef::Point { x: 0, y: 0 };
            host.show_dev_tools(Some(&window_info), None, Some(&settings), Some(&point));
        });
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        log::info!("[browser::tab] set_size({}x{})", width, height);
        let start = Instant::now();
        {
            let mut state = self.render_state.lock();
            state.width = width;
            state.height = height;
        }
        self.with_host(|host| {
            host.was_resized();
        });
        log::info!("[browser::tab] set_size() DONE ({:?})", start.elapsed());
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        log::info!("[browser::tab] set_scale_factor({})", scale);
        self.render_state.lock().scale_factor = scale;
    }

    pub fn invalidate(&self) {
        log::info!("[browser::tab] invalidate()");
        self.with_host(|host| {
            host.invalidate(cef::PaintElementType::default());
        });
    }

    pub fn set_focus(&self, focus: bool) {
        log::info!("[browser::tab] set_focus({})", focus);
        self.with_host(|host| {
            host.set_focus(if focus { 1 } else { 0 });
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
        log::info!("[browser::tab] send_mouse_click({}, {}, {:?}, down={}, clicks={}, mods={})",
            x, y, button, is_down, click_count, modifiers);
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
        log::info!("[browser::tab] send_mouse_move({}, {}, leave={}, mods={})", x, y, mouse_leave, modifiers);
        self.with_host(|host| {
            let event = cef::MouseEvent { x, y, modifiers };
            host.send_mouse_move_event(Some(&event), if mouse_leave { 1 } else { 0 });
        });
    }

    pub fn send_mouse_wheel(&self, x: i32, y: i32, delta_x: i32, delta_y: i32, modifiers: u32) {
        log::info!("[browser::tab] send_mouse_wheel({}, {}, dx={}, dy={}, mods={})", x, y, delta_x, delta_y, modifiers);
        self.with_host(|host| {
            let event = cef::MouseEvent { x, y, modifiers };
            host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
        });
    }

    pub fn send_key_event(&self, event: &cef::KeyEvent) {
        log::info!("[browser::tab] send_key_event(type={:?}, wkc={}, native={}, char={})",
            event.type_, event.windows_key_code, event.native_key_code, event.character);
        self.with_host(|host| {
            host.send_key_event(Some(event));
        });
    }

    pub fn current_frame(&self) -> Option<CVPixelBuffer> {
        let start = Instant::now();
        let frame = self.render_state.lock().current_frame.clone();
        let elapsed = start.elapsed();
        if elapsed.as_micros() > 50 {
            log::info!("[browser::tab] current_frame() lock+clone took {:?}", elapsed);
        }
        frame
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
        log::info!("[browser::tab] BrowserTab::drop()");
        if let Some(browser) = self.browser.take() {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        }
    }
}
