use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, PathPromptOptions,
    Render, SharedString, StatefulInteractiveElement, WeakEntity,
};
use project::DirectoryLister;
use serde::Deserialize;
use service_hub::{
    ServiceArtifactKind, ServiceArtifactRef, ServiceCommandPlan, ServiceHub,
    ServiceOperationRequest, ServiceResourceRef,
};
use task::{HideStrategy, RevealStrategy, SaveStrategy, Shell, SpawnInTerminal, TaskId};
use ui::{Button, ButtonStyle, Color, Label, LabelSize, ListItem, ListItemSpacing, prelude::*};
use util::command::new_command;
use workspace::Workspace;
use workspace::item::{Item, ItemBufferKind, ItemEvent};

#[derive(Clone, Debug)]
struct AscAuthSummary {
    headline: String,
    detail: String,
    healthy: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AscAppSummary {
    id: String,
    name: String,
    bundle_id: String,
    sku: String,
    primary_locale: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AscBuildSummary {
    id: String,
    build_number: String,
    processing_state: String,
    uploaded_date: String,
    expiration_date: Option<String>,
    min_os_version: Option<String>,
}

#[derive(Clone, Debug)]
enum LoadState<T> {
    Loading,
    Ready(T),
    Error(String),
}

pub struct AppStoreConnectPage {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    auth_state: LoadState<AscAuthSummary>,
    apps_state: LoadState<Vec<AscAppSummary>>,
    selected_app_id: Option<String>,
    builds_state: LoadState<Vec<AscBuildSummary>>,
    selected_build_id: Option<String>,
}

impl AppStoreConnectPage {
    pub fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let workspace_handle = workspace.weak_handle();
        let page = cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
            workspace: workspace_handle,
            auth_state: LoadState::Loading,
            apps_state: LoadState::Loading,
            selected_app_id: None,
            builds_state: LoadState::Ready(Vec::new()),
            selected_build_id: None,
        });

