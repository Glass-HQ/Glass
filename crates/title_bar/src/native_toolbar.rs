use crate::title_bar_settings::TitleBarSettings;
use crate::update_version::UpdateVersion;

use auto_update;
use browser;
use client::{Client, UserStore};
use editor::Editor;
use gpui::{
    Action, App, AppContext as _, Context, Entity, IntoElement, NativePanel, NativePanelAnchor,
    NativePanelLevel, NativePanelMaterial, NativePanelStyle, NativePopoverClickableRow,
    NativePopoverContentItem, NativeToolbar, NativeToolbarButton, NativeToolbarDisplayMode,
    NativeToolbarItem, NativeToolbarLabel, NativeToolbarMenuButton, NativeToolbarMenuItem,
    NativeToolbarSearchEvent, NativeToolbarSearchField, NativeToolbarSegment,
    NativeToolbarSegmentedControl, NativeToolbarSizeMode, Render, SharedString, Subscription,
    WeakEntity, Window, px,
};
use image_viewer::ImageView;
use language::LineEnding;
use platform_title_bar::PlatformTitleBar;
use project::image_store::{ImageFormat, ImageMetadata};
use project::{Project, git_store::GitStoreEvent, trusted_worktrees::TrustedWorktrees};
use settings::Settings;
use std::sync::Arc;
use workspace::{
    MultiWorkspace, Pane, TitleBarItemViewHandle, ToggleWorktreeSecurity,
    Workspace, notifications::NotifyResultExt,
};
use workspace_modes::{ModeId, SwitchToBrowserMode, SwitchToEditorMode, SwitchToTerminalMode};
use zed_actions::OpenRemote;

const MAX_PROJECT_NAME_LENGTH: usize = 40;
const MAX_BRANCH_NAME_LENGTH: usize = 40;
const MAX_SHORT_SHA_LENGTH: usize = 8;

pub struct NativeToolbarController {
    platform_titlebar: Entity<PlatformTitleBar>,
    project: Entity<Project>,
    user_store: Entity<UserStore>,
    client: Arc<Client>,
    workspace: WeakEntity<Workspace>,
    update_version: Entity<UpdateVersion>,
    _subscriptions: Vec<Subscription>,
    right_items: Vec<Box<dyn TitleBarItemViewHandle>>,
    active_pane: Option<Entity<Pane>>,
    omnibox_text: String,
    is_user_typing: bool,
    omnibox_suggestions: Vec<browser::history::HistoryMatch>,
    last_toolbar_key: String,
    status_cursor: Option<String>,
    status_language: Option<String>,
    status_encoding: Option<String>,
    status_line_ending: Option<String>,
    status_toolchain: Option<String>,
    status_image_info: Option<String>,
    active_editor_subscription: Option<Subscription>,
    active_image_subscription: Option<Subscription>,
}

impl Render for NativeToolbarController {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_native_toolbar(window, cx);
        self.platform_titlebar.clone().into_any_element()
    }
}

impl NativeToolbarController {
    pub fn new(
        id: impl Into<gpui::ElementId>,
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project = workspace.project().clone();
        let git_store = project.read(cx).git_store().clone();
        let user_store = workspace.app_state().user_store.clone();
        let client = workspace.app_state().client.clone();

        let workspace_handle = workspace.weak_handle().upgrade().unwrap();
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.observe(&workspace_handle, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe_in(
            &workspace_handle,
            window,
            |this, workspace, event: &workspace::Event, window, cx| {
                if matches!(event, workspace::Event::ActiveItemChanged) {
                    this.set_active_pane(&workspace.read(cx).active_pane().clone(), window, cx);
                }
            },
        ));
        subscriptions.push(
            cx.subscribe(&project, |this, _, event: &project::Event, cx| {
                if let project::Event::BufferEdited = event {
                    this.clear_active_worktree_override(cx);
                    cx.notify();
                }
            }),
        );
        subscriptions.push(cx.observe_window_activation(window, Self::window_activation_changed));
        subscriptions.push(
            cx.subscribe(&git_store, move |this, _, event, cx| match event {
                GitStoreEvent::ActiveRepositoryChanged(_) => {
                    this.clear_active_worktree_override(cx);
                    cx.notify();
                }
                GitStoreEvent::RepositoryUpdated(_, _, true) => {
                    cx.notify();
                }
                _ => {}
            }),
        );
        subscriptions.push(cx.observe(&user_store, |_a, _, cx| cx.notify()));
        if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
            subscriptions.push(cx.subscribe(&trusted_worktrees, |_, _, _, cx| {
                cx.notify();
            }));
        }

        let update_version = cx.new(|cx| UpdateVersion::new(cx));
        let platform_titlebar = cx.new(|cx| PlatformTitleBar::new(id, cx));

