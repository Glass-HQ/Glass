use futures::channel::mpsc;
use futures::StreamExt;
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, Render, SharedString, Task,
    UniformListScrollHandle, Window, uniform_list,
};
use native_platforms::apple::build::BuildOutput;
use ui::prelude::*;
use workspace::item::{Item, ItemEvent, TabContentParams};

pub struct BuildLogsView {
    focus_handle: FocusHandle,
    lines: Vec<LogLine>,
    scroll_handle: UniformListScrollHandle,
    is_complete: bool,
    build_success: Option<bool>,
    _receiver_task: Task<()>,
}

#[derive(Clone)]
enum LogLine {
    Normal(String),
    Error(String),
    Warning(String),
    Progress(String),
}

impl BuildLogsView {
    pub fn new(
        mut receiver: mpsc::UnboundedReceiver<BuildOutput>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let receiver_task = cx.spawn_in(window, async move |this, cx| {
            while let Some(output) = receiver.next().await {
                let _ = this.update(cx, |this, cx| {
                    match output {
                        BuildOutput::Line(line) => {
                            this.lines.push(LogLine::Normal(line));
                        }
                        BuildOutput::Error(error) => {
                            let msg = if let Some(file) = &error.file {
                                if let Some(line) = error.line {
                                    format!("{}:{}: error: {}", file, line, error.message)
                                } else {
                                    format!("{}: error: {}", file, error.message)
                                }
                            } else {
                                format!("error: {}", error.message)
                            };
                            this.lines.push(LogLine::Error(msg));
                        }
                        BuildOutput::Warning(warning) => {
                            let msg = if let Some(file) = &warning.file {
                                if let Some(line) = warning.line {
                                    format!("{}:{}: warning: {}", file, line, warning.message)
                                } else {
                                    format!("{}: warning: {}", file, warning.message)
                                }
                            } else {
                                format!("warning: {}", warning.message)
                            };
                            this.lines.push(LogLine::Warning(msg));
                        }
                        BuildOutput::Progress { phase, .. } => {
                            this.lines.push(LogLine::Progress(phase));
                        }
                        BuildOutput::Completed(result) => {
                            this.is_complete = true;
                            this.build_success = Some(result.success);
                            if result.success {
                                this.lines.push(LogLine::Progress("Build succeeded".to_string()));
                            } else {
                                this.lines.push(LogLine::Error("Build failed".to_string()));
                            }
                        }
                    }
                    cx.emit(ItemEvent::UpdateTab);
                    cx.notify();
                });
            }
        });

        Self {
            focus_handle,
            lines: Vec::new(),
            scroll_handle: UniformListScrollHandle::new(),
            is_complete: false,
            build_success: None,
            _receiver_task: receiver_task,
        }
    }

    fn render_line(&self, line: &LogLine, cx: &Context<Self>) -> impl IntoElement {
        match line {
            LogLine::Normal(text) => {
                div()
                    .px_2()
                    .py_px()
                    .child(Label::new(text.clone()).size(LabelSize::Small).color(Color::Default))
            }
            LogLine::Error(text) => {
                div()
                    .px_2()
                    .py_px()
                    .bg(cx.theme().status().error_background)
                    .child(Label::new(text.clone()).size(LabelSize::Small).color(Color::Error))
            }
            LogLine::Warning(text) => {
                div()
                    .px_2()
                    .py_px()
                    .bg(cx.theme().status().warning_background)
                    .child(Label::new(text.clone()).size(LabelSize::Small).color(Color::Warning))
            }
            LogLine::Progress(text) => {
                div()
                    .px_2()
                    .py_px()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Icon::new(IconName::ArrowRight).size(IconSize::Small).color(Color::Accent))
                            .child(Label::new(text.clone()).size(LabelSize::Small).color(Color::Accent))
                    )
            }
        }
    }
}

impl Focusable for BuildLogsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<ItemEvent> for BuildLogsView {}

impl Render for BuildLogsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.lines.clone();
        let line_count = lines.len();

        v_flex()
            .key_context("BuildLogsView")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().editor_background)
            .when(line_count == 0, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new("Building...").color(Color::Muted))
                )
            })
            .when(line_count > 0, |this| {
                let view = cx.weak_entity();
                this.child(
                    uniform_list(
                        "build-logs",
                        line_count,
                        move |range, _window, cx| {
                            let Some(view) = view.upgrade() else {
                                return Vec::new();
                            };
                            view.update(cx, |this, cx| {
                                range
                                    .into_iter()
                                    .filter_map(|ix| this.lines.get(ix).cloned())
                                    .map(|line| this.render_line(&line, cx).into_any_element())
                                    .collect()
                            })
                        },
                    )
                    .flex_1()
                    .track_scroll(&self.scroll_handle)
                )
            })
    }
}

impl Item for BuildLogsView {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        if self.is_complete {
            if self.build_success == Some(true) {
                "Build ✓".into()
            } else {
                "Build ✗".into()
            }
        } else {
            "Building...".into()
        }
    }

    fn tab_icon(&self, _: &Window, _cx: &App) -> Option<Icon> {
        let icon = if self.is_complete {
            if self.build_success == Some(true) {
                Icon::new(IconName::Check).color(Color::Success)
            } else {
                Icon::new(IconName::Close).color(Color::Error)
            }
        } else {
            Icon::new(IconName::ToolHammer).color(Color::Muted)
        };
        Some(icon.size(IconSize::Small))
    }

    fn to_item_events(event: &Self::Event, mut f: impl FnMut(ItemEvent)) {
        f(*event);
    }

    fn tab_content(
        &self,
        params: TabContentParams,
        _window: &Window,
        cx: &App,
    ) -> gpui::AnyElement {
        let color = params.text_color();
        let text = self.tab_content_text(params.detail.unwrap_or(0), cx);

        Label::new(text)
            .color(color)
            .into_any_element()
    }
}
