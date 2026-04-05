use anyhow::Result;
use gpui::{App, Context, ScrollHandle, SharedString, Window};
use project::DirectoryLister;
use serde::Deserialize;
use service_hub::{
    ServiceInputKind, ServiceOperationRequest, ServiceProviderDescriptor, ServiceResourceRef,
};
use ui::{
    AnyElement, Button, ButtonLike, ButtonSize, ButtonStyle, Checkbox, Color, ContextMenu,
    IconButton, IconName, IconSize, Indicator, Label, LabelSize, PopoverMenu, Severity, TintColor,
    Tooltip, WithScrollbar, h_flex, prelude::*, v_flex,
};
use workspace_chrome::SidebarRow;

use crate::{
    app_store_connect_auth::{
        AscAuthSummary, ServiceAuthFieldState, ServiceAuthFormState, load_auth_status,
    },
    command_runner::{run_auth_action, run_json_operation},
    services_page::ServicesPage,
    services_provider::{ServiceResourceMenuEntry, ServiceResourceMenuModel, ServicesPageState},
};

pub(crate) const APP_STORE_CONNECT_PROVIDER_ID: &str = "app-store-connect";

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

pub(crate) struct AppStoreConnectWorkspaceProvider {
    descriptor: ServiceProviderDescriptor,
    auth_form: ServiceAuthFormState,
    auth_state: LoadState<AscAuthSummary>,
    apps_state: LoadState<Vec<AscAppSummary>>,
    builds_state: LoadState<Vec<AscBuildSummary>>,
    builds_scroll_handle: ScrollHandle,
}