        page.update(cx, |page, cx| page.refresh_overview(window, cx));
        page
    }

    fn refresh_overview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.auth_state = LoadState::Loading;
        self.apps_state = LoadState::Loading;
        self.builds_state = LoadState::Ready(Vec::new());
        self.selected_build_id = None;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let (auth_result, apps_result) = cx
                .background_spawn(async move {
                    let auth = load_auth_status().await;
                    let apps = load_apps().await;
                    (auth, apps)
                })
                .await;

            let selected_app_id = this
                .update_in(cx, |this, _window, cx| {
                    this.auth_state = match auth_result {
                        Ok(summary) => LoadState::Ready(summary),
                        Err(error) => LoadState::Error(error.to_string()),
                    };

                    match apps_result {
                        Ok(apps) => {
                            let next_selected_app_id = this
                                .selected_app_id
                                .as_ref()
                                .and_then(|selected_id| {
                                    apps.iter()
                                        .find(|app| &app.id == selected_id)
                                        .map(|app| app.id.clone())
                                })
                                .or_else(|| apps.first().map(|app| app.id.clone()));
                            this.apps_state = LoadState::Ready(apps);
                            this.selected_app_id = next_selected_app_id.clone();
                            if next_selected_app_id.is_some() {
                                this.builds_state = LoadState::Loading;
                            }
                            cx.notify();
                            next_selected_app_id
                        }
                        Err(error) => {
                            this.apps_state = LoadState::Error(error.to_string());
                            this.selected_app_id = None;
                            this.builds_state = LoadState::Ready(Vec::new());
                            cx.notify();
                            None
                        }
                    }
                })
                .ok()
                .flatten();

            if let Some(app_id) = selected_app_id {
                this.update_in(cx, |this, window, cx| {
                    this.load_builds_for_app(app_id, window, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn load_builds_for_app(&mut self, app_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(app) = self.apps().iter().find(|app| app.id == app_id).cloned() else {
            self.builds_state = LoadState::Ready(Vec::new());
            self.selected_build_id = None;
            cx.notify();
            return;
        };

        self.selected_app_id = Some(app.id.clone());
        self.selected_build_id = None;
        self.builds_state = LoadState::Loading;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let builds_result = cx
                .background_spawn(async move { load_builds(&app).await })
                .await;
            this.update_in(cx, |this, _window, cx| {
                match builds_result {
                    Ok(builds) => {
                        this.selected_build_id = builds.first().map(|build| build.id.clone());
                        this.builds_state = LoadState::Ready(builds);
                    }
                    Err(error) => {
                        this.selected_build_id = None;
                        this.builds_state = LoadState::Error(error.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_builds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app_id) = self.selected_app_id.clone() {
            self.load_builds_for_app(app_id, window, cx);
        }
    }

    fn select_app(&mut self, app_id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_app_id.as_ref() == Some(&app_id) {
            return;
        }

        self.load_builds_for_app(app_id, window, cx);
    }

    fn select_build(&mut self, build_id: String, cx: &mut Context<Self>) {
        self.selected_build_id = Some(build_id);
        cx.notify();
    }

    fn choose_upload_artifact(
        &mut self,
        artifact_kind: ServiceArtifactKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(app) = self.selected_app() else {
            self.show_portal_error(
                "Select an App Store Connect app before uploading a build.",
                cx,
            );
            return;
        };

        let prompt_label = match artifact_kind {
            ServiceArtifactKind::Ipa => "Select an .ipa file to upload",
            ServiceArtifactKind::Pkg => "Select a .pkg file to upload",
            ServiceArtifactKind::AppBundle | ServiceArtifactKind::Binary => {
                self.show_portal_error("Only .ipa and .pkg uploads are supported.", cx);
                return;
            }
        };

        let prompt = self
            .workspace
            .update(cx, |workspace, cx| {
                workspace.prompt_for_open_path(
                    PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: false,
                        prompt: Some(SharedString::from(prompt_label)),
                    },
                    DirectoryLister::Local(
                        workspace.project().clone(),
                        workspace.app_state().fs.clone(),
                    ),
                    window,
                    cx,
                )
            })
            .ok();

        let Some(prompt) = prompt else {
            return;
        };

        cx.spawn_in(window, async move |this, cx| {
            let path = match prompt.await {
                Ok(Some(mut paths)) => paths.pop(),
                Ok(None) => None,
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.show_portal_error(error.to_string(), cx);
                    })
                    .ok();
                    None
                }
            };

            let Some(path) = path else {
                return;
            };

            this.update_in(cx, |this, window, cx| {
                this.upload_artifact(app, artifact_kind, path, window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn upload_artifact(
        &mut self,
        app: AscAppSummary,
        artifact_kind: ServiceArtifactKind,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !artifact_matches_kind(path.as_path(), artifact_kind) {
            let expected = match artifact_kind {
                ServiceArtifactKind::Ipa => ".ipa",
                ServiceArtifactKind::Pkg => ".pkg",
                ServiceArtifactKind::AppBundle | ServiceArtifactKind::Binary => {
                    "supported artifact"
                }
            };
            self.show_portal_error(
                format!(
                    "The selected file does not match the expected {} artifact.",
                    expected
                ),
                cx,
            );
            return;
        }

        let request = ServiceOperationRequest {
            provider_id: "app-store-connect".to_string(),
            operation: "upload_build".to_string(),
            resource: Some(ServiceResourceRef {
                provider_id: "app-store-connect".to_string(),
                kind: "app".to_string(),
                external_id: app.id.clone(),
                label: app.name.clone(),
            }),
            artifact: Some(ServiceArtifactRef {
                kind: artifact_kind,
                path,
            }),
            input: [("wait".to_string(), "true".to_string())]
                .into_iter()
                .collect(),
        };

        match ServiceHub::default().build_operation(&request) {
            Ok(plan) => self.execute_terminal_plan(
                format!(
                    "upload-build-{}-{}",
                    app.id,
                    artifact_operation_name(artifact_kind)
                ),
                plan,
                window,
                cx,
            ),
            Err(error) => self.show_portal_error(error.to_string(), cx),
        }
    }

    fn choose_metadata_directory(
        &mut self,
        confirm: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(app) = self.selected_app() else {
            self.show_portal_error(
                "Select an App Store Connect app before running a release.",
                cx,
            );
            return;
        };
        let Some(build) = self.selected_build() else {
            self.show_portal_error("Select a build before running a release.", cx);
            return;
        };

        let prompt = self
            .workspace
            .update(cx, |workspace, cx| {
                workspace.prompt_for_open_path(
                    PathPromptOptions {
                        files: false,
                        directories: true,
                        multiple: false,
                        prompt: Some(SharedString::from(
                            "Select the metadata directory for this release",
                        )),
                    },
                    DirectoryLister::Local(
                        workspace.project().clone(),
                        workspace.app_state().fs.clone(),
                    ),
                    window,
                    cx,
                )
            })
            .ok();

        let Some(prompt) = prompt else {
            return;
        };

        let build_for_version = build.clone();
        cx.spawn_in(window, async move |this, cx| {
            let metadata_dir = match prompt.await {
                Ok(Some(mut paths)) => paths.pop(),
                Ok(None) => None,
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.show_portal_error(error.to_string(), cx);
                    })
                    .ok();
                    None
                }
            };

            let Some(metadata_dir) = metadata_dir else {
                return;
            };

            let version_result = cx
                .background_spawn(async move { load_pre_release_version(&build_for_version).await })
                .await;

            this.update_in(cx, |this, window, cx| match version_result {
                Ok(version) => {
                    this.run_release(app, build, version, metadata_dir, confirm, window, cx);
                }
                Err(error) => {
                    this.show_portal_error(
                        format!("Could not determine the selected build's app version: {error}"),
                        cx,
                    );
                }
            })
            .ok();
        })
        .detach();
    }

    fn run_release(
        &mut self,
        app: AscAppSummary,
        build: AscBuildSummary,
        version: String,
        metadata_dir: PathBuf,
        confirm: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let request = ServiceOperationRequest {
            provider_id: "app-store-connect".to_string(),
            operation: "release_run".to_string(),
            resource: Some(ServiceResourceRef {
                provider_id: "app-store-connect".to_string(),
                kind: "app".to_string(),
                external_id: app.id.clone(),
                label: app.name.clone(),
            }),
            artifact: None,
            input: [
                ("version".to_string(), version),
                ("build".to_string(), build.id.clone()),
                (
                    "metadata_dir".to_string(),
                    metadata_dir.to_string_lossy().into_owned(),
                ),
                ("dry_run".to_string(), (!confirm).to_string()),
            ]
            .into_iter()
            .collect(),
        };

        match ServiceHub::default().build_operation(&request) {
            Ok(plan) => self.execute_terminal_plan(
                format!(
                    "release-run-{}-{}-{}",
                    app.id,
                    build.id,
                    if confirm { "confirm" } else { "dry-run" }
                ),
                plan,
                window,
                cx,
            ),
            Err(error) => self.show_portal_error(error.to_string(), cx),
        }
    }

    fn execute_terminal_plan(
        &mut self,
        operation_id: String,
        plan: ServiceCommandPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace
            .update(cx, |workspace, cx| {
                let command_label = std::iter::once(plan.command.as_str())
                    .chain(plan.args.iter().map(|arg| arg.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");
                let task = SpawnInTerminal {
                    id: TaskId(format!("service-hub-{}", sanitize_for_id(&operation_id))),
                    full_label: plan.label.clone(),
                    label: plan.label,
                    command: Some(plan.command),
                    args: plan.args,
                    command_label,
                    cwd: plan.cwd,
                    env: plan.env.into_iter().collect(),
                    use_new_terminal: false,
                    allow_concurrent_runs: false,
                    reveal: RevealStrategy::Always,
                    reveal_target: task::RevealTarget::Center,
                    hide: HideStrategy::Never,
                    shell: Shell::System,
                    show_summary: true,
                    show_command: true,
                    show_rerun: true,
                    save: SaveStrategy::All,
                };

                workspace.spawn_in_terminal(task, window, cx).detach();
            })
            .ok();
    }

    fn show_portal_error(&self, message: impl Into<String>, cx: &mut Context<Self>) {
        let message = message.into();
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.show_portal_error(message, cx);
            })
            .ok();
    }

    fn apps(&self) -> &[AscAppSummary] {
        match &self.apps_state {
            LoadState::Ready(apps) => apps,
            LoadState::Loading | LoadState::Error(_) => &[],
        }
    }

    fn builds(&self) -> &[AscBuildSummary] {
        match &self.builds_state {
            LoadState::Ready(builds) => builds,
            LoadState::Loading | LoadState::Error(_) => &[],
        }
    }

    fn selected_app(&self) -> Option<AscAppSummary> {
        self.selected_app_id
            .as_ref()
            .and_then(|selected_id| self.apps().iter().find(|app| &app.id == selected_id))
            .cloned()
    }

    fn selected_build(&self) -> Option<AscBuildSummary> {
        self.selected_build_id
            .as_ref()
            .and_then(|selected_id| self.builds().iter().find(|build| &build.id == selected_id))
            .cloned()
    }

    fn render_auth_status(&self) -> AnyElement {
        match &self.auth_state {
            LoadState::Loading => Label::new("Checking App Store Connect authentication…")
                .color(Color::Muted)
                .into_any_element(),
            LoadState::Error(error) => v_flex()
                .gap_1()
                .child(
                    Label::new("App Store Connect authentication check failed").color(Color::Error),
                )
                .child(
                    Label::new(error.clone())
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                )
                .into_any_element(),
            LoadState::Ready(summary) => v_flex()
                .gap_1()
                .child(
                    Label::new(summary.headline.clone()).color(if summary.healthy {
                        Color::Success
                    } else {
                        Color::Warning
                    }),
                )
                .child(
                    Label::new(summary.detail.clone())
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                )
                .into_any_element(),
        }
    }

    fn render_apps_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let apps_list = match &self.apps_state {
            LoadState::Loading => v_flex()
                .gap_2()
                .child(Label::new("Loading apps…").color(Color::Muted))
                .into_any_element(),
            LoadState::Error(error) => v_flex()
                .gap_2()
                .child(Label::new("Could not load apps").color(Color::Error))
                .child(
                    Label::new(error.clone())
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                )
                .into_any_element(),
            LoadState::Ready(apps) if apps.is_empty() => v_flex()
                .gap_2()
                .child(Label::new("No apps were returned by App Store Connect").color(Color::Muted))
                .into_any_element(),
            LoadState::Ready(apps) => v_flex()
                .gap_1()
                .children(apps.iter().map(|app| {
                    let selected = self.selected_app_id.as_ref() == Some(&app.id);
                    ListItem::new(format!("asc-app-{}", app.id))
                        .inset(true)
                        .spacing(ListItemSpacing::Sparse)
                        .toggle_state(selected)
                        .child(
                            v_flex()
                                .gap_0p5()
                                .child(Label::new(app.name.clone()).single_line())
                                .child(
                                    Label::new(format!("{} · {}", app.bundle_id, app.id))
                                        .size(LabelSize::Small)
                                        .color(Color::Muted)
                                        .single_line(),
                                ),
                        )
                        .on_click(cx.listener({
                            let app_id = app.id.clone();
                            move |this, _, window, cx| {
                                this.select_app(app_id.clone(), window, cx);
                            }
                        }))
                }))
                .into_any_element(),
        };

        v_flex()
            .w_80()
            .min_w(rems(22.))
            .gap_2()
            .child(
                Label::new("Apps")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                v_flex()
                    .id("asc-apps-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(apps_list),
            )
    }

    fn render_builds_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let builds_list = match &self.builds_state {
            LoadState::Loading => v_flex()
                .gap_2()
                .child(Label::new("Loading builds…").color(Color::Muted))
                .into_any_element(),
            LoadState::Error(error) => v_flex()
                .gap_2()
                .child(Label::new("Could not load builds").color(Color::Error))
                .child(
                    Label::new(error.clone())
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                )
                .into_any_element(),
            LoadState::Ready(builds) if self.selected_app_id.is_none() => v_flex()
                .gap_2()
                .child(Label::new("Select an app to load its builds").color(Color::Muted))
                .into_any_element(),
            LoadState::Ready(builds) if builds.is_empty() => v_flex()
                .gap_2()
                .child(
                    Label::new("No builds were returned for the selected app").color(Color::Muted),
                )
                .into_any_element(),
            LoadState::Ready(builds) => v_flex()
                .gap_1()
                .children(builds.iter().map(|build| {
                    let selected = self.selected_build_id.as_ref() == Some(&build.id);
                    let subtitle = match &build.expiration_date {
                        Some(expiration_date) => format!(
                            "{} · uploaded {} · expires {}",
                            build.processing_state, build.uploaded_date, expiration_date
                        ),
                        None => format!(
                            "{} · uploaded {}",
                            build.processing_state, build.uploaded_date
                        ),
                    };

                    ListItem::new(format!("asc-build-{}", build.id))
                        .inset(true)
                        .spacing(ListItemSpacing::Sparse)
                        .toggle_state(selected)
                        .child(
                            v_flex()
                                .gap_0p5()
                                .child(
                                    Label::new(format!("Build {}", build.build_number))
                                        .single_line(),
                                )
                                .child(
                                    Label::new(subtitle)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted)
                                        .single_line(),
                                ),
                        )
                        .on_click(cx.listener({
                            let build_id = build.id.clone();
                            move |this, _, _window, cx| {
                                this.select_build(build_id.clone(), cx);
                            }
                        }))
                }))
                .into_any_element(),
        };

        v_flex()
            .flex_1()
            .gap_2()
            .child(
                Label::new("Builds")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                v_flex()
                    .id("asc-builds-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(builds_list),
            )
    }
}

impl EventEmitter<ItemEvent> for AppStoreConnectPage {}

impl Focusable for AppStoreConnectPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for AppStoreConnectPage {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "App Store Connect".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("App Store Connect Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event)
    }
}

impl Render for AppStoreConnectPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_app = self.selected_app();
        let selected_build = self.selected_build();

        v_flex()
            .size_full()
            .gap_4()
            .p_4()
            .bg(cx.theme().colors().editor_background)
            .child(
                v_flex()
                    .gap_2()
                    .child(Label::new("App Store Connect").size(LabelSize::Large))
                    .child(
                        Label::new(
                            "Browse apps and builds, then hand off uploads and releases to the real asc CLI.",
                        )
                        .color(Color::Muted),
                    )
                    .child(self.render_auth_status()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("asc-refresh-overview", "Refresh Apps")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_overview(window, cx);
                            })),
                    )
                    .child(
                        Button::new("asc-refresh-builds", "Refresh Builds")
                            .disabled(selected_app.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_builds(window, cx);
                            })),
                    )
                    .child(
                        Button::new("asc-upload-ipa", "Upload IPA…")
                            .disabled(selected_app.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_upload_artifact(ServiceArtifactKind::Ipa, window, cx);
                            })),
                    )
                    .child(
                        Button::new("asc-upload-pkg", "Upload PKG…")
                            .disabled(selected_app.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_upload_artifact(ServiceArtifactKind::Pkg, window, cx);
                            })),
                    )
                    .child(
                        Button::new("asc-plan-release", "Plan Release…")
                            .disabled(selected_app.is_none() || selected_build.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_metadata_directory(false, window, cx);
                            })),
                    )
                    .child(
                        Button::new("asc-run-release", "Submit Release…")
                            .style(ButtonStyle::Filled)
                            .disabled(selected_app.is_none() || selected_build.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_metadata_directory(true, window, cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .flex_1()
                    .child(self.render_apps_panel(cx))
                    .child(self.render_builds_panel(cx)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .pt_2()
                    .border_t_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Label::new(match selected_app {
                            Some(ref app) => format!("Selected app: {} ({})", app.name, app.bundle_id),
                            None => "Selected app: none".to_string(),
                        })
                        .size(LabelSize::Small),
                    )
                    .child(
                        Label::new(match selected_build {
                            Some(ref build) => {
                                format!("Selected build: {} ({})", build.build_number, build.id)
                            }
                            None => "Selected build: none".to_string(),
                        })
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    ),
            )
    }
}

