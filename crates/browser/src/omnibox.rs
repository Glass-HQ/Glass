use crate::history::{BrowserHistory, HistoryMatch};
use editor::Editor;
use gpui::{
    anchored, canvas, deferred, div, point, prelude::*, px, App, Bounds, Context, Corner, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement, Pixels, Render, SharedString,
    Styled, Subscription, Task, Window,
};
use std::time::Duration;
use ui::{h_flex, prelude::*, v_flex, Icon, IconName, IconSize};

pub enum OmniboxEvent {
    Navigate(String),
}

pub enum OmniboxSuggestion {
    HistoryItem {
        url: String,
        title: String,
    },
    RawUrl(String),
    SearchQuery(String),
}

impl OmniboxSuggestion {
    fn url_or_search(&self) -> String {
        match self {
            OmniboxSuggestion::HistoryItem { url, .. } => url.clone(),
            OmniboxSuggestion::RawUrl(url) => {
                if url.starts_with("http://") || url.starts_with("https://") {
                    url.clone()
                } else {
                    format!("https://{}", url)
                }
            }
            OmniboxSuggestion::SearchQuery(query) => {
                let encoded: String =
                    url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
                format!("https://www.google.com/search?q={}", encoded)
            }
        }
    }
}

pub struct Omnibox {
    url_editor: Entity<Editor>,
    history: Entity<BrowserHistory>,
    suggestions: Vec<OmniboxSuggestion>,
    selected_index: usize,
    is_open: bool,
    suppress_search: bool,
    current_page_url: String,
    pending_search: Option<Task<()>>,
    editor_bounds: Bounds<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OmniboxEvent> for Omnibox {}

impl Omnibox {
    pub fn new(
        history: Entity<BrowserHistory>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let url_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Enter URL or search...", window, cx);
            editor
        });

        let buffer_subscription = cx.subscribe(&url_editor, Self::on_editor_event);
        let blur_subscription =
            cx.on_blur(&url_editor.focus_handle(cx), window, Self::on_editor_blur);

