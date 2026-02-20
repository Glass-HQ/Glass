mod application_menu;
mod onboarding_banner;
mod title_bar_settings;
mod update_version;

pub use workspace::TitleBarItemView;

use crate::application_menu::ApplicationMenu;
#[cfg(not(target_os = "macos"))]
use crate::application_menu::show_menus;
pub use platform_title_bar::{
    self, DraggedWindowTab, MergeAllWindows, MoveTabToNewWindow, PlatformTitleBar,
    ShowNextWindowTab, ShowPreviousWindowTab,
};

#[cfg(not(target_os = "macos"))]
use crate::application_menu::{
    ActivateDirection, ActivateMenuLeft, ActivateMenuRight, OpenApplicationMenu,
};

use auto_update::AutoUpdateStatus;
use client::{Client, UserStore, zed_urls};
use cloud_api_types::Plan;
use feature_flags::{AgentV2FeatureFlag, FeatureFlagAppExt};
#[allow(unused_imports)]
use gpui::{
    Action, AnyElement, App, Context, Corner, Element, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, MouseButton, NativeButton, NativeButtonStyle,
    NativeButtonTint, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription,
    WeakEntity, Window, actions, div, native_button, native_icon_button,
};
#[cfg(target_os = "macos")]
use gpui::{
    NativeToolbar, NativeToolbarButton, NativeToolbarComboBox, NativeToolbarDisplayMode,
    NativeToolbarItem, NativeToolbarLabel, NativeToolbarMenuButton, NativeToolbarMenuItem,
    NativeToolbarSegment, NativeToolbarSegmentedControl, NativeToolbarSizeMode, SharedString, px,
};
use onboarding_banner::OnboardingBanner;
#[cfg(target_os = "macos")]
use {
    editor::Editor,
    image_viewer::ImageView,
    language::LineEnding,
    project::image_store::{ImageFormat, ImageMetadata},
};
use project::{Project, git_store::GitStoreEvent, trusted_worktrees::TrustedWorktrees};
use remote::RemoteConnectionOptions;
use settings::Settings;
use settings::WorktreeId;
use std::sync::Arc;
use theme::ActiveTheme;
use title_bar_settings::TitleBarSettings;
#[allow(unused_imports)]
use ui::{
    Avatar, ButtonLike, Chip, ContextMenu, IconWithIndicator, Indicator, PopoverMenu,
    PopoverMenuHandle, TintColor, Tooltip, prelude::*, utils::platform_title_bar_height,
};
use update_version::UpdateVersion;
use util::ResultExt;
#[allow(unused_imports)]
use workspace::{
    MultiWorkspace, Pane, TitleBarItemViewHandle, ToggleWorkspaceSidebar,
    ToggleWorktreeSecurity, Workspace, notifications::NotifyResultExt,
};
#[allow(unused_imports)]
use workspace_modes::{
    ModeId, ModeSwitcher, ModeViewRegistry, SwitchToBrowserMode, SwitchToEditorMode,
    SwitchToTerminalMode,
};
use zed_actions::OpenRemote;

pub use onboarding_banner::restore_banner;

const MAX_PROJECT_NAME_LENGTH: usize = 40;
const MAX_BRANCH_NAME_LENGTH: usize = 40;
const MAX_SHORT_SHA_LENGTH: usize = 8;

actions!(
    collab,
    [
        /// Toggles the user menu dropdown.
        ToggleUserMenu,
        /// Toggles the project menu dropdown.
        ToggleProjectMenu,
        /// Switches to a different git branch.
        SwitchBranch,
        /// A debug action to simulate an update being available to test the update banner UI.
        SimulateUpdateAvailable
    ]
);

pub fn init(cx: &mut App) {
    platform_title_bar::PlatformTitleBar::init(cx);

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        let item = cx.new(|cx| TitleBar::new("title-bar", workspace, window, cx));
        workspace.set_titlebar_item(item.into(), window, cx);

        workspace.register_action(|workspace, _: &SimulateUpdateAvailable, _window, cx| {
            if let Some(titlebar) = workspace
                .titlebar_item()
                .and_then(|item| item.downcast::<TitleBar>().ok())
            {
                titlebar.update(cx, |titlebar, cx| {
                    titlebar.toggle_update_simulation(cx);
                });
            }
        });

        #[cfg(not(target_os = "macos"))]
        workspace.register_action(|workspace, action: &OpenApplicationMenu, window, cx| {
            if let Some(titlebar) = workspace
                .titlebar_item()
                .and_then(|item| item.downcast::<TitleBar>().ok())
            {
                titlebar.update(cx, |titlebar, cx| {
                    if let Some(ref menu) = titlebar.application_menu {
                        menu.update(cx, |menu, cx| menu.open_menu(action, window, cx));
                    }
                });
            }
        });

        #[cfg(not(target_os = "macos"))]
        workspace.register_action(|workspace, _: &ActivateMenuRight, window, cx| {
            if let Some(titlebar) = workspace
                .titlebar_item()
                .and_then(|item| item.downcast::<TitleBar>().ok())
            {
                titlebar.update(cx, |titlebar, cx| {
                    if let Some(ref menu) = titlebar.application_menu {
                        menu.update(cx, |menu, cx| {
                            menu.navigate_menus_in_direction(ActivateDirection::Right, window, cx)
                        });
                    }
                });
            }
        });

        #[cfg(not(target_os = "macos"))]
        workspace.register_action(|workspace, _: &ActivateMenuLeft, window, cx| {
            if let Some(titlebar) = workspace
                .titlebar_item()
                .and_then(|item| item.downcast::<TitleBar>().ok())
            {
                titlebar.update(cx, |titlebar, cx| {
                    if let Some(ref menu) = titlebar.application_menu {
                        menu.update(cx, |menu, cx| {
                            menu.navigate_menus_in_direction(ActivateDirection::Left, window, cx)
                        });
                    }
                });
            }
        });
    })
    .detach();
}

