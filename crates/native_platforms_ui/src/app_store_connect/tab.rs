use editor::Editor;
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, Render, SharedString, Task,
    Window,
};
use native_platforms::apple::app_store_connect::{
    self, App as AscApp, AscStatus, AuthStatus, BetaGroup, BetaTester, Build,
};
use ui::prelude::*;
use workspace::item::{Item, ItemEvent, TabContentParams};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupStep {
    NotInstalled,
    Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Setup(SetupStep),
    Apps,
    TestFlight,
}

pub struct AppStoreConnectTab {
    focus_handle: FocusHandle,
    view_mode: ViewMode,
    is_loading: bool,
    error_message: Option<String>,

    auth_status: AuthStatus,

    profile_name_editor: Entity<Editor>,
    key_id_editor: Entity<Editor>,
    issuer_id_editor: Entity<Editor>,
    private_key_path_editor: Entity<Editor>,

    apps: Vec<AscApp>,
    selected_app: Option<AscApp>,

    builds: Vec<Build>,
    beta_groups: Vec<BetaGroup>,
    beta_testers: Vec<BetaTester>,

    load_task: Option<Task<()>>,
}

impl AppStoreConnectTab {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let status = app_store_connect::get_status();
        let auth_status = app_store_connect::get_auth_status();

        let view_mode = match status {
            AscStatus::NotInstalled => ViewMode::Setup(SetupStep::NotInstalled),
            AscStatus::InstalledNotAuthenticated => ViewMode::Setup(SetupStep::Login),
            AscStatus::Authenticated => ViewMode::Apps,
        };