impl AppStoreConnectWorkspaceProvider {
    // Failure modes:
    // - Authentication checks fail or return partial data.
    // - App listing fails or returns no apps, leaving the shell without a resource selection.
    // - A selected app disappears between refreshes and the shell must recover cleanly.
    // - Build loading fails independently from auth or app loading.
    pub fn new(descriptor: ServiceProviderDescriptor, window: &mut Window, cx: &mut App) -> Self {
        Self {
            auth_form: ServiceAuthFormState::new(&descriptor, window, cx),
            descriptor,
            auth_state: LoadState::Loading,
            apps_state: LoadState::Loading,
            builds_state: LoadState::Ready(Vec::new()),
            builds_scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn descriptor(&self) -> &ServiceProviderDescriptor {
        &self.descriptor
    }

    pub fn normalize_state(&self, state: &mut ServicesPageState) {
        if !self
            .descriptor
            .shell
            .navigation_items
            .iter()
            .any(|item| item.id == state.navigation_id)
        {
            state.navigation_id = self.descriptor.shell.default_navigation_item_id.clone();
        }

        if let LoadState::Ready(apps) = &self.apps_state {
            if !apps
                .iter()
                .any(|app| Some(app.id.as_str()) == state.selected_resource_id.as_deref())
            {
                state.selected_resource_id = apps.first().map(|app| app.id.clone());
            }
        }
    }

    pub fn refresh(
        &mut self,
        _state: &mut ServicesPageState,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) {
        self.auth_state = LoadState::Loading;
        self.apps_state = LoadState::Loading;
        self.builds_state = LoadState::Ready(Vec::new());
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
                .update_in(cx, |page, _window, cx| {
                    page.with_provider_mut(APP_STORE_CONNECT_PROVIDER_ID, |pane, state| {
                        let Some(provider) = pane.as_app_store_connect_mut() else {
                            return None;
                        };

                        provider.auth_state = match auth_result {
                            Ok(summary) => LoadState::Ready(summary),
                            Err(error) => LoadState::Error(error.to_string()),
                        };

                        match apps_result {
                            Ok(apps) => {
                                let next_selected_app_id = state
                                    .selected_resource_id
                                    .as_ref()
                                    .and_then(|selected_id| {
                                        apps.iter()
                                            .find(|app| &app.id == selected_id)
                                            .map(|app| app.id.clone())
                                    })
                                    .or_else(|| apps.first().map(|app| app.id.clone()));

                                provider.apps_state = LoadState::Ready(apps);
                                state.selected_resource_id = next_selected_app_id.clone();
                                provider.builds_state = if next_selected_app_id.is_some() {
                                    LoadState::Loading
                                } else {
                                    LoadState::Ready(Vec::new())
                                };
                                cx.notify();
                                next_selected_app_id
                            }
                            Err(error) => {
                                provider.apps_state = LoadState::Error(error.to_string());
                                state.selected_resource_id = None;
                                provider.builds_state = LoadState::Ready(Vec::new());
                                cx.notify();
                                None
                            }
                        }
                    })
                    .flatten()
                })
                .ok()
                .flatten();

            if let Some(app_id) = selected_app_id {
                this.update_in(cx, |page, window, cx| {
                    page.with_provider_mut(APP_STORE_CONNECT_PROVIDER_ID, |pane, state| {
                        let Some(provider) = pane.as_app_store_connect_mut() else {
                            return;
                        };
                        provider.load_builds_for_app(state, app_id, window, cx);
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn resource_menu(&self, state: &ServicesPageState) -> Option<ServiceResourceMenuModel> {
        let resource_kind = self.descriptor.shell.resource_kind.as_ref()?;
        let current_label = match &self.apps_state {
            LoadState::Loading => format!("Loading {}…", resource_kind.plural_label),
            LoadState::Error(_) => format!("Select {}", resource_kind.singular_label),
            LoadState::Ready(apps) if apps.is_empty() => {
                format!("No {}", resource_kind.plural_label)
            }
            LoadState::Ready(apps) => state
                .selected_resource_id
                .as_ref()
                .and_then(|selected_id| apps.iter().find(|app| &app.id == selected_id))
                .map(|app| app.name.clone())
                .unwrap_or_else(|| format!("Select {}", resource_kind.singular_label)),
        };

        Some(ServiceResourceMenuModel {
            singular_label: resource_kind.singular_label.clone(),
            current_label,
            entries: self
                .apps()
                .iter()
                .map(|app| ServiceResourceMenuEntry {
                    id: app.id.clone(),
                    label: app.name.clone(),
                    detail: Some(app.bundle_id.clone()),
                })
                .collect(),
            disabled: matches!(self.apps_state, LoadState::Loading) || self.apps().is_empty(),
        })
    }

    pub fn select_resource(
        &mut self,
        state: &mut ServicesPageState,
        resource_id: String,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) {
        if state.selected_resource_id.as_ref() == Some(&resource_id) {
            return;
        }

        self.load_builds_for_app(state, resource_id, window, cx);
    }

    pub fn render_section(
        &self,
        state: &ServicesPageState,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) -> AnyElement {
        match state.navigation_id.as_str() {
            "builds" => self
                .render_builds_content(state, window, cx)
                .into_any_element(),
            _ => self
                .render_overview_content(state, window, cx)
                .into_any_element(),
        }
    }

    fn load_builds_for_app(
        &mut self,
        state: &mut ServicesPageState,
        app_id: String,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) {
        let Some(app) = self.apps().iter().find(|app| app.id == app_id).cloned() else {
            self.builds_state = LoadState::Ready(Vec::new());
            cx.notify();
            return;
        };

        state.selected_resource_id = Some(app.id.clone());
        self.builds_state = LoadState::Loading;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let builds_result = cx
                .background_spawn(async move { load_builds(&app).await })
                .await;
            this.update_in(cx, |page, _window, cx| {
                page.with_provider_mut(APP_STORE_CONNECT_PROVIDER_ID, |pane, _state| {
                    let Some(provider) = pane.as_app_store_connect_mut() else {
                        return;
                    };

                    provider.builds_state = match builds_result {
                        Ok(builds) => LoadState::Ready(builds),
                        Err(error) => LoadState::Error(error.to_string()),
                    };
                    cx.notify();
                });
            })
            .ok();
        })
        .detach();
    }

    fn refresh_builds(
        &mut self,
        state: &mut ServicesPageState,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) {
        if let Some(app_id) = state.selected_resource_id.clone() {
            self.load_builds_for_app(state, app_id, window, cx);
        }
    }

    fn show_authenticate_form(&mut self) {
        self.auth_form.show();
    }

    fn cancel_authenticate_form(&mut self) {
        self.auth_form.cancel();
    }

    fn submit_authenticate(&mut self, window: &mut Window, cx: &mut Context<ServicesPage>) {
        let request = match self
            .auth_form
            .build_authenticate_request(APP_STORE_CONNECT_PROVIDER_ID, cx)
        {
            Ok(request) => request,
            Err(error) => {
                self.auth_form.set_error(error);
                cx.notify();
                return;
            }
        };

        self.auth_form.set_pending(true);
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { run_auth_action(request).await })
                .await;
            this.update_in(cx, |page, window, cx| {
                page.with_provider_mut(APP_STORE_CONNECT_PROVIDER_ID, |pane, state| {
                    let Some(provider) = pane.as_app_store_connect_mut() else {
                        return;
                    };

                    match result {
                        Ok(()) => {
                            provider.auth_form.finish_success();
                            provider.refresh(state, window, cx);
                        }
                        Err(error) => {
                            provider.auth_form.set_pending(false);
                            provider.auth_form.set_error(error.to_string());
                            cx.notify();
                        }
                    }
                });
            })
            .ok();
        })
        .detach();
    }

    fn logout(&mut self, window: &mut Window, cx: &mut Context<ServicesPage>) {
        let Some(request) = self
            .auth_form
            .build_logout_request(APP_STORE_CONNECT_PROVIDER_ID)
        else {
            return;
        };

        self.auth_form.set_pending(true);
        self.auth_form.error_message = None;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { run_auth_action(request).await })
                .await;
            this.update_in(cx, |page, window, cx| {
                page.with_provider_mut(APP_STORE_CONNECT_PROVIDER_ID, |pane, state| {
                    let Some(provider) = pane.as_app_store_connect_mut() else {
                        return;
                    };

                    match result {
                        Ok(()) => {
                            provider.auth_form.finish_success();
                            provider.refresh(state, window, cx);
                        }
                        Err(error) => {
                            provider.auth_form.set_pending(false);
                            provider.auth_form.set_error(error.to_string());
                            cx.notify();
                        }
                    }
                });
            })
            .ok();
        })
        .detach();
    }

