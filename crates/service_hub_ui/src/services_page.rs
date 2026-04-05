use std::collections::BTreeMap;

use gpui::{
    App, Context, Corner, CursorStyle, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, Render, SharedString, Stateful, Subscription, WeakEntity, Window, div,
    point, prelude::FluentBuilder as _, px,
};
use service_hub::{ServiceHub, ServiceProviderDescriptor};
use ui::{
    AnyElement, ButtonSize, ButtonStyle, Clickable, Color, ContextMenu, Icon, IconButton,
    IconButtonShape, IconName, IconSize, Label, LabelSize, PopoverMenu, Toggleable, Tooltip,
    prelude::*,
};
use workspace::item::{Item, ItemBufferKind, ItemEvent};
use workspace::{Workspace, WorkspaceSidebarSection};
use workspace_chrome::SidebarRow;

use crate::services_provider::{
    ServiceWorkspacePane, ServicesPageState, build_service_workspace_panes,
    collect_provider_descriptors, normalize_services_page_state,
};

pub struct ServicesPage {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    providers: Vec<ServiceProviderDescriptor>,
    panes: BTreeMap<String, ServiceWorkspacePane>,
    state: ServicesPageState,
}

impl ServicesPage {
    pub fn open(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
        #[cfg(target_os = "macos")]
        Self::install_sidebar_section_view(workspace, cx);

        if let Some(existing) = workspace.item_of_type::<Self>(cx) {
            workspace.activate_item(&existing, true, true, window, cx);
            #[cfg(target_os = "macos")]
            workspace.select_sidebar_section(WorkspaceSidebarSection::Services, window, cx);
            return;
        }

        let page = Self::new(workspace, None, window, cx);
        workspace.add_item_to_active_pane(Box::new(page), None, true, window, cx);
        #[cfg(target_os = "macos")]
        workspace.select_sidebar_section(WorkspaceSidebarSection::Services, window, cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn install_sidebar_section_view(
        workspace: &mut Workspace,
        cx: &mut Context<Workspace>,
    ) {
        let workspace_handle = workspace.weak_handle();
        let sidebar_panel = cx.new(|cx| ServicesSidebarPanel::new(workspace_handle, cx));
        workspace.set_sidebar_section_view(
            WorkspaceSidebarSection::Services,
            Some(sidebar_panel.into()),
            cx,
        );
    }

    fn new(
        workspace: &mut Workspace,
        initial_state: Option<ServicesPageState>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let panes = build_service_workspace_panes(ServiceHub::default().providers(), window, cx);
        let providers = collect_provider_descriptors(&panes);
        let state = normalize_services_page_state(&providers, initial_state);
        let workspace_handle = workspace.weak_handle();

        let page = cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
            workspace: workspace_handle,
            providers,
            panes,
            state,
        });

        page.update(cx, |page, cx| {
            page.normalize_active_provider_state();
            page.refresh_provider(window, cx);
        });
        page
    }

    pub(crate) fn workspace(&self) -> &WeakEntity<Workspace> {
        &self.workspace
    }

    pub(crate) fn with_provider_mut<R>(
        &mut self,
        provider_id: &str,
        callback: impl FnOnce(&mut ServiceWorkspacePane, &mut ServicesPageState) -> R,
    ) -> Option<R> {
        let pane = self.panes.get_mut(provider_id)?;
        Some(callback(pane, &mut self.state))
    }

    fn provider(&self) -> &ServiceProviderDescriptor {
        self.providers
            .iter()
            .find(|provider| provider.id == self.state.provider_id)
            .expect("selected provider should stay valid")
    }

    fn active_pane(&self) -> &ServiceWorkspacePane {
        self.panes
            .get(&self.state.provider_id)
            .expect("selected provider pane should stay valid")
    }

    fn normalize_active_provider_state(&mut self) {
        let provider_id = self.state.provider_id.clone();
        self.with_provider_mut(&provider_id, |pane, state| {
            pane.normalize_state(state);
        });
    }