#[derive(Deserialize)]
struct AscAuthStatusResponse {
    #[serde(rename = "storageBackend")]
    storage_backend: String,
    credentials: Vec<AscCredential>,
}

#[derive(Deserialize)]
struct AscCredential {
    name: String,
    #[serde(rename = "isDefault")]
    is_default: bool,
    validation: Option<String>,
}

#[derive(Deserialize)]
struct AscAppsResponse {
    data: Vec<AscAppRecord>,
}

#[derive(Deserialize)]
struct AscAppRecord {
    id: String,
    attributes: AscAppAttributes,
}

#[derive(Deserialize)]
struct AscAppAttributes {
    name: String,
    #[serde(rename = "bundleId")]
    bundle_id: String,
    sku: String,
    #[serde(rename = "primaryLocale")]
    primary_locale: Option<String>,
}

#[derive(Deserialize)]
struct AscBuildsResponse {
    data: Vec<AscBuildRecord>,
}

#[derive(Deserialize)]
struct AscBuildRecord {
    id: String,
    attributes: AscBuildAttributes,
}

#[derive(Deserialize)]
struct AscBuildAttributes {
    version: String,
    #[serde(rename = "uploadedDate")]
    uploaded_date: String,
    #[serde(rename = "expirationDate")]
    expiration_date: Option<String>,
    #[serde(rename = "processingState")]
    processing_state: String,
    #[serde(rename = "minOsVersion")]
    min_os_version: Option<String>,
}

