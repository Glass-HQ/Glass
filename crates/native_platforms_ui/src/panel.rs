use crate::app_store_connect::AppStoreConnectTab;
use crate::build_logs::BuildLogsView;
use anyhow::Result;
use db::kvp::KEY_VALUE_STORE;
use gpui::{
    Action, App, AppContext, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Pixels, Render, Subscription, Task, WeakEntity, Window, actions, px,
};
use native_platforms::apple::{build, simulator, xcode};
use native_platforms::{BuildConfiguration, Device, DeviceState, DeviceType};
use project::Project;
use serde::{Deserialize, Serialize};
use ui::prelude::*;
use ui::{ContextMenu, Divider, PopoverMenu, PopoverMenuHandle};
use workspace::dock::{DockPosition, Panel, PanelEvent};
use workspace::Workspace;

const NATIVE_PLATFORMS_PANEL_KEY: &str = "NativePlatformsPanel";

actions!(
    native_platforms_panel,
    [
        ToggleFocus,
        Build,
        Run,
        Deploy,
        RefreshDevices,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<NativePlatformsPanel>(window, cx);
        });
    })
    .detach();
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedNativePlatformsPanel {
    width: Option<f32>,
    selected_scheme: Option<String>,
    selected_device_id: Option<String>,
}

pub struct NativePlatformsPanel {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    width: Option<Pixels>,

    xcode_project: Option<xcode::XcodeProject>,
    schemes: Vec<String>,
    selected_scheme: Option<String>,
    configurations: Vec<String>,
    selected_configuration: BuildConfiguration,

    devices: Vec<Device>,
    selected_device: Option<Device>,
    loading_devices: bool,

    is_building: bool,
    build_task: Option<Task<()>>,

    scheme_menu_handle: PopoverMenuHandle<ContextMenu>,
    device_menu_handle: PopoverMenuHandle<ContextMenu>,

    pending_serialization: Task<Option<()>>,
    _subscriptions: Vec<Subscription>,
}

impl NativePlatformsPanel {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let mut panel = Self {
            focus_handle,
            workspace,
            project: project.clone(),
            width: None,
            xcode_project: None,
            schemes: Vec::new(),
            selected_scheme: None,
            configurations: vec!["Debug".to_string(), "Release".to_string()],
            selected_configuration: BuildConfiguration::Debug,
            devices: Vec::new(),
            selected_device: None,
            loading_devices: false,
            is_building: false,
            build_task: None,
            scheme_menu_handle: PopoverMenuHandle::default(),
            device_menu_handle: PopoverMenuHandle::default(),
            pending_serialization: Task::ready(None),
            _subscriptions: Vec::new(),
        };

        panel._subscriptions.push(
            cx.subscribe(&project, Self::handle_project_event)
        );

        panel.detect_xcode_project(cx);
        panel.refresh_devices(cx);