    fn refresh_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let provider_id = self.state.provider_id.clone();
        self.with_provider_mut(&provider_id, |pane, state| {
            pane.refresh(state, window, cx);
        });
    }

    fn select_provider(
        &mut self,
        provider_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.provider_id == provider_id {
            return;
        }

        self.state.provider_id = provider_id;
        self.state.navigation_id = self.provider().shell.default_navigation_item_id.clone();
        self.state.selected_resource_id = None;
        self.normalize_active_provider_state();
        cx.emit(ItemEvent::UpdateTab);
        self.refresh_provider(window, cx);
    }

    fn select_navigation(&mut self, navigation_id: String, cx: &mut Context<Self>) {
        if self.state.navigation_id == navigation_id {
            return;
        }

        self.state.navigation_id = navigation_id;
        self.normalize_active_provider_state();
        cx.notify();
    }

    fn select_resource(
        &mut self,
        resource_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let provider_id = self.state.provider_id.clone();
        self.with_provider_mut(&provider_id, |pane, state| {
            pane.select_resource(state, resource_id, window, cx);
        });
    }

    fn render_provider_menu(
        &self,
        page: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let menu = ContextMenu::build(window, cx, |mut menu, _, _| {
            for provider in &self.providers {
                let provider_id = provider.id.clone();
                let page = page.clone();
                menu = menu.entry(provider.label.clone(), None, move |window, cx| {
                    page.update(cx, |this, cx| {
                        this.select_provider(provider_id.clone(), window, cx);
                    })
                    .ok();
                });
            }

            menu
        });

        Self::render_sidebar_popover_menu(
            "services-provider-menu",
            self.provider().label.clone(),
            menu,
        )
    }

    fn render_resource_menu(
        &self,
        page: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let resource_menu = self.active_pane().resource_menu(&self.state)?;
        let menu = ContextMenu::build(window, cx, |mut menu, _, _| {
            for entry in &resource_menu.entries {
                let resource_id = entry.id.clone();
                let label = match &entry.detail {
                    Some(detail) => format!("{} ({detail})", entry.label),
                    None => entry.label.clone(),
                };
                let page = page.clone();
                menu = menu.entry(label, None, move |window, cx| {
                    page.update(cx, |this, cx| {
                        this.select_resource(resource_id.clone(), window, cx);
                    })
                    .ok();
                });
            }

            menu
        });

        Some(
            v_flex()
                .gap_1()
                .child(
                    Label::new(resource_menu.singular_label.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(Self::render_sidebar_popover_menu(
                    "services-resource-menu",
                    resource_menu.current_label,
                    menu,
                ))
                .into_any_element(),
        )
    }

    fn render_sidebar_popover_menu(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        menu: Entity<ContextMenu>,
    ) -> impl IntoElement {
        let id = id.into();
        let label = label.into();
        PopoverMenu::new(format!("{id}-popover"))
            .full_width(true)
            .window_overlay()
            .menu(move |_window, _cx| Some(menu.clone()))
            .trigger(ServiceSidebarMenuTrigger::new(id, label))
            .attach(Corner::BottomLeft)
            .anchor(Corner::TopLeft)
            .offset(point(px(0.), px(4.)))
    }

    pub(crate) fn render_sidebar_controls(
        page: &Entity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let page_handle = page.downgrade();
        page.update(cx, |page, cx| {
            page.render_sidebar_controls_inner(page_handle, window, cx)
                .into_any_element()
        })
    }

    fn render_sidebar_controls_inner(
        &self,
        page: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let controls = v_flex()
            .flex_1()
            .min_h_0()
            .gap_1()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .child(
                                Label::new("Provider")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                IconButton::new("services-refresh", IconName::RotateCw)
                                    .shape(IconButtonShape::Square)
                                    .style(ButtonStyle::Transparent)
                                    .size(ButtonSize::Compact)
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::text("Refresh"))
                                    .on_click({
                                        let page = page.clone();
                                        move |_, window, cx| {
                                            page.update(cx, |this, cx| {
                                                this.refresh_provider(window, cx);
                                            })
                                            .ok();
                                        }
                                    }),
                            ),
                    )
                    .child(self.render_provider_menu(page.clone(), window, cx)),
            )
            .when_some(
                self.render_resource_menu(page.clone(), window, cx),
                |this, resource_menu| this.child(resource_menu),
            )
            .child(
                v_flex()
                    .gap_1()
                    .children(self.provider().shell.navigation_items.iter().map(
                        |navigation_item| {
                            SidebarRow::new(
                                format!("services-nav-{}", navigation_item.id),
                                navigation_item.label.clone(),
                                Self::navigation_icon(&navigation_item.id),
                            )
                            .selected(self.state.navigation_id == navigation_item.id)
                            .on_click({
                                let navigation_id = navigation_item.id.clone();
                                let page = page.clone();
                                move |_, _window, cx| {
                                    page.update(cx, |this, cx| {
                                        this.select_navigation(navigation_id.clone(), cx);
                                    })
                                    .ok();
                                }
                            })
                        },
                    )),
            );

        v_flex()
            .size_full()
            .p_3()
            .gap_3()
            .child(controls)
            .when_some(
                self.active_pane().render_sidebar_footer(window, cx),
                |this, footer| this.child(footer),
            )
    }

    fn render_provider_content(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.active_pane().render_section(&self.state, window, cx)
    }

    fn navigation_icon(navigation_id: &str) -> IconName {
        match navigation_id {
            "overview" => IconName::Info,
            "builds" => IconName::BoltOutlined,
            _ => IconName::Globe,
        }
    }
}

