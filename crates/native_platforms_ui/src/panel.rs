use crate::app_store_connect::AppStoreConnectTab;
use crate::build_controller::{BuildController, PipelineKind};
use anyhow::Result;
use db::kvp::KEY_VALUE_STORE;
use gpui::{
    Action, App, AsyncWindowContext, Context, DropdownSelectEvent, Entity, EventEmitter,
    FocusHandle, Focusable, FontWeight, NativeButtonStyle, NativeButtonTint, NativeProgressStyle,
    Pixels, Render, Subscription, Task, WeakEntity, Window, actions, div, native_button,
    native_dropdown, native_icon_button, native_progress_bar, px,
};
use native_platforms::apple::{build, simulator, xcode};
use native_platforms::{BuildConfiguration, Device, DeviceState};
use project::Project;
use serde::{Deserialize, Serialize};
use ui::prelude::*;
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

    devices: Vec<Device>,
    selected_device: Option<Device>,
    loading_devices: bool,

    controller: BuildController,

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
            devices: Vec::new(),
            selected_device: None,
            loading_devices: false,
            controller: BuildController::new(),
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
            .map(|wt| {
                let wt = wt.read(cx);
                wt.abs_path().to_path_buf()
            })
            .collect();

        if worktree_paths.is_empty() {
            return;
        }

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    for path in worktree_paths {
                        if let Some(detected_project) = xcode::detect_xcode_project(&path) {
                            let schemes = xcode::list_schemes(&detected_project).unwrap_or_default();
                            return Some((detected_project, schemes));
                        }
                    }
                    None
                })
                .await;

            this.update(cx, |this, cx| {
                if let Some((project, schemes)) = result {
                    this.xcode_project = Some(project);
                    if this.selected_scheme.is_none() && !schemes.is_empty() {
                        this.selected_scheme = Some(schemes[0].clone());
                    }
                    this.schemes = schemes;
                    cx.notify();
                }
            }).ok();
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

                    let mut all_devices = device::list_physical_devices();

                    let simulators = simulator::list_simulators().unwrap_or_default();
                    all_devices.extend(simulators);

                    all_devices
                })
                .await;

            this.update(cx, |this, cx| {
                this.devices = devices;
                this.loading_devices = false;

                if let Some(selected) = &this.selected_device {
                    let still_exists = this.devices.iter().any(|d| d.id == selected.id);
                    if !still_exists {
                        this.selected_device = None;
                    }
                }

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

    fn start_pipeline(&mut self, kind: PipelineKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(xcode_project) = &self.xcode_project else {
            return;
        };
        let Some(scheme) = &self.selected_scheme else {
            return;
        };

        let options = build::BuildOptions {
            scheme: scheme.clone(),
            configuration: BuildConfiguration::Debug,
            destination: self.selected_device.clone(),
            clean: false,
            derived_data_path: None,
        };

        let workspace = self.workspace.clone();
        let panel = cx.entity().downgrade();

        self.controller.start_pipeline(
            kind,
            xcode_project,
            options,
            workspace,
            panel,
            window,
            cx,
        );
        cx.notify();
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

    fn stop_build(&mut self, cx: &mut Context<Self>) {
        self.controller.stop();
        cx.notify();
    }

    fn terminate_app(&mut self, cx: &mut Context<Self>) {
        self.controller.terminate_app(cx);
        cx.notify();
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

    fn render_project_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let has_project = self.xcode_project.is_some();
        let (name, subtitle) = if let Some(project) = &self.xcode_project {
            let n = project
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string();
            (n, "Xcode Project")
        } else {
            (
                "No Project".to_string(),
                "Open a folder with an Xcode project",
            )
        };

        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if has_project {
                        colors.text
                    } else {
                        colors.text_muted
                    })
                    .min_w_0()
                    .overflow_hidden()
                    .child(name),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(subtitle),
            )
    }

    fn render_scheme_section(&self, cx: &Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let schemes = self.schemes.clone();
        let selected_index = self
            .selected_scheme
            .as_ref()
            .and_then(|selected| schemes.iter().position(|s| s == selected))
            .unwrap_or(0);
        let has_schemes = !schemes.is_empty();

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child("Scheme"),
            )
            .when(has_schemes, |el| {
                el.child(
                    native_dropdown("scheme-selector", &schemes)
                        .w_full()
                        .selected_index(selected_index)
                        .on_select(cx.listener(
                            |this, event: &DropdownSelectEvent, _, cx| {
                                let Some(scheme) =
                                    this.schemes.get(event.index).cloned()
                                else {
                                    return;
                                };
                                this.selected_scheme = Some(scheme);
                                this.serialize(cx);
                                cx.notify();
                            },
                        )),
                )
            })
            .when(!has_schemes, |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(colors.text_muted)
                        .child("No schemes available"),
                )
            })
    }

    fn render_devices_section(&self, cx: &Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let devices = self.devices.clone();
        let device_labels: Vec<String> = devices
            .iter()
            .map(|d| {
                let os = d.os_version.as_deref().unwrap_or("");
                let name_with_os = if os.is_empty() {
                    d.name.clone()
                } else {
                    format!("{} ({})", d.name, os)
                };
                match d.device_type {
                    native_platforms::DeviceType::PhysicalDevice => {
                        format!("{name_with_os} · Physical")
                    }
                    native_platforms::DeviceType::Simulator => {
                        format!("{name_with_os} · Simulator")
                    }
                }
            })
            .collect();
        let has_devices = !devices.is_empty();
        let loading = self.loading_devices;
        let selected_index = self
            .selected_device
            .as_ref()
            .and_then(|sel| devices.iter().position(|d| d.id == sel.id))
            .unwrap_or(0);

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(colors.text_muted)
                            .child("Destination"),
                    )
                    .child(
                        native_icon_button("refresh-devices", "arrow.clockwise")
                            .tooltip("Refresh Devices")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_devices(cx);
                            })),
                    ),
            )
            .when(loading, |el| {
                el.child(
                    native_progress_bar("devices-loading")
                        .indeterminate(true)
                        .progress_style(NativeProgressStyle::Bar)
                        .w_full()
                        .h(px(2.0)),
                )
            })
            .when(has_devices && !loading, |el| {
                el.child(
                    native_dropdown("device-selector", &device_labels)
                        .w_full()
                        .selected_index(selected_index)
                        .on_select(cx.listener(
                            |this, event: &DropdownSelectEvent, _, cx| {
                                let Some(device) =
                                    this.devices.get(event.index).cloned()
                                else {
                                    return;
                                };
                                this.selected_device = Some(device);
                                this.serialize(cx);
                                cx.notify();
                            },
                        )),
                )
            })
            .when(!has_devices && !loading, |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(colors.text_muted)
                        .child("No devices available"),
                )
            })
    }

    fn render_footer(&self, cx: &Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let has_project = self.xcode_project.is_some();
        let has_scheme = self.selected_scheme.is_some();
        let is_active = self.controller.is_active();
        let can_build = has_project && has_scheme && !is_active;
        let has_launched = self.controller.last_launched().is_some() && !is_active;

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .border_t_1()
            .border_color(colors.border)
            .p(px(12.0))
            .when(is_active, |el| {
                el.child(
                    native_progress_bar("build-progress")
                        .indeterminate(true)
                        .progress_style(NativeProgressStyle::Bar)
                        .w_full()
                        .h(px(2.0)),
                )
            })
            .when(is_active, |el| {
                el.child(
                    native_button("stop", "Stop")
                        .button_style(NativeButtonStyle::Filled)
                        .tint(NativeButtonTint::Destructive)
                        .w_full()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.stop_build(cx);
                        })),
                )
            })
            .when(!is_active, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            native_button("build", "Build")
                                .button_style(NativeButtonStyle::Rounded)
                                .flex_1()
                                .disabled(!can_build)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.start_pipeline(
                                        PipelineKind::Build,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            native_button("run", "Run")
                                .button_style(NativeButtonStyle::Filled)
                                .tint(NativeButtonTint::Accent)
                                .flex_1()
                                .disabled(!can_build)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.start_pipeline(
                                        PipelineKind::Run,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .when(has_launched, |el| {
                            el.child(
                                native_icon_button("terminate", "xmark.circle")
                                    .tooltip("Terminate App")
                                    .tint(NativeButtonTint::Warning)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.terminate_app(cx);
                                    })),
                            )
                        }),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_center()
                    .pt(px(2.0))
                    .child(
                        native_button("deploy", "Deploy to App Store")
                            .button_style(NativeButtonStyle::Borderless)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.deploy(window, cx);
                            })),
                    ),
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
        self.controller.poll_completion();

        div()
            .key_context("NativePlatformsPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .bg(cx.theme().colors().panel_background)
            .flex()
            .flex_col()
            .child(
                div()
                    .id("native-platforms-content")
                    .flex_1()
                    .min_w_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .p(px(12.0))
                    .gap(px(16.0))
                    .child(self.render_project_header(cx))
                    .child(self.render_scheme_section(cx))
                    .child(self.render_devices_section(cx)),
            )
            .child(self.render_footer(cx))
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
        self.width.unwrap_or(px(300.0))
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
