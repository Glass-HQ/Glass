use crate::history::{BrowserHistory, HistoryMatch};
use editor::{Editor, actions::SelectAll};
use gpui::{
    App, Bounds, Context, Corner, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Pixels, Render, SharedString, Styled, Subscription, Task, Window, anchored,
    canvas, deferred, div, native_image_view, point, prelude::*, px,
};
use std::time::Duration;
use ui::{Icon, IconName, IconSize, h_flex, prelude::*, v_flex};

pub enum OmniboxEvent {
    Navigate(String),
}

pub enum OmniboxSuggestion {
    HistoryItem { url: String, title: String },
    RawUrl(String),
    SearchQuery(String),
}

impl OmniboxSuggestion {
    fn url_or_search(&self) -> String {
        match self {
            OmniboxSuggestion::HistoryItem { url, .. } => url.clone(),
            OmniboxSuggestion::RawUrl(url) => text_to_url(url),
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
    content_focus_handle: FocusHandle,
    suggestions: Vec<OmniboxSuggestion>,
    selected_index: usize,
    is_open: bool,
    suppress_search: bool,
    navigation_started: bool,
    current_page_url: String,
    pending_search: Option<Task<()>>,
    editor_bounds: Bounds<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OmniboxEvent> for Omnibox {}

impl Omnibox {
    pub fn new(
        history: Entity<BrowserHistory>,
        content_focus_handle: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let url_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Enter URL or search...", window, cx);
            editor
        });

        let buffer_subscription = cx.subscribe(&url_editor, Self::on_editor_event);
        let focus_subscription =
            cx.on_focus(&url_editor.focus_handle(cx), window, Self::on_editor_focus);
        let blur_subscription =
            cx.on_blur(&url_editor.focus_handle(cx), window, Self::on_editor_blur);

        Self {
            url_editor,
            history,
            content_focus_handle,
            suggestions: Vec::new(),
            selected_index: 0,
            is_open: false,
            suppress_search: false,
            navigation_started: false,
            current_page_url: String::new(),
            pending_search: None,
            editor_bounds: Bounds::default(),
            _subscriptions: vec![buffer_subscription, focus_subscription, blur_subscription],
        }
    }

    pub fn set_url(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let display_url = display_url(url);
        self.navigation_started = false;
        self.current_page_url = display_url.clone();
        self.close_dropdown(cx);
        self.suppress_search = true;
        self.url_editor.update(cx, |editor, cx| {
            editor.set_text(display_url, window, cx);
        });
    }

    pub fn focus_and_select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_dropdown(cx);
        let focus_handle = self.url_editor.focus_handle(cx);
        window.focus(&focus_handle, cx);
        self.url_editor.update(cx, |editor, cx| {
            editor.select_all(&SelectAll, window, cx);
        });
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

    fn on_editor_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.url_editor.update(cx, |editor, cx| {
            editor.select_all(&SelectAll, window, cx);
        });
    }

    fn on_editor_blur(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.close_dropdown(cx);
        if self.navigation_started {
            self.navigation_started = false;
            return;
        }

        let current_page_url = self.current_page_url.clone();
        if self.url_editor.read(cx).text(cx) != current_page_url {
            self.suppress_search = true;
            self.url_editor.update(cx, |editor, cx| {
                editor.set_text(current_page_url, _window, cx);
            });
        }
    }

    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        let query = self.url_editor.read(cx).text(cx);

