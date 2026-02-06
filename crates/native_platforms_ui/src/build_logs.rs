use futures::channel::mpsc;
use futures::StreamExt;
use gpui::{
    App, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, Render, ScrollStrategy,
    SharedString, Task, UniformListScrollHandle, Window, uniform_list,
};
use native_platforms::apple::build::BuildOutput;
use native_platforms::apple::install::InstallOutput;
use native_platforms::apple::launch::LaunchOutput;
use native_platforms::apple::run::RunOutput;
use ui::prelude::*;
use ui::Tooltip;
use workspace::item::{Item, ItemEvent, TabContentParams};

pub enum AnyOutput {
    Build(BuildOutput),
    Run(RunOutput),
}

pub struct BuildLogsView {
    focus_handle: FocusHandle,
    lines: Vec<LogLine>,
    scroll_handle: UniformListScrollHandle,
    is_complete: bool,
    build_success: Option<bool>,
    header_label: &'static str,
    show_verbose: bool,
    _receiver_task: Task<()>,
}

#[derive(Clone)]
enum LogLine {
    Normal(String),
    Error(String),
    Warning(String),
    Progress(String),
    Verbose(String),
    InstallProgress { phase: String, percent: f32 },
    PhaseChange(String),
    Retry(String),
}

impl BuildLogsView {
    pub fn new(
        mut receiver: mpsc::UnboundedReceiver<AnyOutput>,
        header_label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let receiver_task = cx.spawn_in(window, async move |this, cx| {
            let mut pending_lines: Vec<LogLine> = Vec::new();
            let mut last_notify = std::time::Instant::now();
            let mut is_complete = false;
            let mut build_success = None;

            loop {
                use futures::future::{select, Either};
                use std::time::Duration;

                let timeout = cx.background_executor().timer(Duration::from_millis(50));
                let next_output = receiver.next();

                futures::pin_mut!(timeout);
                futures::pin_mut!(next_output);

                match select(next_output, timeout).await {
                    Either::Left((Some(output), _)) => {
                        Self::process_output(
                            output,
                            &mut pending_lines,
                            &mut is_complete,
                            &mut build_success,
                        );

                        let should_flush = is_complete
                            || pending_lines.len() >= 100
                            || last_notify.elapsed() > Duration::from_millis(100);

                        if should_flush && !pending_lines.is_empty() {
                            let lines_to_add = std::mem::take(&mut pending_lines);
                            let complete = is_complete;
                            let success = build_success;
                            let _ = this.update(cx, |this, cx| {
                                this.lines.extend(lines_to_add);
                                if complete {
                                    this.is_complete = true;
                                    this.build_success = success;
                                }
                                this.scroll_to_bottom();
                                cx.emit(ItemEvent::UpdateTab);
                                cx.notify();
                            });
                            last_notify = std::time::Instant::now();
                        }

                        if is_complete {
                            break;
                        }
                    }
                    Either::Left((None, _)) => {
                        if !pending_lines.is_empty() {
                            let lines_to_add = std::mem::take(&mut pending_lines);
                            let _ = this.update(cx, |this, cx| {
                                this.lines.extend(lines_to_add);
                                this.is_complete = true;
                                this.scroll_to_bottom();
                                cx.emit(ItemEvent::UpdateTab);
                                cx.notify();
                            });
                        } else {
                            let _ = this.update(cx, |this, cx| {
                                this.is_complete = true;
                                cx.emit(ItemEvent::UpdateTab);
                                cx.notify();
                            });
                        }
                        break;
                    }
                    Either::Right((_, _)) => {
                        if !pending_lines.is_empty() {
                            let lines_to_add = std::mem::take(&mut pending_lines);
                            let _ = this.update(cx, |this, cx| {
                                this.lines.extend(lines_to_add);
                                this.scroll_to_bottom();
                                cx.emit(ItemEvent::UpdateTab);
                                cx.notify();
                            });
                            last_notify = std::time::Instant::now();
                        }
                    }
                }
            }
        });

        Self {
            focus_handle,
            lines: Vec::new(),
            scroll_handle: UniformListScrollHandle::new(),
            is_complete: false,
            build_success: None,
            header_label,
            show_verbose: false,
            _receiver_task: receiver_task,
        }
    }

