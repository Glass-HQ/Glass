use std::{collections::HashMap, process::ExitStatus};

use anyhow::{Context as _, anyhow};
use app_runtime::{ExecutionPlan, ExecutionRequest, RuntimeAction};
use editor::Editor;
use futures::{
    AsyncRead, AsyncReadExt as _, StreamExt as _,
    channel::mpsc::{UnboundedSender, unbounded},
};
use gpui::{
    App, AppContext as _, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Global, Render, SharedString, Task, WeakEntity,
};
use language::{Buffer, PLAIN_TEXT};
use multi_buffer::MultiBuffer;
use ui::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Icon, IconButton, IconName, Label,
    LabelSize, Tooltip, prelude::*,
};
use util::command::{Stdio, new_command};
use workspace::Workspace;
use workspace::item::{Item, ItemBufferKind, ItemEvent};

#[derive(Default)]
struct RuntimeExecutionRegistry(HashMap<String, WeakEntity<RuntimeExecutionPage>>);

impl Global for RuntimeExecutionRegistry {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecutionStatus {
    Idle,
    Running,
    Succeeded,
    Failed,
}

impl ExecutionStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Ready",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Idle => Color::Muted,
            Self::Running => Color::Accent,
            Self::Succeeded => Color::Success,
            Self::Failed => Color::Error,
        }
    }
}

enum OutputEvent {
    Chunk(String),
    Exited(ExitStatus),
}

pub struct RuntimeExecutionPage {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    session_key: String,
    session_label: String,
    request: ExecutionRequest,
    plan: ExecutionPlan,
    status: ExecutionStatus,
    output_buffer: Entity<Buffer>,
    editor: Entity<Editor>,
    execution_task: Option<Task<()>>,
}

impl RuntimeExecutionPage {
    pub fn open_or_reuse(
        workspace: &mut Workspace,
        session_key: String,
        session_label: String,
        request: ExecutionRequest,
        plan: ExecutionPlan,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if let Some(existing) = cx
            .default_global::<RuntimeExecutionRegistry>()
            .0
            .get(&session_key)
            .and_then(|page| page.upgrade())
        {
            workspace.activate_item(&existing, true, true, window, cx);
            existing.update(cx, |page, cx| page.start_execution(window, cx));
            return;
        }

        let page = Self::new(
            workspace.weak_handle(),
            session_key.clone(),
            session_label,
            request,
            plan,
            window,
            cx,
        );
        cx.default_global::<RuntimeExecutionRegistry>()
            .0
            .insert(session_key, page.downgrade());
        workspace.add_item_to_active_pane(Box::new(page.clone()), None, true, window, cx);
        page.update(cx, |page, cx| page.start_execution(window, cx));
    }

    fn new(
        workspace: WeakEntity<Workspace>,
        session_key: String,
        session_label: String,
        request: ExecutionRequest,
        plan: ExecutionPlan,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let output_buffer =
            cx.new(|cx| Buffer::local("", cx).with_language(PLAIN_TEXT.clone(), cx));
        let output = cx.new(|cx| {
            MultiBuffer::singleton(output_buffer.clone(), cx)
                .with_title(session_label.clone().into())
        });
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(output, None, window, cx);
            editor.set_read_only(true);
            editor
        });

        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
            workspace,
            session_key,
            session_label,
            request,
            plan,
            status: ExecutionStatus::Idle,
            output_buffer,
            editor,
            execution_task: None,
        })
    }

    fn start_execution(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.execution_task.is_some() {
            return;
        }

        self.status = ExecutionStatus::Running;
        self.replace_output(String::new(), window, cx);
        self.append_output(
            &format!("$ {}\n\nLaunching runtime command…\n", self.command_line()),
            window,
            cx,
        );

        let plan = self.plan.clone();
        self.execution_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = run_execution_stream(plan, this.clone(), cx).await;
            this.update_in(cx, |this, window, cx| {
                this.execution_task = None;

                match result {
                    Ok(status) if status.success() => {
                        this.status = ExecutionStatus::Succeeded;
                        this.append_output(
                            &format!("\nProcess exited successfully.\n"),
                            window,
                            cx,
                        );
                    }
                    Ok(status) => {
                        this.status = ExecutionStatus::Failed;
                        this.append_output(
                            &format!("\nProcess exited with {}.\n", describe_exit_status(status)),
                            window,
                            cx,
                        );
                    }
                    Err(error) => {
                        this.status = ExecutionStatus::Failed;
                        this.append_output(
                            &format!("\nRuntime execution failed: {error:#}\n"),
                            window,
                            cx,
                        );
                    }
                }

                cx.notify();
            })
            .ok();
        }));

        cx.notify();
    }

    fn rerun(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_execution(window, cx);
    }

    fn replace_output(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.output_buffer.update(cx, |buffer, cx| {
            buffer.set_text(text, cx);
        });
        self.scroll_to_end(window, cx);
    }

    fn append_output(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }

        self.output_buffer.update(cx, |buffer, cx| {
            let end = buffer.len();
            buffer.edit([(end..end, text)], None, cx);
        });
        self.scroll_to_end(window, cx);
    }

    fn scroll_to_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let end = self.editor.read(cx).buffer().read(cx).len(cx);
        self.editor.update(cx, |editor, cx| {
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections.select_ranges(Some(end..end));
            });
        });
    }

    fn command_line(&self) -> String {
        std::iter::once(self.plan.command.as_str())
            .chain(self.plan.args.iter().map(|argument| argument.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl EventEmitter<ItemEvent> for RuntimeExecutionPage {}

impl Focusable for RuntimeExecutionPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for RuntimeExecutionPage {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        self.session_label.clone().into()
    }

    fn tab_tooltip_text(&self, _cx: &App) -> Option<SharedString> {
        Some(self.command_line().into())
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(match self.request.action {
            RuntimeAction::Run => IconName::PlayFilled,
            RuntimeAction::Build => IconName::BoltOutlined,
        }))
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Runtime Execution Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }

    fn on_removed(&self, cx: &mut Context<Self>) {
        cx.default_global::<RuntimeExecutionRegistry>()
            .0
            .remove(&self.session_key);
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event)
    }
}

