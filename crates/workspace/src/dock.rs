use crate::persistence::model::DockData;
use crate::{DraggedDock, Event, ModalLayer, Pane};
use crate::{MultiWorkspace, Workspace};
use anyhow::Context as _;
use client::proto;

use gpui::{
    Action, AnyView, App, Axis, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    IntoElement, KeyContext, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement, Render,
    SharedString, StyleRefinement, Styled, Subscription, WeakEntity, Window,
    WindowBackgroundAppearance, actions, deferred, div,
};
use settings::SettingsStore;
use std::sync::Arc;
use theme::{ActiveTheme, active_component_radius};
use ui::{Divider, IconButtonShape, Tooltip, prelude::*};
use workspace_chrome::SidebarRow;
use zed_actions::OpenRecent;

actions!(
    workspace,
    [
        /// Opens the project diagnostics view from the dock button bar.
        DeployProjectDiagnostics,
        /// Opens the runtime actions menu from the dock button bar.
        OpenRuntimeActions,
        /// Opens the services view from the dock button bar.
        OpenServices,
        /// Toggles the project search view open or closed.
        ToggleProjectSearch,
        /// Toggles the project diagnostics view open or closed.
        ToggleProjectDiagnostics,
    ]
);

pub(crate) const RESIZE_HANDLE_SIZE: Pixels = px(6.);

/// Shared sidebar chrome rendered above dock or hosted sidebar content.
/// This is a separate entity to avoid borrow conflicts when reading workspace
/// state during render - when this entity renders, the workspace update is complete.
pub struct DockButtonBar {
    workspace: WeakEntity<Workspace>,
    language_server_button: Option<AnyView>,
    _subscriptions: Vec<Subscription>,
}

fn show_project_sidebar_tab(
    workspace: &WeakEntity<Workspace>,
    multi_workspace: Option<&Entity<MultiWorkspace>>,
    show_threads: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(multi_workspace) = multi_workspace {
        multi_workspace.update(cx, |multi_workspace, cx| {
            {
                if let Some(sidebar) = multi_workspace.sidebar() {
                    if show_threads {
                        sidebar.show_project_threads(window, cx);
                    } else {
                        sidebar.show_project_files(window, cx);
                    }
                }
            }

            if multi_workspace.sidebar_open() {
                multi_workspace.close_sidebar(window, cx);
            }
        });
    }

    if let Some(workspace) = workspace.upgrade() {
        workspace.update(cx, |workspace, cx| {
            workspace.select_sidebar_section(crate::WorkspaceSidebarSection::Project, window, cx);
        });
    }
}

impl DockButtonBar {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut App) -> Entity<Self> {
        cx.new(|_cx| Self {
            workspace,
            language_server_button: None,
            _subscriptions: vec![],
        })
    }

    pub fn set_language_server_button(
        &mut self,
        language_server_button: Option<AnyView>,
        cx: &mut Context<Self>,
    ) {
        self.language_server_button = language_server_button;
        cx.notify();
    }
}