    fn process_output(
        output: AnyOutput,
        pending_lines: &mut Vec<LogLine>,
        is_complete: &mut bool,
        build_success: &mut Option<bool>,
    ) {
        match output {
            AnyOutput::Build(build_output) => {
                Self::process_build_output(
                    build_output,
                    pending_lines,
                    is_complete,
                    build_success,
                    true,
                );
            }
            AnyOutput::Run(run_output) => match run_output {
                RunOutput::PhaseChanged(phase) => {
                    pending_lines.push(LogLine::PhaseChange(phase.label().to_string()));
                }
                RunOutput::Build(build_output) => {
                    Self::process_build_output(
                        build_output,
                        pending_lines,
                        is_complete,
                        build_success,
                        false,
                    );
                }
                RunOutput::Install(install_output) => {
                    Self::process_install_output(install_output, pending_lines);
                }
                RunOutput::Launch(launch_output) => {
                    Self::process_launch_output(launch_output, pending_lines);
                }
                RunOutput::AppLaunched { .. } => {
                    // Controller handles this; view ignores it
                }
                RunOutput::Completed => {
                    *is_complete = true;
                    *build_success = Some(true);
                    pending_lines.push(LogLine::Progress("Pipeline complete".to_string()));
                }
                RunOutput::Failed { phase, message } => {
                    *is_complete = true;
                    *build_success = Some(false);
                    pending_lines.push(LogLine::Error(format!(
                        "Failed during {}: {}",
                        phase.label(),
                        message
                    )));
                }
            },
        }
    }

    fn process_build_output(
        output: BuildOutput,
        pending_lines: &mut Vec<LogLine>,
        is_complete: &mut bool,
        build_success: &mut Option<bool>,
        is_standalone_build: bool,
    ) {
        match output {
            BuildOutput::Line(line) => {
                pending_lines.push(LogLine::Normal(line));
            }
            BuildOutput::Verbose(line) => {
                pending_lines.push(LogLine::Verbose(line));
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
                pending_lines.push(LogLine::Error(msg));
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
                pending_lines.push(LogLine::Warning(msg));
            }
            BuildOutput::Progress { phase, .. } => {
                pending_lines.push(LogLine::Progress(phase));
            }
            BuildOutput::Completed(result) => {
                if is_standalone_build {
                    *is_complete = true;
                    *build_success = Some(result.success);
                }
                if result.success {
                    pending_lines.push(LogLine::Progress("Build succeeded".to_string()));
                } else {
                    pending_lines.push(LogLine::Error("Build failed".to_string()));
                }
            }
        }
    }

    fn process_install_output(output: InstallOutput, pending_lines: &mut Vec<LogLine>) {
        match output {
            InstallOutput::Line(line) => {
                pending_lines.push(LogLine::Normal(line));
            }
            InstallOutput::Progress(progress) => {
                pending_lines.push(LogLine::InstallProgress {
                    phase: progress.phase.label().to_string(),
                    percent: progress.percent,
                });
            }
            InstallOutput::Error(err) => {
                if err.kind.is_retryable() {
                    pending_lines.push(LogLine::Warning(format!(
                        "Install error (retryable): {}",
                        err.message
                    )));
                } else {
                    pending_lines.push(LogLine::Error(format!(
                        "Install error: {} — {}",
                        err.message,
                        err.kind.user_suggestion()
                    )));
                }
            }
            InstallOutput::Retrying {
                attempt,
                max_retries,
                reason,
            } => {
                pending_lines.push(LogLine::Retry(format!(
                    "Retrying install ({}/{}) — {}",
                    attempt, max_retries, reason
                )));
            }
            InstallOutput::Completed => {
                pending_lines.push(LogLine::Progress("Installation complete".to_string()));
            }
            InstallOutput::Failed(err) => {
                pending_lines.push(LogLine::Error(format!(
                    "Installation failed: {} — {}",
                    err.message,
                    err.kind.user_suggestion()
                )));
            }
        }
    }