#[derive(Deserialize)]
struct AscPreReleaseVersionResponse {
    data: AscPreReleaseVersionRecord,
}

#[derive(Deserialize)]
struct AscPreReleaseVersionRecord {
    attributes: AscPreReleaseVersionAttributes,
}

#[derive(Deserialize)]
struct AscPreReleaseVersionAttributes {
    version: String,
}

async fn load_auth_status() -> Result<AscAuthSummary> {
    let response: AscAuthStatusResponse = run_json_operation(ServiceOperationRequest {
        provider_id: "app-store-connect".to_string(),
        operation: "auth_status".to_string(),
        resource: None,
        artifact: None,
        input: Default::default(),
    })
    .await?;

    let default_credential = response
        .credentials
        .iter()
        .find(|credential| credential.is_default);
    let validation = default_credential
        .and_then(|credential| credential.validation.as_deref())
        .unwrap_or("unknown");
    let credential_name = default_credential
        .map(|credential| credential.name.as_str())
        .unwrap_or("No default credential");

    Ok(AscAuthSummary {
        headline: format!(
            "{} via {}",
            validation_label(validation),
            response.storage_backend
        ),
        detail: format!("Default credential: {}", credential_name),
        healthy: validation.eq_ignore_ascii_case("works"),
    })
}