impl Render for DockButtonBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(workspace) = self.workspace.upgrade() else {
            return div().into_any_element();
        };

        let workspace_read = workspace.read(cx);

        let multi_workspace = window.root::<MultiWorkspace>().flatten();
        let active_sidebar_section = workspace_read.active_sidebar_section();
        let project = workspace_read.project();
        let selected_worktree = workspace_read
            .active_worktree_override()
            .and_then(|worktree_id| project.read(cx).worktree_for_id(worktree_id, cx))
            .or_else(|| project.read(cx).visible_worktrees(cx).next());

        let project_label: SharedString = selected_worktree
            .as_ref()
            .map(|worktree| worktree.read(cx).root_name().as_unix_str().to_string())
            .unwrap_or_else(|| "Open Recent Project".to_string())
            .into();

        let selected_repository = selected_worktree.as_ref().and_then(|worktree| {
            let worktree_root = worktree.read(cx).abs_path().to_path_buf();
            project
                .read(cx)
                .repositories(cx)
                .values()
                .find(|repository| {
                    let snapshot = repository.read(cx).snapshot();
                    let repo_root: &std::path::Path = snapshot.work_directory_abs_path.as_ref();
                    repo_root == worktree_root.as_path()
                })
                .cloned()
        });

        const MAX_BRANCH_BUTTON_LABEL_LEN: usize = 18;

        let branch_label = selected_repository
            .as_ref()
            .and_then(|repository| {
                let repository = repository.read(cx);
                repository
                    .branch
                    .as_ref()
                    .map(|branch| branch.name().to_string())
                    .or_else(|| {
                        repository
                            .head_commit
                            .as_ref()
                            .map(|commit| commit.sha.chars().take(8).collect::<String>())
                    })
            })
            .unwrap_or_else(|| "Switch Branch".to_string());
        let branch_button_label = if branch_label.len() <= MAX_BRANCH_BUTTON_LABEL_LEN {
            branch_label.clone()
        } else {
            util::truncate_and_trailoff(branch_label.trim_ascii(), MAX_BRANCH_BUTTON_LABEL_LEN)
        };

        let mut project_panel_badge = None;
        let mut git_panel_badge = None;

        for dock_entity in [&workspace_read.left_dock, &workspace_read.right_dock] {
            let dock = dock_entity.read(cx);

            for entry in &dock.panel_entries {
                match entry.panel.panel_key() {
                    "ProjectPanel" => {
                        project_panel_badge = entry.panel.icon_label(window, cx);
                    }
                    "GitPanel" => {
                        git_panel_badge = entry.panel.icon_label(window, cx);
                    }
                    _ => {}
                }
            }
        }

        let project_picker_row = SidebarRow::new(
            "sidebar-project-picker",
            project_label,
            IconName::OpenFolder,
        )
        .end_slot(
            div().max_w(rems(9.5)).child(
                Button::new("sidebar-branch-picker", branch_button_label)
                    .style(ButtonStyle::Transparent)
                    .size(ButtonSize::None)
                    .label_size(LabelSize::Small)
                    .start_icon(
                        Icon::new(IconName::GitBranchAlt)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .truncate(true)
                    .tooltip(Tooltip::text(branch_label.clone()))
                    .on_click(|_, window: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                        window.dispatch_action(zed_actions::git::Branch.boxed_clone(), cx);
                    }),
            ),
        )
        .on_click(|_, window, cx| {
            window.dispatch_action(
                OpenRecent {
                    create_new_window: false,
                }
                .boxed_clone(),
                cx,
            );
        })
        .into_any_element();

        let mut mode_rows = Vec::new();

        mode_rows.push(
            SidebarRow::new("sidebar-project-panel", "Project", IconName::FileTree)
                .selected(active_sidebar_section == crate::WorkspaceSidebarSection::Project)
                .end_slot(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new("sidebar-project-threads", "Threads")
                                .style(ButtonStyle::Transparent)
                                .size(ButtonSize::None)
                                .label_size(LabelSize::Small)
                                .start_icon(
                                    Icon::new(IconName::Thread)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .on_click({
                                    let workspace = self.workspace.clone();
                                    let multi_workspace = multi_workspace.clone();
                                    move |_, window: &mut Window, cx: &mut App| {
                                        cx.stop_propagation();
                                        show_project_sidebar_tab(
                                            &workspace,
                                            multi_workspace.as_ref(),
                                            true,
                                            window,
                                            cx,
                                        );
                                    }
                                }),
                        )
                        .when_some(project_panel_badge, |row, badge| {
                            row.child(
                                Label::new(badge)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                        }),
                )
                .on_click({
                    let workspace = self.workspace.clone();
                    let multi_workspace = multi_workspace.clone();
                    move |_, window, cx| {
                        show_project_sidebar_tab(
                            &workspace,
                            multi_workspace.as_ref(),
                            false,
                            window,
                            cx,
                        );
                    }
                })
                .into_any_element(),
        );

        mode_rows.push(
            SidebarRow::new("sidebar-git-panel", "Git", IconName::GitBranchAlt)
                .selected(active_sidebar_section == crate::WorkspaceSidebarSection::Git)
                .when_some(git_panel_badge, |row, badge| {
                    row.end_slot(
                        Label::new(badge)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                })
                .on_click({
                    let workspace = self.workspace.clone();
                    let multi_workspace = multi_workspace.clone();
                    move |_, window, cx| {
                        if let Some(multi_workspace) = multi_workspace.as_ref()
                            && multi_workspace.read(cx).sidebar_open()
                        {
                            multi_workspace.update(cx, |multi_workspace, cx| {
                                multi_workspace.close_sidebar(window, cx);
                            });
                        }

                        if let Some(workspace) = workspace.upgrade() {
                            workspace.update(cx, |workspace, cx| {
                                workspace.select_sidebar_section(
                                    crate::WorkspaceSidebarSection::Git,
                                    window,
                                    cx,
                                );
                            });
                        }
                    }
                })
                .into_any_element(),
        );

        mode_rows.push(
            SidebarRow::new("sidebar-browser-tabs", "Browser Tabs", IconName::Globe)
                .selected(active_sidebar_section == crate::WorkspaceSidebarSection::BrowserTabs)
                .on_click({
                    let workspace = self.workspace.clone();
                    let multi_workspace = multi_workspace.clone();
                    move |_, window, cx| {
                        if let Some(multi_workspace) = multi_workspace.as_ref()
                            && multi_workspace.read(cx).sidebar_open()
                        {
                            multi_workspace.update(cx, |multi_workspace, cx| {
                                multi_workspace.close_sidebar(window, cx);
                            });
                        }

                        if let Some(workspace) = workspace.upgrade() {
                            workspace.update(cx, |workspace, cx| {
                                workspace.select_sidebar_section(
                                    crate::WorkspaceSidebarSection::BrowserTabs,
                                    window,
                                    cx,
                                );
                            });
                        }
                    }
                })
                .into_any_element(),
        );

        mode_rows.push(
            SidebarRow::new("sidebar-terminal", "Terminal Tabs", IconName::Terminal)
                .selected(active_sidebar_section == crate::WorkspaceSidebarSection::Terminal)
                .on_click({
                    let workspace = self.workspace.clone();
                    let multi_workspace = multi_workspace.clone();
                    move |_, window, cx| {
                        if let Some(multi_workspace) = multi_workspace.as_ref()
                            && multi_workspace.read(cx).sidebar_open()
                        {
                            multi_workspace.update(cx, |multi_workspace, cx| {
                                multi_workspace.close_sidebar(window, cx);
                            });
                        }

                        if let Some(workspace) = workspace.upgrade() {
                            workspace.update(cx, |workspace, cx| {
                                workspace.select_sidebar_section(
                                    crate::WorkspaceSidebarSection::Terminal,
                                    window,
                                    cx,
                                );
                            });
                        }
                    }
                })
                .into_any_element(),
        );

        mode_rows.push(
            SidebarRow::new("sidebar-services", "Services", IconName::Server)
                .selected(active_sidebar_section == crate::WorkspaceSidebarSection::Services)
                .on_click({
                    let multi_workspace = multi_workspace.clone();
                    move |_, window, cx| {
                        if let Some(multi_workspace) = multi_workspace.as_ref()
                            && multi_workspace.read(cx).sidebar_open()
                        {
                            multi_workspace.update(cx, |multi_workspace, cx| {
                                multi_workspace.close_sidebar(window, cx);
                            });
                        }

                        window.dispatch_action(OpenServices.boxed_clone(), cx);
                    }
                })
                .into_any_element(),
        );

        let radius = cx.theme().component_radius().panel.unwrap_or(px(10.0));
        let diagnostics = project.read(cx).diagnostic_summary(false, cx);
        let (diagnostics_icon, diagnostics_icon_color) = if diagnostics.error_count > 0 {
            (IconName::XCircle, Color::Error)
        } else if diagnostics.warning_count > 0 {
            (IconName::Warning, Color::Warning)
        } else {
            (IconName::Check, Color::Success)
        };

        let supplementary_actions = h_flex()
            .w_full()
            .h(px(28.0))
            .items_center()
            .justify_center()
            .gap_1()
            .children([
                IconButton::new("sidebar-action-agent", IconName::Thread)
                    .shape(IconButtonShape::Square)
                    .style(ButtonStyle::Transparent)
                    .size(ButtonSize::Compact)
                    .icon_size(IconSize::Small)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action(
                            "Toggle Agent Panel",
                            &zed_actions::assistant::Toggle,
                            cx,
                        )
                    })
                    .on_click(|_, window: &mut Window, cx: &mut App| {
                        window.dispatch_action(zed_actions::assistant::Toggle.boxed_clone(), cx);
                    })
                    .into_any_element(),
                IconButton::new("sidebar-action-search", IconName::MagnifyingGlass)
                    .shape(IconButtonShape::Square)
                    .style(ButtonStyle::Transparent)
                    .size(ButtonSize::Compact)
                    .icon_size(IconSize::Small)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action("Project Search", &ToggleProjectSearch, cx)
                    })
                    .on_click(|_, window, cx| {
                        window.dispatch_action(ToggleProjectSearch.boxed_clone(), cx);
                    })
                    .into_any_element(),
                IconButton::new("sidebar-action-runtime", IconName::PlayFilled)
                    .shape(IconButtonShape::Square)
                    .style(ButtonStyle::Transparent)
                    .size(ButtonSize::Compact)
                    .icon_size(IconSize::Small)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action("Runtime Actions", &OpenRuntimeActions, cx)
                    })
                    .on_click(|_, window, cx| {
                        window.dispatch_action(OpenRuntimeActions.boxed_clone(), cx);
                    })
                    .into_any_element(),
                IconButton::new("sidebar-action-diagnostics", diagnostics_icon)
                    .shape(IconButtonShape::Square)
                    .style(ButtonStyle::Transparent)
                    .size(ButtonSize::Compact)
                    .icon_size(IconSize::Small)
                    .icon_color(diagnostics_icon_color)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action("Project Diagnostics", &ToggleProjectDiagnostics, cx)
                    })
                    .on_click(|_, window, cx| {
                        window.dispatch_action(ToggleProjectDiagnostics.boxed_clone(), cx);
                    })
                    .into_any_element(),
                IconButton::new("sidebar-action-debugger", IconName::Debug)
                    .shape(IconButtonShape::Square)
                    .style(ButtonStyle::Transparent)
                    .size(ButtonSize::Compact)
                    .icon_size(IconSize::Small)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action(
                            "Toggle Debug Panel",
                            &zed_actions::debug_panel::Toggle,
                            cx,
                        )
                    })
                    .on_click(|_, window, cx| {
                        window.dispatch_action(zed_actions::debug_panel::Toggle.boxed_clone(), cx);
                    })
                    .into_any_element(),
            ])
            .when_some(
                self.language_server_button.clone(),
                |this, language_server_button| this.child(language_server_button),
            );

        let has_sidebar_fill = matches!(
            cx.theme().window_background_appearance(),
            WindowBackgroundAppearance::Opaque
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .px_1()
            .py_1()
            .gap_1()
            .when(has_sidebar_fill, |this| {
                this.bg(cx.theme().colors().panel_background)
            })
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .p_1()
                    .when(has_sidebar_fill, |this| {
                        this.bg(cx.theme().colors().elevated_surface_background)
                    })
                    .border_1()
                    .border_color(cx.theme().colors().border_variant)
                    .rounded(radius)
                    .overflow_hidden()
                    .child(project_picker_row)
                    .child(Divider::horizontal())
                    .children(mode_rows)
                    .child(supplementary_actions),
            )
            .into_any_element()
    }
}