#[allow(dead_code)]
pub struct TitleBar {
    platform_titlebar: Entity<PlatformTitleBar>,
    project: Entity<Project>,
    user_store: Entity<UserStore>,
    client: Arc<Client>,
    workspace: WeakEntity<Workspace>,
    application_menu: Option<Entity<ApplicationMenu>>,
    _subscriptions: Vec<Subscription>,
    banner: Entity<OnboardingBanner>,
    update_version: Entity<UpdateVersion>,
    right_items: Vec<Box<dyn TitleBarItemViewHandle>>,
    active_pane: Option<Entity<Pane>>,
    #[cfg(target_os = "macos")]
    omnibox_text: String,
    #[cfg(target_os = "macos")]
    is_user_typing: bool,
    #[cfg(target_os = "macos")]
    omnibox_suggestions: Vec<browser::history::HistoryMatch>,
    #[cfg(target_os = "macos")]
    last_toolbar_key: String,
    #[cfg(target_os = "macos")]
    status_cursor: Option<String>,
    #[cfg(target_os = "macos")]
    status_language: Option<String>,
    #[cfg(target_os = "macos")]
    status_encoding: Option<String>,
    #[cfg(target_os = "macos")]
    status_line_ending: Option<String>,
    #[cfg(target_os = "macos")]
    status_toolchain: Option<String>,
    #[cfg(target_os = "macos")]
    status_image_info: Option<String>,
    #[cfg(target_os = "macos")]
    active_editor_subscription: Option<Subscription>,
    #[cfg(target_os = "macos")]
    active_image_subscription: Option<Subscription>,
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(target_os = "macos")]
        {
            self.update_native_toolbar(window, cx);
            return self.platform_titlebar.clone().into_any_element();
        }

        #[cfg(not(target_os = "macos"))]
        self.render_gpui_title_bar(window, cx).into_any_element()
    }
}

impl TitleBar {
    #[cfg(not(target_os = "macos"))]
    fn render_gpui_title_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title_bar_settings = *TitleBarSettings::get_global(cx);

        let show_menus = show_menus(cx);

        let active_mode = self
            .workspace
            .upgrade()
            .map(|ws| ws.read(cx).active_mode_id());

        let is_browser_mode = active_mode == Some(ModeId::BROWSER);
        let is_terminal_mode = active_mode == Some(ModeId::TERMINAL);

        let mut children = Vec::new();

        children.push(
            h_flex()
                .gap_0p5()
                .map(|title_bar| {
                    let mut render_project_items = !is_browser_mode
                        && (title_bar_settings.show_branch_name
                            || title_bar_settings.show_project_items);
                    title_bar
                        .children(self.render_workspace_sidebar_toggle(window, cx))
                        .when_some(
                            self.application_menu.clone().filter(|_| !show_menus),
                            |title_bar, menu| {
                                render_project_items &=
                                    !menu.update(cx, |menu, cx| menu.all_menus_shown(cx));
                                title_bar.child(menu)
                            },
                        )
                        .child(self.render_mode_switcher(window, cx))
                        .children(self.render_restricted_mode(cx))
                        .when(render_project_items, |title_bar| {
                            title_bar
                                .when(title_bar_settings.show_project_items, |title_bar| {
                                    title_bar
                                        .children(self.render_project_host(cx))
                                        .child(self.render_project_name(cx))
                                })
                                .when(title_bar_settings.show_branch_name, |title_bar| {
                                    title_bar.children(self.render_project_branch(cx))
                                })
                        })
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .into_any_element(),
        );

        let titlebar_center = active_mode.and_then(|mode_id| {
            ModeViewRegistry::try_global(cx)
                .and_then(|reg| reg.titlebar_center_view(mode_id).cloned())
        });

        if let Some(center_view) = titlebar_center {
            children.push(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(center_view)
                    .into_any_element(),
            );
        } else if title_bar_settings.show_onboarding_banner {
            children.push(self.banner.clone().into_any_element())
        }

        let status = self.client.status();
        let status = &*status.borrow();
        let user = self.user_store.read(cx).current_user();

        let signed_in = user.is_some();

        children.push(
            h_flex()
                .map(|this| {
                    if signed_in {
                        this.pr_1p5()
                    } else {
                        this.pr_1()
                    }
                })
                .gap_1()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(self.render_connection_status(status, cx))
                .child(self.update_version.clone())
                .when(!is_browser_mode && !is_terminal_mode, |this| {
                    this.child(self.render_right_items())
                })
                .when(
                    user.is_none() && TitleBarSettings::get_global(cx).show_sign_in,
                    |this| this.child(self.render_sign_in_button(cx)),
                )
                .when(TitleBarSettings::get_global(cx).show_user_menu, |this| {
                    this.child(self.render_user_menu_button(cx))
                })
                .into_any_element(),
        );

        if show_menus {
            self.platform_titlebar.update(cx, |this, _| {
                this.set_children(
                    self.application_menu
                        .clone()
                        .map(|menu| menu.into_any_element()),
                );
            });

            let height = platform_title_bar_height(window);
            let title_bar_color = self.platform_titlebar.update(cx, |platform_titlebar, cx| {
                platform_titlebar.title_bar_color(window, cx)
            });

            v_flex()
                .w_full()
                .child(self.platform_titlebar.clone().into_any_element())
                .child(
                    h_flex()
                        .bg(title_bar_color)
                        .h(height)
                        .pl_2()
                        .justify_between()
                        .w_full()
                        .children(children),
                )
                .into_any_element()
        } else {
            self.platform_titlebar.update(cx, |this, _| {
                this.set_children(children);
            });
            self.platform_titlebar.clone().into_any_element()
        }
    }
}

#[cfg(target_os = "macos")]
impl TitleBar {
    fn update_native_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_mode = self
            .workspace
            .upgrade()
            .map(|ws| ws.read(cx).active_mode_id())
            .unwrap_or(ModeId::BROWSER);