async fn load_apps() -> Result<Vec<AscAppSummary>> {
    let response: AscAppsResponse = run_json_operation(ServiceOperationRequest {
        provider_id: "app-store-connect".to_string(),
        operation: "list_apps".to_string(),
        resource: None,
        artifact: None,
        input: [("paginate".to_string(), "true".to_string())]
            .into_iter()
            .collect(),
    })
    .await?;

    let mut apps = response
        .data
        .into_iter()
        .map(|app| AscAppSummary {
            id: app.id,
            name: app.attributes.name,
            bundle_id: app.attributes.bundle_id,
            sku: app.attributes.sku,
            primary_locale: app.attributes.primary_locale,
        })
        .collect::<Vec<_>>();
    apps.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.bundle_id.cmp(&right.bundle_id))
    });
    Ok(apps)
}

async fn load_builds(app: &AscAppSummary) -> Result<Vec<AscBuildSummary>> {
    let response: AscBuildsResponse = run_json_operation(ServiceOperationRequest {
        provider_id: "app-store-connect".to_string(),
        operation: "list_builds".to_string(),
        resource: Some(ServiceResourceRef {
            provider_id: "app-store-connect".to_string(),
            kind: "app".to_string(),
            external_id: app.id.clone(),
            label: app.name.clone(),
        }),
        artifact: None,
        input: [
            ("paginate".to_string(), "true".to_string()),
            ("sort".to_string(), "-uploadedDate".to_string()),
        ]
        .into_iter()
        .collect(),
    })
    .await?;

    Ok(response
        .data
        .into_iter()
        .map(|build| AscBuildSummary {
            id: build.id,
            build_number: build.attributes.version,
            processing_state: build.attributes.processing_state,
            uploaded_date: build.attributes.uploaded_date,
            expiration_date: build.attributes.expiration_date,
            min_os_version: build.attributes.min_os_version,
        })
        .collect())
}