#[derive(IntoElement)]
struct ServiceSidebarMenuTrigger {
    div: Stateful<gpui::Div>,
    label: SharedString,
    selected: bool,
}

impl ServiceSidebarMenuTrigger {
    fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            div: div().id(id.into()),
            label: label.into(),
            selected: false,
        }
    }
}

impl Clickable for ServiceSidebarMenuTrigger {
    fn on_click(
        mut self,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.div = self.div.on_click(handler);
        self
    }

    fn cursor_style(mut self, cursor_style: CursorStyle) -> Self {
        self.div = self.div.cursor(cursor_style);
        self
    }
}

impl Toggleable for ServiceSidebarMenuTrigger {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl RenderOnce for ServiceSidebarMenuTrigger {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (text_color, background, hover_background, active_background) = match self.selected {
            false => (
                cx.theme().colors().text_muted,
                cx.theme().colors().tab_inactive_background.opacity(0.0),
                cx.theme().colors().text.opacity(0.09),
                cx.theme().colors().text.opacity(0.14),
            ),
            true => (
                cx.theme().colors().text,
                cx.theme().colors().text.opacity(0.14),
                cx.theme().colors().text.opacity(0.14),
                cx.theme().colors().text.opacity(0.20),
            ),
        };

        self.div
            .w_full()
            .h(px(28.))
            .bg(background)
            .rounded(cx.theme().component_radius().tab.unwrap_or(px(6.0)))
            .when(!self.selected, |this| {
                this.hover(move |style| style.bg(hover_background))
            })
            .active(move |style| style.bg(active_background))
            .cursor_pointer()
            .child(
                h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .gap_2()
                    .text_color(text_color)
                    .child(Label::new(self.label).size(LabelSize::Small).truncate())
                    .child(
                        Icon::new(IconName::ChevronUpDown)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
    }
}

impl EventEmitter<ItemEvent> for ServicesPage {}

impl Focusable for ServicesPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for ServicesPage {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        self.provider().label.clone().into()
    }

    fn tab_tooltip_text(&self, _cx: &App) -> Option<SharedString> {
        Some("Services".into())
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Server))
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Services Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::None
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event)
    }
}

impl Render for ServicesPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .p_5()
            .child(self.render_provider_content(window, cx))
    }
}

#[cfg(target_os = "macos")]
struct ServicesSidebarPanel {
    workspace: WeakEntity<Workspace>,
    observed_page: Option<WeakEntity<ServicesPage>>,
    subscriptions_initialized: bool,
    _workspace_subscription: Option<Subscription>,
    _page_subscription: Option<Subscription>,
}

#[cfg(target_os = "macos")]
impl ServicesSidebarPanel {
    fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            workspace,
            observed_page: None,
            subscriptions_initialized: false,
            _workspace_subscription: None,
            _page_subscription: None,
        };

        if let Some(workspace) = panel.workspace.upgrade() {
            panel._workspace_subscription = Some(cx.observe(&workspace, |this, _, cx| {
                this.sync_page_subscription(cx);
                cx.notify();
            }));
        }

        panel
    }

    fn sync_page_subscription(&mut self, cx: &mut Context<Self>) {
        let next_page = self
            .workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).item_of_type::<ServicesPage>(cx));
        let next_page_id = next_page.as_ref().map(|page| page.entity_id());
        let current_page_id = self
            .observed_page
            .as_ref()
            .and_then(|page| page.upgrade())
            .map(|page| page.entity_id());

        if next_page_id == current_page_id {
            return;
        }

        self._page_subscription = None;
        self.observed_page = next_page.as_ref().map(|page| page.downgrade());

        if let Some(page) = next_page {
            self._page_subscription = Some(cx.observe(&page, |this, _, cx| {
                this.sync_page_subscription(cx);
                cx.notify();
            }));
        }
    }
}

#[cfg(target_os = "macos")]
impl Render for ServicesSidebarPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.subscriptions_initialized {
            self.subscriptions_initialized = true;
            let this = cx.entity().downgrade();
            cx.defer(move |cx| {
                let Some(this) = this.upgrade() else {
                    return;
                };

                this.update(cx, |this, cx| {
                    this.sync_page_subscription(cx);
                    cx.notify();
                });
            });
        }

        if let Some(page) = self.observed_page.as_ref().and_then(|page| page.upgrade()) {
            return ServicesPage::render_sidebar_controls(&page, window, cx);
        }

        v_flex()
            .size_full()
            .justify_center()
            .p_4()
            .gap_2()
            .child(Label::new("Services").size(LabelSize::Large))
            .child(
                Label::new("Open the Service Hub to manage providers, apps, and releases.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .into_any_element()
    }
}