pub enum PanelEvent {
    ZoomIn,
    ZoomOut,
    Activate,
    Close,
    NavigationUpdated,
}

#[derive(Clone)]
pub struct PanelNavigationEntry {
    pub id: SharedString,
    pub label: SharedString,
    pub detail: Option<SharedString>,
    pub is_pinned: bool,
    pub is_selected: bool,
}

pub use proto::PanelId;

pub trait Panel: Focusable + EventEmitter<PanelEvent> + Render + Sized {
    fn persistent_name() -> &'static str;
    fn panel_key() -> &'static str;
    fn position(&self, window: &Window, cx: &App) -> DockPosition;
    fn position_is_valid(&self, position: DockPosition) -> bool;
    fn set_position(&mut self, position: DockPosition, window: &mut Window, cx: &mut Context<Self>);
    fn size(&self, window: &Window, cx: &App) -> Pixels;
    fn set_size(&mut self, size: Option<Pixels>, window: &mut Window, cx: &mut Context<Self>);
    fn icon(&self, window: &Window, cx: &App) -> Option<ui::IconName>;
    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str>;
    fn toggle_action(&self) -> Box<dyn Action>;
    fn icon_label(&self, _window: &Window, _: &App) -> Option<String> {
        None
    }
    fn is_zoomed(&self, _window: &Window, _cx: &App) -> bool {
        false
    }
    fn starts_open(&self, _window: &Window, _cx: &App) -> bool {
        false
    }
    fn set_zoomed(&mut self, _zoomed: bool, _window: &mut Window, _cx: &mut Context<Self>) {}
    fn set_active(&mut self, _active: bool, _window: &mut Window, _cx: &mut Context<Self>) {}
    fn pane(&self, _cx: &App) -> Option<Entity<Pane>> {
        None
    }
    fn navigation_panes(&self, cx: &App) -> Vec<Entity<Pane>> {
        self.pane(cx).into_iter().collect::<Vec<_>>()
    }
    fn navigation_entries(&self, _window: &Window, _cx: &App) -> Vec<PanelNavigationEntry> {
        Vec::new()
    }
    fn activate_navigation_entry(
        &mut self,
        _entry_id: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
    fn close_navigation_entry(
        &mut self,
        _entry_id: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
    fn create_navigation_entry(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
    fn remote_id() -> Option<proto::PanelId> {
        None
    }
    fn activation_priority(&self) -> u32;
    fn enabled(&self, _cx: &App) -> bool {
        true
    }
}

pub trait PanelHandle: Send + Sync {
    fn panel_id(&self) -> EntityId;
    fn persistent_name(&self) -> &'static str;
    fn panel_key(&self) -> &'static str;
    fn position(&self, window: &Window, cx: &App) -> DockPosition;
    fn position_is_valid(&self, position: DockPosition, cx: &App) -> bool;
    fn set_position(&self, position: DockPosition, window: &mut Window, cx: &mut App);
    fn is_zoomed(&self, window: &Window, cx: &App) -> bool;
    fn set_zoomed(&self, zoomed: bool, window: &mut Window, cx: &mut App);
    fn set_active(&self, active: bool, window: &mut Window, cx: &mut App);
    fn remote_id(&self) -> Option<proto::PanelId>;
    fn pane(&self, cx: &App) -> Option<Entity<Pane>>;
    fn navigation_panes(&self, cx: &App) -> Vec<Entity<Pane>>;
    fn size(&self, window: &Window, cx: &App) -> Pixels;
    fn set_size(&self, size: Option<Pixels>, window: &mut Window, cx: &mut App);
    fn icon(&self, window: &Window, cx: &App) -> Option<ui::IconName>;
    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str>;
    fn toggle_action(&self, window: &Window, cx: &App) -> Box<dyn Action>;
    fn icon_label(&self, window: &Window, cx: &App) -> Option<String>;
    fn panel_focus_handle(&self, cx: &App) -> FocusHandle;
    fn to_any(&self) -> AnyView;
    fn activation_priority(&self, cx: &App) -> u32;
    fn enabled(&self, cx: &App) -> bool;
    fn navigation_entries(&self, window: &Window, cx: &App) -> Vec<PanelNavigationEntry>;
    fn activate_navigation_entry(&self, entry_id: &str, window: &mut Window, cx: &mut App);
    fn close_navigation_entry(&self, entry_id: &str, window: &mut Window, cx: &mut App);
    fn create_navigation_entry(&self, window: &mut Window, cx: &mut App);
    fn move_to_next_position(&self, window: &mut Window, cx: &mut App) {
        let current_position = self.position(window, cx);
        let next_position = [
            DockPosition::Left,
            DockPosition::Bottom,
            DockPosition::Right,
        ]
        .into_iter()
        .filter(|position| self.position_is_valid(*position, cx))
        .skip_while(|valid_position| *valid_position != current_position)
        .nth(1)
        .unwrap_or(DockPosition::Left);

        self.set_position(next_position, window, cx);
    }
}

impl<T> PanelHandle for Entity<T>
where
    T: Panel,
{
    fn panel_id(&self) -> EntityId {
        Entity::entity_id(self)
    }

    fn persistent_name(&self) -> &'static str {
        T::persistent_name()
    }

    fn panel_key(&self) -> &'static str {
        T::panel_key()
    }

    fn position(&self, window: &Window, cx: &App) -> DockPosition {
        self.read(cx).position(window, cx)
    }

    fn position_is_valid(&self, position: DockPosition, cx: &App) -> bool {
        self.read(cx).position_is_valid(position)
    }

    fn set_position(&self, position: DockPosition, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_position(position, window, cx))
    }