async fn load_pre_release_version(build: &AscBuildSummary) -> Result<String> {
    let response: AscPreReleaseVersionResponse = run_json_operation(ServiceOperationRequest {
        provider_id: "app-store-connect".to_string(),
        operation: "build_pre_release_version".to_string(),
        resource: Some(ServiceResourceRef {
            provider_id: "app-store-connect".to_string(),
            kind: "build".to_string(),
            external_id: build.id.clone(),
            label: build.build_number.clone(),
        }),
        artifact: None,
        input: Default::default(),
    })
    .await?;

    Ok(response.data.attributes.version)
}

async fn run_json_operation<T>(request: ServiceOperationRequest) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let plan = ServiceHub::default()
        .build_operation(&request)
        .map_err(|error| anyhow!(error.to_string()))?;
    let output = run_command_plan(plan).await?;
    serde_json::from_slice(&output)
        .with_context(|| "Failed to parse JSON output from App Store Connect CLI")
}

async fn run_command_plan(plan: ServiceCommandPlan) -> Result<Vec<u8>> {
    let mut command = new_command(&plan.command);
    command.args(&plan.args);
    if let Some(cwd) = plan.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in plan.env {
        command.env(key, value);
    }

    let output = command
        .output()
        .await
        .with_context(|| format!("Failed to start `{}`", plan.command))?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(anyhow!(
            "{}",
            if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("`{}` exited unsuccessfully", plan.command)
            }
        ))
    }
}

