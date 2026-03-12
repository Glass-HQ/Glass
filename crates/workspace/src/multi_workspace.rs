use anyhow::Result;
#[cfg(target_os = "macos")]
use gpui::native_sidebar;
use gpui::{
    App, Context, Entity, EntityId, EventEmitter, Focusable, ManagedView, Pixels, Render,
    Subscription, Task, Tiling, Window, WindowBackgroundAppearance, WindowId, actions, px,
};
use project::Project;
use std::future::Future;
use std::path::PathBuf;
use theme::ActiveTheme;
use ui::prelude::*;
use util::ResultExt;
use workspace_modes::{ModeId, ModeViewRegistry, RegisteredModeView};

pub const SIDEBAR_RESIZE_HANDLE_SIZE: Pixels = px(6.0);

use crate::{
    CloseIntent, CloseWindow, DockPosition, Event as WorkspaceEvent, Item, ModalView, Panel, Toast,
    UnifiedSidebar, Workspace, WorkspaceId, client_side_decorations, notifications::NotificationId,
    persistence::model::MultiWorkspaceId,
};

actions!(
    multi_workspace,
    [
        /// Creates a new workspace within the current window.
        NewWorkspaceInWindow,
        /// Switches to the next workspace within the current window.
        NextWorkspaceInWindow,
        /// Switches to the previous workspace within the current window.
        PreviousWorkspaceInWindow,
        /// Toggles the workspace switcher sidebar.
        ToggleWorkspaceSidebar,
        /// Moves focus to or from the workspace sidebar without closing it.
        FocusWorkspaceSidebar,
    ]
);

pub enum MultiWorkspaceEvent {
    ActiveWorkspaceChanged,
    WorkspaceAdded(Entity<Workspace>),
    WorkspaceRemoved(EntityId),
}

#[derive(Clone)]
pub struct DraggedSidebar;

impl Render for DraggedSidebar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

pub struct MultiWorkspace {
    window_id: WindowId,
    workspaces: Vec<Entity<Workspace>>,
    database_id: Option<MultiWorkspaceId>,
    active_workspace_index: usize,
    #[cfg(target_os = "macos")]
    unified_sidebar: Entity<UnifiedSidebar>,
    sidebar_open: bool,
    sidebar_has_notifications: bool,
    pending_removal_tasks: Vec<Task<()>>,
    _serialize_task: Option<Task<()>>,
    _create_task: Option<Task<()>>,
    shared_mode_views: collections::HashMap<ModeId, RegisteredModeView>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<MultiWorkspaceEvent> for MultiWorkspace {}

pub fn multi_workspace_enabled(_cx: &App) -> bool {
    true
}

impl MultiWorkspace {
    pub fn new(workspace: Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shared_mode_views = Self::shared_mode_views(cx);
        for (mode_id, mode_view) in &shared_mode_views {
            workspace.update(cx, |workspace, cx| {
                workspace.set_shared_mode_view(*mode_id, mode_view.clone(), cx);
            });
        }

        let release_subscription = cx.on_release(|this: &mut MultiWorkspace, _cx| {
            if let Some(task) = this._serialize_task.take() {
                task.detach();
            }
            if let Some(task) = this._create_task.take() {
                task.detach();
            }
            for task in std::mem::take(&mut this.pending_removal_tasks) {
                task.detach();
            }
        });
        let quit_subscription = cx.on_app_quit(Self::app_will_quit);
        Self::subscribe_to_workspace(&workspace, cx);
        #[cfg(target_os = "macos")]
        let unified_sidebar = {
            let left_dock = workspace.read(cx).left_dock().clone();
            cx.new(|_cx| UnifiedSidebar::new(left_dock))
        };
        Self {
            window_id: window.window_handle().window_id(),
            workspaces: vec![workspace],
            database_id: None,
            active_workspace_index: 0,
            #[cfg(target_os = "macos")]
            unified_sidebar,
            sidebar_open: false,
            sidebar_has_notifications: false,
            pending_removal_tasks: Vec::new(),
            _serialize_task: None,
            _create_task: None,
            shared_mode_views,
            _subscriptions: vec![release_subscription, quit_subscription],
        }
    }