    fn is_zoomed(&self, window: &Window, cx: &App) -> bool {
        self.read(cx).is_zoomed(window, cx)
    }

    fn set_zoomed(&self, zoomed: bool, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_zoomed(zoomed, window, cx))
    }

    fn set_active(&self, active: bool, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_active(active, window, cx))
    }

    fn pane(&self, cx: &App) -> Option<Entity<Pane>> {
        self.read(cx).pane(cx)
    }

    fn navigation_panes(&self, cx: &App) -> Vec<Entity<Pane>> {
        self.read(cx).navigation_panes(cx)
    }

    fn remote_id(&self) -> Option<PanelId> {
        T::remote_id()
    }

    fn size(&self, window: &Window, cx: &App) -> Pixels {
        self.read(cx).size(window, cx)
    }

    fn set_size(&self, size: Option<Pixels>, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_size(size, window, cx))
    }

    fn icon(&self, window: &Window, cx: &App) -> Option<ui::IconName> {
        self.read(cx).icon(window, cx)
    }

    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str> {
        self.read(cx).icon_tooltip(window, cx)
    }

    fn toggle_action(&self, _: &Window, cx: &App) -> Box<dyn Action> {
        self.read(cx).toggle_action()
    }

    fn icon_label(&self, window: &Window, cx: &App) -> Option<String> {
        self.read(cx).icon_label(window, cx)
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }

    fn panel_focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn activation_priority(&self, cx: &App) -> u32 {
        self.read(cx).activation_priority()
    }

    fn enabled(&self, cx: &App) -> bool {
        self.read(cx).enabled(cx)
    }

    fn navigation_entries(&self, window: &Window, cx: &App) -> Vec<PanelNavigationEntry> {
        self.read(cx).navigation_entries(window, cx)
    }

    fn activate_navigation_entry(&self, entry_id: &str, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.activate_navigation_entry(entry_id, window, cx)
        })
    }

    fn close_navigation_entry(&self, entry_id: &str, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.close_navigation_entry(entry_id, window, cx)
        })
    }

    fn create_navigation_entry(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.create_navigation_entry(window, cx))
    }
}

impl From<&dyn PanelHandle> for AnyView {
    fn from(val: &dyn PanelHandle) -> Self {
        val.to_any()
    }
}

/// A container with a fixed [`DockPosition`] adjacent to a certain widown edge.
/// Can contain multiple panels and show/hide itself with all contents.
pub struct Dock {
    position: DockPosition,
    pub(crate) panel_entries: Vec<PanelEntry>,
    workspace: WeakEntity<Workspace>,
    is_open: bool,
    active_panel_index: Option<usize>,
    focus_handle: FocusHandle,
    pub(crate) serialized_dock: Option<DockData>,
    zoom_layer_open: bool,
    modal_layer: Entity<ModalLayer>,
    dock_button_bar: Option<Entity<DockButtonBar>>,
    pub(crate) in_native_sidebar: bool,
    _subscriptions: [Subscription; 2],
}

impl Focusable for Dock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DockPosition {
    Left,
    Bottom,
    Right,
}

impl From<settings::DockPosition> for DockPosition {
    fn from(value: settings::DockPosition) -> Self {
        match value {
            settings::DockPosition::Left => Self::Left,
            settings::DockPosition::Bottom => Self::Bottom,
            settings::DockPosition::Right => Self::Right,
        }
    }
}