        panel
    }

    fn handle_project_event(
        &mut self,
        _project: Entity<Project>,
        event: &project::Event,
        cx: &mut Context<Self>,
    ) {
        match event {
            project::Event::WorktreeAdded(_) | project::Event::WorktreeRemoved(_) => {
                log::info!("handle_project_event: worktree changed, re-detecting Xcode project");
                self.detect_xcode_project(cx);
            }
            _ => {}
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        let serialized_panel = cx
            .background_spawn(async move {
                KEY_VALUE_STORE
                    .read_kvp(NATIVE_PLATFORMS_PANEL_KEY)
                    .ok()
                    .flatten()
                    .and_then(|value| serde_json::from_str::<SerializedNativePlatformsPanel>(&value).ok())
            })
            .await;

        workspace.update_in(&mut cx, |workspace, window, cx| {
            let project = workspace.project().clone();
            let panel = cx.new(|cx| {
                let mut panel = Self::new(workspace.weak_handle(), project, window, cx);
                if let Some(serialized) = serialized_panel {
                    panel.width = serialized.width.map(px);
                    panel.selected_scheme = serialized.selected_scheme;
                    if let Some(device_id) = serialized.selected_device_id {
                        panel.selected_device = panel.devices.iter().find(|d| d.id == device_id).cloned();
                    }
                }
                panel
            });
            panel
        })
    }

    fn detect_xcode_project(&mut self, cx: &mut Context<Self>) {
        let worktree_paths: Vec<std::path::PathBuf> = self
            .project
            .read(cx)
            .worktrees(cx)
            .filter_map(|wt| {
                let wt = wt.read(cx);
                Some(wt.abs_path().to_path_buf())
            })
            .collect();

        log::info!("detect_xcode_project: found {} worktrees", worktree_paths.len());

        if worktree_paths.is_empty() {
            return;
        }

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    for path in worktree_paths {
                        log::info!("detect_xcode_project: checking worktree at {:?}", path);

                        if let Some(detected_project) = xcode::detect_xcode_project(&path) {
                            log::info!("detect_xcode_project: detected project at {:?}", detected_project.path);

                            let schemes = xcode::list_schemes(&detected_project).unwrap_or_default();
                            log::info!("detect_xcode_project: found {} schemes", schemes.len());

                            let configurations = xcode::list_configurations(&detected_project)
                                .unwrap_or_else(|_| vec!["Debug".to_string(), "Release".to_string()]);

                            return Some((detected_project, schemes, configurations));
                        }
                    }
                    None
                })
                .await;

            log::info!("detect_xcode_project: background task completed, result is_some={}", result.is_some());

            let update_result = this.update(cx, |this, cx| {
                if let Some((project, schemes, configurations)) = result {
                    log::info!(
                        "detect_xcode_project: updating UI state with {} schemes",
                        schemes.len()
                    );
                    this.xcode_project = Some(project);
                    if this.selected_scheme.is_none() && !schemes.is_empty() {
                        this.selected_scheme = Some(schemes[0].clone());
                        log::info!(
                            "detect_xcode_project: auto-selected scheme: {:?}",
                            this.selected_scheme
                        );
                    }
                    this.schemes = schemes;
                    if !configurations.is_empty() {
                        this.configurations = configurations;
                    }
                    log::info!("detect_xcode_project: calling cx.notify()");
                    cx.notify();
                    log::info!("detect_xcode_project: UI state updated successfully");
                } else {
                    log::info!("detect_xcode_project: no project found in background task");
                }
            });

            if let Err(e) = update_result {
                log::error!("detect_xcode_project: failed to update panel state: {:?}", e);
            }
        })
        .detach();
    }

    fn refresh_devices(&mut self, cx: &mut Context<Self>) {
        self.loading_devices = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let devices = cx
                .background_spawn(async {
                    use native_platforms::apple::device;

                    // Get physical devices first (they appear at the top)
                    let mut all_devices = device::list_physical_devices();

                    // Then get simulators
                    let simulators = simulator::list_simulators().unwrap_or_default();
                    all_devices.extend(simulators);

                    all_devices
                })
                .await;

            this.update(cx, |this, cx| {
                this.devices = devices;
                this.loading_devices = false;
                if this.selected_device.is_none() {
                    this.selected_device = this
                        .devices
                        .iter()
                        .find(|d| d.state == DeviceState::Booted)
                        .or_else(|| this.devices.first())
                        .cloned();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn build(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(xcode_project) = &self.xcode_project else {
            return;
        };
        let Some(scheme) = &self.selected_scheme else {
            return;
        };

        self.is_building = true;
        cx.notify();

        let xcode_project = xcode_project.clone();
        let options = build::BuildOptions {
            scheme: scheme.clone(),
            configuration: self.selected_configuration.clone(),
            destination: self.selected_device.clone(),
            clean: false,
            derived_data_path: None,
        };

        let workspace = self.workspace.clone();

        self.build_task = Some(cx.spawn_in(window, async move |this, cx| {
            let build_result = build::build(&xcode_project, &options).await;

            match build_result {
                Ok(process) => {
                    if let Some(workspace) = workspace.upgrade() {
                        workspace.update_in(cx, |workspace, window, cx| {
                            let build_logs = cx.new(|cx| {
                                BuildLogsView::new(process.output_receiver, window, cx)
                            });
                            workspace.add_item_to_active_pane(
                                Box::new(build_logs),
                                None,
                                true,
                                window,
                                cx,
                            );
                        }).ok();
                    }
                }
                Err(_e) => {
                    // Build failed to start
                }
            }

            this.update(cx, |this, cx| {
                this.is_building = false;
                cx.notify();
            }).ok();
        }));
    }

    fn run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(xcode_project) = &self.xcode_project else {
            return;
        };
        let Some(scheme) = &self.selected_scheme else {
            return;
        };

        self.is_building = true;
        cx.notify();

        let xcode_project = xcode_project.clone();
        let options = build::BuildOptions {
            scheme: scheme.clone(),
            configuration: self.selected_configuration.clone(),
            destination: self.selected_device.clone(),
            clean: false,
            derived_data_path: None,
        };

        let workspace = self.workspace.clone();

        self.build_task = Some(cx.spawn_in(window, async move |this, cx| {
            let build_result = build::run(&xcode_project, &options).await;

            match build_result {
                Ok(process) => {
                    if let Some(workspace) = workspace.upgrade() {
                        workspace.update_in(cx, |workspace, window, cx| {
                            let build_logs = cx.new(|cx| {
                                BuildLogsView::new(process.output_receiver, window, cx)
                            });
                            workspace.add_item_to_active_pane(
                                Box::new(build_logs),
                                None,
                                true,
                                window,
                                cx,
                            );
                        }).ok();
                    }
                }
                Err(_e) => {
                    // Run failed to start
                }
            }

            this.update(cx, |this, cx| {
                this.is_building = false;
                cx.notify();
            }).ok();
        }));
    }

    fn deploy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        if let Some(workspace) = workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                let tab = cx.new(|cx| AppStoreConnectTab::new(window, cx));
                workspace.add_item_to_active_pane(Box::new(tab), None, true, window, cx);
            });
        }
    }

    fn serialize(&mut self, cx: &mut Context<Self>) {
        let width = self.width.map(|w| w.into());
        let selected_scheme = self.selected_scheme.clone();
        let selected_device_id = self.selected_device.as_ref().map(|d| d.id.clone());

        self.pending_serialization = cx.background_spawn(async move {
            let serialized = SerializedNativePlatformsPanel {
                width,
                selected_scheme,
                selected_device_id,
            };
            KEY_VALUE_STORE
                .write_kvp(
                    NATIVE_PLATFORMS_PANEL_KEY.to_string(),
                    serde_json::to_string(&serialized).ok()?,
                )
                .await
                .ok()?;
            Some(())
        });
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new("Native Platforms")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    fn render_project_section(&self, _cx: &Context<Self>) -> impl IntoElement {
        let has_project = self.xcode_project.is_some();
        log::debug!(
            "render_project_section: has_project={}, schemes_count={}",
            has_project,
            self.schemes.len()
        );

        v_flex()
            .w_full()
            .p_2()
            .gap_2()
            .child(
                Label::new("Xcode Project")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .when(has_project, |this| {
                let project = self.xcode_project.as_ref().unwrap();
                let name = project.path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown");
                this.child(
                    h_flex()
                        .gap_2()
                        .child(Icon::new(IconName::Folder).size(IconSize::Small))
                        .child(Label::new(name.to_string()).size(LabelSize::Small))
                )
            })
            .when(!has_project, |this| {
                this.child(
                    Label::new("No Xcode project found")
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                )
            })
    }

    fn render_scheme_section(&self, cx: &Context<Self>) -> impl IntoElement {
        let schemes = self.schemes.clone();
        let selected = self.selected_scheme.clone().unwrap_or_else(|| "Select Scheme".to_string());
        let has_schemes = !schemes.is_empty();
        let weak_panel = cx.entity().downgrade();

        v_flex()
            .w_full()
            .p_2()
            .gap_2()
            .child(
                Label::new("Scheme")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .when(has_schemes, |this| {
                this.child(
                    PopoverMenu::new("scheme-selector")
                        .trigger(
                            Button::new("scheme-trigger", selected)
                                .style(ButtonStyle::Subtle)
                                .full_width()
                                .icon(IconName::ChevronDown)
                                .icon_position(IconPosition::End)
                                .icon_size(IconSize::Small)
                        )
                        .menu({
                            let schemes = schemes.clone();
                            let weak_panel = weak_panel.clone();
                            move |window, cx| {
                                let schemes = schemes.clone();
                                let weak_panel = weak_panel.clone();
                                Some(ContextMenu::build(window, cx, move |mut menu, _window, _cx| {
                                    for scheme in &schemes {
                                        let scheme_name = scheme.clone();
                                        let weak_panel = weak_panel.clone();
                                        menu = menu.entry(scheme.clone(), None, {
                                            move |_window, cx| {
                                                weak_panel.update(cx, |panel, cx| {
                                                    panel.selected_scheme = Some(scheme_name.clone());
                                                    panel.serialize(cx);
                                                    cx.notify();
                                                }).ok();
                                            }
                                        });
                                    }
                                    menu
                                }))
                            }
                        })
                        .with_handle(self.scheme_menu_handle.clone())
                )
            })
            .when(!has_schemes, |this| {
                this.child(
                    Label::new("No schemes available")
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                )
            })
    }

    fn render_devices_section(&self, cx: &Context<Self>) -> impl IntoElement {
        let devices = self.devices.clone();
        let has_devices = !devices.is_empty();
        let loading = self.loading_devices;
        let weak_panel = cx.entity().downgrade();

        let selected_label = self.selected_device
            .as_ref()
            .map(|d| {
                let prefix = if d.device_type == DeviceType::PhysicalDevice {
                    "📱 "
                } else {
                    ""
                };
                let os = d.os_version.clone().unwrap_or_default();
                if os.is_empty() {
                    format!("{}{}", prefix, d.name)
                } else {
                    format!("{}{} ({})", prefix, d.name, os)
                }
            })
            .unwrap_or_else(|| "Select Device".to_string());

        v_flex()
            .w_full()
            .p_2()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Label::new("Destination")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .when(loading, |this| {
                        this.child(Label::new("Loading...").size(LabelSize::XSmall).color(Color::Muted))
                    })
            )
            .when(has_devices, |this| {
                this.child(
                    PopoverMenu::new("device-selector")
                        .trigger(
                            Button::new("device-trigger", selected_label)
                                .style(ButtonStyle::Subtle)
                                .full_width()
                                .icon(IconName::ChevronDown)
                                .icon_position(IconPosition::End)
                                .icon_size(IconSize::Small)
                        )
                        .menu({
                            let devices = devices.clone();
                            let weak_panel = weak_panel.clone();
                            move |window, cx| {
                                let devices = devices.clone();
                                let weak_panel = weak_panel.clone();
                                Some(ContextMenu::build(window, cx, move |mut menu, _window, _cx| {
                                    let mut last_was_physical = None;

                                    for device in &devices {
                                        let is_physical = device.device_type == DeviceType::PhysicalDevice;

                                        // Add section headers
                                        if last_was_physical != Some(is_physical) {
                                            if is_physical {
                                                menu = menu.header("Physical Devices");
                                            } else if last_was_physical == Some(true) {
                                                menu = menu.separator();
                                                menu = menu.header("Simulators");
                                            }
                                            last_was_physical = Some(is_physical);
                                        }

                                        let device_clone = device.clone();
                                        let os_version = device.os_version.clone().unwrap_or_default();
                                        let label = if os_version.is_empty() {
                                            device.name.clone()
                                        } else {
                                            format!("{} ({})", device.name, os_version)
                                        };
                                        let is_booted = device.state == DeviceState::Booted;
                                        let label = if is_booted {
                                            format!("● {}", label)
                                        } else {
                                            format!("  {}", label)
                                        };

                                        let weak_panel = weak_panel.clone();
                                        menu = menu.entry(label, None, {
                                            move |_window, cx| {
                                                weak_panel.update(cx, |panel, cx| {
                                                    panel.selected_device = Some(device_clone.clone());
                                                    panel.serialize(cx);
                                                    cx.notify();
                                                }).ok();
                                            }
                                        });
                                    }
                                    menu
                                }))
                            }
                        })
                        .with_handle(self.device_menu_handle.clone())
                )
            })
            .when(!has_devices && !loading, |this| {
                this.child(
                    Label::new("No devices available")
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                )
            })
    }

    fn render_actions(&self, cx: &Context<Self>) -> impl IntoElement {
        let has_project = self.xcode_project.is_some();
        let has_scheme = self.selected_scheme.is_some();
        let can_build = has_project && has_scheme && !self.is_building;

        v_flex()
            .w_full()
            .p_2()
            .gap_2()
            .child(Divider::horizontal())
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("build", "Build")
                            .style(ButtonStyle::Filled)
                            .disabled(!can_build)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.build(window, cx);
                            }))
                    )
                    .child(
                        Button::new("run", "Run")
                            .style(ButtonStyle::Filled)
                            .disabled(!can_build)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(window, cx);
                            }))
                    )
            )
            .child(
                Button::new("deploy", "Deploy to App Store")
                    .style(ButtonStyle::Subtle)
                    .full_width()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.deploy(window, cx);
                    }))
            )
    }
}

impl Focusable for NativePlatformsPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for NativePlatformsPanel {}

impl Render for NativePlatformsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("NativePlatformsPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().panel_background)
            .child(self.render_header(cx))
            .child(
                v_flex()
                    .id("native-platforms-content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(self.render_project_section(cx))
                    .child(self.render_scheme_section(cx))
                    .child(self.render_devices_section(cx))
            )
            .child(self.render_actions(cx))
    }
}

impl Panel for NativePlatformsPanel {
    fn persistent_name() -> &'static str {
        "NativePlatformsPanel"
    }

    fn panel_key() -> &'static str {
        NATIVE_PLATFORMS_PANEL_KEY
    }

    fn position(&self, _: &Window, _cx: &App) -> DockPosition {
        DockPosition::Left
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, _position: DockPosition, _: &mut Window, _cx: &mut Context<Self>) {
    }

    fn size(&self, _: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(260.0))
    }

    fn set_size(&mut self, size: Option<Pixels>, _: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        self.serialize(cx);
        cx.notify();
    }

    fn icon(&self, _: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Screen)
    }

    fn icon_tooltip(&self, _: &Window, _cx: &App) -> Option<&'static str> {
        Some("Native Platforms")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        3
    }
}