    fn pick_auth_file(
        &mut self,
        field_key: String,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) {
        let workspace = {
            let page = cx.entity().read(cx);
            page.workspace().clone()
        };

        let prompt = workspace
            .update(cx, |workspace, cx| {
                workspace.prompt_for_open_path(
                    gpui::PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: false,
                        prompt: Some(SharedString::from("Select an App Store Connect key file")),
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
                    this.update(cx, |page, cx| {
                        page.with_provider_mut(APP_STORE_CONNECT_PROVIDER_ID, |pane, _state| {
                            let Some(provider) = pane.as_app_store_connect_mut() else {
                                return;
                            };

                            provider.auth_form.set_error(error.to_string());
                            cx.notify();
                        });
                    })
                    .ok();
                    None
                }
            };

            let Some(path) = path else {
                return;
            };

            this.update_in(cx, |page, window, cx| {
                page.with_provider_mut(APP_STORE_CONNECT_PROVIDER_ID, |pane, _state| {
                    let Some(provider) = pane.as_app_store_connect_mut() else {
                        return;
                    };

                    provider
                        .auth_form
                        .set_text(&field_key, &path.to_string_lossy(), window, cx);
                    cx.notify();
                });
            })
            .ok();
        })
        .detach();
    }

    fn apps(&self) -> &[AscAppSummary] {
        match &self.apps_state {
            LoadState::Ready(apps) => apps,
            LoadState::Loading | LoadState::Error(_) => &[],
        }
    }

    fn selected_app(&self, state: &ServicesPageState) -> Option<AscAppSummary> {
        state
            .selected_resource_id
            .as_ref()
            .and_then(|selected_id| self.apps().iter().find(|app| &app.id == selected_id))
            .cloned()
    }

    fn selected_build(&self) -> Option<&AscBuildSummary> {
        match &self.builds_state {
            LoadState::Ready(builds) => builds.first(),
            LoadState::Loading | LoadState::Error(_) => None,
        }
    }

    fn render_auth_status_summary(&self) -> (Severity, String, String, Vec<String>, bool) {
        match &self.auth_state {
            LoadState::Loading => (
                Severity::Success,
                "Checking authentication…".to_string(),
                "Validating the current App Store Connect profile.".to_string(),
                Vec::new(),
                false,
            ),
            LoadState::Error(error) => (
                Severity::Warning,
                "Authentication check failed".to_string(),
                error.clone(),
                Vec::new(),
                false,
            ),
            LoadState::Ready(summary) => (
                if summary.healthy {
                    Severity::Success
                } else {
                    Severity::Warning
                },
                summary.headline.clone(),
                summary.detail.clone(),
                summary.warnings.clone(),
                summary.authenticated,
            ),
        }
    }

    pub fn render_sidebar_footer(
        &self,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) -> Option<AnyElement> {
        Some(self.render_auth_footer(window, cx).into_any_element())
    }

    fn render_auth_footer(
        &self,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) -> impl IntoElement {
        let (severity, headline, detail, warnings, authenticated) =
            self.render_auth_status_summary();
        let authenticate_label = if authenticated {
            "Re-authenticate"
        } else {
            "Authenticate"
        };
        let (indicator_color, status_tooltip) =
            self.render_auth_status_indicator(severity, headline, detail.clone(), warnings);

        v_flex()
            .gap_3()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                h_flex()
                    .justify_between()
                    .items_start()
                    .gap_2()
                    .child(
                        v_flex().min_w_0().gap_1().child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    ButtonLike::new("services-auth-status-indicator")
                                        .style(ButtonStyle::Transparent)
                                        .size(ButtonSize::None)
                                        .cursor_style(gpui::CursorStyle::Arrow)
                                        .tooltip(Tooltip::text(status_tooltip))
                                        .child(Indicator::dot().color(indicator_color)),
                                )
                                .child(
                                    Label::new(detail)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted)
                                        .truncate(),
                                ),
                        ),
                    )
                    .child(h_flex().items_center().gap_1().child(if authenticated {
                        self.render_auth_overflow_menu(window, cx, authenticate_label)
                            .into_any_element()
                    } else {
                        Button::new("services-auth-open", authenticate_label)
                            .style(ButtonStyle::Filled)
                            .size(ButtonSize::Compact)
                            .disabled(self.auth_form.pending)
                            .on_click(cx.listener(|page, _, _window, cx| {
                                page.with_provider_mut(
                                    APP_STORE_CONNECT_PROVIDER_ID,
                                    |pane, _state| {
                                        let Some(provider) = pane.as_app_store_connect_mut() else {
                                            return;
                                        };

                                        provider.show_authenticate_form();
                                        cx.notify();
                                    },
                                );
                            }))
                            .into_any_element()
                    })),
            )
            .when_some(self.auth_form.error_message.clone(), |this, error| {
                this.child(Label::new(error).size(LabelSize::Small).color(Color::Error))
            })
            .when(self.auth_form.logout_available && authenticated, |this| {
                this.child(
                    SidebarRow::new("services-auth-logout-row", "Log Out", IconName::Exit)
                        .on_click(cx.listener(|page, _, window, cx| {
                            page.with_provider_mut(
                                APP_STORE_CONNECT_PROVIDER_ID,
                                |pane, _state| {
                                    let Some(provider) = pane.as_app_store_connect_mut() else {
                                        return;
                                    };

                                    provider.logout(window, cx);
                                },
                            );
                        })),
                )
            })
            .when(self.auth_form.expanded, |this| {
                this.child(self.render_auth_form(cx, authenticate_label))
            })
    }

    fn render_auth_status_indicator(
        &self,
        severity: Severity,
        headline: String,
        detail: String,
        warnings: Vec<String>,
    ) -> (Color, String) {
        let indicator_color = match severity {
            Severity::Success => Color::Success,
            Severity::Warning => Color::Warning,
            Severity::Error => Color::Error,
            Severity::Info => Color::Info,
        };

        let mut tooltip_lines = vec![headline, detail];
        tooltip_lines.extend(warnings);
        (indicator_color, tooltip_lines.join("\n"))
    }

    fn render_auth_overflow_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
        authenticate_label: &'static str,
    ) -> impl IntoElement {
        let page = cx.entity().downgrade();
        let menu = ContextMenu::build(window, cx, |menu, _window, _cx| {
            menu.entry(authenticate_label, None, {
                let page = page.clone();
                move |_window, cx| {
                    page.update(cx, |page, cx| {
                        page.with_provider_mut(APP_STORE_CONNECT_PROVIDER_ID, |pane, _state| {
                            let Some(provider) = pane.as_app_store_connect_mut() else {
                                return;
                            };

                            provider.show_authenticate_form();
                            cx.notify();
                        });
                    })
                    .ok();
                }
            })
        });

        PopoverMenu::new("services-auth-overflow-menu")
            .window_overlay()
            .menu(move |_window, _cx| Some(menu.clone()))
            .trigger(
                IconButton::new("services-auth-overflow", IconName::Ellipsis)
                    .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                    .shape(ui::IconButtonShape::Square)
                    .style(ButtonStyle::Transparent)
                    .size(ButtonSize::Compact)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Authentication actions")),
            )
            .attach(gpui::Corner::BottomRight)
            .anchor(gpui::Corner::TopRight)
            .offset(gpui::Point {
                x: gpui::px(0.),
                y: gpui::px(4.),
            })
    }

    fn render_auth_form(
        &self,
        cx: &mut Context<ServicesPage>,
        authenticate_label: &'static str,
    ) -> impl IntoElement {
        v_flex()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().background)
            .child(
                v_flex()
                    .gap_2()
                    .children(self.auth_form.fields.iter().map(|field| {
                        match field {
                            ServiceAuthFieldState::Text { descriptor, input } => {
                                match descriptor.kind {
                                    ServiceInputKind::FilePath => {
                                        h_flex()
                                            .items_end()
                                            .gap_2()
                                            .child(input.clone())
                                            .child(
                                                Button::new(
                                                    SharedString::from(format!(
                                                        "browse-auth-{}",
                                                        descriptor.key
                                                    )),
                                                    "Browse…",
                                                )
                                                .style(ButtonStyle::Outlined)
                                                .size(ButtonSize::Compact)
                                                .disabled(self.auth_form.pending)
                                                .on_click(cx.listener({
                                                    let field_key = descriptor.key.clone();
                                                    move |page, _, window, cx| {
                                                        page.with_provider_mut(
                                                            APP_STORE_CONNECT_PROVIDER_ID,
                                                            |pane, _state| {
                                                                let Some(provider) =
                                                                    pane.as_app_store_connect_mut()
                                                                else {
                                                                    return;
                                                                };

                                                                provider.pick_auth_file(
                                                                    field_key.clone(),
                                                                    window,
                                                                    cx,
                                                                );
                                                            },
                                                        );
                                                    }
                                                })),
                                            )
                                            .into_any_element()
                                    }
                                    ServiceInputKind::Text | ServiceInputKind::Toggle => {
                                        input.clone().into_any_element()
                                    }
                                }
                            }
                            ServiceAuthFieldState::Toggle { descriptor, value } => Checkbox::new(
                                SharedString::from(format!("auth-toggle-{}", descriptor.key)),
                                *value,
                            )
                            .label(descriptor.label.clone())
                            .disabled(self.auth_form.pending)
                            .on_click(cx.listener({
                                let field_key = descriptor.key.clone();
                                move |page, checked, _window, cx| {
                                    page.with_provider_mut(
                                        APP_STORE_CONNECT_PROVIDER_ID,
                                        |pane, _state| {
                                            let Some(provider) = pane.as_app_store_connect_mut()
                                            else {
                                                return;
                                            };

                                            provider.auth_form.set_toggle(&field_key, *checked);
                                            cx.notify();
                                        },
                                    );
                                }
                            }))
                            .into_any_element(),
                        }
                    }))
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("services-auth-cancel", "Cancel")
                                    .style(ButtonStyle::Outlined)
                                    .size(ButtonSize::Compact)
                                    .disabled(self.auth_form.pending)
                                    .on_click(cx.listener(|page, _, _window, cx| {
                                        page.with_provider_mut(
                                            APP_STORE_CONNECT_PROVIDER_ID,
                                            |pane, _state| {
                                                let Some(provider) =
                                                    pane.as_app_store_connect_mut()
                                                else {
                                                    return;
                                                };

                                                provider.cancel_authenticate_form();
                                                cx.notify();
                                            },
                                        );
                                    })),
                            )
                            .child(
                                Button::new("services-auth-submit", authenticate_label)
                                    .style(ButtonStyle::Filled)
                                    .size(ButtonSize::Compact)
                                    .disabled(self.auth_form.pending)
                                    .on_click(cx.listener(|page, _, window, cx| {
                                        page.with_provider_mut(
                                            APP_STORE_CONNECT_PROVIDER_ID,
                                            |pane, _state| {
                                                let Some(provider) =
                                                    pane.as_app_store_connect_mut()
                                                else {
                                                    return;
                                                };

                                                provider.submit_authenticate(window, cx);
                                            },
                                        );
                                    })),
                            ),
                    ),
            )
    }

    fn render_summary_card(
        &self,
        title: impl Into<SharedString>,
        value: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .min_w(rems(12.))
            .flex_1()
            .gap_1()
            .p_4()
            .rounded_xl()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().background)
            .child(Label::new(title).size(LabelSize::Small).color(Color::Muted))
            .child(Label::new(value).size(LabelSize::Large))
            .child(
                Label::new(detail)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    fn render_detail_row(
        &self,
        title: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> impl IntoElement {
        h_flex()
            .justify_between()
            .gap_3()
            .child(Label::new(title).size(LabelSize::Small).color(Color::Muted))
            .child(
                Label::new(value)
                    .size(LabelSize::Small)
                    .single_line()
                    .truncate(),
            )
    }

    fn render_empty_panel(
        &self,
        title: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_2()
            .p_5()
            .rounded_xl()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().background)
            .child(Label::new(title))
            .child(
                Label::new(detail)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    fn render_overview_content(
        &self,
        state: &ServicesPageState,
        _window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) -> impl IntoElement {
        let selected_app = self.selected_app(state);
        let selected_build = self.selected_build();
        let app_count = self.apps().len();

        v_flex()
            .size_full()
            .min_h_0()
            .gap_4()
            .child(
                h_flex()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        self.render_summary_card(
                            "Apps",
                            app_count.to_string(),
                            self.descriptor
                                .shell
                                .resource_kind
                                .as_ref()
                                .map(|resource| resource.plural_label.clone())
                                .unwrap_or_else(|| "Resources".to_string()),
                            cx,
                        ),
                    )
                    .child(
                        self.render_summary_card(
                            "Latest Build",
                            selected_build
                                .map(|build| format!("#{}", build.build_number))
                                .unwrap_or_else(|| "None".to_string()),
                            selected_build
                                .map(|build| build.processing_state.clone())
                                .unwrap_or_else(|| "Select an app to inspect builds".to_string()),
                            cx,
                        ),
                    ),
            )
            .when_some(selected_app.clone(), |this, app| {
                this.child(
                    h_flex()
                        .gap_3()
                        .flex_wrap()
                        .child(
                            v_flex()
                                .min_w(rems(22.))
                                .flex_1()
                                .gap_3()
                                .p_5()
                                .rounded_xl()
                                .border_1()
                                .border_color(cx.theme().colors().border_variant)
                                .bg(cx.theme().colors().background)
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(Label::new(app.name.clone()).size(LabelSize::Large))
                                        .child(
                                            Label::new(app.bundle_id.clone())
                                                .size(LabelSize::Small)
                                                .color(Color::Muted),
                                        ),
                                )
                                .child(self.render_detail_row("SKU", app.sku.clone()))
                                .child(
                                    self.render_detail_row(
                                        "Primary Locale",
                                        app.primary_locale
                                            .clone()
                                            .unwrap_or_else(|| "Not Set".to_string()),
                                    ),
                                )
                                .child(self.render_detail_row("App ID", app.id.clone())),
                        )
                        .child(
                            v_flex()
                                .min_w(rems(18.))
                                .flex_1()
                                .gap_3()
                                .p_5()
                                .rounded_xl()
                                .border_1()
                                .border_color(cx.theme().colors().border_variant)
                                .bg(cx.theme().colors().background)
                                .child(Label::new("Latest Build").size(LabelSize::Small))
                                .when_some(selected_build, |panel, build| {
                                    panel
                                        .child(
                                            Label::new(format!("Build {}", build.build_number))
                                                .size(LabelSize::Large),
                                        )
                                        .child(self.render_detail_row(
                                            "Status",
                                            build.processing_state.clone(),
                                        ))
                                        .child(self.render_detail_row(
                                            "Uploaded",
                                            build.uploaded_date.clone(),
                                        ))
                                        .child(
                                            self.render_detail_row(
                                                "Expires",
                                                build
                                                    .expiration_date
                                                    .clone()
                                                    .unwrap_or_else(|| "Not Set".to_string()),
                                            ),
                                        )
                                })
                                .when(selected_build.is_none(), |panel| {
                                    panel.child(
                                        Label::new("No builds are available for the selected app.")
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                }),
                        ),
                )
            })
            .when(selected_app.is_none(), |this| {
                this.child(self.render_empty_panel(
                    "No app selected",
                    "Choose an app from the top bar to inspect its release data.",
                    cx,
                ))
            })
    }

    fn render_builds_content(
        &self,
        state: &ServicesPageState,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) -> impl IntoElement {
        let selected_app = self.selected_app(state);
        let content = match &self.builds_state {
            LoadState::Loading => Label::new("Loading builds…")
                .color(Color::Muted)
                .into_any_element(),
            LoadState::Error(error) => v_flex()
                .gap_1()
                .child(Label::new("Could not load builds").color(Color::Error))
                .child(
                    Label::new(error.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element(),
            LoadState::Ready(_) if selected_app.is_none() => {
                Label::new("Select an app to load its builds.")
                    .color(Color::Muted)
                    .into_any_element()
            }
            LoadState::Ready(builds) if builds.is_empty() => {
                Label::new("No builds were returned for the selected app.")
                    .color(Color::Muted)
                    .into_any_element()
            }
            LoadState::Ready(builds) => v_flex()
                .gap_1()
                .children(builds.iter().map(|build| {
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

                    let detail = build
                        .min_os_version
                        .as_ref()
                        .map(|min_os_version| format!("Min OS {}", min_os_version))
                        .unwrap_or_else(|| "No minimum OS recorded".to_string());

                    v_flex()
                        .gap_2()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(cx.theme().colors().border_variant)
                        .bg(cx.theme().colors().background)
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .gap_3()
                                .child(
                                    Label::new(format!("Build {}", build.build_number))
                                        .size(LabelSize::Large),
                                )
                                .child(
                                    Label::new(build.processing_state.clone())
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                ),
                        )
                        .child(
                            Label::new(subtitle)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(detail)
                                .size(LabelSize::Small)
                                .color(Color::Hidden),
                        )
                }))
                .into_any_element(),
        };

        v_flex().size_full().min_h_0().gap_4().child(
            v_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                .p_5()
                .rounded_xl()
                .border_1()
                .border_color(cx.theme().colors().border_variant)
                .bg(cx.theme().colors().editor_background)
                .child(
                    h_flex()
                        .justify_between()
                        .items_start()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_0p5()
                                .child(Label::new("Builds").size(LabelSize::Large))
                                .child(
                                    Label::new(match selected_app {
                                        Some(ref app) => {
                                            format!("{} · {}", app.name, app.bundle_id)
                                        }
                                        None => "Select an app to inspect its builds".to_string(),
                                    })
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                                ),
                        )
                        .child(
                            Button::new("services-refresh-builds", "Refresh Builds")
                                .style(ButtonStyle::Outlined)
                                .size(ButtonSize::Compact)
                                .disabled(selected_app.is_none())
                                .on_click(cx.listener(|page, _, window, cx| {
                                    page.with_provider_mut(
                                        APP_STORE_CONNECT_PROVIDER_ID,
                                        |pane, state| {
                                            let Some(provider) = pane.as_app_store_connect_mut()
                                            else {
                                                return;
                                            };

                                            provider.refresh_builds(state, window, cx);
                                        },
                                    );
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(
                            v_flex()
                                .id("services-builds-scroll-content")
                                .track_scroll(&self.builds_scroll_handle)
                                .size_full()
                                .min_w_0()
                                .overflow_y_scroll()
                                .gap_3()
                                .child(content),
                        )
                        .vertical_scrollbar_for(&self.builds_scroll_handle, window, cx),
                ),
        )
    }
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

async fn load_apps() -> Result<Vec<AscAppSummary>> {
    let response: AscAppsResponse = run_json_operation(ServiceOperationRequest {
        provider_id: APP_STORE_CONNECT_PROVIDER_ID.to_string(),
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
        provider_id: APP_STORE_CONNECT_PROVIDER_ID.to_string(),
        operation: "list_builds".to_string(),
        resource: Some(ServiceResourceRef {
            provider_id: APP_STORE_CONNECT_PROVIDER_ID.to_string(),
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