impl Into<settings::DockPosition> for DockPosition {
    fn into(self) -> settings::DockPosition {
        match self {
            Self::Left => settings::DockPosition::Left,
            Self::Bottom => settings::DockPosition::Bottom,
            Self::Right => settings::DockPosition::Right,
        }
    }
}

impl DockPosition {
    fn label(&self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Bottom => "Bottom",
            Self::Right => "Right",
        }
    }

    pub fn axis(&self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Bottom => Axis::Vertical,
        }
    }
}

pub(crate) struct PanelEntry {
    pub(crate) panel: Arc<dyn PanelHandle>,
    _subscriptions: [Subscription; 3],
}

impl Dock {
    pub fn new(
        position: DockPosition,
        modal_layer: Entity<ModalLayer>,
        dock_button_bar: Option<Entity<DockButtonBar>>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let focus_handle = cx.focus_handle();
        let workspace = cx.entity();
        let dock = cx.new(|cx| {
            let focus_subscription =
                cx.on_focus(&focus_handle, window, |dock: &mut Dock, window, cx| {
                    if let Some(active_entry) = dock.active_panel_entry() {
                        active_entry.panel.panel_focus_handle(cx).focus(window, cx)
                    }
                });
            let zoom_subscription = cx.subscribe(&workspace, |dock, workspace, e: &Event, cx| {
                if matches!(e, Event::ZoomChanged) {
                    let is_zoomed = workspace.read(cx).zoomed.is_some();
                    dock.zoom_layer_open = is_zoomed;
                }
            });
            Self {
                position,
                workspace: workspace.downgrade(),
                panel_entries: Default::default(),
                active_panel_index: None,
                is_open: false,
                focus_handle: focus_handle.clone(),
                _subscriptions: [focus_subscription, zoom_subscription],
                serialized_dock: None,
                zoom_layer_open: false,
                modal_layer,
                dock_button_bar,
                in_native_sidebar: false,
            }
        });

        cx.on_focus_in(&focus_handle, window, {
            let dock = dock.downgrade();
            move |workspace, window, cx| {
                let Some(dock) = dock.upgrade() else {
                    return;
                };
                let Some(panel) = dock.read(cx).active_panel() else {
                    return;
                };
                if panel.is_zoomed(window, cx) {
                    workspace.zoomed = Some(panel.to_any().downgrade());
                    workspace.zoomed_position = Some(position);
                } else {
                    workspace.zoomed = None;
                    workspace.zoomed_position = None;
                }
                cx.emit(Event::ZoomChanged);
                workspace.dismiss_zoomed_items_to_reveal(Some(position), window, cx);
            }
        })
        .detach();

        cx.observe_in(&dock, window, move |workspace, dock, window, cx| {
            if dock.read(cx).is_open()
                && let Some(panel) = dock.read(cx).active_panel()
                && panel.is_zoomed(window, cx)
            {
                workspace.zoomed = Some(panel.to_any().downgrade());
                workspace.zoomed_position = Some(position);
                cx.emit(Event::ZoomChanged);
                return;
            }
            if workspace.zoomed_position == Some(position) {
                workspace.zoomed = None;
                workspace.zoomed_position = None;
                cx.emit(Event::ZoomChanged);
            }
        })
        .detach();

        dock
    }