    fn shared_mode_views(cx: &mut App) -> collections::HashMap<ModeId, RegisteredModeView> {
        let mut views = collections::HashMap::default();

        if let Some(mode_view) = Self::create_shared_mode_view(ModeId::BROWSER, cx) {
            views.insert(ModeId::BROWSER, mode_view);
        }

        views
    }

    fn create_shared_mode_view(mode_id: ModeId, cx: &mut App) -> Option<RegisteredModeView> {
        if let Some(factory) = ModeViewRegistry::try_global(cx)
            .and_then(|registry| registry.factory(mode_id))
            .cloned()
        {
            return Some(factory(cx));
        }

        ModeViewRegistry::try_global(cx)
            .and_then(|registry| registry.get(mode_id))
            .cloned()
    }

    #[cfg(target_os = "macos")]
    pub fn unified_sidebar(&self) -> &Entity<UnifiedSidebar> {
        &self.unified_sidebar
    }

    pub fn sidebar_has_notifications(&self, cx: &App) -> bool {
        self.sidebar_has_notifications && multi_workspace_enabled(cx)
    }

    pub fn is_sidebar_open(&self) -> bool {
        self.sidebar_open
    }

    pub fn set_sidebar_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.sidebar_open == open {
            return;
        }

        self.sidebar_open = open;
        cx.notify();
    }

    pub fn set_sidebar_has_notifications(
        &mut self,
        has_notifications: bool,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_has_notifications == has_notifications {
            return;
        }

        self.sidebar_has_notifications = has_notifications;
        cx.notify();
    }

    pub fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let workspaces = this.update(cx, |multi_workspace, _cx| {
                multi_workspace.workspaces().to_vec()
            })?;