fn validation_label(validation: &str) -> String {
    if validation.eq_ignore_ascii_case("works") {
        "Authentication validated".to_string()
    } else {
        format!("Authentication status: {validation}")
    }
}

fn artifact_matches_kind(path: &Path, artifact_kind: ServiceArtifactKind) -> bool {
    match artifact_kind {
        ServiceArtifactKind::Ipa => path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ipa")),
        ServiceArtifactKind::Pkg => path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pkg")),
        ServiceArtifactKind::AppBundle | ServiceArtifactKind::Binary => false,
    }
}

fn artifact_operation_name(artifact_kind: ServiceArtifactKind) -> &'static str {
    match artifact_kind {
        ServiceArtifactKind::Ipa => "ipa",
        ServiceArtifactKind::Pkg => "pkg",
        ServiceArtifactKind::AppBundle => "app-bundle",
        ServiceArtifactKind::Binary => "binary",
    }
}

fn sanitize_for_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{artifact_matches_kind, sanitize_for_id};
    use service_hub::ServiceArtifactKind;
    use std::path::Path;

    #[test]
    fn matches_supported_artifact_extensions() {
        assert!(artifact_matches_kind(
            Path::new("/tmp/App.ipa"),
            ServiceArtifactKind::Ipa
        ));
        assert!(artifact_matches_kind(
            Path::new("/tmp/App.PKG"),
            ServiceArtifactKind::Pkg
        ));
        assert!(!artifact_matches_kind(
            Path::new("/tmp/App.app"),
            ServiceArtifactKind::Ipa
        ));
    }

    #[test]
    fn sanitizes_terminal_item_ids() {
        assert_eq!(
            sanitize_for_id("release run / build"),
            "release-run---build"
        );
    }
}