    pub fn position(&self) -> DockPosition {
        self.position
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    fn resizable(&self, cx: &App) -> bool {
        !(self.zoom_layer_open || self.modal_layer.read(cx).has_active_modal())
    }

    pub fn panel<T: Panel>(&self) -> Option<Entity<T>> {
        self.panel_entries
            .iter()
            .find_map(|entry| entry.panel.to_any().downcast().ok())
    }

    pub fn panel_index_for_type<T: Panel>(&self) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel.to_any().downcast::<T>().is_ok())
    }

    pub fn panel_index_for_persistent_name(&self, ui_name: &str, _cx: &App) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel.persistent_name() == ui_name)
    }

    pub fn panel_index_for_proto_id(&self, panel_id: PanelId) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel.remote_id() == Some(panel_id))
    }

    pub fn panel_index_for_id(&self, panel_id: EntityId) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel.panel_id() == panel_id)
    }

    pub fn panel_for_id(&self, panel_id: EntityId) -> Option<&Arc<dyn PanelHandle>> {
        self.panel_entries
            .iter()
            .find(|entry| entry.panel.panel_id() == panel_id)
            .map(|entry| &entry.panel)
    }

    /// Get a panel by its key (e.g., "TerminalPanel")
    pub fn panel_for_key(&self, key: &str) -> Option<&Arc<dyn PanelHandle>> {
        self.panel_entries
            .iter()
            .find(|entry| entry.panel.panel_key() == key)
            .map(|entry| &entry.panel)
    }

    pub(crate) fn native_sidebar_button_bar(&self) -> Option<Entity<DockButtonBar>> {
        self.dock_button_bar.clone()
    }

    pub fn first_enabled_panel_idx(&mut self, cx: &mut Context<Self>) -> anyhow::Result<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel.enabled(cx))
            .with_context(|| {
                format!(
                    "Couldn't find any enabled panel for the {} dock.",
                    self.position.label()
                )
            })
    }

    pub(crate) fn active_panel_entry(&self) -> Option<&PanelEntry> {
        self.active_panel_index
            .and_then(|index| self.panel_entries.get(index))
    }

    pub fn active_panel_index(&self) -> Option<usize> {
        self.active_panel_index
    }

    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open != self.is_open {
            self.is_open = open;
            if let Some(active_panel) = self.active_panel_entry() {
                active_panel.panel.set_active(open, window, cx);
            }

            cx.notify();
        }
    }

    pub fn set_panel_zoomed(
        &mut self,
        panel: &AnyView,
        zoomed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for entry in &mut self.panel_entries {
            if entry.panel.panel_id() == panel.entity_id() {
                if zoomed != entry.panel.is_zoomed(window, cx) {
                    entry.panel.set_zoomed(zoomed, window, cx);
                }
            } else if entry.panel.is_zoomed(window, cx) {
                entry.panel.set_zoomed(false, window, cx);
            }
        }

        self.workspace
            .update(cx, |workspace, cx| {
                workspace.serialize_workspace(window, cx);
            })
            .ok();
        cx.notify();
    }

    pub fn zoom_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for entry in &mut self.panel_entries {
            if entry.panel.is_zoomed(window, cx) {
                entry.panel.set_zoomed(false, window, cx);
            }
        }
    }

    pub(crate) fn add_panel<T: Panel>(
        &mut self,
        panel: Entity<T>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let subscriptions = [
            cx.observe(&panel, |_, _, cx| cx.notify()),
            cx.observe_global_in::<SettingsStore>(window, {
                let workspace = workspace.clone();
                let panel = panel.clone();

                move |this, window, cx| {
                    let new_position = panel.read(cx).position(window, cx);
                    if new_position == this.position {
                        return;
                    }

                    let Ok(new_dock) = workspace.update(cx, |workspace, cx| {
                        if panel.is_zoomed(window, cx) {
                            workspace.zoomed_position = Some(new_position);
                        }
                        match new_position {
                            DockPosition::Left => &workspace.left_dock,
                            DockPosition::Bottom => &workspace.bottom_dock,
                            DockPosition::Right => &workspace.right_dock,
                        }
                        .clone()
                    }) else {
                        return;
                    };

                    let was_visible = this.is_open()
                        && this.visible_panel().is_some_and(|active_panel| {
                            active_panel.panel_id() == Entity::entity_id(&panel)
                        });

                    this.remove_panel(&panel, window, cx);

                    new_dock.update(cx, |new_dock, cx| {
                        new_dock.remove_panel(&panel, window, cx);
                    });

                    new_dock.update(cx, |new_dock, cx| {
                        let index =
                            new_dock.add_panel(panel.clone(), workspace.clone(), window, cx);
                        if was_visible {
                            new_dock.set_open(true, window, cx);
                            new_dock.activate_panel(index, window, cx);
                        }
                    });

                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.serialize_workspace(window, cx);
                        })
                        .ok();
                }
            }),
            cx.subscribe_in(
                &panel,
                window,
                move |this, panel, event, window, cx| match event {
                    PanelEvent::ZoomIn => {
                        this.set_panel_zoomed(&panel.to_any(), true, window, cx);
                        if !PanelHandle::panel_focus_handle(panel, cx).contains_focused(window, cx)
                        {
                            window.focus(&panel.focus_handle(cx), cx);
                        }
                        workspace
                            .update(cx, |workspace, cx| {
                                workspace.zoomed = Some(panel.downgrade().into());
                                workspace.zoomed_position =
                                    Some(panel.read(cx).position(window, cx));
                                cx.emit(Event::ZoomChanged);
                            })
                            .ok();
                    }
                    PanelEvent::ZoomOut => {
                        this.set_panel_zoomed(&panel.to_any(), false, window, cx);
                        workspace
                            .update(cx, |workspace, cx| {
                                if workspace.zoomed_position == Some(this.position) {
                                    workspace.zoomed = None;
                                    workspace.zoomed_position = None;
                                    cx.emit(Event::ZoomChanged);
                                }
                                cx.notify();
                            })
                            .ok();
                    }
                    PanelEvent::Activate => {
                        if let Some(ix) = this
                            .panel_entries
                            .iter()
                            .position(|entry| entry.panel.panel_id() == Entity::entity_id(panel))
                        {
                            this.set_open(true, window, cx);
                            this.activate_panel(ix, window, cx);
                            window.focus(&panel.read(cx).focus_handle(cx), cx);
                        }
                    }
                    PanelEvent::Close => {
                        let panel_had_focus =
                            PanelHandle::panel_focus_handle(panel, cx).contains_focused(window, cx);
                        if this
                            .visible_panel()
                            .is_some_and(|p| p.panel_id() == Entity::entity_id(panel))
                        {
                            this.set_open(false, window, cx);
                            if panel_had_focus {
                                workspace
                                    .update(cx, |workspace, cx| {
                                        workspace.focus_primary_surface(window, cx);
                                    })
                                    .ok();
                            }
                        }
                    }
                    PanelEvent::NavigationUpdated => {
                        cx.notify();
                    }
                },
            ),
        ];

        let index = match self
            .panel_entries
            .binary_search_by_key(&panel.read(cx).activation_priority(), |entry| {
                entry.panel.activation_priority(cx)
            }) {
            Ok(ix) => {
                if cfg!(debug_assertions) {
                    panic!(
                        "Panels `{}` and `{}` have the same activation priority. Each panel must have a unique priority so the dock button order is deterministic.",
                        T::panel_key(),
                        self.panel_entries[ix].panel.panel_key()
                    );
                }
                ix
            }
            Err(ix) => ix,
        };
        if let Some(active_index) = self.active_panel_index.as_mut()
            && *active_index >= index
        {
            *active_index += 1;
        }
        self.panel_entries.insert(
            index,
            PanelEntry {
                panel: Arc::new(panel.clone()),
                _subscriptions: subscriptions,
            },
        );

        self.restore_state(window, cx);

        if panel.read(cx).starts_open(window, cx) {
            self.activate_panel(index, window, cx);
            self.set_open(true, window, cx);
        }

        cx.notify();
        index
    }

    pub fn restore_state(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(serialized) = self.serialized_dock.clone() {
            let mut activated_panel = false;
            if let Some(active_panel) = serialized.active_panel.filter(|_| serialized.visible)
                && let Some(idx) = self.panel_index_for_persistent_name(active_panel.as_str(), cx)
            {
                // Activate the panel directly without querying visible_content_size,
                // which would try to read the window root entity (MultiWorkspace) and
                // panic if we're already inside a window.update closure.
                if let Some(previously_active) =
                    self.active_panel_entry().map(|entry| entry.panel.clone())
                {
                    previously_active.set_active(false, window, cx);
                }
                self.active_panel_index = Some(idx);
                if let Some(entry) = self.panel_entries.get(idx) {
                    entry.panel.set_active(true, window, cx);
                }
                activated_panel = true;
            }

            if serialized.zoom
                && let Some(panel) = self.active_panel()
            {
                panel.set_zoomed(true, window, cx)
            }

            // Only open the dock if we actually activated a panel
            if activated_panel {
                self.set_open(serialized.visible, window, cx);
            }
            return true;
        }
        false
    }

    pub fn remove_panel<T: Panel>(
        &mut self,
        panel: &Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(panel_ix) = self
            .panel_entries
            .iter()
            .position(|entry| entry.panel.panel_id() == Entity::entity_id(panel))
        {
            if let Some(active_panel_index) = self.active_panel_index.as_mut() {
                match panel_ix.cmp(active_panel_index) {
                    std::cmp::Ordering::Less => {
                        *active_panel_index -= 1;
                    }
                    std::cmp::Ordering::Equal => {
                        self.active_panel_index = None;
                        self.set_open(false, window, cx);
                    }
                    std::cmp::Ordering::Greater => {}
                }
            }

            self.panel_entries.remove(panel_ix);
            cx.notify();

            true
        } else {
            false
        }
    }

    pub fn panels_len(&self) -> usize {
        self.panel_entries.len()
    }

    pub fn activate_panel(&mut self, panel_ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if Some(panel_ix) != self.active_panel_index {
            let size_to_preserve = self.visible_content_size(window, cx);
            let previously_active_panel =
                self.active_panel_entry().map(|entry| entry.panel.clone());
            let next_panel = self
                .panel_entries
                .get(panel_ix)
                .map(|entry| entry.panel.clone());

            if let Some(active_panel) = previously_active_panel {
                active_panel.set_active(false, window, cx);
            }

            self.active_panel_index = Some(panel_ix);
            if let Some(next_panel) = next_panel {
                if let Some(size_to_preserve) = size_to_preserve {
                    next_panel.set_size(Some(size_to_preserve), window, cx);
                }
                next_panel.set_active(true, window, cx);
            }

            cx.notify();
        }
    }

    pub fn visible_panel(&self) -> Option<&Arc<dyn PanelHandle>> {
        let entry = self.visible_entry()?;
        Some(&entry.panel)
    }

    pub fn active_panel(&self) -> Option<&Arc<dyn PanelHandle>> {
        let panel_entry = self.active_panel_entry()?;
        Some(&panel_entry.panel)
    }

    fn visible_entry(&self) -> Option<&PanelEntry> {
        if self.is_open {
            self.active_panel_entry()
        } else {
            None
        }
    }

    pub fn has_visible_content(&self, _window: &Window, _cx: &App) -> bool {
        self.visible_panel().is_some()
    }

    pub fn visible_content_size(&self, window: &Window, cx: &App) -> Option<Pixels> {
        self.active_panel_size(window, cx)
    }

    pub fn zoomed_panel(&self, window: &Window, cx: &App) -> Option<Arc<dyn PanelHandle>> {
        let entry = self.visible_entry()?;
        if entry.panel.is_zoomed(window, cx) {
            Some(entry.panel.clone())
        } else {
            None
        }
    }

    pub fn panel_size(&self, panel: &dyn PanelHandle, window: &Window, cx: &App) -> Option<Pixels> {
        self.panel_entries
            .iter()
            .find(|entry| entry.panel.panel_id() == panel.panel_id())
            .map(|entry| entry.panel.size(window, cx))
    }

    pub fn active_panel_size(&self, window: &Window, cx: &App) -> Option<Pixels> {
        if self.is_open {
            self.active_panel_entry()
                .map(|entry| entry.panel.size(window, cx))
        } else {
            None
        }
    }

    pub fn resize_active_panel(
        &mut self,
        size: Option<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.active_panel_entry() {
            let size = size.map(|size| size.max(RESIZE_HANDLE_SIZE).round());

            entry.panel.set_size(size, window, cx);
            cx.notify();
        }
    }

    pub fn resize_all_panels(
        &mut self,
        size: Option<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for entry in &mut self.panel_entries {
            let size = size.map(|size| size.max(RESIZE_HANDLE_SIZE).round());
            entry.panel.set_size(size, window, cx);
        }
        cx.notify();
    }

    pub fn toggle_action(&self) -> Box<dyn Action> {
        match self.position {
            DockPosition::Left => crate::ToggleLeftDock.boxed_clone(),
            DockPosition::Bottom => crate::ToggleBottomDock.boxed_clone(),
            DockPosition::Right => crate::ToggleRightDock.boxed_clone(),
        }
    }

    fn dispatch_context() -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("Dock");

        dispatch_context
    }

    pub fn clamp_panel_size(&mut self, max_size: Pixels, window: &mut Window, cx: &mut App) {
        let max_size = (max_size - RESIZE_HANDLE_SIZE).abs();
        for panel in self.panel_entries.iter().map(|entry| &entry.panel) {
            if panel.size(window, cx) > max_size {
                panel.set_size(Some(max_size.max(RESIZE_HANDLE_SIZE)), window, cx);
            }
        }
    }

    fn render_native_sidebar_content(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
        dispatch_context: KeyContext,
    ) -> Div {
        let active_panel = self.active_panel_entry().map(|entry| entry.panel.to_any());
        let content = active_panel;

        div()
            .key_context(dispatch_context)
            .track_focus(&self.focus_handle(cx))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when_some(content, |this, panel| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .size_full()
                        .overflow_hidden()
                        .child(panel),
                )
            })
    }
}