        if query.is_empty() || query == self.current_page_url {
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

            let (query_for_search, current_url) = this
                .read_with(cx, |this, cx| {
                    (
                        this.url_editor.read(cx).text(cx),
                        this.current_page_url.clone(),
                    )
                })
                .ok()
                .unwrap_or_default();

            if query_for_search.is_empty() || query_for_search == current_url {
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

        self.suggestions
            .push(OmniboxSuggestion::SearchQuery(query.clone()));

        if looks_like_url(&query) {
            self.suggestions.push(OmniboxSuggestion::RawUrl(query));
        }

        for m in history_matches {
            self.suggestions.push(OmniboxSuggestion::HistoryItem {
                url: m.url,
                title: m.title,
            });
        }

        self.selected_index = 0;
        self.is_open = true;
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_open && !self.suggestions.is_empty() {
            let index = self
                .selected_index
                .min(self.suggestions.len().saturating_sub(1));
            let url = self.suggestions[index].url_or_search();
            self.navigate(url, window, cx);
            return;
        }

        // Fallback: if dropdown is not open, just navigate to whatever is in the editor
        let text = self.url_editor.read(cx).text(cx);
        if text.is_empty() {
            return;
        }

        let url = text_to_url(&text);

        self.navigate(url, window, cx);
    }

    fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.close_dropdown(cx);
        self.navigation_started = false;
        let current_page_url = self.current_page_url.clone();
        if self.url_editor.read(cx).text(cx) != current_page_url {
            self.suppress_search = true;
            self.url_editor.update(cx, |editor, cx| {
                editor.set_text(current_page_url, window, cx);
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

    fn navigate(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        self.navigation_started = true;
        self.close_dropdown(cx);
        cx.emit(OmniboxEvent::Navigate(url));
        window.focus(&self.content_focus_handle, cx);
    }

    fn move_up(
        &mut self,
        _: &zed_actions::editor::MoveUp,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        _: &zed_actions::editor::MoveDown,
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
                let (leading_icon, title, subtitle) = match suggestion {
                    OmniboxSuggestion::HistoryItem { url, title, .. } => {
                        let display_title: SharedString = if title.is_empty() {
                            url.clone().into()
                        } else {
                            title.clone().into()
                        };
                        (
                            Icon::new(IconName::HistoryRerun)
                                .size(IconSize::Small)
                                .color(Color::Muted)
                                .into_any_element(),
                            display_title,
                            Some(url.clone()),
                        )
                    }
                    OmniboxSuggestion::RawUrl(url) => {
                        let display: SharedString = url.clone().into();
                        (
                            native_image_view(format!("omnibox-suggestion-globe-{index}"))
                                .sf_symbol("globe")
                                .size(px(14.0))
                                .into_any_element(),
                            display,
                            None,
                        )
                    }
                    OmniboxSuggestion::SearchQuery(query) => {
                        let truncated = if query.len() > 80 {
                            format!("{}...", &query[..77])
                        } else {
                            query.clone()
                        };
                        let display: SharedString =
                            format!("Search Google for \"{}\"", truncated).into();
                        (
                            Icon::new(IconName::MagnifyingGlass)
                                .size(IconSize::Small)
                                .color(Color::Muted)
                                .into_any_element(),
                            display,
                            None,
                        )
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
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.selected_index = index;
                            if let Some(suggestion) = this.suggestions.get(index) {
                                let url = suggestion.url_or_search();
                                this.navigate(url, window, cx);
                            }
                        }),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .overflow_hidden()
                            .child(leading_icon)
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
            .theme_rounded_md(cx)
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
                    .theme_rounded_md(cx)
                    .bg(theme.colors().editor_background)
                    .border_1()
                    .border_color(theme.colors().border)
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(bounds_tracker)
                    .child(self.url_editor.clone()),
            )
            .when(show_dropdown, |this| this.child(self.render_dropdown(cx)))
    }
}

fn looks_like_url(input: &str) -> bool {
    if input.starts_with("http://") || input.starts_with("https://") {
        return true;
    }
    // Contains :// scheme
    if input.contains("://") {
        return true;
    }

    if input.chars().any(char::is_whitespace) {
        return false;
    }

    let Ok(url) = url::Url::parse(&format!("http://{input}")) else {
        return false;
    };

    let Some(host) = url.host_str() else {
        return false;
    };

    host.eq_ignore_ascii_case("localhost")
        || host.contains('.')
        || host.parse::<std::net::IpAddr>().is_ok()
        || (url.port().is_some() && !host.contains('.'))
}

fn should_use_http_by_default(input: &str) -> bool {
    let Ok(url) = url::Url::parse(&format!("http://{input}")) else {
        return false;
    };

    let Some(host) = url.host_str() else {
        return false;
    };

    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    if let Ok(address) = host.parse::<std::net::IpAddr>() {
        return address.is_loopback();
    }

    url.port().is_some() && !host.contains('.')
}

fn text_to_url(text: &str) -> String {
    if text.starts_with("http://") || text.starts_with("https://") {
        return text.to_string();
    }

    if !looks_like_url(text) {
        let encoded: String = url::form_urlencoded::byte_serialize(text.as_bytes()).collect();
        return format!("https://www.google.com/search?q={encoded}");
    }

    if should_use_http_by_default(text) {
        format!("http://{text}")
    } else {
        format!("https://{text}")
    }
}

fn display_url(url: &str) -> String {
    if url == "glass://newtab" {
        return String::new();
    }

    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::{looks_like_url, text_to_url};

    #[test]
    fn localhost_inputs_are_treated_as_urls() {
        assert!(looks_like_url("localhost"));
        assert!(looks_like_url("localhost:3000"));
        assert_eq!(text_to_url("localhost"), "http://localhost");
        assert_eq!(text_to_url("localhost:3000"), "http://localhost:3000");
    }

    #[test]
    fn regular_domains_default_to_https() {
        assert!(looks_like_url("example.com"));
        assert_eq!(text_to_url("example.com"), "https://example.com");
    }

    #[test]
    fn plain_queries_still_search() {
        assert!(!looks_like_url("rust ownership"));
        assert_eq!(
            text_to_url("rust ownership"),
            "https://www.google.com/search?q=rust+ownership"
        );
    }
}