            for workspace in workspaces {
                let should_continue = workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.prepare_to_close(CloseIntent::CloseWindow, window, cx)
                    })?
                    .await?;
                if !should_continue {
                    return anyhow::Ok(());
                }
            }

            cx.update(|window, _cx| {
                window.remove_window();
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn subscribe_to_workspace(workspace: &Entity<Workspace>, cx: &mut Context<Self>) {
        cx.subscribe(workspace, |this, workspace, event, cx| {
            if let WorkspaceEvent::Activate = event {
                this.activate(workspace, cx);
            }
        })
        .detach();
    }

    /// Sync the shared unified sidebar to point at the active workspace's left dock,
    /// mode, and hosted sidebar view. Width is NOT synced because the NSSplitView manages
    /// its own divider position independently.
    #[cfg(target_os = "macos")]
    fn sync_unified_sidebar(&self, cx: &mut App) {
        let active_ws = self.workspace().clone();
        let (left_dock, mode, sidebar_view) = {
            let workspace = active_ws.read(cx);
            let mode = workspace.active_mode_id();
            let sidebar_view = workspace
                .unified_sidebar
                .read(cx)
                .mode_sidebar_view(mode)
                .cloned();
            (workspace.left_dock().clone(), mode, sidebar_view)
        };
        self.unified_sidebar.update(cx, |sidebar, cx| {
            sidebar.set_left_dock(left_dock, cx);
            sidebar.set_mode(mode, cx);
            if let Some(view) = sidebar_view {
                sidebar.set_mode_sidebar_view(mode, view, cx);
            } else {
                sidebar.clear_mode_sidebar_view(mode, cx);
            }
        });
    }

    pub fn workspace(&self) -> &Entity<Workspace> {
        &self.workspaces[self.active_workspace_index]
    }

    pub fn workspaces(&self) -> &[Entity<Workspace>] {
        &self.workspaces
    }

    pub fn active_workspace_index(&self) -> usize {
        self.active_workspace_index
    }

    pub fn activate(&mut self, workspace: Entity<Workspace>, cx: &mut Context<Self>) {
        if !multi_workspace_enabled(cx) {
            self.workspaces[0] = workspace;
            self.active_workspace_index = 0;
            cx.emit(MultiWorkspaceEvent::ActiveWorkspaceChanged);
            cx.notify();
            return;
        }
        let old_index = self.active_workspace_index;
        let new_index = self.set_active_workspace(workspace, cx);
        if old_index != new_index {
            self.serialize(cx);
        }
    }

    fn set_active_workspace(
        &mut self,
        workspace: Entity<Workspace>,
        cx: &mut Context<Self>,
    ) -> usize {
        let index = self.add_workspace(workspace.clone(), cx);
        let changed = self.active_workspace_index != index;
        self.active_workspace_index = index;
        if changed {
            cx.emit(MultiWorkspaceEvent::ActiveWorkspaceChanged);
        }
        // Force the workspace to re-render when it becomes active.
        workspace.update(cx, |_, cx| cx.notify());
        cx.notify();
        index
    }

    /// Adds a workspace to this window without changing which workspace is active.
    /// Returns the index of the workspace (existing or newly inserted).
    pub fn add_workspace(&mut self, workspace: Entity<Workspace>, cx: &mut Context<Self>) -> usize {
        if let Some(index) = self.workspaces.iter().position(|w| *w == workspace) {
            index
        } else {
            for (mode_id, mode_view) in &self.shared_mode_views {
                workspace.update(cx, |workspace, cx| {
                    workspace.set_shared_mode_view(*mode_id, mode_view.clone(), cx);
                });
            }
            Self::subscribe_to_workspace(&workspace, cx);
            self.workspaces.push(workspace.clone());
            cx.emit(MultiWorkspaceEvent::WorkspaceAdded(workspace));
            cx.notify();
            self.workspaces.len() - 1
        }
    }

    pub fn activate_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        debug_assert!(
            index < self.workspaces.len(),
            "workspace index out of bounds"
        );
        let changed = self.active_workspace_index != index;
        self.active_workspace_index = index;
        // Force the workspace to re-render and push its window title/toolbar,
        // which may be stale if this workspace was previously inactive.
        self.workspace().update(cx, |workspace, cx| {
            workspace.invalidate_window_caches(window, cx);
            cx.notify();
        });
        self.sync_unified_sidebar(cx);
        self.serialize(cx);
        self.focus_active_workspace(window, cx);
        if changed {
            cx.emit(MultiWorkspaceEvent::ActiveWorkspaceChanged);
        }
        cx.notify();
    }

    pub fn activate_next_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspaces.len() > 1 {
            let next_index = (self.active_workspace_index + 1) % self.workspaces.len();
            self.activate_index(next_index, window, cx);
        }
    }

    pub fn activate_previous_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspaces.len() > 1 {
            let prev_index = if self.active_workspace_index == 0 {
                self.workspaces.len() - 1
            } else {
                self.active_workspace_index - 1
            };
            self.activate_index(prev_index, window, cx);
        }
    }

    fn serialize(&mut self, cx: &mut App) {
        let window_id = self.window_id;
        let state = crate::persistence::model::MultiWorkspaceState {
            active_workspace_id: self.workspace().read(cx).database_id(),
        };
        self._serialize_task = Some(cx.background_spawn(async move {
            crate::persistence::write_multi_workspace_state(window_id, state).await;
        }));
    }

    /// Returns the in-flight serialization task (if any) so the caller can
    /// await it. Used by the quit handler to ensure pending DB writes
    /// complete before the process exits.
    pub fn flush_serialization(&mut self) -> Task<()> {
        self._serialize_task.take().unwrap_or(Task::ready(()))
    }

    fn app_will_quit(&mut self, _cx: &mut Context<Self>) -> impl Future<Output = ()> + use<> {
        let mut tasks: Vec<Task<()>> = Vec::new();
        if let Some(task) = self._serialize_task.take() {
            tasks.push(task);
        }
        if let Some(task) = self._create_task.take() {
            tasks.push(task);
        }
        tasks.extend(std::mem::take(&mut self.pending_removal_tasks));

        async move {
            futures::future::join_all(tasks).await;
        }
    }

    pub fn focus_active_workspace(&self, window: &mut Window, cx: &mut App) {
        // If a dock panel is zoomed, focus it instead of the center pane.
        // Otherwise, focusing the center pane triggers dismiss_zoomed_items_to_reveal
        // which closes the zoomed dock.
        let focus_handle = {
            let workspace = self.workspace().read(cx);
            let mut target = None;
            for dock in workspace.all_docks() {
                let dock = dock.read(cx);
                if dock.is_open() {
                    if let Some(panel) = dock.active_panel() {
                        if panel.is_zoomed(window, cx) {
                            target = Some(panel.panel_focus_handle(cx));
                            break;
                        }
                    }
                }
            }
            target.unwrap_or_else(|| {
                let pane = workspace.active_pane().clone();
                pane.read(cx).focus_handle(cx)
            })
        };
        window.focus(&focus_handle, cx);
    }

    pub fn panel<T: Panel>(&self, cx: &App) -> Option<Entity<T>> {
        self.workspace().read(cx).panel::<T>(cx)
    }

    pub fn active_modal<V: ManagedView + 'static>(&self, cx: &App) -> Option<Entity<V>> {
        self.workspace().read(cx).active_modal::<V>(cx)
    }

    pub fn add_panel<T: Panel>(
        &mut self,
        panel: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace().update(cx, |workspace, cx| {
            workspace.add_panel(panel, window, cx);
        });
    }

    pub fn focus_panel<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<T>> {
        self.workspace()
            .update(cx, |workspace, cx| workspace.focus_panel::<T>(window, cx))
    }

    // used in a test
    pub fn toggle_modal<V: ModalView, B>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: B,
    ) where
        B: FnOnce(&mut Window, &mut gpui::Context<V>) -> V,
    {
        self.workspace().update(cx, |workspace, cx| {
            workspace.toggle_modal(window, cx, build);
        });
    }

    pub fn toggle_dock(
        &mut self,
        dock_side: DockPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace().update(cx, |workspace, cx| {
            workspace.toggle_dock(dock_side, window, cx);
        });
    }

    pub fn active_item_as<I: 'static>(&self, cx: &App) -> Option<Entity<I>> {
        self.workspace().read(cx).active_item_as::<I>(cx)
    }

    pub fn items_of_type<'a, T: Item>(
        &'a self,
        cx: &'a App,
    ) -> impl 'a + Iterator<Item = Entity<T>> {
        self.workspace().read(cx).items_of_type::<T>(cx)
    }

    pub fn database_id(&self) -> Option<MultiWorkspaceId> {
        self.database_id
    }

    pub fn set_database_id(&mut self, id: Option<MultiWorkspaceId>) {
        self.database_id = id;
    }

    pub fn active_workspace_database_id(&self, cx: &App) -> Option<WorkspaceId> {
        self.workspace().read(cx).database_id()
    }

    pub fn take_pending_removal_tasks(&mut self) -> Vec<Task<()>> {
        let mut tasks: Vec<Task<()>> = std::mem::take(&mut self.pending_removal_tasks)
            .into_iter()
            .filter(|task| !task.is_ready())
            .collect();
        if let Some(task) = self._create_task.take() {
            if !task.is_ready() {
                tasks.push(task);
            }
        }
        tasks
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_random_database_id(&mut self, cx: &mut Context<Self>) {
        self.workspace().update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(|cx| Workspace::test_new(project, window, cx));
        Self::new(workspace, window, cx)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_add_workspace(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Workspace> {
        let workspace = cx.new(|cx| Workspace::test_new(project, window, cx));
        self.activate(workspace.clone(), cx);
        workspace
    }

    pub fn create_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app_state = self.workspace().read(cx).app_state().clone();
        let project = Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        );
        let new_workspace = cx.new(|cx| Workspace::new(None, project, app_state, window, cx));
        self.set_active_workspace(new_workspace.clone(), cx);
        self.focus_active_workspace(window, cx);

        let weak_workspace = new_workspace.downgrade();
        self._create_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = crate::persistence::DB.next_id().await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(workspace_id) => {
                    if let Some(workspace) = weak_workspace.upgrade() {
                        let session_id = workspace.read(cx).session_id();
                        let window_id = window.window_handle().window_id().as_u64();
                        workspace.update(cx, |workspace, _cx| {
                            workspace.set_database_id(workspace_id);
                        });
                        cx.background_spawn(async move {
                            crate::persistence::DB
                                .set_session_binding(workspace_id, session_id, Some(window_id))
                                .await
                                .log_err();
                        })
                        .detach();
                    } else {
                        cx.background_spawn(async move {
                            crate::persistence::DB
                                .delete_workspace_by_id(workspace_id)
                                .await
                                .log_err();
                        })
                        .detach();
                    }
                    this.serialize(cx);
                }
                Err(error) => {
                    log::error!("Failed to create workspace: {error:#}");
                    if let Some(index) = weak_workspace
                        .upgrade()
                        .and_then(|w| this.workspaces.iter().position(|ws| *ws == w))
                    {
                        this.remove_workspace(index, window, cx);
                    }
                    this.workspace().update(cx, |workspace, cx| {
                        let id = NotificationId::unique::<MultiWorkspace>();
                        workspace.show_toast(
                            Toast::new(id, format!("Failed to create workspace: {error}")),
                            cx,
                        );
                    });
                }
            })
            .log_err();
        }));
    }

    pub fn remove_workspace(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspaces.len() <= 1 || index >= self.workspaces.len() {
            return;
        }

        let removed_workspace = self.workspaces.remove(index);

        if self.active_workspace_index >= self.workspaces.len() {
            self.active_workspace_index = self.workspaces.len() - 1;
        } else if self.active_workspace_index > index {
            self.active_workspace_index -= 1;
        }

        if let Some(workspace_id) = removed_workspace.read(cx).database_id() {
            self.pending_removal_tasks.retain(|task| !task.is_ready());
            self.pending_removal_tasks
                .push(cx.background_spawn(async move {
                    crate::persistence::DB
                        .delete_workspace_by_id(workspace_id)
                        .await
                        .log_err();
                }));
        }

        self.serialize(cx);
        self.focus_active_workspace(window, cx);
        cx.emit(MultiWorkspaceEvent::WorkspaceRemoved(
            removed_workspace.entity_id(),
        ));
        cx.emit(MultiWorkspaceEvent::ActiveWorkspaceChanged);
        cx.notify();
    }

    pub fn open_project(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let workspace = self.workspace().clone();

        workspace.update(cx, |workspace, cx| {
            workspace.open_workspace_for_paths(true, paths, window, cx)
        })
    }
}