        {
            let platform_titlebar = platform_titlebar.clone();
            let window_handle = window.window_handle();
            cx.spawn(async move |this: WeakEntity<NativeToolbarController>, cx| {
                let Some(multi_workspace_handle) = window_handle.downcast::<MultiWorkspace>()
                else {
                    return;
                };

                let _ = cx.update(|cx| {
                    let Ok(multi_workspace) = multi_workspace_handle.entity(cx) else {
                        return;
                    };

                    let is_open = multi_workspace.read(cx).is_sidebar_open();
                    let has_notifications = multi_workspace.read(cx).sidebar_has_notifications(cx);
                    platform_titlebar.update(cx, |titlebar: &mut PlatformTitleBar, cx| {
                        titlebar.set_workspace_sidebar_open(is_open, cx);
                        titlebar.set_sidebar_has_notifications(has_notifications, cx);
                    });

                    let platform_titlebar = platform_titlebar.clone();
                    let subscription = cx.observe(&multi_workspace, move |mw, cx| {
                        let is_open = mw.read(cx).is_sidebar_open();
                        let has_notifications = mw.read(cx).sidebar_has_notifications(cx);
                        platform_titlebar.update(cx, |titlebar: &mut PlatformTitleBar, cx| {
                            titlebar.set_workspace_sidebar_open(is_open, cx);
                            titlebar.set_sidebar_has_notifications(has_notifications, cx);
                        });
                    });

                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, _| {
                            this._subscriptions.push(subscription);
                        });
                    }
                });
            })
            .detach();
        }

        Self {
            platform_titlebar,
            workspace: workspace.weak_handle(),
            project,
            user_store,
            client,
            _subscriptions: subscriptions,
            update_version,
            right_items: Vec::new(),
            active_pane: None,
            omnibox_text: String::new(),
            is_user_typing: false,
            omnibox_suggestions: Vec::new(),
            last_toolbar_key: String::new(),
            status_cursor: None,
            status_language: None,
            status_encoding: None,
            status_line_ending: None,
            status_toolchain: None,
            status_image_info: None,
            active_editor_subscription: None,
            active_image_subscription: None,
        }
    }

    pub fn add_right_item<T>(
        &mut self,
        item: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        T: 'static + workspace::TitleBarItemView,
    {
        if let Some(active_pane) = &self.active_pane {
            let active_pane_item = active_pane.read(cx).active_item();
            item.update(cx, |item, cx| {
                item.set_active_pane_item(active_pane_item.as_deref(), window, cx);
            });
        }
        self.right_items.push(Box::new(item));
        cx.notify();
    }

    pub fn set_active_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_pane = Some(pane.clone());
        self._subscriptions
            .push(cx.observe_in(pane, window, |this, _, window, cx| {
                this.update_active_pane_item(window, cx);
            }));
        self.update_active_pane_item(window, cx);
    }

    fn update_active_pane_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_pane_item = self
            .active_pane
            .as_ref()
            .and_then(|pane| pane.read(cx).active_item());
        for item in &self.right_items {
            item.set_active_pane_item(active_pane_item.as_deref(), window, cx);
        }
        self.refresh_status_data(window, cx);
    }

    pub fn toggle_update_simulation(&mut self, cx: &mut Context<Self>) {
        self.update_version
            .update(cx, |banner, cx| banner.update_simulation(cx));
        cx.notify();
    }

    fn window_activation_changed(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.update_active_view_for_followers(_window, cx);
            })
            .ok();
    }

    fn effective_active_worktree(&self, cx: &App) -> Option<Entity<project::Worktree>> {
        let project = self.project.read(cx);

        if let Some(workspace) = self.workspace.upgrade() {
            if let Some(override_id) = workspace.read(cx).active_worktree_override() {
                if let Some(worktree) = project.worktree_for_id(override_id, cx) {
                    return Some(worktree);
                }
            }
        }

        if let Some(repo) = project.active_repository(cx) {
            let repo = repo.read(cx);
            let repo_path = &repo.work_directory_abs_path;

            for worktree in project.visible_worktrees(cx) {
                let worktree_path = worktree.read(cx).abs_path();
                if worktree_path == *repo_path || worktree_path.starts_with(repo_path.as_ref()) {
                    return Some(worktree);
                }
            }
        }

        project.visible_worktrees(cx).next()
    }

    fn get_repository_for_worktree(
        &self,
        worktree: &Entity<project::Worktree>,
        cx: &App,
    ) -> Option<Entity<project::git_store::Repository>> {
        let project = self.project.read(cx);
        let git_store = project.git_store().read(cx);
        let worktree_path = worktree.read(cx).abs_path();

        for repo in git_store.repositories().values() {
            let repo_path = &repo.read(cx).work_directory_abs_path;
            if worktree_path == *repo_path || worktree_path.starts_with(repo_path.as_ref()) {
                return Some(repo.clone());
            }
        }

        None
    }

    fn clear_active_worktree_override(&mut self, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.clear_active_worktree_override(cx);
            });
        }
        cx.notify();
    }

    // -- Native toolbar building --

    fn update_native_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_mode = self
            .workspace
            .upgrade()
            .map(|ws| ws.read(cx).active_mode_id())
            .unwrap_or(ModeId::BROWSER);

        let is_browser_mode = active_mode == ModeId::BROWSER;
        let is_new_tab_page = self.active_tab_is_new_tab_page(cx);
        let title_bar_settings = *TitleBarSettings::get_global(cx);

        self.sync_omnibox_url(cx);

        let project_name = self
            .effective_active_worktree(cx)
            .map(|wt| wt.read(cx).root_name().as_unix_str().to_string())
            .unwrap_or_default();

        let branch_name = self
            .effective_active_worktree(cx)
            .and_then(|wt| self.get_repository_for_worktree(&wt, cx))
            .and_then(|repo| {
                let repo = repo.read(cx);
                repo.branch.as_ref().map(|b| b.name().to_string())
            })
            .unwrap_or_default();

        let has_restricted = TrustedWorktrees::try_get_global(cx)
            .map(|tw| {
                tw.read(cx)
                    .has_restricted_worktrees(&self.project.read(cx).worktree_store(), cx)
            })
            .unwrap_or(false);

        let is_remote = self.project.read(cx).is_via_remote_server();

        let user = self.user_store.read(cx).current_user();
        let is_signed_in = user.is_some();
        let user_login = user
            .as_ref()
            .map(|u| u.github_login.as_ref().to_owned())
            .unwrap_or_default();
        let connection_status_key = {
            let status = self.client.status();
            let status = &*status.borrow();
            match status {
                client::Status::ConnectionError => "conn_error",
                client::Status::ConnectionLost => "conn_lost",
                client::Status::Reauthenticating => "reauth",
                client::Status::Reconnecting => "reconnecting",
                client::Status::ReconnectionError { .. } => "reconn_error",
                client::Status::UpgradeRequired => "upgrade",
                _ => "ok",
            }
        };
        let show_update = self.update_version.read(cx).show_update_in_menu_bar();

        // Show/update the suggestion panel on every render (independent of toolbar rebuild)
        // so that typing and search results don't cause the NSSearchField to be recreated.
        if is_browser_mode && self.is_user_typing && !self.omnibox_text.is_empty() {
            self.show_suggestion_panel(window);
        }

        // Omnibox text and suggestion count are excluded from the toolbar key so that
        // keystrokes and search-history results don't rebuild the NSSearchField (which
        // would steal focus). When the user isn't typing, include the URL so that
        // programmatic navigation (e.g. clicking a link) updates the search field text.
        let omnibox_key = if self.is_user_typing {
            String::new()
        } else {
            self.omnibox_text.clone()
        };

        let toolbar_key = format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
            active_mode.0,
            project_name,
            branch_name,
            omnibox_key,
            is_new_tab_page,
            has_restricted,
            is_remote,
            title_bar_settings.show_project_items,
            title_bar_settings.show_branch_name,
            user_login,
            connection_status_key,
            show_update,
            self.status_cursor,
            self.status_language,
            self.status_encoding,
            self.status_line_ending,
            self.status_toolchain,
            self.status_image_info,
        );

        if toolbar_key == self.last_toolbar_key {
            return;
        }
        self.last_toolbar_key = toolbar_key;

        let mut toolbar = NativeToolbar::new("glass.main.toolbar")
            .display_mode(NativeToolbarDisplayMode::IconOnly)
            .size_mode(NativeToolbarSizeMode::Regular)
            .shows_baseline_separator(false);

        toolbar = toolbar.item(self.build_mode_switcher_item(active_mode, cx));

        if let Some(restricted_mode) = self.build_restricted_mode_item(cx) {
            toolbar = toolbar.item(restricted_mode);
        }

        if !is_browser_mode {
            if title_bar_settings.show_project_items {
                if let Some(host_button) = self.build_project_host_item(cx) {
                    toolbar = toolbar.item(host_button);
                }
                if let Some(project_button) = self.build_project_button_item(cx) {
                    toolbar = toolbar.item(project_button);
                }
            }
            if title_bar_settings.show_branch_name {
                if let Some(branch_button) = self.build_branch_button_item(cx) {
                    toolbar = toolbar.item(branch_button);
                }
            }
        }

        toolbar = toolbar.item(NativeToolbarItem::FlexibleSpace);

        if is_browser_mode {
            if !is_new_tab_page {
                toolbar = toolbar.item(self.build_back_item());
                toolbar = toolbar.item(self.build_forward_item());
                toolbar = toolbar.item(self.build_reload_item());
                toolbar = toolbar.item(self.build_omnibox_item(cx));
            }
            toolbar = toolbar.item(NativeToolbarItem::FlexibleSpace);
        }

        if !is_browser_mode {
            if let Some(ref cursor) = self.status_cursor {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.cursor", cursor.clone())
                        .tool_tip("Go to Line/Column")
                        .icon("line.3.horizontal")
                        .on_click(|_event, window, cx| {
                            window
                                .dispatch_action(editor::actions::ToggleGoToLine.boxed_clone(), cx);
                        }),
                ));
            }
            if let Some(ref language) = self.status_language {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.language", language.clone())
                        .tool_tip("Select Language")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(language_selector::Toggle.boxed_clone(), cx);
                        }),
                ));
            }
            if let Some(ref toolchain) = self.status_toolchain {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.toolchain", toolchain.clone())
                        .tool_tip("Select Toolchain")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(toolchain_selector::Select.boxed_clone(), cx);
                        }),
                ));
            }
            if let Some(ref encoding) = self.status_encoding {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.encoding", encoding.clone())
                        .tool_tip("Select Encoding")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(encoding_selector::Toggle.boxed_clone(), cx);
                        }),
                ));
            }
            if let Some(ref line_ending) = self.status_line_ending {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.line_ending", line_ending.clone())
                        .tool_tip("Select Line Ending")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(line_ending_selector::Toggle.boxed_clone(), cx);
                        }),
                ));
            }
            if let Some(ref image_info) = self.status_image_info {
                toolbar = toolbar.item(NativeToolbarItem::Label(NativeToolbarLabel::new(
                    "glass.status.image_info",
                    image_info.clone(),
                )));
            }

            toolbar = toolbar.item(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.status.activity", "")
                    .tool_tip("View Logs")
                    .icon("arrow.triangle.2.circlepath")
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(workspace::OpenLog.boxed_clone(), cx);
                    }),
            ));

            toolbar = toolbar.item(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.status.lsp", "")
                    .tool_tip("Language Servers")
                    .icon("bolt")
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(
                            language_tools::lsp_button::ToggleMenu.boxed_clone(),
                            cx,
                        );
                    }),
            ));

            toolbar = toolbar.item(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.status.predictions", "")
                    .tool_tip("Edit Predictions")
                    .icon("sparkles")
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(edit_prediction_ui::ToggleMenu.boxed_clone(), cx);
                    }),
            ));
        }

        toolbar = toolbar.item(NativeToolbarItem::Space);

        if let Some(connection_item) = self.build_connection_status_item(cx) {
            toolbar = toolbar.item(connection_item);
        }

        if show_update {
            toolbar = toolbar.item(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.update", "Update Available")
                    .tool_tip("Restart to update")
                    .icon("arrow.down.circle")
                    .on_click(|_event, _window, cx| {
                        workspace::reload(cx);
                    }),
            ));
        }

        if !is_signed_in && title_bar_settings.show_sign_in {
            toolbar = toolbar.item(self.build_sign_in_item(cx));
        }

        if title_bar_settings.show_user_menu {
            toolbar = toolbar.item(self.build_user_menu_item(&user, cx));
        }

        window.set_native_toolbar(Some(toolbar));
    }

    fn build_mode_switcher_item(
        &self,
        active_mode: ModeId,
        _cx: &Context<Self>,
    ) -> NativeToolbarItem {
        let (label, icon) = match active_mode {
            ModeId::BROWSER => ("Browser", "globe"),
            ModeId::EDITOR => ("Editor", "doc.text"),
            ModeId::TERMINAL => ("Terminal", "terminal"),
            _ => ("Browser", "globe"),
        };

        let menu_items = vec![
            NativeToolbarMenuItem::action("Browser").icon("globe"),
            NativeToolbarMenuItem::action("Editor").icon("doc.text"),
            NativeToolbarMenuItem::action("Terminal").icon("terminal"),
        ];

        let workspace = self.workspace.clone();
        NativeToolbarItem::MenuButton(
            NativeToolbarMenuButton::new("glass.mode_switcher", label, menu_items)
                .tool_tip("Switch Mode")
                .icon(icon)
                .shows_indicator(true)
                .on_select(move |event, window, cx| {
                    if let Some(workspace) = workspace.upgrade() {
                        workspace.update(cx, |_workspace, cx| match event.index {
                            0 => {
                                window.dispatch_action(SwitchToBrowserMode.boxed_clone(), cx);
                            }
                            1 => {
                                window.dispatch_action(SwitchToEditorMode.boxed_clone(), cx);
                            }
                            2 => {
                                window.dispatch_action(SwitchToTerminalMode.boxed_clone(), cx);
                            }
                            _ => {}
                        });
                    }
                }),
        )
    }

    fn build_project_button_item(&self, cx: &Context<Self>) -> Option<NativeToolbarItem> {
        let name = self.effective_active_worktree(cx).map(|worktree| {
            let worktree = worktree.read(cx);
            worktree.root_name().as_unix_str().to_string()
        });

        let display_name = if let Some(ref name) = name {
            util::truncate_and_trailoff(name, MAX_PROJECT_NAME_LENGTH)
        } else {
            "Open Project".to_string()
        };

        Some(NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.project_name", display_name)
                .icon("folder")
                .on_click(move |_event, window, cx| {
                    window.dispatch_action(zed_actions::OpenRecent::default().boxed_clone(), cx);
                }),
        ))
    }

    fn build_branch_button_item(&self, cx: &Context<Self>) -> Option<NativeToolbarItem> {
        let effective_worktree = self.effective_active_worktree(cx)?;
        let repository = self.get_repository_for_worktree(&effective_worktree, cx)?;

        let branch_name = {
            let repo = repository.read(cx);
            repo.branch
                .as_ref()
                .map(|branch| branch.name())
                .map(|name| util::truncate_and_trailoff(name, MAX_BRANCH_NAME_LENGTH))
                .or_else(|| {
                    repo.head_commit.as_ref().map(|commit| {
                        commit
                            .sha
                            .chars()
                            .take(MAX_SHORT_SHA_LENGTH)
                            .collect::<String>()
                    })
                })
        }?;

        Some(NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.branch_name", branch_name)
                .icon("arrow.triangle.branch")
                .on_click(move |_event, window, cx| {
                    window.dispatch_action(zed_actions::git::Branch.boxed_clone(), cx);
                }),
        ))
    }

    fn build_omnibox_item(&self, _cx: &Context<Self>) -> NativeToolbarItem {
        let workspace_for_submit = self.workspace.clone();
        let workspace_for_change = self.workspace.clone();

        NativeToolbarItem::SearchField(
            NativeToolbarSearchField::new("glass.omnibox")
                .placeholder("Search or enter URL")
                .text(SharedString::from(self.omnibox_text.clone()))
                .min_width(px(300.0))
                .max_width(px(600.0))
                .on_change(move |event: &NativeToolbarSearchEvent, window, cx| {
                    let text = event.text.clone();
                    if let Some(workspace) = workspace_for_change.upgrade() {
                        if let Some(controller) = workspace
                            .read(cx)
                            .titlebar_item()
                            .and_then(|item| item.downcast::<NativeToolbarController>().ok())
                        {
                            controller.update(cx, |controller, cx| {
                                controller.is_user_typing = true;
                                controller.omnibox_text = text.to_string();
                                controller.search_history(text.to_string(), cx);
                            });
                        }
                    }
                    if text.is_empty() {
                        window.dismiss_native_panel();
                    }
                })
                .on_submit(move |event: &NativeToolbarSearchEvent, window, cx| {
                    let text = event.text.clone();
                    window.dismiss_native_panel();
                    if let Some(workspace) = workspace_for_submit.upgrade() {
                        if let Some(controller) = workspace
                            .read(cx)
                            .titlebar_item()
                            .and_then(|item| item.downcast::<NativeToolbarController>().ok())
                        {
                            controller.update(cx, |controller, cx| {
                                controller.navigate_omnibox(&text, cx);
                            });
                        }
                    }
                }),
        )
    }

    fn build_back_item(&self) -> NativeToolbarItem {
        let workspace = self.workspace.clone();
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.browser.back", "")
                .tool_tip("Go Back")
                .icon("chevron.left")
                .on_click(move |_event, _window, cx| {
                    if let Some(workspace) = workspace.upgrade()
                        && let Some(browser_view) = workspace
                            .read(cx)
                            .get_mode_view(ModeId::BROWSER)
                            .and_then(|view| view.downcast::<browser::BrowserView>().ok())
                    {
                        browser_view.update(cx, |browser_view, cx| {
                            if let Some(tab) = browser_view.active_tab() {
                                tab.update(cx, |tab, _| {
                                    tab.go_back();
                                });
                            }
                        });
                    }
                }),
        )
    }

    fn build_forward_item(&self) -> NativeToolbarItem {
        let workspace = self.workspace.clone();
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.browser.forward", "")
                .tool_tip("Go Forward")
                .icon("chevron.right")
                .on_click(move |_event, _window, cx| {
                    if let Some(workspace) = workspace.upgrade()
                        && let Some(browser_view) = workspace
                            .read(cx)
                            .get_mode_view(ModeId::BROWSER)
                            .and_then(|view| view.downcast::<browser::BrowserView>().ok())
                    {
                        browser_view.update(cx, |browser_view, cx| {
                            if let Some(tab) = browser_view.active_tab() {
                                tab.update(cx, |tab, _| {
                                    tab.go_forward();
                                });
                            }
                        });
                    }
                }),
        )
    }

    fn build_reload_item(&self) -> NativeToolbarItem {
        let workspace = self.workspace.clone();
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.browser.reload", "")
                .tool_tip("Reload")
                .icon("arrow.clockwise")
                .on_click(move |_event, _window, cx| {
                    if let Some(workspace) = workspace.upgrade()
                        && let Some(browser_view) = workspace
                            .read(cx)
                            .get_mode_view(ModeId::BROWSER)
                            .and_then(|view| view.downcast::<browser::BrowserView>().ok())
                    {
                        browser_view.update(cx, |browser_view, cx| {
                            if let Some(tab) = browser_view.active_tab() {
                                tab.update(cx, |tab, _| {
                                    tab.reload();
                                });
                            }
                        });
                    }
                }),
        )
    }

    fn show_suggestion_panel(&self, window: &mut Window) {
        if self.omnibox_suggestions.is_empty() {
            window.dismiss_native_panel();
            return;
        }

        let workspace = self.workspace.clone();
        let mut items: Vec<NativePopoverContentItem> = Vec::new();
        let mut row_count = 0usize;
        let has_search_row = !self.omnibox_text.is_empty();

        // "Search Google" row first
        if has_search_row {
            let query = self.omnibox_text.clone();
            let search_workspace = workspace.clone();
            items.push(
                NativePopoverClickableRow::new(format!("Search \"{}\"", query))
                    .icon("magnifyingglass")
                    .detail("Google")
                    .on_click(move |window, cx| {
                        window.dismiss_native_panel();
                        let url = text_to_url(&query);
                        if let Some(workspace) = search_workspace.upgrade() {
                            if let Some(controller) = workspace
                                .read(cx)
                                .titlebar_item()
                                .and_then(|item| item.downcast::<NativeToolbarController>().ok())
                            {
                                controller.update(cx, |controller, cx| {
                                    controller.navigate_omnibox(&url, cx);
                                });
                            }
                        }
                    })
                    .into(),
            );
            row_count += 1;
            items.push(NativePopoverContentItem::separator());
        }

        items.push(NativePopoverContentItem::heading("History"));
        for suggestion in &self.omnibox_suggestions {
            let url = suggestion.url.clone();
            let navigate_workspace = workspace.clone();
            let title = if suggestion.title.is_empty() {
                suggestion.url.clone()
            } else {
                suggestion.title.clone()
            };
            let detail = extract_domain(&suggestion.url);
            items.push(
                NativePopoverClickableRow::new(title)
                    .icon("clock")
                    .detail(detail)
                    .on_click(move |window, cx| {
                        window.dismiss_native_panel();
                        if let Some(workspace) = navigate_workspace.upgrade() {
                            if let Some(controller) = workspace
                                .read(cx)
                                .titlebar_item()
                                .and_then(|item| item.downcast::<NativeToolbarController>().ok())
                            {
                                controller.update(cx, |controller, cx| {
                                    controller.navigate_omnibox(&url, cx);
                                });
                            }
                        }
                    })
                    .into(),
            );
            row_count += 1;
        }

        let padding = 16.0;
        let row_height = 28.0;
        let heading_height = 28.0;
        let separator_height = 12.0;
        let content_height = (row_count as f64 * row_height)
            + heading_height
            + if has_search_row {
                separator_height
            } else {
                0.0
            }
            + padding * 2.0;
        let panel_height = content_height.min(400.0);

        let panel = NativePanel::new(450.0, panel_height)
            .style(NativePanelStyle::Borderless)
            .level(NativePanelLevel::PopUpMenu)
            .non_activating(true)
            .has_shadow(true)
            .corner_radius(10.0)
            .material(NativePanelMaterial::Popover)
            .on_close(|_, _, _| {})
            .items(items);

        window.show_native_panel(
            panel,
            NativePanelAnchor::ToolbarItem("glass.omnibox".into()),
        );
    }

    fn build_restricted_mode_item(&self, cx: &Context<Self>) -> Option<NativeToolbarItem> {
        let has_restricted_worktrees = TrustedWorktrees::try_get_global(cx)
            .map(|trusted_worktrees| {
                trusted_worktrees
                    .read(cx)
                    .has_restricted_worktrees(&self.project.read(cx).worktree_store(), cx)
            })
            .unwrap_or(false);

        if !has_restricted_worktrees {
            return None;
        }

        Some(NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.restricted_mode", "Restricted Mode")
                .tool_tip("Restricted Mode - Click to manage worktree trust")
                .icon("exclamationmark.shield")
                .on_click(move |_event, window, cx| {
                    window.dispatch_action(ToggleWorktreeSecurity.boxed_clone(), cx);
                }),
        ))
    }

    fn build_project_host_item(&self, cx: &Context<Self>) -> Option<NativeToolbarItem> {
        if self.project.read(cx).is_via_remote_server() {
            let options = self.project.read(cx).remote_connection_options(cx)?;
            let host_name = options.display_name();
            return Some(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.project_host", host_name)
                    .tool_tip("Remote Project")
                    .icon("server.rack")
                    .on_click(move |_event, window, cx| {
                        window.dispatch_action(
                            OpenRemote {
                                from_existing_connection: false,
                                create_new_window: false,
                            }
                            .boxed_clone(),
                            cx,
                        );
                    }),
            ));
        }

        if self.project.read(cx).is_disconnected(cx) {
            return Some(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.project_host", "Disconnected")
                    .tool_tip("Disconnected from remote project")
                    .icon("bolt.horizontal.circle"),
            ));
        }

        let host = self.project.read(cx).host()?;
        let host_user = self.user_store.read(cx).get_cached_user(host.user_id)?;
        let workspace = self.workspace.clone();
        let peer_id = host.peer_id;
        let mut button =
            NativeToolbarButton::new("glass.project_host", host_user.github_login.clone())
                .tool_tip("Project Host - Click to follow")
                .on_click(move |_event, window, cx| {
                    if let Some(workspace) = workspace.upgrade() {
                        workspace.update(cx, |workspace, cx| {
                            workspace.follow(peer_id, window, cx);
                        });
                    }
                });
        let avatar_url = host_user.avatar_uri.to_string();
        if !avatar_url.is_empty() {
            button = button.image_url(avatar_url).image_circular(true);
        }
        Some(NativeToolbarItem::Button(button))
    }

    fn build_connection_status_item(&self, _cx: &Context<Self>) -> Option<NativeToolbarItem> {
        let status = self.client.status();
        let status = &*status.borrow();
        match status {
            client::Status::ConnectionError
            | client::Status::ConnectionLost
            | client::Status::Reauthenticating
            | client::Status::Reconnecting
            | client::Status::ReconnectionError { .. } => Some(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.connection_status", "Disconnected")
                    .tool_tip("Connection lost - reconnecting...")
                    .icon("wifi.exclamationmark"),
            )),
            client::Status::UpgradeRequired => Some(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.connection_status", "Update Required")
                    .tool_tip("Please update to collaborate")
                    .icon("exclamationmark.arrow.circlepath")
                    .on_click(|_event, window, cx| {
                        auto_update::check(&Default::default(), window, cx);
                    }),
            )),
            _ => None,
        }
    }

    fn build_sign_in_item(&self, _cx: &Context<Self>) -> NativeToolbarItem {
        let client = self.client.clone();
        let workspace = self.workspace.clone();
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.sign_in", "Sign In")
                .tool_tip("Sign in to your account")
                .icon("person.crop.circle.badge.plus")
                .on_click(move |_event, window, cx| {
                    let client = client.clone();
                    let workspace = workspace.clone();
                    window
                        .spawn(cx, async move |mut cx| {
                            client
                                .sign_in_with_optional_connect(true, cx)
                                .await
                                .notify_workspace_async_err(workspace, &mut cx);
                        })
                        .detach();
                }),
        )
    }

    fn build_user_menu_item(
        &self,
        user: &Option<Arc<client::User>>,
        cx: &Context<Self>,
    ) -> NativeToolbarItem {
        let show_update = self.update_version.read(cx).show_update_in_menu_bar();
        let is_signed_in = user.is_some();
        let user_login = user
            .as_ref()
            .map(|u| u.github_login.to_string())
            .unwrap_or_else(|| "Account".to_string());

        let mut menu_items = Vec::new();

        if is_signed_in {
            menu_items.push(NativeToolbarMenuItem::action(&user_login).enabled(false));
            menu_items.push(NativeToolbarMenuItem::separator());
        }

        if show_update {
            menu_items
                .push(NativeToolbarMenuItem::action("Restart to Update").icon("arrow.down.circle"));
            menu_items.push(NativeToolbarMenuItem::separator());
        }

        menu_items.push(NativeToolbarMenuItem::action("Settings").icon("gearshape"));
        menu_items.push(NativeToolbarMenuItem::action("Keymap").icon("keyboard"));
        menu_items.push(NativeToolbarMenuItem::action("Themes…").icon("paintbrush"));
        menu_items.push(NativeToolbarMenuItem::action("Icon Themes…").icon("photo"));
        menu_items.push(NativeToolbarMenuItem::action("Extensions").icon("puzzlepiece.extension"));

        if is_signed_in {
            menu_items.push(NativeToolbarMenuItem::separator());
            menu_items.push(
                NativeToolbarMenuItem::action("Sign Out")
                    .icon("rectangle.portrait.and.arrow.right"),
            );
        }

        let mut menu_button =
            NativeToolbarMenuButton::new("glass.user_menu", "Account", menu_items)
                .tool_tip("User Menu")
                .shows_indicator(false);

        let show_user_picture = TitleBarSettings::get_global(cx).show_user_picture;
        let user_avatar_url = user.as_ref().map(|u| u.avatar_uri.to_string());
        if show_user_picture
            && let Some(user_avatar_url) = user_avatar_url
            && !user_avatar_url.is_empty()
        {
            menu_button = menu_button.image_url(user_avatar_url).image_circular(true);
        } else {
            menu_button = menu_button.icon("person.crop.circle");
        }

        let workspace = self.workspace.clone();
        let client = self.client.clone();
        NativeToolbarItem::MenuButton(menu_button.on_select(move |event, window, cx| {
            let show_update_offset = if show_update { 1 } else { 0 };
            let signed_in_offset = if is_signed_in { 1 } else { 0 };
            let base = signed_in_offset + show_update_offset;

            if is_signed_in && event.index == 0 {
                cx.open_url(&client::zed_urls::account_url(cx));
                return;
            }

            if show_update && event.index == signed_in_offset {
                workspace::reload(cx);
                return;
            }

            match event.index.saturating_sub(base) {
                0 => window.dispatch_action(zed_actions::OpenSettings.boxed_clone(), cx),
                1 => window.dispatch_action(zed_actions::OpenKeymap.boxed_clone(), cx),
                2 => window.dispatch_action(
                    zed_actions::theme_selector::Toggle::default().boxed_clone(),
                    cx,
                ),
                3 => window.dispatch_action(
                    zed_actions::icon_theme_selector::Toggle::default().boxed_clone(),
                    cx,
                ),
                4 => window.dispatch_action(zed_actions::Extensions::default().boxed_clone(), cx),
                5 if is_signed_in => {
                    let client = client.clone();
                    let _workspace = workspace.clone();
                    window
                        .spawn(cx, async move |mut cx| {
                            client.sign_out(&mut cx).await;
                        })
                        .detach();
                }
                _ => {}
            }
        }))
    }

    // -- Browser / omnibox helpers --

    fn browser_view(&self, cx: &App) -> Option<Entity<browser::BrowserView>> {
        let workspace = self.workspace.upgrade()?;
        let view = workspace.read(cx).get_mode_view(ModeId::BROWSER)?;
        view.downcast::<browser::BrowserView>().ok()
    }

    fn active_tab_is_new_tab_page(&self, cx: &App) -> bool {
        self.browser_view(cx)
            .and_then(|browser_view| {
                let browser_view = browser_view.read(cx);
                browser_view
                    .active_tab()
                    .map(|tab| tab.read(cx).is_new_tab_page())
            })
            .unwrap_or(false)
    }

    fn sync_omnibox_url(&mut self, cx: &mut App) {
        if self.is_user_typing {
            return;
        }

        let url = self.browser_view(cx).and_then(|bv| {
            let bv = bv.read(cx);
            bv.active_tab().map(|tab| tab.read(cx).url().to_string())
        });

        if let Some(url) = url {
            let omnibox_text = display_omnibox_text(&url);
            if self.omnibox_text != omnibox_text {
                self.omnibox_text = omnibox_text;
            }
        }
    }

    fn navigate_omnibox(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }

        let url = text_to_url(text);
        self.omnibox_text = url.clone();
        self.is_user_typing = false;
        self.omnibox_suggestions.clear();

        if let Some(browser_view) = self.browser_view(cx) {
            browser_view.update(cx, |bv, cx| {
                if let Some(tab) = bv.active_tab() {
                    tab.update(cx, |tab, cx| {
                        tab.navigate(&url, cx);
                    });
                }
            });
        }

        cx.notify();
    }

    // -- Status data --

    fn refresh_status_data(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_pane_item = self
            .active_pane
            .as_ref()
            .and_then(|pane| pane.read(cx).active_item());

        self.status_cursor = None;
        self.status_language = None;
        self.status_encoding = None;
        self.status_line_ending = None;
        self.status_toolchain = None;
        self.status_image_info = None;
        self.active_editor_subscription = None;
        self.active_image_subscription = None;

        if let Some(ref item) = active_pane_item {
            if let Some(editor) = item.act_as::<Editor>(cx) {
                self.active_editor_subscription =
                    Some(
                        cx.subscribe_in(&editor, window, |_this, _editor, event, _window, cx| {
                            if matches!(
                                event,
                                editor::EditorEvent::SelectionsChanged { .. }
                                    | editor::EditorEvent::BufferEdited
                            ) {
                                cx.notify();
                            }
                        }),
                    );

                let (cursor, language, encoding, line_ending) =
                    editor.update(cx, |editor_ref, cx| {
                        let mut cursor = None;
                        let mut language = None;
                        let mut encoding = None;
                        let mut line_ending_str = None;

                        if matches!(editor_ref.mode(), editor::EditorMode::Full { .. }) {
                            let snapshot = editor_ref.display_snapshot(cx);
                            if snapshot.buffer_snapshot().excerpts().count() > 0 {
                                let newest = editor_ref.selections.newest::<text::Point>(&snapshot);
                                let head = newest.head();
                                if let Some((buffer_snapshot, point, _)) =
                                    snapshot.buffer_snapshot().point_to_buffer_point(head)
                                {
                                    let line_start = text::Point::new(point.row, 0);
                                    let chars = buffer_snapshot
                                        .text_summary_for_range::<text::TextSummary, _>(
                                            line_start..point,
                                        )
                                        .chars
                                        as u32;
                                    cursor = Some(format!("{}:{}", point.row + 1, chars + 1));
                                }
                            }
                        }

                        if let Some((_, buffer, _)) = editor_ref.active_excerpt(cx) {
                            let buffer = buffer.read(cx);

                            if let Some(lang) = buffer.language() {
                                language = Some(lang.name().to_string());
                            }

                            let enc = buffer.encoding();
                            let has_bom = buffer.has_bom();
                            if enc != encoding_rs::UTF_8 || has_bom {
                                let mut text = enc.name().to_string();
                                if has_bom {
                                    text.push_str(" (BOM)");
                                }
                                encoding = Some(text);
                            }

                            let le = buffer.line_ending();
                            if le != LineEnding::Unix {
                                line_ending_str = Some(le.label().to_string());
                            }
                        }

                        (cursor, language, encoding, line_ending_str)
                    });

                self.status_cursor = cursor;
                self.status_language = language;
                self.status_encoding = encoding;
                self.status_line_ending = line_ending;
            }

            if let Some(image_view) = item.act_as::<ImageView>(cx) {
                if let Some(metadata) = image_view.read(cx).image_metadata(cx) {
                    self.status_image_info = Some(Self::format_image_metadata(&metadata, cx));
                } else {
                    self.active_image_subscription =
                        Some(cx.observe(&image_view, |this, image_view, cx| {
                            if let Some(metadata) = image_view.read(cx).image_metadata(cx) {
                                this.status_image_info =
                                    Some(Self::format_image_metadata(&metadata, cx));
                                cx.notify();
                            }
                        }));
                }
            }
        }
    }

    fn format_image_metadata(metadata: &ImageMetadata, cx: &App) -> String {
        let settings = image_viewer::ImageViewerSettings::get_global(cx);
        let mut components = Vec::new();
        components.push(format!("{}x{}", metadata.width, metadata.height));
        let use_decimal = matches!(settings.unit, image_viewer::ImageFileSizeUnit::Decimal);
        components.push(util::size::format_file_size(
            metadata.file_size,
            use_decimal,
        ));
        components.push(
            match metadata.format {
                ImageFormat::Png => "PNG",
                ImageFormat::Jpeg => "JPEG",
                ImageFormat::Gif => "GIF",
                ImageFormat::WebP => "WebP",
                ImageFormat::Tiff => "TIFF",
                ImageFormat::Bmp => "BMP",
                ImageFormat::Ico => "ICO",
                ImageFormat::Avif => "Avif",
                _ => "Unknown",
            }
            .to_string(),
        );
        components.join(" \u{2022} ")
    }

    fn search_history(&mut self, query: String, cx: &mut Context<Self>) {
        let entries = self
            .browser_view(cx)
            .map(|bv| bv.read(cx).history().read(cx).entries().to_vec());

        let Some(entries) = entries else {
            return;
        };

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let matches =
                browser::history::BrowserHistory::search(entries, query, 8, executor).await;
            let _ = cx.update(|cx| {
                let _ = this.update(cx, |this, cx| {
                    this.omnibox_suggestions = matches;
                    cx.notify();
                });
            });
        })
        .detach();
    }
}

fn text_to_url(text: &str) -> String {
    if text.starts_with("http://") || text.starts_with("https://") {
        text.to_string()
    } else if text.contains('.') && !text.contains(' ') {
        format!("https://{}", text)
    } else {
        let encoded: String = url::form_urlencoded::byte_serialize(text.as_bytes()).collect();
        format!("https://www.google.com/search?q={}", encoded)
    }
}

fn display_omnibox_text(url: &str) -> String {
    if url == "glass://newtab" {
        return String::new();
    }

    url.to_string()
}

fn extract_domain(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}
