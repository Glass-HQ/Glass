use std::sync::Arc;

use app_runtime::{ExecutionRequest, RuntimeAction, RuntimeCatalog, SystemCommandRunner};
use gpui::{
    App, AppContext, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Render,
    SharedString, Task, WeakEntity,
};
use picker::{Picker, PickerDelegate};
use task::{HideStrategy, RevealStrategy, SaveStrategy, Shell, SpawnInTerminal, TaskId};
use ui::{Color, Label, LabelSize, ListItem, ListItemSpacing, prelude::*};
use uuid::Uuid;
use workspace::notifications::NotificationId;
use workspace::{ModalView, Toast, Workspace};

use crate::OpenRuntimeActions;

pub struct RuntimeActionsModal {
    picker: Entity<Picker<RuntimeActionsDelegate>>,
}

impl RuntimeActionsModal {
    pub fn toggle(
        workspace: &mut Workspace,
        _: &OpenRuntimeActions,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let workspace_paths = workspace
            .project()
            .read(cx)
            .visible_worktrees(cx)
            .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
            .collect::<Vec<_>>();
        let workspace_handle = workspace.weak_handle();

        workspace.toggle_modal(window, cx, |window, cx| {
            Self::new(workspace_handle.clone(), workspace_paths.clone(), window, cx)
        });
    }

    fn new(
        workspace: WeakEntity<Workspace>,
        workspace_paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let modal = cx.entity().downgrade();
        let delegate = RuntimeActionsDelegate::new(modal, workspace, cx);
        let picker = cx.new(|cx| {
            Picker::list(delegate, window, cx)
                .list_measure_all()
                .show_scrollbar(true)
        });

        let picker_handle = picker.downgrade();
        let load_task = cx.spawn_in(window, async move |_, cx| {
            let catalog = cx
                .background_spawn(async move {
                    let runner = SystemCommandRunner;
                    RuntimeCatalog::discover(&workspace_paths, &runner)
                })
                .await;

            picker_handle
                .update_in(cx, |picker, window, cx| {
                    picker
                        .delegate
                        .catalog_loaded(catalog, picker.query(cx).to_string(), window, cx);
                    picker.refresh(window, cx);
                })
                .ok();
        });
        picker.update(cx, |picker, _| {
            picker.delegate.set_loading_task(load_task);
        });

        Self { picker }
    }
}

impl EventEmitter<DismissEvent> for RuntimeActionsModal {}

impl Focusable for RuntimeActionsModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl ModalView for RuntimeActionsModal {}

impl Render for RuntimeActionsModal {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(rems(34.)).child(self.picker.clone())
    }
}

#[derive(Clone)]
struct RuntimeActionEntry {
    title: String,
    subtitle: String,
    searchable_text: String,
    request: Option<ExecutionRequest>,
}

struct RuntimeActionsDelegate {
    modal: WeakEntity<RuntimeActionsModal>,
    workspace: WeakEntity<Workspace>,
    catalog: Option<RuntimeCatalog>,
    all_entries: Vec<RuntimeActionEntry>,
    filtered_entries: Vec<RuntimeActionEntry>,
    selected_index: usize,
    loading: bool,
    _loading_task: Option<Task<()>>,
}

impl RuntimeActionsDelegate {
    fn new(
        modal: WeakEntity<RuntimeActionsModal>,
        workspace: WeakEntity<Workspace>,
        _: &mut Context<RuntimeActionsModal>,
    ) -> Self {
        Self {
            modal,
            workspace,
            catalog: None,
            all_entries: Vec::new(),
            filtered_entries: Vec::new(),
            selected_index: 0,
            loading: true,
            _loading_task: None,
        }
    }

    fn set_loading_task(&mut self, task: Task<()>) {
        self._loading_task = Some(task);
    }

    fn catalog_loaded(
        &mut self,
        catalog: RuntimeCatalog,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        self.catalog = Some(catalog);
        self.loading = false;
        self.all_entries = build_entries(self.catalog.as_ref().expect("catalog should be set"));
        let _ = self.update_matches(query, window, cx);
    }
}

impl PickerDelegate for RuntimeActionsDelegate {
    type ListItem = ListItem;

    fn match_count(&self) -> usize {
        self.filtered_entries.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = index.min(self.filtered_entries.len().saturating_sub(1));
        cx.notify();
    }

    fn can_select(
        &self,
        index: usize,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> bool {
        self.filtered_entries
            .get(index)
            .is_some_and(|entry| entry.request.is_some())
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        Arc::from("Search runtime actions")
    }

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        if self.loading {
            Some("Detecting Apple runtime capabilities…".into())
        } else if self.all_entries.is_empty() {
            Some("No runnable Apple project was detected in this workspace".into())
        } else {
            Some("No runtime actions match the current query".into())
        }
    }

