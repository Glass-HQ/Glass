use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, Render, SharedString, Task, Window,
};
use native_platforms::apple::app_store_connect::{self, App as AscApp, BetaGroup, BetaTester, Build};
use ui::prelude::*;
use workspace::item::{Item, ItemEvent, TabContentParams};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Apps,
    TestFlight,
}

pub struct AppStoreConnectTab {
    focus_handle: FocusHandle,
    view_mode: ViewMode,
    is_authenticated: bool,
    is_loading: bool,

    apps: Vec<AscApp>,
    selected_app: Option<AscApp>,

    builds: Vec<Build>,
    beta_groups: Vec<BetaGroup>,
    beta_testers: Vec<BetaTester>,

    load_task: Option<Task<()>>,
}

impl AppStoreConnectTab {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let is_authenticated = app_store_connect::check_asc_installed() && app_store_connect::is_authenticated();

        let mut tab = Self {
            focus_handle,
            view_mode: ViewMode::Apps,
            is_authenticated,
            is_loading: false,
            apps: Vec::new(),
            selected_app: None,
            builds: Vec::new(),
            beta_groups: Vec::new(),
            beta_testers: Vec::new(),
            load_task: None,
        };

        if is_authenticated {
            tab.load_apps(cx);
        }

        tab
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

    fn render_not_configured(&self, cx: &Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(Icon::new(IconName::CloudDownload).size(IconSize::XLarge).color(Color::Muted))
            .child(Label::new("App Store Connect").size(LabelSize::Large))
            .child(
                v_flex()
                    .gap_2()
                    .items_center()
                    .child(Label::new("Connect to App Store Connect to manage your apps").color(Color::Muted))
                    .child(
                        Label::new("Install the asc CLI: brew tap rudrankriyam/tap && brew install asc")
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                    )
                    .child(
                        Label::new("Then run: asc auth login")
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                    )
            )
            .child(
                Button::new("retry", "Retry Connection")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.is_authenticated = app_store_connect::check_asc_installed()
                            && app_store_connect::is_authenticated();
                        if this.is_authenticated {
                            this.load_apps(cx);
                        }
                        cx.notify();
                    }))
            )
    }

    fn render_apps_list(&self, cx: &Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .w_full()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(Label::new("Your Apps").size(LabelSize::Large))
            )
            .when(self.is_loading, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new("Loading...").color(Color::Muted))
                )
            })
            .when(!self.is_loading && self.apps.is_empty(), |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new("No apps found").color(Color::Muted))
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
                                                .child(Icon::new(IconName::Globe).size(IconSize::Medium))
                                        )
                                        .child(
                                            v_flex()
                                                .child(Label::new(app.name.clone()).size(LabelSize::Default))
                                                .child(Label::new(app.bundle_id.clone()).size(LabelSize::Small).color(Color::Muted))
                                        )
                                )
                        }))
                )
            })
    }

    fn render_testflight(&self, cx: &Context<Self>) -> impl IntoElement {
        let app_name = self.selected_app.as_ref()
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
                            }))
                    )
                    .child(Label::new(format!("{} - TestFlight", app_name)).size(LabelSize::Large))
            )
            .when(self.is_loading, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new("Loading...").color(Color::Muted))
                )
            })
            .when(!self.is_loading, |this| {
                this.child(
                    h_flex()
                        .flex_1()
                        .overflow_hidden()
                        .child(self.render_builds_section(cx))
                        .child(self.render_groups_section(cx))
                        .child(self.render_testers_section(cx))
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
                    .child(Label::new("Builds").size(LabelSize::Default).color(Color::Muted))
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
                                    })
                            )
                            .child(
                                v_flex()
                                    .child(Label::new(format!("v{}", build.version)).size(LabelSize::Small))
                                    .child(Label::new(build.processing_state.clone()).size(LabelSize::XSmall).color(Color::Muted))
                            )
                    }))
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
                    .child(Label::new("Beta Groups").size(LabelSize::Default).color(Color::Muted))
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
                                    .color(if group.is_internal { Color::Accent } else { Color::Muted })
                            )
                            .child(
                                v_flex()
                                    .child(Label::new(group.name.clone()).size(LabelSize::Small))
                                    .child(
                                        Label::new(if group.is_internal { "Internal" } else { "External" })
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted)
                                    )
                            )
                    }))
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
                    .child(Label::new("Beta Testers").size(LabelSize::Default).color(Color::Muted))
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
                            .child(Icon::new(IconName::Person).size(IconSize::Small).color(Color::Muted))
                            .child(
                                v_flex()
                                    .child(Label::new(name).size(LabelSize::Small))
                                    .child(Label::new(tester.email.clone()).size(LabelSize::XSmall).color(Color::Muted))
                            )
                    }))
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
            .when(!self.is_authenticated, |this| {
                this.child(self.render_not_configured(cx))
            })
            .when(self.is_authenticated && self.view_mode == ViewMode::Apps, |this| {
                this.child(self.render_apps_list(cx))
            })
            .when(self.is_authenticated && self.view_mode == ViewMode::TestFlight, |this| {
                this.child(self.render_testflight(cx))
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

        Label::new(text)
            .color(color)
            .into_any_element()
    }
}