    fn process_launch_output(output: LaunchOutput, pending_lines: &mut Vec<LogLine>) {
        match output {
            LaunchOutput::Line(line) => {
                pending_lines.push(LogLine::Normal(line));
            }
            LaunchOutput::Progress(msg) => {
                pending_lines.push(LogLine::Progress(msg));
            }
            LaunchOutput::Completed { pid } => {
                let msg = if let Some(pid) = pid {
                    format!("App launched (PID: {})", pid)
                } else {
                    "App launched".to_string()
                };
                pending_lines.push(LogLine::Progress(msg));
            }
            LaunchOutput::Failed(err) => {
                pending_lines.push(LogLine::Error(format!("Launch failed: {}", err.message)));
            }
        }
    }

    fn scroll_to_bottom(&self) {
        let count = self.visible_indices().len();
        if count > 0 {
            self.scroll_handle
                .scroll_to_item(count - 1, ScrollStrategy::Bottom);
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, line)| self.show_verbose || !matches!(line, LogLine::Verbose(_)))
            .map(|(i, _)| i)
            .collect()
    }

    fn render_line(&self, line: &LogLine, cx: &Context<Self>) -> impl IntoElement {
        match line {
            LogLine::Normal(text) => div().px_2().py_px().child(
                Label::new(text.clone())
                    .size(LabelSize::Small)
                    .color(Color::Default)
                    .single_line()
                    .truncate(),
            ),
            LogLine::Verbose(text) => div().px_2().py_px().child(
                Label::new(text.clone())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .single_line()
                    .truncate(),
            ),
            LogLine::Error(text) => div()
                .px_2()
                .py_px()
                .bg(cx.theme().status().error_background)
                .child(
                    Label::new(text.clone())
                        .size(LabelSize::Small)
                        .color(Color::Error)
                        .single_line()
                        .truncate(),
                ),
            LogLine::Warning(text) => div()
                .px_2()
                .py_px()
                .bg(cx.theme().status().warning_background)
                .child(
                    Label::new(text.clone())
                        .size(LabelSize::Small)
                        .color(Color::Warning)
                        .single_line()
                        .truncate(),
                ),
            LogLine::Progress(text) => div().px_2().py_px().child(
                h_flex()
                    .gap_2()
                    .overflow_x_hidden()
                    .child(
                        Icon::new(IconName::ArrowRight)
                            .size(IconSize::Small)
                            .color(Color::Accent),
                    )
                    .child(
                        Label::new(text.clone())
                            .size(LabelSize::Small)
                            .color(Color::Accent)
                            .single_line()
                            .truncate(),
                    ),
            ),
            LogLine::InstallProgress { phase, percent } => {
                let label = format!("{} — {:.0}%", phase, percent);
                let bar_width = (*percent).clamp(0.0, 100.0);
                div().px_2().py_px().child(
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .overflow_x_hidden()
                                .child(
                                    Icon::new(IconName::ArrowRight)
                                        .size(IconSize::Small)
                                        .color(Color::Accent),
                                )
                                .child(
                                    Label::new(label)
                                        .size(LabelSize::Small)
                                        .color(Color::Accent)
                                        .single_line()
                                        .truncate(),
                                ),
                        )
                        .child(
                            div()
                                .h(px(3.0))
                                .w_full()
                                .bg(cx.theme().colors().border)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(bar_width / 100.0))
                                        .bg(cx.theme().status().info),
                                ),
                        ),
                )
            }
            LogLine::PhaseChange(text) => div()
                .px_2()
                .py_1()
                .border_t_1()
                .border_color(cx.theme().colors().border)
                .child(
                    h_flex()
                        .gap_2()
                        .overflow_x_hidden()
                        .child(
                            Icon::new(IconName::ArrowRight)
                                .size(IconSize::Small)
                                .color(Color::Accent),
                        )
                        .child(
                            Label::new(text.clone())
                                .size(LabelSize::Small)
                                .color(Color::Accent)
                                .weight(gpui::FontWeight::BOLD)
                                .single_line()
                                .truncate(),
                        ),
                ),
            LogLine::Retry(text) => div()
                .px_2()
                .py_px()
                .bg(cx.theme().status().warning_background)
                .child(
                    h_flex()
                        .gap_2()
                        .overflow_x_hidden()
                        .child(
                            Icon::new(IconName::ArrowCircle)
                                .size(IconSize::Small)
                                .color(Color::Warning),
                        )
                        .child(
                            Label::new(text.clone())
                                .size(LabelSize::Small)
                                .color(Color::Warning)
                                .single_line()
                                .truncate(),
                        ),
                ),
        }
    }

    fn full_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| match line {
                LogLine::Normal(text) => text.clone(),
                LogLine::Verbose(text) => text.clone(),
                LogLine::Error(text) => text.clone(),
                LogLine::Warning(text) => text.clone(),
                LogLine::Progress(text) => text.clone(),
                LogLine::InstallProgress { phase, percent } => {
                    format!("{} — {:.0}%", phase, percent)
                }
                LogLine::PhaseChange(text) => format!("--- {} ---", text),
                LogLine::Retry(text) => text.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n")
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
        let visible_indices = self.visible_indices();
        let visible_count = visible_indices.len();
        let has_lines = !self.lines.is_empty();
        let full_text = self.full_text();
        let header_label = self.header_label;
        let show_verbose = self.show_verbose;

        v_flex()
            .key_context("BuildLogsView")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().editor_background)
            .child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Label::new(header_label)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .when(has_lines, |this| {
                        this.child(
                            h_flex()
                                .gap_1()
                                .child(
                                    IconButton::new("toggle-verbose", IconName::ListTree)
                                        .icon_size(IconSize::Small)
                                        .icon_color(if show_verbose {
                                            Color::Accent
                                        } else {
                                            Color::Muted
                                        })
                                        .tooltip(Tooltip::text(if show_verbose {
                                            "Hide Verbose Output"
                                        } else {
                                            "Show All Output"
                                        }))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.show_verbose = !this.show_verbose;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    IconButton::new("copy-logs", IconName::Copy)
                                        .icon_size(IconSize::Small)
                                        .icon_color(Color::Muted)
                                        .tooltip(Tooltip::text("Copy All Logs"))
                                        .on_click({
                                            let full_text = full_text.clone();
                                            move |_, _window, cx| {
                                                cx.write_to_clipboard(
                                                    ClipboardItem::new_string(full_text.clone()),
                                                );
                                            }
                                        }),
                                ),
                        )
                    }),
            )
            .when(!has_lines, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new("Building...").color(Color::Muted)),
                )
            })
            .when(has_lines, |this| {
                let view = cx.weak_entity();
                this.child(
                    uniform_list(
                        "build-logs",
                        visible_count,
                        move |range, _window, cx| {
                            let Some(view) = view.upgrade() else {
                                return Vec::new();
                            };
                            view.update(cx, |this, cx| {
                                let indices = this.visible_indices();
                                range
                                    .into_iter()
                                    .filter_map(|ix| {
                                        let real_idx = indices.get(ix).copied()?;
                                        this.lines.get(real_idx).cloned()
                                    })
                                    .map(|line| this.render_line(&line, cx).into_any_element())
                                    .collect()
                            })
                        },
                    )
                    .flex_1()
                    .track_scroll(&self.scroll_handle),
                )
            })
    }
}

impl Item for BuildLogsView {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        if self.is_complete {
            if self.build_success == Some(true) {
                "Build".into()
            } else {
                "Build failed".into()
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

        Label::new(text).color(color).into_any_element()
    }
}