impl Render for MultiWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(target_os = "macos")]
        self.sync_unified_sidebar(cx);

        let ui_font = theme::setup_ui_font(window, cx);
        let text_color = cx.theme().colors().text;

        let workspace = self.workspace().clone();
        let workspace_key_context = workspace.update(cx, |workspace, cx| workspace.key_context(cx));
        let root = workspace.update(cx, |workspace, cx| workspace.actions(h_flex(), window, cx));

        client_side_decorations(
            root.key_context(workspace_key_context)
                .relative()
                .size_full()
                .font(ui_font)
                .text_color(text_color)
                .on_action(cx.listener(Self::close_window))
                .on_action(
                    cx.listener(|this: &mut Self, _: &NewWorkspaceInWindow, window, cx| {
                        this.create_workspace(window, cx);
                    }),
                )
                .on_action(
                    cx.listener(|this: &mut Self, _: &NextWorkspaceInWindow, window, cx| {
                        this.activate_next_workspace(window, cx);
                    }),
                )
                .on_action(cx.listener(
                    |this: &mut Self, _: &PreviousWorkspaceInWindow, window, cx| {
                        this.activate_previous_workspace(window, cx);
                    },
                ))
                .child({
                    let workspace_content = div()
                        .flex()
                        .flex_1()
                        .size_full()
                        .overflow_hidden()
                        .child(self.workspace().clone());

                    #[cfg(target_os = "macos")]
                    let workspace_content = {
                        let ws = self.workspace().read(cx);
                        let sidebar_collapsed = ws.left_dock().read(cx).visible_panel().is_none();
                        let sidebar_width = self.unified_sidebar.read(cx).width();
                        let sidebar_titlebar_fill = match cx.theme().window_background_appearance()
                        {
                            WindowBackgroundAppearance::Opaque => {
                                Some(cx.theme().colors().panel_background)
                            }
                            _ => None,
                        };
                        div()
                            .size_full()
                            .flex()
                            .flex_row()
                            .child(
                                native_sidebar("workspace-unified-sidebar", &[""; 0])
                                    .sidebar_view(self.unified_sidebar.clone())
                                    .sidebar_width(sidebar_width)
                                    .min_sidebar_width(160.0)
                                    .max_sidebar_width(480.0)
                                    .manage_window_chrome(false)
                                    .manage_toolbar(false)
                                    .collapsed(sidebar_collapsed)
                                    .sidebar_background_color(sidebar_titlebar_fill)
                                    .size_full(),
                            )
                            .child(workspace_content)
                    };

                    workspace_content
                })
                .child(self.workspace().read(cx).modal_layer.clone()),
            window,
            cx,
            Tiling::default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use feature_flags::FeatureFlagAppExt;
    use fs::FakeFs;
    use gpui::{Context, FocusHandle, Focusable, Render, TestAppContext, div};
    use project::DisableAiSettings;
    use settings::SettingsStore;
    use std::sync::Arc;
    use workspace_modes::RegisteredModeView;

    struct TestBrowserModeView {
        focus_handle: FocusHandle,
    }

    impl TestBrowserModeView {
        fn new(cx: &mut Context<Self>) -> Self {
            Self {
                focus_handle: cx.focus_handle(),
            }
        }
    }

    impl Focusable for TestBrowserModeView {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Render for TestBrowserModeView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme::init(theme::LoadThemes::JustBase, cx);
            workspace_modes::init(cx);
            DisableAiSettings::register(cx);
            cx.update_flags(false, vec!["agent-v2".into()]);
        });
    }

    #[gpui::test]
    async fn test_browser_mode_view_is_shared_across_workspaces(cx: &mut TestAppContext) {
        init_test(cx);

        cx.update(|cx| {
            ModeViewRegistry::global_mut(cx).register_factory(
                ModeId::BROWSER,
                Arc::new(|cx| {
                    let browser_view: Entity<TestBrowserModeView> =
                        cx.new(|cx| TestBrowserModeView::new(cx));
                    let focus_handle = browser_view.focus_handle(cx);

                    RegisteredModeView {
                        view: browser_view.into(),
                        focus_handle,
                        titlebar_center_view: None,
                        sidebar_view: None,
                        sidebar_visibility: None,
                        on_deactivate: None,
                    }
                }),
            );
        });

        let fs = FakeFs::new(cx.executor());
        let project_a = Project::test(fs.clone(), [], cx).await;
        let project_b = Project::test(fs, [], cx).await;

        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));

        multi_workspace.update_in(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project_b, window, cx);
        });

        multi_workspace.read_with(cx, |multi_workspace, cx| {
            let first_browser_view = multi_workspace.workspaces()[0]
                .read(cx)
                .get_mode_view(ModeId::BROWSER)
                .and_then(|view| view.downcast::<TestBrowserModeView>().ok())
                .expect("first workspace should resolve the shared browser view");
            let second_browser_view = multi_workspace.workspaces()[1]
                .read(cx)
                .get_mode_view(ModeId::BROWSER)
                .and_then(|view| view.downcast::<TestBrowserModeView>().ok())
                .expect("second workspace should resolve the shared browser view");

            assert_eq!(
                first_browser_view.entity_id(),
                second_browser_view.entity_id(),
            );
        });
    }

}