        let profile_name_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("default", window, cx);
            editor
        });

        let key_id_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Your API Key ID", window, cx);
            editor
        });

        let issuer_id_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Your Issuer ID", window, cx);
            editor
        });

        let private_key_path_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("/path/to/AuthKey_XXX.p8", window, cx);
            editor
        });

        let mut tab = Self {
            focus_handle,
            view_mode,
            is_loading: false,
            error_message: None,
            auth_status,
            profile_name_editor,
            key_id_editor,
            issuer_id_editor,
            private_key_path_editor,
            apps: Vec::new(),
            selected_app: None,
            builds: Vec::new(),
            beta_groups: Vec::new(),
            beta_testers: Vec::new(),
            load_task: None,
        };

        if matches!(status, AscStatus::Authenticated) {
            tab.load_apps(cx);
        }

        tab
    }

    fn install_asc(&mut self, cx: &mut Context<Self>) {
        self.is_loading = true;
        self.error_message = None;
        cx.notify();

        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async { app_store_connect::install_asc() })
                .await;

            this.update(cx, |this, cx| {
                this.is_loading = false;
                match result {
                    Ok(()) => {
                        this.view_mode = ViewMode::Setup(SetupStep::Login);
                        this.error_message = None;
                    }
                    Err(e) => {
                        this.error_message = Some(format!("Installation failed: {}", e));
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn authenticate(&mut self, cx: &mut Context<Self>) {
        let profile_name = self.profile_name_editor.read(cx).text(cx).trim().to_string();
        let key_id = self.key_id_editor.read(cx).text(cx).trim().to_string();
        let issuer_id = self.issuer_id_editor.read(cx).text(cx).trim().to_string();
        let private_key_path = self
            .private_key_path_editor
            .read(cx)
            .text(cx)
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();

        if key_id.is_empty() || issuer_id.is_empty() || private_key_path.is_empty() {
            self.error_message =
                Some("Please fill in Key ID, Issuer ID, and Private Key Path".to_string());
            cx.notify();
            return;
        }

        if !std::path::Path::new(&private_key_path).exists() {
            self.error_message = Some(format!(
                "Private key file not found: {}",
                private_key_path
            ));
            cx.notify();
            return;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&private_key_path) {
                let mode = metadata.permissions().mode();
                if mode & 0o077 != 0 {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o600);
                    if let Err(e) = std::fs::set_permissions(&private_key_path, perms) {
                        self.error_message = Some(format!(
                            "Failed to fix key file permissions: {}. Run: chmod 600 \"{}\"",
                            e, private_key_path
                        ));
                        cx.notify();
                        return;
                    }
                }
            }
        }

        let profile_name = if profile_name.is_empty() {
            "default".to_string()
        } else {
            profile_name
        };

        self.is_loading = true;
        self.error_message = None;
        cx.notify();

        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    app_store_connect::authenticate(
                        &profile_name,
                        &key_id,
                        &issuer_id,
                        &private_key_path,
                    )
                })
                .await;

            this.update(cx, |this, cx| {
                this.is_loading = false;
                match result {
                    Ok(()) => {
                        this.view_mode = ViewMode::Apps;
                        this.error_message = None;
                        this.auth_status = app_store_connect::get_auth_status();
                        this.load_apps(cx);
                    }
                    Err(e) => {
                        this.error_message = Some(format!("Authentication failed: {}", e));
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn load_apps(&mut self, cx: &mut Context<Self>) {
        self.is_loading = true;
        cx.notify();

        self.load_task = Some(cx.spawn(async move |this, cx| {
            let apps = cx
                .background_spawn(async { app_store_connect::list_apps().unwrap_or_default() })
                .await;

            this.update(cx, |this, cx| {
                this.apps = apps;
                this.is_loading = false;
                cx.notify();
            })
            .ok();
        }));
    }

    fn logout(&mut self, cx: &mut Context<Self>) {
        self.is_loading = true;
        self.error_message = None;
        cx.notify();

        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async { app_store_connect::logout() })
                .await;

            this.update(cx, |this, cx| {
                this.is_loading = false;
                match result {
                    Ok(()) => {
                        this.view_mode = ViewMode::Setup(SetupStep::Login);
                        this.auth_status = AuthStatus::default();
                        this.apps.clear();
                        this.error_message = None;
                    }
                    Err(e) => {
                        this.error_message = Some(format!("Logout failed: {}", e));
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn load_app_details(&mut self, app: &AscApp, cx: &mut Context<Self>) {
        self.selected_app = Some(app.clone());
        self.view_mode = ViewMode::TestFlight;
        self.is_loading = true;
        cx.notify();

        let app_id = app.id.clone();

        self.load_task = Some(cx.spawn(async move |this, cx| {
            let (builds, groups, testers) = cx
                .background_spawn(async move {
                    let builds = app_store_connect::list_builds(&app_id).unwrap_or_default();
                    let groups = app_store_connect::list_beta_groups(&app_id).unwrap_or_default();
                    let testers = app_store_connect::list_beta_testers(&app_id).unwrap_or_default();
                    (builds, groups, testers)
                })
                .await;

            this.update(cx, |this, cx| {
                this.builds = builds;
                this.beta_groups = groups;
                this.beta_testers = testers;
                this.is_loading = false;
                cx.notify();
            })
            .ok();
        }));
    }

    fn render_not_installed(&self, cx: &Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_6()
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::CloudDownload)
                            .size(IconSize::XLarge)
                            .color(Color::Accent),
                    )
                    .child(Label::new("App Store Connect").size(LabelSize::Large))
                    .child(
                        Label::new("Install the ASC CLI to connect to App Store Connect")
                            .color(Color::Muted),
                    ),
            )
            .when_some(self.error_message.as_ref(), |this, error| {
                this.child(
                    div()
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .bg(cx.theme().status().error_background)
                        .child(Label::new(error.clone()).color(Color::Error)),
                )
            })
            .child(
                v_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        Button::new("install", "Install ASC CLI")
                            .style(ButtonStyle::Filled)
                            .disabled(self.is_loading)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.install_asc(cx);
                            })),
                    )
                    .when(self.is_loading, |this| {
                        this.child(Label::new("Installing...").color(Color::Muted))
                    }),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .pt_4()
                    .child(
                        Label::new("Or install manually:")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new("brew tap rudrankriyam/tap && brew install asc")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
    }

    fn render_login(&self, cx: &Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_6()
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::LockOutlined)
                            .size(IconSize::XLarge)
                            .color(Color::Accent),
                    )
                    .child(Label::new("Connect to App Store").size(LabelSize::Large))
                    .child(
                        Label::new("Enter your App Store Connect API credentials")
                            .color(Color::Muted),
                    ),
            )
            .when_some(self.error_message.as_ref(), |this, error| {
                this.child(
                    div()
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .bg(cx.theme().status().error_background)
                        .child(Label::new(error.clone()).color(Color::Error)),
                )
            })
            .child(
                v_flex()
                    .w(px(400.0))
                    .gap_3()
                    .child(
                        div()
                            .id("api-keys-link")
                            .cursor_pointer()
                            .on_click(cx.listener(|_, _, _, _| {
                                let _ = app_store_connect::open_api_keys_page();
                            }))
                            .child(
                                Label::new("Get your API credentials from App Store Connect →")
                                    .size(LabelSize::Small)
                                    .color(Color::Accent),
                            ),
                    )
                    .child(self.render_editor_field("Key ID", &self.key_id_editor, cx))
                    .child(self.render_editor_field("Issuer ID", &self.issuer_id_editor, cx))
                    .child(self.render_editor_field(
                        "Private Key Path (.p8)",
                        &self.private_key_path_editor,
                        cx,
                    ))
                    .child(self.render_editor_field(
                        "Profile Name (optional)",
                        &self.profile_name_editor,
                        cx,
                    )),
            )
            .child(
                Button::new("login", "Connect")
                    .style(ButtonStyle::Filled)
                    .disabled(self.is_loading)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.authenticate(cx);
                    })),
            )
            .when(self.is_loading, |this| {
                this.child(Label::new("Authenticating...").color(Color::Muted))
            })
    }

    fn render_editor_field(
        &self,
        label: impl Into<SharedString>,
        editor: &Entity<Editor>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(
                Label::new(label.into())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().colors().border)
                    .bg(cx.theme().colors().editor_background)
                    .child(editor.clone()),
            )
    }

    fn render_apps_list(&self, cx: &Context<Self>) -> impl IntoElement {
        let profile_name = self
            .auth_status
            .profile_name
            .clone()
            .unwrap_or_else(|| "default".to_string());

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .w_full()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .justify_between()
                    .child(Label::new("Your Apps").size(LabelSize::Large))
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                Label::new(format!("Profile: {}", profile_name))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                Button::new("logout", "Log out")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.logout(cx);
                                    })),
                            ),
                    ),
            )
            .when(self.is_loading, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new("Loading...").color(Color::Muted)),
                )
            })
            .when(!self.is_loading && self.apps.is_empty(), |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new("No apps found").color(Color::Muted)),
                )
            })
            .when(!self.is_loading && !self.apps.is_empty(), |this| {
                this.child(
                    v_flex()
                        .id("apps-list")
                        .flex_1()
                        .p_4()
                        .gap_2()
                        .overflow_y_scroll()
                        .children(self.apps.iter().enumerate().map(|(ix, app)| {
                            let app_clone = app.clone();
                            div()
                                .id(("app-item", ix))
                                .w_full()
                                .p_3()
                                .rounded_md()
                                .border_1()
                                .border_color(cx.theme().colors().border)
                                .hover(|this| this.bg(cx.theme().colors().element_hover))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.load_app_details(&app_clone, cx);
                                }))
                                .child(
                                    h_flex()
                                        .gap_3()
                                        .child(
                                            div()
                                                .w_10()
                                                .h_10()
                                                .rounded_lg()
                                                .bg(cx.theme().colors().element_background)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    Icon::new(IconName::Globe).size(IconSize::Medium),
                                                ),
                                        )
                                        .child(
                                            v_flex()
                                                .child(
                                                    Label::new(app.name.clone())
                                                        .size(LabelSize::Default),
                                                )
                                                .child(
                                                    Label::new(app.bundle_id.clone())
                                                        .size(LabelSize::Small)
                                                        .color(Color::Muted),
                                                ),
                                        ),
                                )
                        })),
                )
            })
    }

    fn render_testflight(&self, cx: &Context<Self>) -> impl IntoElement {
        let app_name = self
            .selected_app
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "App".to_string());

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .w_full()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .gap_2()
                    .child(
                        Button::new("back", "←")
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.view_mode = ViewMode::Apps;
                                this.selected_app = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Label::new(format!("{} - TestFlight", app_name)).size(LabelSize::Large),
                    ),
            )
            .when(self.is_loading, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new("Loading...").color(Color::Muted)),
                )
            })
            .when(!self.is_loading, |this| {
                this.child(
                    h_flex()
                        .flex_1()
                        .overflow_hidden()
                        .child(self.render_builds_section(cx))
                        .child(self.render_groups_section(cx))
                        .child(self.render_testers_section(cx)),
                )
            })
    }

    fn render_builds_section(&self, cx: &Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .border_r_1()
            .border_color(cx.theme().colors().border)
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Label::new("Builds")
                            .size(LabelSize::Default)
                            .color(Color::Muted),
                    ),
            )
            .child(
                v_flex()
                    .id("builds-list")
                    .flex_1()
                    .p_2()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(self.builds.iter().map(|build| {
                        h_flex()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Box)
                                    .size(IconSize::Small)
                                    .color(match build.processing_state.as_str() {
                                        "VALID" => Color::Success,
                                        "PROCESSING" => Color::Warning,
                                        _ => Color::Muted,
                                    }),
                            )
                            .child(
                                v_flex()
                                    .child(
                                        Label::new(format!("v{}", build.version))
                                            .size(LabelSize::Small),
                                    )
                                    .child(
                                        Label::new(build.processing_state.clone())
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                            )
                    })),
            )
    }

    fn render_groups_section(&self, cx: &Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .border_r_1()
            .border_color(cx.theme().colors().border)
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Label::new("Beta Groups")
                            .size(LabelSize::Default)
                            .color(Color::Muted),
                    ),
            )
            .child(
                v_flex()
                    .id("beta-groups-list")
                    .flex_1()
                    .p_2()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(self.beta_groups.iter().map(|group| {
                        h_flex()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .gap_2()
                            .child(
                                Icon::new(IconName::UserGroup)
                                    .size(IconSize::Small)
                                    .color(if group.is_internal {
                                        Color::Accent
                                    } else {
                                        Color::Muted
                                    }),
                            )
                            .child(
                                v_flex()
                                    .child(Label::new(group.name.clone()).size(LabelSize::Small))
                                    .child(
                                        Label::new(if group.is_internal {
                                            "Internal"
                                        } else {
                                            "External"
                                        })
                                        .size(LabelSize::XSmall)
                                        .color(Color::Muted),
                                    ),
                            )
                    })),
            )
    }

    fn render_testers_section(&self, cx: &Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Label::new("Beta Testers")
                            .size(LabelSize::Default)
                            .color(Color::Muted),
                    ),
            )
            .child(
                v_flex()
                    .id("beta-testers-list")
                    .flex_1()
                    .p_2()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(self.beta_testers.iter().map(|tester| {
                        let name = match (&tester.first_name, &tester.last_name) {
                            (Some(first), Some(last)) => format!("{} {}", first, last),
                            (Some(first), None) => first.clone(),
                            (None, Some(last)) => last.clone(),
                            (None, None) => tester.email.clone(),
                        };

                        h_flex()
                            .w_full()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .gap_2()
                            .child(
                                Icon::new(IconName::Person)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                v_flex()
                                    .child(Label::new(name).size(LabelSize::Small))
                                    .child(
                                        Label::new(tester.email.clone())
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                            )
                    })),
            )
    }
}

impl Focusable for AppStoreConnectTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<ItemEvent> for AppStoreConnectTab {}

impl Render for AppStoreConnectTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("AppStoreConnectTab")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .map(|this| match self.view_mode {
                ViewMode::Setup(SetupStep::NotInstalled) => {
                    this.child(self.render_not_installed(cx))
                }
                ViewMode::Setup(SetupStep::Login) => this.child(self.render_login(cx)),
                ViewMode::Apps => this.child(self.render_apps_list(cx)),
                ViewMode::TestFlight => this.child(self.render_testflight(cx)),
            })
    }
}

impl Item for AppStoreConnectTab {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "App Store Connect".into()
    }

    fn tab_icon(&self, _: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::CloudDownload).size(IconSize::Small))
    }

    fn to_item_events(event: &Self::Event, mut f: impl FnMut(ItemEvent)) {
        f(*event);
    }

    fn tab_content(
        &self,
        params: TabContentParams,
        _window: &Window,
        _cx: &App,
    ) -> gpui::AnyElement {
        let color = params.text_color();
        let text = self.tab_content_text(params.detail.unwrap_or(0), _cx);

        Label::new(text).color(color).into_any_element()
    }
}