        Self {
            url_editor,
            history,
            suggestions: Vec::new(),
            selected_index: 0,
            is_open: false,
            suppress_search: false,
            current_page_url: String::new(),
            pending_search: None,
            editor_bounds: Bounds::default(),
            _subscriptions: vec![buffer_subscription, blur_subscription],
        }
    }

    pub fn set_url(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.current_page_url = url.to_string();
        if !self.is_open {
            self.suppress_search = true;
            self.url_editor.update(cx, |editor, cx| {
                editor.set_text(url.to_string(), window, cx);
            });
        }
    }

    fn on_editor_event(
        &mut self,
        _editor: Entity<Editor>,
        event: &editor::EditorEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, editor::EditorEvent::BufferEdited) {
            if self.suppress_search {
                self.suppress_search = false;
                return;
            }
            self.schedule_search(cx);
        }
    }

    fn on_editor_blur(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.close_dropdown(cx);
    }

    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        let query = self.url_editor.read(cx).text(cx);

        if query.is_empty() {
            self.suggestions.clear();
            self.is_open = false;
            self.pending_search = None;
            cx.notify();
            return;
        }

        let executor = cx.background_executor().clone();

        self.pending_search = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;

            let query_for_search = this
                .read_with(cx, |this, cx| this.url_editor.read(cx).text(cx))
                .ok()
                .unwrap_or_default();

            if query_for_search.is_empty() {
                let _ = this.update(cx, |this, cx| {
                    this.suggestions.clear();
                    this.is_open = false;
                    this.pending_search = None;
                    cx.notify();
                });
                return;
            }

            let entries = this
                .read_with(cx, |this, cx| this.history.read(cx).entries().to_vec())
                .ok()
                .unwrap_or_default();

            let history_matches =
                BrowserHistory::search(entries, query_for_search.clone(), 8, executor).await;

            let _ = this.update(cx, |this, cx| {
                this.build_suggestions(query_for_search, history_matches);
                this.pending_search = None;
                cx.notify();
            });
        }));
    }

    fn build_suggestions(&mut self, query: String, history_matches: Vec<HistoryMatch>) {
        self.suggestions.clear();

        // If query looks like a URL, prepend a RawUrl suggestion
        if looks_like_url(&query) {
            self.suggestions
                .push(OmniboxSuggestion::RawUrl(query.clone()));
        }

        // History matches
        for m in history_matches {
            self.suggestions.push(OmniboxSuggestion::HistoryItem {
                url: m.url,
                title: m.title,
            });
        }

        // Always append a search query suggestion as the last item
        self.suggestions
            .push(OmniboxSuggestion::SearchQuery(query));

        self.selected_index = 0;
        self.is_open = true;
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_open && !self.suggestions.is_empty() {
            let index = self.selected_index.min(self.suggestions.len().saturating_sub(1));
            let url = self.suggestions[index].url_or_search();
            self.close_dropdown(cx);
            cx.emit(OmniboxEvent::Navigate(url));
            window.blur();
            return;
        }

        // Fallback: if dropdown is not open, just navigate to whatever is in the editor
        let text = self.url_editor.read(cx).text(cx);
        if text.is_empty() {
            return;
        }

        let url = if text.starts_with("http://") || text.starts_with("https://") {
            text
        } else if text.contains('.') {
            format!("https://{}", text)
        } else {
            let encoded: String = url::form_urlencoded::byte_serialize(text.as_bytes()).collect();
            format!("https://www.google.com/search?q={}", encoded)
        };

        self.close_dropdown(cx);
        cx.emit(OmniboxEvent::Navigate(url));
        window.blur();
    }

    fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_open {
            self.close_dropdown(cx);
            // Restore the current page URL
            let url = self.current_page_url.clone();
            self.suppress_search = true;
            self.url_editor.update(cx, |editor, cx| {
                editor.set_text(url, window, cx);
            });
        }
    }

    fn close_dropdown(&mut self, cx: &mut Context<Self>) {
        self.suggestions.clear();
        self.is_open = false;
        self.selected_index = 0;
        self.pending_search = None;
        cx.notify();
    }

    fn move_up(&mut self, _: &editor::actions::MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_open || self.suggestions.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.suggestions.len() - 1;
        } else {
            self.selected_index -= 1;
        }
        cx.notify();
    }

    fn move_down(
        &mut self,
        _: &editor::actions::MoveDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_open || self.suggestions.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.suggestions.len();
        cx.notify();
    }

    fn render_dropdown(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let rows = self
            .suggestions
            .iter()
            .enumerate()
            .map(|(index, suggestion)| {
                let is_selected = index == self.selected_index;
                let (icon_name, title, subtitle) = match suggestion {
                    OmniboxSuggestion::HistoryItem { url, title, .. } => {
                        let display_title: SharedString = if title.is_empty() {
                            url.clone().into()
                        } else {
                            title.clone().into()
                        };
                        (IconName::HistoryRerun, display_title, Some(url.clone()))
                    }
                    OmniboxSuggestion::RawUrl(url) => {
                        let display: SharedString = url.clone().into();
                        (IconName::Globe, display, None)
                    }
                    OmniboxSuggestion::SearchQuery(query) => {
                        let truncated = if query.len() > 80 {
                            format!("{}...", &query[..77])
                        } else {
                            query.clone()
                        };
                        let display: SharedString =
                            format!("Search Google for \"{}\"", truncated).into();
                        (IconName::MagnifyingGlass, display, None)
                    }
                };

                let bg = if is_selected {
                    theme.colors().ghost_element_selected
                } else {
                    theme.colors().ghost_element_background
                };

                div()
                    .id(("omnibox-suggestion", index))
                    .w_full()
                    .px_2()
                    .py_0p5()
                    .bg(bg)
                    .when(!is_selected, |this| {
                        this.hover(|style| style.bg(theme.colors().ghost_element_hover))
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_index = index;
                        if let Some(suggestion) = this.suggestions.get(index) {
                            let url = suggestion.url_or_search();
                            this.close_dropdown(cx);
                            cx.emit(OmniboxEvent::Navigate(url));
                            window.blur();
                        }
                    }))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .overflow_hidden()
                            .child(
                                Icon::new(icon_name)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(rems(0.8125))
                                    .text_color(theme.colors().text)
                                    .child(title),
                            )
                            .when_some(subtitle, |this, subtitle| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .max_w(px(300.))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(rems(0.75))
                                        .text_color(theme.colors().text_muted)
                                        .child(SharedString::from(subtitle)),
                                )
                            }),
                    )
            })
            .collect::<Vec<_>>();

        let dropdown_content = v_flex()
            .id("omnibox-dropdown")
            .w(self.editor_bounds.size.width)
            .max_h(px(300.))
            .overflow_y_scroll()
            .bg(theme.colors().elevated_surface_background)
            .border_1()
            .border_color(theme.colors().border)
            .rounded_md()
            .shadow_md()
            .py_1()
            .children(rows);

        let position = point(
            self.editor_bounds.origin.x,
            self.editor_bounds.origin.y + self.editor_bounds.size.height,
        );

        deferred(
            anchored()
                .position(position)
                .anchor(Corner::TopLeft)
                .snap_to_window_with_margin(px(8.))
                .child(dropdown_content),
        )
        .with_priority(1)
    }
}

impl Focusable for Omnibox {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.url_editor.focus_handle(cx)
    }
}

impl Render for Omnibox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let this = cx.entity();
        let bounds_tracker = canvas(
            move |bounds, _window, cx| {
                this.update(cx, |view, _| {
                    view.editor_bounds = bounds;
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let show_dropdown = self.is_open && !self.suggestions.is_empty();

        div()
            .relative()
            .flex_1()
            .min_w(px(100.))
            .key_context("Omnibox")
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .child(
                div()
                    .h(px(24.))
                    .px_2()
                    .rounded_md()
                    .bg(theme.colors().editor_background)
                    .border_1()
                    .border_color(theme.colors().border)
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(bounds_tracker)
                    .child(self.url_editor.clone()),
            )
            .when(show_dropdown, |this| {
                this.child(self.render_dropdown(cx))
            })
    }
}

fn looks_like_url(input: &str) -> bool {
    if input.starts_with("http://") || input.starts_with("https://") {
        return true;
    }
    // Contains a dot with no spaces — likely a domain
    if input.contains('.') && !input.contains(' ') {
        return true;
    }
    // Contains :// scheme
    if input.contains("://") {
        return true;
    }
    false
}