        let is_browser_mode = active_mode == ModeId::BROWSER;
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

        let toolbar_key = format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
            active_mode.0,
            project_name,
            branch_name,
            self.omnibox_text,
            self.omnibox_suggestions.len(),
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

        toolbar = toolbar.item(self.build_sidebar_toggle_item(cx));
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
            toolbar = toolbar.item(self.build_omnibox_item(cx));
            toolbar = toolbar.item(NativeToolbarItem::FlexibleSpace);
        }

        // Status items (only in editor/terminal modes)
        if !is_browser_mode {
            if let Some(ref cursor) = self.status_cursor {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.cursor", cursor.clone())
                        .tool_tip("Go to Line/Column")
                        .icon("line.3.horizontal")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(
                                editor::actions::ToggleGoToLine.boxed_clone(),
                                cx,
                            );
                        }),
                ));
            }
            if let Some(ref language) = self.status_language {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.language", language.clone())
                        .tool_tip("Select Language")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(
                                language_selector::Toggle.boxed_clone(),
                                cx,
                            );
                        }),
                ));
            }
            if let Some(ref toolchain) = self.status_toolchain {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.toolchain", toolchain.clone())
                        .tool_tip("Select Toolchain")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(
                                toolchain_selector::Select.boxed_clone(),
                                cx,
                            );
                        }),
                ));
            }
            if let Some(ref encoding) = self.status_encoding {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.encoding", encoding.clone())
                        .tool_tip("Select Encoding")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(
                                encoding_selector::Toggle.boxed_clone(),
                                cx,
                            );
                        }),
                ));
            }
            if let Some(ref line_ending) = self.status_line_ending {
                toolbar = toolbar.item(NativeToolbarItem::Button(
                    NativeToolbarButton::new("glass.status.line_ending", line_ending.clone())
                        .tool_tip("Select Line Ending")
                        .on_click(|_event, window, cx| {
                            window.dispatch_action(
                                line_ending_selector::Toggle.boxed_clone(),
                                cx,
                            );
                        }),
                ));
            }
            if let Some(ref image_info) = self.status_image_info {
                toolbar = toolbar.item(NativeToolbarItem::Label(
                    NativeToolbarLabel::new("glass.status.image_info", image_info.clone()),
                ));
            }

            // Activity indicator
            toolbar = toolbar.item(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.status.activity", "")
                    .tool_tip("View Logs")
                    .icon("arrow.triangle.2.circlepath")
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(workspace::OpenLog.boxed_clone(), cx);
                    }),
            ));

            // Language server status
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

            // Edit predictions toggle
            toolbar = toolbar.item(NativeToolbarItem::Button(
                NativeToolbarButton::new("glass.status.predictions", "")
                    .tool_tip("Edit Predictions")
                    .icon("sparkles")
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(
                            edit_prediction_ui::ToggleMenu.boxed_clone(),
                            cx,
                        );
                    }),
            ));
        }

        toolbar = toolbar.item(self.build_settings_item(cx));

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

    fn build_sidebar_toggle_item(&self, _cx: &Context<Self>) -> NativeToolbarItem {
        let workspace = self.workspace.clone();
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.sidebar_toggle", "Sidebar")
                .tool_tip("Toggle Sidebar")
                .icon("sidebar.leading")
                .on_click(move |_event, window, cx| {
                    if let Some(workspace) = workspace.upgrade() {
                        workspace.update(cx, |_workspace, cx| {
                            window.dispatch_action(
                                ToggleWorkspaceSidebar.boxed_clone(),
                                cx,
                            );
                        });
                    }
                }),
        )
    }

    fn build_mode_switcher_item(
        &self,
        active_mode: ModeId,
        _cx: &Context<Self>,
    ) -> NativeToolbarItem {
        let selected_index = match active_mode {
            ModeId::BROWSER => 0,
            ModeId::EDITOR => 1,
            ModeId::TERMINAL => 2,
            _ => 0,
        };

        let segments = vec![
            NativeToolbarSegment::new("Browser").icon("globe"),
            NativeToolbarSegment::new("Editor").icon("doc.text"),
            NativeToolbarSegment::new("Terminal").icon("terminal"),
        ];

        let workspace = self.workspace.clone();
        NativeToolbarItem::SegmentedControl(
            NativeToolbarSegmentedControl::new("glass.mode_switcher", segments)
                .selected_index(selected_index)
                .on_select(move |event, window, cx| {
                    if let Some(workspace) = workspace.upgrade() {
                        workspace.update(cx, |_workspace, cx| match event.selected_index {
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
                    window.dispatch_action(
                        zed_actions::OpenRecent::default().boxed_clone(),
                        cx,
                    );
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
                .on_click(
                move |_event, window, cx| {
                    window.dispatch_action(zed_actions::git::Branch.boxed_clone(), cx);
                },
            ),
        ))
    }

    fn build_omnibox_item(&self, _cx: &Context<Self>) -> NativeToolbarItem {
        let workspace_for_submit = self.workspace.clone();
        let workspace_for_change = self.workspace.clone();
        let workspace_for_select = self.workspace.clone();

        let suggestion_items: Vec<SharedString> = self
            .omnibox_suggestions
            .iter()
            .map(|m| {
                if m.title.is_empty() {
                    SharedString::from(m.url.clone())
                } else {
                    SharedString::from(format!("{} — {}", m.title, m.url))
                }
            })
            .collect();

        NativeToolbarItem::ComboBox(
            NativeToolbarComboBox::new("glass.omnibox")
                .placeholder("Search or enter URL")
                .text(SharedString::from(self.omnibox_text.clone()))
                .items(suggestion_items)
                .min_width(px(300.0))
                .max_width(px(600.0))
                .on_change(move |event, _window, cx| {
                    if let Some(workspace) = workspace_for_change.upgrade() {
                        let text = event.text.clone();
                        workspace.update(cx, |workspace, cx| {
                            if let Some(titlebar) = workspace
                                .titlebar_item()
                                .and_then(|item| item.downcast::<TitleBar>().ok())
                            {
                                titlebar.update(cx, |titlebar, cx| {
                                    titlebar.is_user_typing = true;
                                    titlebar.omnibox_text = text.to_string();
                                    titlebar.search_history(text.to_string(), cx);
                                });
                            }
                        });
                    }
                })
                .on_select(move |event, _window, cx| {
                    if let Some(workspace) = workspace_for_select.upgrade() {
                        let index = event.selected_index;
                        workspace.update(cx, |workspace, cx| {
                            if let Some(titlebar) = workspace
                                .titlebar_item()
                                .and_then(|item| item.downcast::<TitleBar>().ok())
                            {
                                titlebar.update(cx, |titlebar, cx| {
                                    if let Some(suggestion) =
                                        titlebar.omnibox_suggestions.get(index)
                                    {
                                        let url = suggestion.url.clone();
                                        titlebar.navigate_omnibox(&url, cx);
                                    }
                                });
                            }
                        });
                    }
                })
                .on_submit(move |event, _window, cx| {
                    if let Some(workspace) = workspace_for_submit.upgrade() {
                        let text = event.text.clone();
                        workspace.update(cx, |workspace, cx| {
                            if let Some(titlebar) = workspace
                                .titlebar_item()
                                .and_then(|item| item.downcast::<TitleBar>().ok())
                            {
                                titlebar.update(cx, |titlebar, cx| {
                                    titlebar.navigate_omnibox(&text, cx);
                                });
                            }
                        });
                    }
                }),
        )
    }

    fn build_settings_item(&self, _cx: &Context<Self>) -> NativeToolbarItem {
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.settings", "Settings")
                .tool_tip("Settings")
                .icon("gearshape")
                .on_click(move |_event, window, cx| {
                    window.dispatch_action(zed_actions::OpenSettings.boxed_clone(), cx);
                }),
        )
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
            menu_items.push(
                NativeToolbarMenuItem::action("Restart to Update")
                    .icon("arrow.down.circle"),
            );
            menu_items.push(NativeToolbarMenuItem::separator());
        }

        menu_items.push(NativeToolbarMenuItem::action("Settings").icon("gearshape"));
        menu_items.push(NativeToolbarMenuItem::action("Keymap").icon("keyboard"));
        menu_items.push(NativeToolbarMenuItem::action("Themes…").icon("paintbrush"));
        menu_items.push(NativeToolbarMenuItem::action("Icon Themes…").icon("photo"));
        menu_items.push(NativeToolbarMenuItem::action("Extensions").icon("puzzlepiece.extension"));

        if is_signed_in {
            menu_items.push(NativeToolbarMenuItem::separator());
            menu_items.push(NativeToolbarMenuItem::action("Sign Out").icon("rectangle.portrait.and.arrow.right"));
        }

        let mut menu_button = NativeToolbarMenuButton::new(
            "glass.user_menu",
            "Account",
            menu_items,
        )
        .tool_tip("User Menu")
        .shows_indicator(false);

        menu_button = menu_button.icon("person.crop.circle");

        let workspace = self.workspace.clone();
        let client = self.client.clone();
        NativeToolbarItem::MenuButton(
            menu_button.on_select(move |event, window, cx| {
                // Offset indices based on what's in the menu
                let show_update_offset = if show_update { 1 } else { 0 };
                let signed_in_offset = if is_signed_in { 1 } else { 0 };
                let base = signed_in_offset + show_update_offset;

                if is_signed_in && event.index == 0 {
                    cx.open_url(&zed_urls::account_url(cx));
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
                    4 => window.dispatch_action(
                        zed_actions::Extensions::default().boxed_clone(),
                        cx,
                    ),
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
            }),
        )
    }

    fn browser_view(&self, cx: &mut App) -> Option<Entity<browser::BrowserView>> {
        let workspace = self.workspace.upgrade()?;
        let view = workspace.update(cx, |workspace, cx| {
            workspace.mode_view(ModeId::BROWSER, cx)
        })?;
        view.downcast::<browser::BrowserView>().ok()
    }

    fn sync_omnibox_url(&mut self, cx: &mut App) {
        if self.is_user_typing {
            return;
        }

        let url = self
            .browser_view(cx)
            .and_then(|bv| {
                let bv = bv.read(cx);
                bv.active_tab().map(|tab| tab.read(cx).url().to_string())
            });

        if let Some(url) = url {
            if self.omnibox_text != url {
                self.omnibox_text = url;
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
                // Subscribe to editor selection changes
                self.active_editor_subscription = Some(cx.subscribe_in(
                    &editor,
                    window,
                    |_this, _editor, event, _window, cx| {
                        if matches!(event, editor::EditorEvent::SelectionsChanged { .. }
                            | editor::EditorEvent::BufferEdited)
                        {
                            cx.notify();
                        }
                    },
                ));

                // Extract status data from editor via update to avoid borrow conflicts
                // (display_snapshot requires &mut App)
                let (cursor, language, encoding, line_ending) =
                    editor.update(cx, |editor_ref, cx| {
                        let mut cursor = None;
                        let mut language = None;
                        let mut encoding = None;
                        let mut line_ending_str = None;

                        // Cursor position
                        if matches!(editor_ref.mode(), editor::EditorMode::Full { .. }) {
                            let snapshot = editor_ref.display_snapshot(cx);
                            if snapshot.buffer_snapshot().excerpts().count() > 0 {
                                let newest = editor_ref
                                    .selections
                                    .newest::<text::Point>(&snapshot);
                                let head = newest.head();
                                if let Some((buffer_snapshot, point, _)) =
                                    snapshot.buffer_snapshot().point_to_buffer_point(head)
                                {
                                    let line_start = text::Point::new(point.row, 0);
                                    let chars = buffer_snapshot
                                        .text_summary_for_range::<text::TextSummary, _>(
                                            line_start..point,
                                        )
                                        .chars as u32;
                                    cursor =
                                        Some(format!("{}:{}", point.row + 1, chars + 1));
                                }
                            }
                        }

                        // Language, encoding, line ending
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

            // Image info
            if let Some(image_view) = item.act_as::<ImageView>(cx) {
                if let Some(metadata) = image_view.read(cx).image_metadata(cx) {
                    self.status_image_info = Some(Self::format_image_metadata(&metadata, cx));
                } else {
                    // Observe image view for metadata loading
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
        components.push(util::size::format_file_size(metadata.file_size, use_decimal));
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

#[cfg(target_os = "macos")]
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

impl TitleBar {
    pub fn new(
        id: impl Into<ElementId>,
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project = workspace.project().clone();
        let git_store = project.read(cx).git_store().clone();
        let user_store = workspace.app_state().user_store.clone();
        let client = workspace.app_state().client.clone();

        let platform_style = PlatformStyle::platform();
        let application_menu = match platform_style {
            PlatformStyle::Mac => {
                if option_env!("ZED_USE_CROSS_PLATFORM_MENU").is_some() {
                    Some(cx.new(|cx| ApplicationMenu::new(window, cx)))
                } else {
                    None
                }
            }
            PlatformStyle::Linux | PlatformStyle::Windows => {
                Some(cx.new(|cx| ApplicationMenu::new(window, cx)))
            }
        };

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
                    // Clear override when user types in any editor,
                    // so the title bar reflects the project they're actually working in
                    this.clear_active_worktree_override(cx);
                    cx.notify();
                }
            }),
        );
        subscriptions.push(cx.observe_window_activation(window, Self::window_activation_changed));
        subscriptions.push(
            cx.subscribe(&git_store, move |this, _, event, cx| match event {
                GitStoreEvent::ActiveRepositoryChanged(_) => {
                    // Clear override when focus-derived active repo changes
                    // (meaning the user focused a file from a different project)
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

        let banner = cx.new(|cx| {
            OnboardingBanner::new(
                "ACP Claude Code Onboarding",
                IconName::AiClaude,
                "Claude Code",
                Some("Introducing:".into()),
                zed_actions::agent::OpenClaudeCodeOnboardingModal.boxed_clone(),
                cx,
            )
            // When updating this to a non-AI feature release, remove this line.
            .visible_when(|cx| !project::DisableAiSettings::get_global(cx).disable_ai)
        });

        let update_version = cx.new(|cx| UpdateVersion::new(cx));
        let platform_titlebar = cx.new(|cx| PlatformTitleBar::new(id, cx));

        // Set up observer to sync sidebar state from MultiWorkspace to PlatformTitleBar.
        {
            let platform_titlebar = platform_titlebar.clone();
            let window_handle = window.window_handle();
            cx.spawn(async move |this: WeakEntity<TitleBar>, cx| {
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
                    platform_titlebar.update(cx, |titlebar, cx| {
                        titlebar.set_workspace_sidebar_open(is_open, cx);
                        titlebar.set_sidebar_has_notifications(has_notifications, cx);
                    });

                    let platform_titlebar = platform_titlebar.clone();
                    let subscription = cx.observe(&multi_workspace, move |mw, cx| {
                        let is_open = mw.read(cx).is_sidebar_open();
                        let has_notifications = mw.read(cx).sidebar_has_notifications(cx);
                        platform_titlebar.update(cx, |titlebar, cx| {
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
            application_menu,
            workspace: workspace.weak_handle(),
            project,
            user_store,
            client,
            _subscriptions: subscriptions,
            banner,
            update_version,
            right_items: Vec::new(),
            active_pane: None,
            #[cfg(target_os = "macos")]
            omnibox_text: String::new(),
            #[cfg(target_os = "macos")]
            is_user_typing: false,
            #[cfg(target_os = "macos")]
            omnibox_suggestions: Vec::new(),
            #[cfg(target_os = "macos")]
            last_toolbar_key: String::new(),
            #[cfg(target_os = "macos")]
            status_cursor: None,
            #[cfg(target_os = "macos")]
            status_language: None,
            #[cfg(target_os = "macos")]
            status_encoding: None,
            #[cfg(target_os = "macos")]
            status_line_ending: None,
            #[cfg(target_os = "macos")]
            status_toolchain: None,
            #[cfg(target_os = "macos")]
            status_image_info: None,
            #[cfg(target_os = "macos")]
            active_editor_subscription: None,
            #[cfg(target_os = "macos")]
            active_image_subscription: None,
        }
    }

    pub fn add_right_item<T>(
        &mut self,
        item: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        T: 'static + TitleBarItemView,
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
        #[cfg(target_os = "macos")]
        self.refresh_status_data(window, cx);
    }

    #[cfg(not(target_os = "macos"))]
    fn render_right_items(&self) -> impl IntoElement {
        h_flex()
            .gap_1()
            .children(self.right_items.iter().map(|item| item.to_any()))
    }

    #[cfg(not(target_os = "macos"))]
    fn worktree_count(&self, cx: &App) -> usize {
        self.project.read(cx).visible_worktrees(cx).count()
    }

    fn toggle_update_simulation(&mut self, cx: &mut Context<Self>) {
        self.update_version
            .update(cx, |banner, cx| banner.update_simulation(cx));
        cx.notify();
    }

    /// Returns the worktree to display in the title bar.
    /// - If there's an override set on the workspace, use that (if still valid)
    /// - Otherwise, derive from the active repository
    /// - Fall back to the first visible worktree
    pub fn effective_active_worktree(&self, cx: &App) -> Option<Entity<project::Worktree>> {
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

    pub fn set_active_worktree_override(
        &mut self,
        worktree_id: WorktreeId,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.set_active_worktree_override(Some(worktree_id), cx);
            });
        }
        cx.notify();
    }

    fn clear_active_worktree_override(&mut self, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.clear_active_worktree_override(cx);
            });
        }
        cx.notify();
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

    fn render_remote_project_connection(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let workspace = self.workspace.clone();

        let options = self.project.read(cx).remote_connection_options(cx)?;
        let host: SharedString = options.display_name().into();

        #[allow(unreachable_patterns)]
        let (nickname, tooltip_title, icon) = match options {
            RemoteConnectionOptions::Ssh(options) => (
                options.nickname.map(|nick| nick.into()),
                "Remote Project",
                IconName::Server,
            ),
            RemoteConnectionOptions::Wsl(_) => (None, "Remote Project", IconName::Linux),
            RemoteConnectionOptions::Docker(_dev_container_connection) => {
                (None, "Dev Container", IconName::Box)
            }
            #[cfg(any(test, feature = "test-support"))]
            RemoteConnectionOptions::Mock(_) => (None, "Mock Remote Project", IconName::Server),
            _ => (None, "Unknown Remote", IconName::Server),
        };

        let nickname = nickname.unwrap_or_else(|| host.clone());

        let (indicator_color, meta) = match self.project.read(cx).remote_connection_state(cx)? {
            remote::ConnectionState::Connecting => (Color::Info, format!("Connecting to: {host}")),
            remote::ConnectionState::Connected => (Color::Success, format!("Connected to: {host}")),
            remote::ConnectionState::HeartbeatMissed => (
                Color::Warning,
                format!("Connection attempt to {host} missed. Retrying..."),
            ),
            remote::ConnectionState::Reconnecting => (
                Color::Warning,
                format!("Lost connection to {host}. Reconnecting..."),
            ),
            remote::ConnectionState::Disconnected => {
                (Color::Error, format!("Disconnected from {host}"))
            }
        };

        let icon_color = match self.project.read(cx).remote_connection_state(cx)? {
            remote::ConnectionState::Connecting => Color::Info,
            remote::ConnectionState::Connected => Color::Default,
            remote::ConnectionState::HeartbeatMissed => Color::Warning,
            remote::ConnectionState::Reconnecting => Color::Warning,
            remote::ConnectionState::Disconnected => Color::Error,
        };

        let meta = SharedString::from(meta);

        Some(
            PopoverMenu::new("remote-project-menu")
                .menu(move |window, cx| {
                    let workspace_entity = workspace.upgrade()?;
                    let fs = workspace_entity.read(cx).project().read(cx).fs().clone();
                    Some(recent_projects::RemoteServerProjects::popover(
                        fs,
                        workspace.clone(),
                        false,
                        window,
                        cx,
                    ))
                })
                .trigger_with_tooltip(
                    ButtonLike::new("remote_project")
                        .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                        .child(
                            h_flex()
                                .gap_2()
                                .max_w_32()
                                .child(
                                    IconWithIndicator::new(
                                        Icon::new(icon).size(IconSize::Small).color(icon_color),
                                        Some(Indicator::dot().color(indicator_color)),
                                    )
                                    .indicator_border_color(Some(
                                        cx.theme().colors().title_bar_background,
                                    ))
                                    .into_any_element(),
                                )
                                .child(Label::new(nickname).size(LabelSize::Small).truncate()),
                        ),
                    move |_window, cx| {
                        Tooltip::with_meta(
                            tooltip_title,
                            Some(&OpenRemote {
                                from_existing_connection: false,
                                create_new_window: false,
                            }),
                            meta.clone(),
                            cx,
                        )
                    },
                )
                .anchor(gpui::Corner::TopLeft)
                .into_any_element(),
        )
    }

    pub fn render_restricted_mode(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
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

        let button = native_button("restricted_mode_trigger", "Restricted Mode")
            .button_style(NativeButtonStyle::Filled)
            .tint(NativeButtonTint::Warning)
            .on_click({
                cx.listener(move |this, _, window, cx| {
                    this.workspace
                        .update(cx, |workspace, cx| {
                            workspace.show_worktree_trust_security_modal(true, window, cx)
                        })
                        .log_err();
                })
            });

        if cfg!(macos_sdk_26) {
            // Make up for Tahoe's traffic light buttons having less spacing around them
            Some(div().child(button).ml_0p5().into_any_element())
        } else {
            Some(button.into_any_element())
        }
    }

    pub fn render_project_host(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.project.read(cx).is_via_remote_server() {
            return self.render_remote_project_connection(cx);
        }

        if self.project.read(cx).is_disconnected(cx) {
            return Some(
                native_button("disconnected", "Disconnected")
                    .disabled(true)
                    .into_any_element(),
            );
        }

        let host = self.project.read(cx).host()?;
        let host_user = self.user_store.read(cx).get_cached_user(host.user_id)?;
        Some(
            native_button("project_owner_trigger", host_user.github_login.clone())
                .button_style(NativeButtonStyle::Inline)
                .on_click({
                    let host_peer_id = host.peer_id;
                    cx.listener(move |this, _, window, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| {
                                workspace.follow(host_peer_id, window, cx);
                            })
                            .log_err();
                    })
                })
                .into_any_element(),
        )
    }

    #[cfg(not(target_os = "macos"))]
    fn render_mode_switcher(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let workspace = self.workspace.clone();
        let active_mode = self
            .workspace
            .upgrade()
            .map(|ws| ws.read(cx).active_mode_id())
            .unwrap_or(ModeId::BROWSER);

        ModeSwitcher::new(active_mode).on_mode_select(move |mode_id, window, cx| {
            if let Some(workspace) = workspace.upgrade() {
                workspace.update(cx, |_workspace, cx| match mode_id {
                    ModeId::BROWSER => {
                        window.dispatch_action(SwitchToBrowserMode.boxed_clone(), cx);
                    }
                    ModeId::EDITOR => {
                        window.dispatch_action(SwitchToEditorMode.boxed_clone(), cx);
                    }
                    ModeId::TERMINAL => {
                        window.dispatch_action(SwitchToTerminalMode.boxed_clone(), cx);
                    }
                    _ => {}
                });
            }
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn render_workspace_sidebar_toggle(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !cx.has_flag::<AgentV2FeatureFlag>() {
            return None;
        }

        let is_sidebar_open = self.platform_titlebar.read(cx).is_workspace_sidebar_open();

        if is_sidebar_open {
            return None;
        }

        let has_notifications = self.platform_titlebar.read(cx).sidebar_has_notifications();

        Some(
            IconButton::new("toggle-workspace-sidebar", IconName::WorkspaceNavClosed)
                .icon_size(IconSize::Small)
                .when(has_notifications, |button| {
                    button
                        .indicator(Indicator::dot().color(Color::Accent))
                        .indicator_border_color(Some(cx.theme().colors().title_bar_background))
                })
                .tooltip(move |_, cx| {
                    Tooltip::for_action("Open Workspace Sidebar", &ToggleWorkspaceSidebar, cx)
                })
                .on_click(|_, window, cx| {
                    window.dispatch_action(ToggleWorkspaceSidebar.boxed_clone(), cx);
                })
                .into_any_element(),
        )
    }

    pub fn render_project_name(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.clone();

        let name = self.effective_active_worktree(cx).map(|worktree| {
            let worktree = worktree.read(cx);
            SharedString::from(worktree.root_name().as_unix_str().to_string())
        });

        let is_project_selected = name.is_some();

        let display_name = if let Some(ref name) = name {
            util::truncate_and_trailoff(name, MAX_PROJECT_NAME_LENGTH)
        } else {
            "Open Recent Project".to_string()
        };

        let focus_handle = workspace
            .upgrade()
            .map(|w| w.read(cx).focus_handle(cx))
            .unwrap_or_else(|| cx.focus_handle());

        PopoverMenu::new("recent-projects-menu")
            .menu(move |window, cx| {
                Some(recent_projects::RecentProjects::popover(
                    workspace.clone(),
                    false,
                    focus_handle.clone(),
                    window,
                    cx,
                ))
            })
            .trigger(native_button("project_name_trigger", display_name).button_style(
                if is_project_selected {
                    NativeButtonStyle::Rounded
                } else {
                    NativeButtonStyle::Inline
                },
            ))
            .anchor(gpui::Corner::TopLeft)
    }

    pub fn render_project_branch(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let effective_worktree = self.effective_active_worktree(cx)?;
        let repository = self.get_repository_for_worktree(&effective_worktree, cx)?;
        let workspace = self.workspace.upgrade()?;

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
        };

        let show_branch_icon = TitleBarSettings::get_global(cx).show_branch_icon;
        let effective_repository = Some(repository);

        Some(
            PopoverMenu::new("branch-menu")
                .menu(move |window, cx| {
                    Some(git_ui::git_picker::popover(
                        workspace.downgrade(),
                        effective_repository.clone(),
                        git_ui::git_picker::GitPickerTab::Branches,
                        gpui::rems(34.),
                        window,
                        cx,
                    ))
                })
                .trigger(
                    native_button("project_branch_trigger", branch_name?).button_style(
                        if show_branch_icon {
                            NativeButtonStyle::Rounded
                        } else {
                            NativeButtonStyle::Inline
                        },
                    ),
                )
                .anchor(gpui::Corner::TopLeft),
        )
    }

    fn window_activation_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.update_active_view_for_followers(window, cx);
            })
            .ok();
    }

    #[cfg(not(target_os = "macos"))]
    fn render_connection_status(
        &self,
        status: &client::Status,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match status {
            client::Status::ConnectionError
            | client::Status::ConnectionLost
            | client::Status::Reauthenticating
            | client::Status::Reconnecting
            | client::Status::ReconnectionError { .. } => Some(
                div()
                    .id("disconnected")
                    .child(Icon::new(IconName::Disconnected).size(IconSize::Small))
                    .tooltip(Tooltip::text("Disconnected"))
                    .into_any_element(),
            ),
            client::Status::UpgradeRequired => {
                let auto_updater = auto_update::AutoUpdater::get(cx);
                let label = match auto_updater.map(|auto_update| auto_update.read(cx).status()) {
                    Some(AutoUpdateStatus::Updated { .. }) => "Please restart Zed to Collaborate",
                    Some(AutoUpdateStatus::Installing { .. })
                    | Some(AutoUpdateStatus::Downloading { .. })
                    | Some(AutoUpdateStatus::Checking) => "Updating...",
                    Some(AutoUpdateStatus::Idle)
                    | Some(AutoUpdateStatus::Errored { .. })
                    | None => "Please update Zed to Collaborate",
                };

                Some(
                    native_button("connection-status", label)
                        .on_click(|_, window, cx| {
                            if let Some(auto_updater) = auto_update::AutoUpdater::get(cx)
                                && auto_updater.read(cx).status().is_updated()
                            {
                                workspace::reload(cx);
                                return;
                            }
                            auto_update::check(&Default::default(), window, cx);
                        })
                        .into_any_element(),
                )
            }
            _ => None,
        }
    }

    pub fn render_sign_in_button(&mut self, _: &mut Context<Self>) -> NativeButton {
        let client = self.client.clone();
        let workspace = self.workspace.clone();
        native_button("sign_in", "Sign In").on_click(move |_, window, cx| {
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
        })
    }

    pub fn render_user_menu_button(&mut self, cx: &mut Context<Self>) -> impl Element {
        let show_update_badge = self.update_version.read(cx).show_update_in_menu_bar();

        let user_store = self.user_store.read(cx);
        let user = user_store.current_user();

        let user_avatar = user.as_ref().map(|u| u.avatar_uri.clone());
        let user_login = user.as_ref().map(|u| u.github_login.clone());

        let is_signed_in = user.is_some();

        let has_subscription_period = user_store.subscription_period().is_some();
        let plan = user_store.plan().filter(|_| {
            // Since the user might be on the legacy free plan we filter based on whether we have a subscription period.
            has_subscription_period
        });

        let free_chip_bg = cx
            .theme()
            .colors()
            .editor_background
            .opacity(0.5)
            .blend(cx.theme().colors().text_accent.opacity(0.05));

        let pro_chip_bg = cx
            .theme()
            .colors()
            .editor_background
            .opacity(0.5)
            .blend(cx.theme().colors().text_accent.opacity(0.2));

        PopoverMenu::new("user-menu")
            .anchor(Corner::TopRight)
            .menu(move |window, cx| {
                ContextMenu::build(window, cx, |menu, _, _cx| {
                    let user_login = user_login.clone();

                    let (plan_name, label_color, bg_color) = match plan {
                        None | Some(Plan::ZedFree) => ("Free", Color::Default, free_chip_bg),
                        Some(Plan::ZedProTrial) => ("Pro Trial", Color::Accent, pro_chip_bg),
                        Some(Plan::ZedPro) => ("Pro", Color::Accent, pro_chip_bg),
                        Some(Plan::ZedStudent) => ("Student", Color::Accent, pro_chip_bg),
                    };

                    menu.when(is_signed_in, |this| {
                        this.custom_entry(
                            move |_window, _cx| {
                                let user_login = user_login.clone().unwrap_or_default();

                                h_flex()
                                    .w_full()
                                    .justify_between()
                                    .child(Label::new(user_login))
                                    .child(
                                        Chip::new(plan_name.to_string())
                                            .bg_color(bg_color)
                                            .label_color(label_color),
                                    )
                                    .into_any_element()
                            },
                            move |_, cx| {
                                cx.open_url(&zed_urls::account_url(cx));
                            },
                        )
                        .separator()
                    })
                    .when(show_update_badge, |this| {
                        this.custom_entry(
                            move |_window, _cx| {
                                h_flex()
                                    .w_full()
                                    .gap_1()
                                    .justify_between()
                                    .child(Label::new("Restart to update Zed").color(Color::Accent))
                                    .child(
                                        Icon::new(IconName::Download)
                                            .size(IconSize::Small)
                                            .color(Color::Accent),
                                    )
                                    .into_any_element()
                            },
                            move |_, cx| {
                                workspace::reload(cx);
                            },
                        )
                        .separator()
                    })
                    .action("Settings", zed_actions::OpenSettings.boxed_clone())
                    .action("Keymap", Box::new(zed_actions::OpenKeymap))
                    .action(
                        "Themes…",
                        zed_actions::theme_selector::Toggle::default().boxed_clone(),
                    )
                    .action(
                        "Icon Themes…",
                        zed_actions::icon_theme_selector::Toggle::default().boxed_clone(),
                    )
                    .action(
                        "Extensions",
                        zed_actions::Extensions::default().boxed_clone(),
                    )
                    .when(is_signed_in, |this| {
                        this.separator()
                            .action("Sign Out", client::SignOut.boxed_clone())
                    })
                })
                .into()
            })
            .map(|this| {
                if is_signed_in && TitleBarSettings::get_global(cx).show_user_picture {
                    let avatar =
                        user_avatar
                            .clone()
                            .map(|avatar| Avatar::new(avatar))
                            .map(|avatar| {
                                if show_update_badge {
                                    avatar.indicator(
                                        div()
                                            .absolute()
                                            .bottom_0()
                                            .right_0()
                                            .child(Indicator::dot().color(Color::Accent)),
                                    )
                                } else {
                                    avatar
                                }
                            });
                    this.trigger_with_tooltip(
                        ButtonLike::new("user-menu").children(avatar),
                        Tooltip::text("Toggle User Menu"),
                    )
                } else {
                    this.trigger(native_icon_button("user-menu", "chevron.down"))
                }
            })
            .anchor(gpui::Corner::TopRight)
    }
}