impl Render for Dock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dispatch_context = Self::dispatch_context();

        if self.in_native_sidebar {
            return self.render_native_sidebar_content(window, cx, dispatch_context);
        }

        let content = self.visible_entry().map(|entry| entry.panel.to_any());

        if let Some(content) = content {
            let size = self.active_panel_size(window, cx).unwrap_or(px(300.));

            let position = self.position;
            let create_resize_handle = || {
                let handle = div()
                    .id("resize-handle")
                    .on_drag(DraggedDock(position), |dock, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| dock.clone())
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|dock, e: &MouseUpEvent, window, cx| {
                            if e.click_count == 2 {
                                dock.resize_active_panel(None, window, cx);
                                dock.workspace
                                    .update(cx, |workspace, cx| {
                                        workspace.serialize_workspace(window, cx);
                                    })
                                    .ok();
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .occlude();
                match self.position() {
                    DockPosition::Left => deferred(
                        handle
                            .absolute()
                            .right(-RESIZE_HANDLE_SIZE / 2.)
                            .top(px(0.))
                            .h_full()
                            .w(RESIZE_HANDLE_SIZE)
                            .cursor_col_resize(),
                    ),
                    DockPosition::Bottom => deferred(
                        handle
                            .absolute()
                            .top(-RESIZE_HANDLE_SIZE / 2.)
                            .left(px(0.))
                            .w_full()
                            .h(RESIZE_HANDLE_SIZE)
                            .cursor_row_resize(),
                    ),
                    DockPosition::Right => deferred(
                        handle
                            .absolute()
                            .top(px(0.))
                            .left(-RESIZE_HANDLE_SIZE / 2.)
                            .h_full()
                            .w(RESIZE_HANDLE_SIZE)
                            .cursor_col_resize(),
                    ),
                }
            };

            div()
                .key_context(dispatch_context)
                .track_focus(&self.focus_handle(cx))
                .flex()
                .map(|this| match self.position().axis() {
                    Axis::Horizontal => this.w(size).h_full().flex_row(),
                    Axis::Vertical => this.h(size).w_full().flex_col(),
                })
                .map(|this| {
                    let show_shell_background = !self.in_native_sidebar
                        || matches!(
                            cx.theme().window_background_appearance(),
                            WindowBackgroundAppearance::Opaque
                        );

                    this.map(|this| {
                        match active_component_radius(cx.theme().component_radius().panel) {
                            Some(_) => match self.position() {
                                DockPosition::Left => this
                                    .when(show_shell_background, |this| {
                                        this.bg(cx.theme().colors().surface_background)
                                    })
                                    .pl_2()
                                    .pb_2(),
                                DockPosition::Right => this
                                    .when(show_shell_background, |this| {
                                        this.bg(cx.theme().colors().surface_background)
                                    })
                                    .pr_2()
                                    .pb_2(),
                                DockPosition::Bottom => this
                                    .when(show_shell_background, |this| {
                                        this.bg(cx.theme().colors().surface_background)
                                    })
                                    .px_2()
                                    .pb_2(),
                            },
                            None => this
                                .when(show_shell_background, |this| {
                                    this.bg(cx.theme().colors().panel_background)
                                })
                                .border_color(cx.theme().colors().border)
                                .overflow_hidden()
                                .map(|this| match self.position() {
                                    DockPosition::Left => this.border_r_1(),
                                    DockPosition::Right => this.border_l_1(),
                                    DockPosition::Bottom => this.border_t_1(),
                                }),
                        }
                    })
                })
                .child(
                    div()
                        .map(|this| {
                            if active_component_radius(cx.theme().component_radius().panel)
                                .is_some()
                            {
                                this.size_full()
                            } else {
                                match self.position().axis() {
                                    Axis::Horizontal => this.min_w(size).h_full(),
                                    Axis::Vertical => this.min_h(size).w_full(),
                                }
                            }
                        })
                        .flex()
                        .flex_col()
                        .map(|this| {
                            let show_shell_background = !self.in_native_sidebar
                                || matches!(
                                    cx.theme().window_background_appearance(),
                                    WindowBackgroundAppearance::Opaque
                                );

                            this.when_some(
                                active_component_radius(cx.theme().component_radius().panel),
                                |this, radius| {
                                    this.when(show_shell_background, |this| {
                                        this.bg(cx.theme().colors().panel_background)
                                    })
                                    .border_1()
                                    .border_color(cx.theme().colors().border)
                                    .rounded(radius)
                                    .overflow_hidden()
                                },
                            )
                        })
                        .when_some(self.dock_button_bar.clone(), |this, dock_button_bar| {
                            this.child(dock_button_bar)
                        })
                        .child(div().flex().flex_1().overflow_hidden().child(
                            content.cached(StyleRefinement::default().v_flex().size_full()),
                        )),
                )
                .when(self.resizable(cx), |this| {
                    this.child(create_resize_handle())
                })
        } else {
            div()
                .key_context(dispatch_context)
                .track_focus(&self.focus_handle(cx))
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod test {
    use super::*;
    use gpui::{App, Context, Window, actions, div};

    pub struct TestPanel {
        pub position: DockPosition,
        pub zoomed: bool,
        pub active: bool,
        pub focus_handle: FocusHandle,
        pub size: Pixels,
        pub activation_priority: u32,
    }
    actions!(test_only, [ToggleTestPanel]);

    impl EventEmitter<PanelEvent> for TestPanel {}

    impl TestPanel {
        pub fn new(position: DockPosition, activation_priority: u32, cx: &mut App) -> Self {
            Self {
                position,
                zoomed: false,
                active: false,
                focus_handle: cx.focus_handle(),
                size: px(300.),
                activation_priority,
            }
        }
    }

    impl Render for TestPanel {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().id("test").track_focus(&self.focus_handle(cx))
        }
    }

    impl Panel for TestPanel {
        fn persistent_name() -> &'static str {
            "TestPanel"
        }

        fn panel_key() -> &'static str {
            "TestPanel"
        }

        fn position(&self, _window: &Window, _: &App) -> super::DockPosition {
            self.position
        }

        fn position_is_valid(&self, _: super::DockPosition) -> bool {
            true
        }

        fn set_position(&mut self, position: DockPosition, _: &mut Window, cx: &mut Context<Self>) {
            self.position = position;
            cx.update_global::<SettingsStore, _>(|_, _| {});
        }

        fn size(&self, _window: &Window, _: &App) -> Pixels {
            self.size
        }

        fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, _: &mut Context<Self>) {
            self.size = size.unwrap_or(px(300.));
        }

        fn icon(&self, _window: &Window, _: &App) -> Option<ui::IconName> {
            None
        }

        fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
            None
        }

        fn toggle_action(&self) -> Box<dyn Action> {
            ToggleTestPanel.boxed_clone()
        }

        fn is_zoomed(&self, _window: &Window, _: &App) -> bool {
            self.zoomed
        }

        fn set_zoomed(&mut self, zoomed: bool, _window: &mut Window, _cx: &mut Context<Self>) {
            self.zoomed = zoomed;
        }

        fn set_active(&mut self, active: bool, _window: &mut Window, _cx: &mut Context<Self>) {
            self.active = active;
        }

        fn activation_priority(&self) -> u32 {
            self.activation_priority
        }
    }

    impl Focusable for TestPanel {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }
}
