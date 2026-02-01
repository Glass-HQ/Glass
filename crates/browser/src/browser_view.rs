//! Browser View
//!
//! The main view for Browser Mode. Renders CEF browser content and handles
//! user input for navigation and interaction.

use crate::cef_browser::CefBrowser;
use crate::cef_instance::CefInstance;
use crate::input_handler;
use crate::toolbar::BrowserToolbar;
use gpui::{
    div, img, point, prelude::*, px, App, Bounds, Context, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Render,
    Styled, Task, Window,
};
use std::time::Duration;
use ui::{prelude::*, Icon, IconName, IconSize};

const DEFAULT_URL: &str = "https://www.google.com";
const CEF_MESSAGE_PUMP_INTERVAL_MS: u64 = 16;

pub struct BrowserView {
    focus_handle: FocusHandle,
    browser: Option<Entity<CefBrowser>>,
    toolbar: Option<Entity<BrowserToolbar>>,
    content_bounds: Bounds<Pixels>,
    cef_available: bool,
    browser_created: bool,
    message_pump_started: bool,
    _message_pump_task: Option<Task<()>>,
}

impl BrowserView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let cef_available = CefInstance::global().is_some();

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            browser: None,
            toolbar: None,
            content_bounds: Bounds::default(),
            cef_available,
            browser_created: false,
            message_pump_started: false,
            _message_pump_task: None,
        };

        if cef_available {
            this.initialize_browser_entity(cx);
        }

        this
    }

    fn start_message_pump(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(CEF_MESSAGE_PUMP_INTERVAL_MS))
                    .await;

                // Check if the entity still exists
                let entity_exists = this.upgrade().is_some();
                if !entity_exists {
                    break;
                }

                // Pump CEF messages on the main thread and request re-render
                cx.update(|cx| {
                    CefInstance::pump_messages();
                    if let Some(this) = this.upgrade() {
                        cx.notify(this.entity_id());
                    }
                });
            }
        })
    }

    fn initialize_browser_entity(&mut self, cx: &mut Context<Self>) {
        let browser = cx.new(|cx| CefBrowser::new(cx));
        self.browser = Some(browser);
    }

    fn ensure_toolbar_exists(&mut self) -> bool {
        self.toolbar.is_some()
    }

    fn create_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser.clone() {
            let toolbar = cx.new(|cx| BrowserToolbar::new(browser, window, cx));
            self.toolbar = Some(toolbar);
        }
    }

    fn ensure_browser_created(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        cx: &mut Context<Self>,
    ) {
        if self.browser_created {
            return;
        }

        // Wait for CEF context to be initialized before creating browser
        if !CefInstance::is_context_ready() {
            log::debug!("Waiting for CEF context to be ready...");
            return;
        }

        if let Some(browser) = &self.browser {
            browser.update(cx, |browser, _| {
                browser.set_scale_factor(scale_factor);
                browser.set_size(width, height);
                if let Err(e) = browser.create_browser(DEFAULT_URL) {
                    log::error!("Failed to create browser: {}", e);
                    return;
                }
                // Set focus and force initial paint
                browser.set_focus(true);
                browser.invalidate();
            });
            self.browser_created = true;
            log::info!(
                "Browser created successfully with size {}x{} at scale {}",
                width,
                height,
                scale_factor
            );

            // Start the message pump now that the browser is created
            if !self.message_pump_started {
                self._message_pump_task = Some(Self::start_message_pump(cx));
                self.message_pump_started = true;
            }
        }
    }

    fn handle_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(browser) = &self.browser {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input_handler::handle_mouse_down(&browser.read(cx), event, offset);
        }
    }

    fn handle_mouse_up(
        &mut self,
        event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(browser) = &self.browser {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input_handler::handle_mouse_up(&browser.read(cx), event, offset);
        }
    }

    fn handle_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(browser) = &self.browser {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input_handler::handle_mouse_move(&browser.read(cx), event, offset);
        }
    }

    fn handle_scroll(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(browser) = &self.browser {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input_handler::handle_scroll_wheel(&browser.read(cx), event, offset);
        }
    }

    fn handle_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(browser) = &self.browser {
            input_handler::handle_key_down(&browser.read(cx), event);
        }
    }

    fn handle_key_up(
        &mut self,
        event: &gpui::KeyUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(browser) = &self.browser {
            input_handler::handle_key_up(&browser.read(cx), event);
        }
    }

    fn render_placeholder(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(theme.colors().editor_background)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        Icon::new(IconName::Globe)
                            .size(IconSize::Custom(rems(6.0)))
                            .color(Color::Muted),
                    )
                    .child(
                        div()
                            .text_color(theme.colors().text_muted)
                            .text_size(rems(1.0))
                            .child("Browser"),
                    )
                    .child(
                        div()
                            .text_color(theme.colors().text_muted)
                            .text_size(rems(0.875))
                            .max_w(px(400.))
                            .text_center()
                            .child("CEF is not initialized. Set CEF_PATH environment variable and restart."),
                    ),
            )
    }

    fn render_browser_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let current_frame = self
            .browser
            .as_ref()
            .and_then(|b| b.read(cx).current_frame());

        let has_frame = current_frame.is_some();

        div()
            .id("browser-content")
            .flex_1()
            .w_full()
            .bg(theme.colors().editor_background)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::handle_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::handle_mouse_up))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_scroll_wheel(cx.listener(Self::handle_scroll))
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_key_up(cx.listener(Self::handle_key_up))
            .when_some(current_frame, |this, frame| {
                this.child(img(frame).size_full().object_fit(gpui::ObjectFit::Fill))
            })
            .when(!has_frame, |this| {
                this.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_color(theme.colors().text_muted)
                                .child("Loading..."),
                        ),
                )
            })
    }
}

impl EventEmitter<()> for BrowserView {}

impl Focusable for BrowserView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BrowserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.cef_available {
            return div()
                .id("browser-view")
                .track_focus(&self.focus_handle)
                .size_full()
                .child(self.render_placeholder(cx))
                .into_any_element();
        }

        // Schedule toolbar creation for next frame if not exists
        if !self.ensure_toolbar_exists() && self.browser.is_some() {
            cx.defer_in(window, |this, window, cx| {
                this.create_toolbar(window, cx);
            });
        }

        let bounds = window.bounds();
        let scale_factor = window.scale_factor();
        let toolbar_height = px(40.);

        let content_width = f32::from(bounds.size.width) as u32;
        let content_height = (f32::from(bounds.size.height) - f32::from(toolbar_height)) as u32;

        if content_width > 0 && content_height > 0 {
            if !self.browser_created {
                self.ensure_browser_created(content_width, content_height, scale_factor, cx);
                // If browser not created yet (CEF not ready), schedule re-render to retry
                if !self.browser_created {
                    cx.notify();
                }
            } else if let Some(browser) = &self.browser {
                browser.update(cx, |browser, _| {
                    browser.set_scale_factor(scale_factor);
                    browser.set_size(content_width, content_height);
                });
            }

            self.content_bounds = Bounds {
                origin: point(px(0.), toolbar_height),
                size: gpui::size(px(content_width as f32), px(content_height as f32)),
            };
        }

        div()
            .id("browser-view")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .when_some(self.toolbar.clone(), |this, toolbar| {
                this.child(toolbar)
            })
            .child(self.render_browser_content(cx))
            .into_any_element()
    }
}
