//! Browser View
//!
//! The main view for Browser Mode. Renders CEF browser content and handles
//! user input for navigation and interaction.

use crate::cef_instance::CefInstance;
use crate::input;
use crate::tab::{BrowserTab, TabEvent};
use crate::toolbar::BrowserToolbar;
use gpui::{
    actions, canvas, div, img, point, prelude::*, px, App, Bounds, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Render, Styled, Subscription, Task, Window,
};
use std::time::Duration;
use ui::{prelude::*, Icon, IconName, IconSize};

actions!(
    browser,
    [
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        SelectAll,
    ]
);

const DEFAULT_URL: &str = "https://www.google.com";
const CEF_MESSAGE_PUMP_INTERVAL_MS: u64 = 16;
pub const TOOLBAR_HEIGHT: f32 = 40.;

pub struct BrowserView {
    focus_handle: FocusHandle,
    tab: Option<Entity<BrowserTab>>,
    toolbar: Option<Entity<BrowserToolbar>>,
    content_bounds: Bounds<Pixels>,
    cef_available: bool,
    browser_created: bool,
    last_viewport: Option<(u32, u32, u32)>,
    _message_pump_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl BrowserView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let cef_available = CefInstance::global().is_some();

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            tab: None,
            toolbar: None,
            content_bounds: Bounds::default(),
            cef_available,
            browser_created: false,
            last_viewport: None,
            _message_pump_task: None,
            _subscriptions: Vec::new(),
        };

        if cef_available {
            this.initialize_tab(cx);
        }

        this
    }

    fn initialize_tab(&mut self, cx: &mut Context<Self>) {
        let tab = cx.new(|cx| BrowserTab::new(cx));

        let subscription = cx.subscribe(&tab, Self::handle_tab_event);
        self._subscriptions.push(subscription);

        self.tab = Some(tab);
    }

    fn handle_tab_event(
        &mut self,
        _tab: Entity<BrowserTab>,
        event: &TabEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TabEvent::FrameReady => {
                cx.notify();
            }
            TabEvent::NavigateToUrl(url) => {
                if let Some(tab) = &self.tab {
                    let url = url.clone();
                    tab.update(cx, |tab, _| {
                        tab.navigate(&url);
                    });
                }
            }
            TabEvent::AddressChanged(_)
            | TabEvent::TitleChanged(_)
            | TabEvent::LoadingStateChanged => {
                cx.notify();
            }
            TabEvent::LoadError { url, error_text, .. } => {
                log::warn!("Load error for {}: {}", url, error_text);
                cx.notify();
            }
        }
    }

    fn start_message_pump(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(CEF_MESSAGE_PUMP_INTERVAL_MS))
                    .await;

                let entity_exists = this.upgrade().is_some();
                if !entity_exists {
                    break;
                }

                CefInstance::pump_messages();

                let _ = cx.update(|cx| {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |view, cx| {
                            if let Some(tab) = &view.tab {
                                tab.update(cx, |tab, cx| {
                                    tab.drain_events(cx);
                                });
                            }
                        });
                    }
                });
            }
        })
    }

    fn create_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab.clone() {
            let toolbar = cx.new(|cx| BrowserToolbar::new(tab, window, cx));
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

        if !CefInstance::is_context_ready() {
            return;
        }

        if let Some(tab) = &self.tab {
            tab.update(cx, |tab, _| {
                tab.set_scale_factor(scale_factor);
                tab.set_size(width, height);
                if let Err(e) = tab.create_browser(DEFAULT_URL) {
                    log::error!("Failed to create browser: {}", e);
                    return;
                }
                tab.set_focus(true);
                tab.invalidate();
            });
            self.browser_created = true;
            self.last_viewport = Some((width, height, (scale_factor * 1000.0) as u32));

            self._message_pump_task = Some(Self::start_message_pump(cx));
        }
    }

    fn handle_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = &self.tab {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_mouse_down(&tab.read(cx), event, offset);

            tab.update(cx, |tab, _| {
                tab.set_focus(true);
            });
        }
        window.focus(&self.focus_handle, cx);
    }

    fn handle_mouse_up(
        &mut self,
        event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = &self.tab {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_mouse_up(&tab.read(cx), event, offset);
        }
    }

    fn handle_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = &self.tab {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_mouse_move(&tab.read(cx), event, offset);
        }
    }

    fn handle_scroll(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = &self.tab {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_scroll_wheel(&tab.read(cx), event, offset);
        }
    }

    fn handle_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = &self.tab {
            tab.update(cx, |tab, _| {
                tab.set_focus(true);
            });

            let keystroke = event.keystroke.clone();
            let is_held = event.is_held;
            let tab = tab.clone();

            cx.defer(move |cx| {
                tab.update(cx, |tab, _| {
                    input::handle_key_down_deferred(tab, &keystroke, is_held);
                });
            });
        }
    }

    fn handle_key_up(
        &mut self,
        event: &gpui::KeyUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = &self.tab {
            let keystroke = event.keystroke.clone();
            let tab = tab.clone();

            cx.defer(move |cx| {
                tab.update(cx, |tab, _| {
                    input::handle_key_up_deferred(tab, &keystroke);
                });
            });
        }
    }

    fn handle_copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = &self.tab {
            tab.read(cx).copy();
        }
    }

    fn handle_cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = &self.tab {
            tab.read(cx).cut();
        }
    }

    fn handle_paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = &self.tab {
            tab.read(cx).paste();
        }
    }

    fn handle_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = &self.tab {
            tab.read(cx).undo();
        }
    }

    fn handle_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = &self.tab {
            tab.read(cx).redo();
        }
    }

    fn handle_select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = &self.tab {
            tab.read(cx).select_all();
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
            .tab
            .as_ref()
            .and_then(|t| t.read(cx).current_frame());

        let has_frame = current_frame.is_some();

        let this = cx.entity();
        let bounds_tracker = canvas(
            move |bounds, _window, cx| {
                this.update(cx, |view, _| {
                    view.content_bounds = bounds;
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        div()
            .id("browser-content")
            .relative()
            .flex_1()
            .w_full()
            .bg(theme.colors().editor_background)
            .child(bounds_tracker)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::handle_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::handle_mouse_up))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_scroll_wheel(cx.listener(Self::handle_scroll))
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

        if self.toolbar.is_none() && self.tab.is_some() {
            cx.defer_in(window, |this, window, cx| {
                this.create_toolbar(window, cx);
            });
        }

        let viewport_size = window.viewport_size();
        let scale_factor = window.scale_factor();
        let toolbar_height = px(TOOLBAR_HEIGHT);

        let content_width = f32::from(viewport_size.width) as u32;
        let content_height = (f32::from(viewport_size.height) - f32::from(toolbar_height)) as u32;

        if content_width > 0 && content_height > 0 {
            if !self.browser_created {
                self.ensure_browser_created(content_width, content_height, scale_factor, cx);
                if !self.browser_created {
                    cx.notify();
                }
            } else {
                let scale_key = (scale_factor * 1000.0) as u32;
                let new_viewport = (content_width, content_height, scale_key);
                if self.last_viewport != Some(new_viewport) {
                    self.last_viewport = Some(new_viewport);
                    if let Some(tab) = &self.tab {
                        tab.update(cx, |tab, _| {
                            tab.set_scale_factor(scale_factor);
                            tab.set_size(content_width, content_height);
                        });
                    }
                }
            }
        }

        div()
            .id("browser-view")
            .track_focus(&self.focus_handle)
            .key_context("BrowserView")
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_key_up(cx.listener(Self::handle_key_up))
            .on_action(cx.listener(Self::handle_copy))
            .on_action(cx.listener(Self::handle_cut))
            .on_action(cx.listener(Self::handle_paste))
            .on_action(cx.listener(Self::handle_undo))
            .on_action(cx.listener(Self::handle_redo))
            .on_action(cx.listener(Self::handle_select_all))
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