impl Render for RuntimeExecutionPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rerun_disabled = self.execution_task.is_some();

        v_flex()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap_1()
                            .child(Label::new(self.session_label.clone()).size(LabelSize::Large))
                            .child(
                                Label::new(self.command_line())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted)
                                    .single_line()
                                    .truncate(),
                            )
                            .child(
                                Label::new(format!(
                                    "{} · {}",
                                    self.status.label(),
                                    self.plan.cwd.display()
                                ))
                                .size(LabelSize::Small)
                                .color(self.status.color())
                                .single_line(),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("runtime-execution-rerun", "Rerun")
                                    .style(ButtonStyle::Filled)
                                    .label_size(LabelSize::Small)
                                    .disabled(rerun_disabled)
                                    .tooltip(Tooltip::text("Run this command again"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.rerun(window, cx);
                                    })),
                            )
                            .child(
                                IconButton::new("runtime-execution-actions", IconName::PlayFilled)
                                    .icon_size(ui::IconSize::Small)
                                    .shape(ui::IconButtonShape::Square)
                                    .tooltip(Tooltip::text("Open runtime actions"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let Some(workspace) = this.workspace.upgrade() else {
                                            return;
                                        };
                                        workspace.update(cx, |workspace, cx| {
                                            crate::runtime_actions_modal::RuntimeActionsModal::toggle(
                                                workspace,
                                                &crate::OpenRuntimeActions,
                                                window,
                                                cx,
                                            );
                                        });
                                    })),
                            ),
                    ),
            )
            .child(div().size_full().child(self.editor.clone()))
    }
}

async fn run_execution_stream(
    plan: ExecutionPlan,
    this: WeakEntity<RuntimeExecutionPage>,
    cx: &mut AsyncWindowContext,
) -> anyhow::Result<ExitStatus> {
    let mut command = new_command(&plan.command);
    command
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .with_context(|| format!("Failed to start `{}`", plan.command))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture stdout for `{}`", plan.command))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to capture stderr for `{}`", plan.command))?;

    let (events_tx, mut events_rx) = unbounded();
    let stdout_task = cx.background_spawn(read_stream(stdout, events_tx.clone()));
    let stderr_task = cx.background_spawn(read_stream(stderr, events_tx.clone()));
    let exit_task = cx.background_spawn(async move {
        let status = child
            .status()
            .await
            .with_context(|| format!("Failed while waiting for `{}`", plan.command))?;
        events_tx.unbounded_send(OutputEvent::Exited(status)).ok();
        anyhow::Ok(())
    });

    let mut exit_status = None;
    while let Some(event) = events_rx.next().await {
        match event {
            OutputEvent::Chunk(text) => {
                this.update_in(cx, |this, window, cx| {
                    this.append_output(&text, window, cx);
                })
                .ok();
            }
            OutputEvent::Exited(status) => {
                exit_status = Some(status);
                break;
            }
        }
    }

    stdout_task.await?;
    stderr_task.await?;
    exit_task.await?;

    exit_status.ok_or_else(|| anyhow!("Runtime process exited without a status"))
}

async fn read_stream<R>(
    mut reader: R,
    output_tx: UnboundedSender<OutputEvent>,
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut buffer = [0_u8; 4096];

    loop {
        let bytes_read = reader.read(&mut buffer).await?;
        if bytes_read == 0 {
            return Ok(());
        }

        output_tx
            .unbounded_send(OutputEvent::Chunk(
                String::from_utf8_lossy(&buffer[..bytes_read]).into_owned(),
            ))
            .ok();
    }
}

fn describe_exit_status(status: ExitStatus) -> String {
    status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "an unknown status".to_string())
}