    fn update_matches(
        &mut self,
        query: String,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let normalized_query = query.trim().to_lowercase();
        self.filtered_entries = if normalized_query.is_empty() {
            self.all_entries.clone()
        } else {
            self.all_entries
                .iter()
                .filter(|entry| entry.searchable_text.contains(&normalized_query))
                .cloned()
                .collect()
        };
        self.selected_index = 0;
        Task::ready(())
    }

    fn confirm(
        &mut self,
        _: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(entry) = self.filtered_entries.get(self.selected_index) else {
            return;
        };
        let Some(request) = entry.request.clone() else {
            return;
        };
        let Some(catalog) = self.catalog.as_ref() else {
            return;
        };

        let plan = match catalog.build_execution_plan(&request) {
            Ok(plan) => plan,
            Err(error) => {
                self.workspace
                    .update(cx, |workspace, cx| {
                        workspace.show_toast(
                            Toast::new(
                                NotificationId::unique::<RuntimeActionsModal>(),
                                error.to_string(),
                            )
                            .autohide(),
                            cx,
                        );
                    })
                    .ok();
                return;
            }
        };

        let task = SpawnInTerminal {
            id: TaskId(format!("app-runtime-{}", Uuid::new_v4())),
            full_label: plan.label.clone(),
            label: plan.label,
            command: Some(plan.command),
            args: plan.args,
            command_label: plan.command_label,
            cwd: Some(plan.cwd),
            env: Default::default(),
            use_new_terminal: true,
            allow_concurrent_runs: true,
            reveal: RevealStrategy::Always,
            reveal_target: task::RevealTarget::Dock,
            hide: HideStrategy::Never,
            shell: Shell::System,
            show_summary: true,
            show_command: true,
            show_rerun: true,
            save: SaveStrategy::All,
        };

        self.workspace
            .update(cx, |workspace, cx| {
                workspace.spawn_in_terminal(task, window, cx).detach();
                workspace.hide_modal(window, cx);
            })
            .ok();
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.modal.update(cx, |_, cx| cx.emit(DismissEvent)).ok();
    }

    fn render_match(
        &self,
        index: usize,
        selected: bool,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let entry = self.filtered_entries.get(index)?;
        let enabled = entry.request.is_some();

        Some(
            ListItem::new(index)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected && enabled)
                .disabled(!enabled)
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .gap_0p5()
                        .child(Label::new(entry.title.clone()).single_line())
                        .child(
                            Label::new(entry.subtitle.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .single_line()
                                .truncate(),
                        ),
                ),
        )
    }
}

fn build_entries(catalog: &RuntimeCatalog) -> Vec<RuntimeActionEntry> {
    let mut entries = Vec::new();

    for project in &catalog.projects {
        for target in &project.targets {
            entries.push(RuntimeActionEntry {
                title: format!("Build {}", target.label),
                subtitle: format!("{} in {}", project.label, project.workspace_root.display()),
                searchable_text: format!(
                    "build {} {} {}",
                    project.label.to_lowercase(),
                    target.label.to_lowercase(),
                    project.workspace_root.display()
                ),
                request: Some(ExecutionRequest {
                    project_id: project.id.clone(),
                    target_id: target.id.clone(),
                    device_id: None,
                    action: RuntimeAction::Build,
                }),
            });

            if project.devices.is_empty() {
                entries.push(RuntimeActionEntry {
                    title: format!("Run {}", target.label),
                    subtitle: "No iOS simulator is currently available".into(),
                    searchable_text: format!(
                        "run {} {}",
                        project.label.to_lowercase(),
                        target.label.to_lowercase()
                    ),
                    request: None,
                });
                continue;
            }

            for device in &project.devices {
                let device_label = match &device.os_version {
                    Some(os_version) => format!("{} ({os_version})", device.name),
                    None => device.name.clone(),
                };

                entries.push(RuntimeActionEntry {
                    title: format!("Run {} on {}", target.label, device_label),
                    subtitle: project.label.clone(),
                    searchable_text: format!(
                        "run {} {} {}",
                        project.label.to_lowercase(),
                        target.label.to_lowercase(),
                        device_label.to_lowercase()
                    ),
                    request: Some(ExecutionRequest {
                        project_id: project.id.clone(),
                        target_id: target.id.clone(),
                        device_id: Some(device.id.clone()),
                        action: RuntimeAction::Run,
                    }),
                });
            }
        }
    }

    entries
}
