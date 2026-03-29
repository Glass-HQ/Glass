use gpui::{
    Action, App, Context, Entity, IntoElement, MouseButton, NativeImageScaling, NativeMenuItem,
    ParentElement, Pixels, Point, Render, SharedString, Styled, Subscription, WeakEntity, Window,
    div, native_image_view, prelude::*, px, show_native_popup_menu,
};
#[cfg(not(target_os = "macos"))]
use gpui::{native_tracking_view, rems};
use ui::{IconButtonShape, Tooltip, prelude::*};
use workspace::{Workspace, WorkspaceSidebarSection};
use workspace_chrome::SidebarRow;
use workspace_modes::ModeId;

use super::BrowserView;

#[cfg(not(target_os = "macos"))]
const SIDEBAR_WIDTH_PX: f32 = 200.0;

fn show_tab_context_menu(
    view: WeakEntity<BrowserView>,
    index: usize,
    is_pinned: bool,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let mut menu_items = Vec::new();
    menu_items.push(if is_pinned {
        NativeMenuItem::action("Unpin Tab")
    } else {
        NativeMenuItem::action("Pin Tab")
    });
    menu_items.push(NativeMenuItem::separator());
    menu_items.push(NativeMenuItem::action("Close Tab"));
    let close_others_index = menu_items.len();
    menu_items.push(NativeMenuItem::action("Close Other Tabs"));
    let bookmark_index = if !is_pinned {
        menu_items.push(NativeMenuItem::separator());
        let index = menu_items.len();
        menu_items.push(NativeMenuItem::action("Bookmark This Page"));
        Some(index)
    } else {
        None
    };

    show_native_popup_menu(
        &menu_items,
        position,
        window,
        cx,
        move |action_index, window, cx| {
            if action_index == 0 {
                if is_pinned {
                    view.update(cx, |this, cx| {
                        this.unpin_tab_at(index, cx);
                    })
                    .ok();
                } else {
                    view.update(cx, |this, cx| {
                        this.pin_tab_at(index, cx);
                    })
                    .ok();
                }
                return;
            }

            if action_index == 2 {
                view.update(cx, |this, cx| {
                    this.close_tab_at(index, window, cx);
                })
                .ok();
                return;
            }

            if action_index == close_others_index {
                view.update(cx, |this, cx| {
                    this.close_other_tabs_at(index, cx);
                })
                .ok();
                return;
            }

            if bookmark_index == Some(action_index) {
                view.update(cx, |this, cx| {
                    this.toggle_bookmark_at(index, cx);
                })
                .ok();
            }
        },
    );
}

fn render_tab_favicon(id: SharedString, favicon_url: Option<&str>, _cx: &App) -> gpui::AnyElement {
    if let Some(url) = favicon_url {
        native_image_view(id)
            .image_uri(url.to_string())
            .scaling(NativeImageScaling::ScaleUpOrDown)
            .size(px(14.))
            .rounded_sm()
            .flex_shrink_0()
            .into_any_element()
    } else {
        native_image_view(id)
            .sf_symbol("globe")
            .scaling(NativeImageScaling::ScaleUpOrDown)
            .size(px(14.))
            .flex_shrink_0()
            .into_any_element()
    }
}

pub struct BrowserSidebarPanel {
    browser_view: WeakEntity<BrowserView>,
    hovered_tab_index: Option<usize>,
    hovered_close_tab_index: Option<usize>,
    hovered_new_tab_button: bool,
    _subscriptions: Vec<Subscription>,
}

impl BrowserSidebarPanel {
    pub(super) fn new(browser_view: WeakEntity<BrowserView>, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(browser_entity) = browser_view.upgrade() {
            subscriptions.push(cx.observe(&browser_entity, |_this, _entity, cx| {
                cx.notify();
            }));
        }

        Self {
            browser_view,
            hovered_tab_index: None,
            hovered_close_tab_index: None,
            hovered_new_tab_button: false,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn clear_hover_state(&mut self, cx: &mut Context<Self>) {
        if self.hovered_tab_index.is_some()
            || self.hovered_close_tab_index.is_some()
            || self.hovered_new_tab_button
        {
            self.hovered_tab_index = None;
            self.hovered_close_tab_index = None;
            self.hovered_new_tab_button = false;
            cx.notify();
        }
    }
}

impl Render for BrowserSidebarPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let Some(browser_view) = self.browser_view.upgrade() else {
            return v_flex().size_full().into_any_element();
        };

        let browser_view_data = browser_view.read(cx);
        let active_tab_index = browser_view_data.active_tab_index;
        let active_mode = Workspace::for_window(window, cx)
            .map(|workspace| workspace.read(cx).active_mode_id())
            .unwrap_or(ModeId::BROWSER);
        let open_browser_label = match active_mode {
            ModeId::TERMINAL => Some("Show Browser in Terminal"),
            ModeId::BROWSER => None,
            _ => Some("Show Browser in Editor"),
        };

        v_flex()
            .size_full()
            .bg(theme.colors().editor_background)
            .child(
                v_flex()
                    .id("native-sidebar-tab-list")
                    .flex_1()
                    .items_stretch()
                    .overflow_y_scroll()
                    .p_1()
                    .gap_1()
                    .children(
                        browser_view_data
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(index, tab)| {
                                let tab_data = tab.read(cx);
                                let title = tab_data.title().to_string();
                                let favicon_url = tab_data.favicon_url();
                                let is_pinned = tab_data.is_pinned();
                                let is_active = index == active_tab_index;
                                let tab_id =
                                    SharedString::from(tab.entity_id().as_u64().to_string());
                                let close_tab_id = tab_id.clone();
                                let activate_tab_id = tab_id.clone();
                                let favicon_element = render_tab_favicon(
                                    SharedString::from(format!(
                                        "native-sidebar-tab-favicon-{index}"
                                    )),
                                    favicon_url,
                                    cx,
                                );

                                let displayed_title = if title.len() > 32 {
                                    let truncated_title = match title.char_indices().nth(29) {
                                        Some((byte_index, _)) => &title[..byte_index],
                                        None => &title,
                                    };
                                    format!("{truncated_title}...")
                                } else {
                                    title
                                };
                                let context_menu_view = browser_view.clone().downgrade();

                                let row = SidebarRow::new(
                                    format!("native-sidebar-tab-{index}"),
                                    displayed_title,
                                    IconName::Globe,
                                )
                                .start_slot(favicon_element)
                                .selected(is_active)
                                .end_slot(if is_pinned {
                                    Icon::new(IconName::Pin)
                                        .size(IconSize::Small)
                                        .color(Color::Muted)
                                        .into_any_element()
                                } else {
                                    IconButton::new(
                                        SharedString::from(format!(
                                            "native-sidebar-close-tab-{index}"
                                        )),
                                        IconName::Close,
                                    )
                                    .shape(IconButtonShape::Square)
                                    .icon_size(IconSize::XSmall)
                                    .icon_color(Color::Muted)
                                    .tooltip(Tooltip::text("Close tab"))
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        let Some(workspace) = Workspace::for_window(window, cx)
                                        else {
                                            return;
                                        };
                                        workspace.update(cx, |workspace, cx| {
                                            workspace.close_sidebar_entry(
                                                WorkspaceSidebarSection::BrowserTabs,
                                                &close_tab_id,
                                                window,
                                                cx,
                                            );
                                        });
                                    })
                                    .into_any_element()
                                })
                                .on_click(move |_, window, cx| {
                                    let Some(workspace) = Workspace::for_window(window, cx) else {
                                        return;
                                    };
                                    workspace.update(cx, |workspace, cx| {
                                        workspace.activate_sidebar_entry(
                                            WorkspaceSidebarSection::BrowserTabs,
                                            &activate_tab_id,
                                            window,
                                            cx,
                                        );
                                    });
                                });

                                div().w_full().child(row).on_mouse_down(
                                    MouseButton::Right,
                                    move |event, window, cx| {
                                        show_tab_context_menu(
                                            context_menu_view.clone(),
                                            index,
                                            is_pinned,
                                            event.position,
                                            window,
                                            cx,
                                        );
                                    },
                                )
                            }),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .p_1()
                    .gap_1()
                    .child(
                        SidebarRow::new("native-sidebar-new-tab-button", "New Tab", IconName::Plus)
                            .centered()
                            .on_click(move |_, window, cx| {
                                let Some(workspace) = Workspace::for_window(window, cx) else {
                                    return;
                                };
                                workspace.update(cx, |workspace, cx| {
                                    workspace.create_sidebar_entry(
                                        WorkspaceSidebarSection::BrowserTabs,
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .when_some(open_browser_label, |this, label| {
                        this.child(
                            SidebarRow::new(
                                "native-sidebar-open-browser-surface",
                                label,
                                IconName::Globe,
                            )
                            .centered()
                            .on_click(|_, window, cx| {
                                window.dispatch_action(super::OpenBrowserPane.boxed_clone(), cx);
                            }),
                        )
                    }),
            )
            .into_any_element()
    }
}

impl BrowserView {
    #[cfg(not(target_os = "macos"))]
    pub(super) fn render_tab_strip(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active_index = self.active_tab_index;
        let view = cx.entity().downgrade();

        let pinned_count = self.tabs.iter().filter(|t| t.read(cx).is_pinned()).count();

        h_flex()
            .w_full()
            .h(px(34.))
            .px_1()
            .gap_1()
            .items_center()
            .flex_shrink_0()
            // Pinned tabs dock
            .when(pinned_count > 0, |this| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .px_1()
                        .h(px(28.))
                        .rounded(cx.theme().component_radius().tab.unwrap_or(px(8.0)))
                        .bg(theme.colors().text.opacity(0.06))
                        .border_1()
                        .border_color(theme.colors().border.opacity(0.4))
                        .children(self.tabs.iter().enumerate().take(pinned_count).map(
                            |(index, tab)| {
                                let tab_data = tab.read(cx);
                                let favicon_url = tab_data.favicon_url();
                                let is_active = index == active_index;
                                let is_hovered = self.hovered_top_tab_index == Some(index);
                                let selected_bg = theme.colors().text.opacity(0.14);
                                let hover_bg = theme.colors().text.opacity(0.09);

                                let favicon_element = render_tab_favicon(
                                    SharedString::from(format!("browser-tab-favicon-{index}")),
                                    favicon_url,
                                    cx,
                                );

                                let hover_view = view.clone();
                                let context_view = view.clone();
                                div()
                                    .id(("browser-tab-inner", index))
                                    .relative()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(22.))
                                    .w(px(30.))
                                    .flex_shrink_0()
                                    .rounded(
                                        cx.theme().component_radius().button.unwrap_or(px(4.0)),
                                    )
                                    .cursor_pointer()
                                    .when(is_active, |this| this.bg(selected_bg))
                                    .when(is_hovered && !is_active, |this| this.bg(hover_bg))
                                    .when(!is_active, |this| {
                                        this.hover(move |style| style.bg(hover_bg))
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.switch_to_tab(index, window, cx);
                                    }))
                                    .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                        show_tab_context_menu(
                                            context_view.clone(),
                                            index,
                                            true,
                                            event.position,
                                            window,
                                            cx,
                                        );
                                    })
                                    .child(favicon_element)
                                    .child(
                                        native_tracking_view(format!("browser-tab-track-{index}"))
                                            .on_mouse_enter(move |_, _window, cx| {
                                                hover_view
                                                    .update(cx, |this, cx| {
                                                        if this.hovered_top_tab_index != Some(index)
                                                        {
                                                            this.hovered_top_tab_index =
                                                                Some(index);
                                                            cx.notify();
                                                        }
                                                    })
                                                    .ok();
                                            })
                                            .on_mouse_exit({
                                                let hover_view = view.clone();
                                                move |_, _window, cx| {
                                                    hover_view
                                                        .update(cx, |this, cx| {
                                                            if this.hovered_top_tab_index
                                                                == Some(index)
                                                            {
                                                                this.hovered_top_tab_index = None;
                                                                this.hovered_top_tab_close_index =
                                                                    None;
                                                                cx.notify();
                                                            }
                                                        })
                                                        .ok();
                                                }
                                            })
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .size_full(),
                                    )
                                    .into_any_element()
                            },
                        )),
                )
            })
            // Unpinned tabs
            .children(
                self.tabs
                    .iter()
                    .enumerate()
                    .skip(pinned_count)
                    .map(|(index, tab)| {
                        let tab_data = tab.read(cx);
                        let title = tab_data.title().to_string();
                        let favicon_url = tab_data.favicon_url();
                        let is_pinned = tab_data.is_pinned();
                        let is_active = index == active_index;
                        let is_hovered = self.hovered_top_tab_index == Some(index);
                        let is_close_hovered = self.hovered_top_tab_close_index == Some(index);
                        let selected_bg = theme.colors().text.opacity(0.14);
                        let hover_bg = theme.colors().text.opacity(0.09);

                        let favicon_element = render_tab_favicon(
                            SharedString::from(format!("browser-tab-favicon-{index}")),
                            favicon_url,
                            cx,
                        );

                        let display_title = if title.len() > 24 {
                            let truncated = match title.char_indices().nth(21) {
                                Some((byte_index, _)) => &title[..byte_index],
                                None => &title,
                            };
                            format!("{truncated}...")
                        } else {
                            title
                        };

                        let hover_view = view.clone();
                        let context_view = view.clone();
                        div()
                            .id(("browser-tab-inner", index))
                            .relative()
                            .flex()
                            .items_center()
                            .h(px(24.))
                            .px_2()
                            .gap_1()
                            .min_w(px(92.))
                            .max_w(px(220.))
                            .rounded(cx.theme().component_radius().tab.unwrap_or(px(8.0)))
                            .cursor_pointer()
                            .when(is_active, |this| this.bg(selected_bg))
                            .when(is_hovered && !is_active, |this| this.bg(hover_bg))
                            .when(!is_active, |this| {
                                this.hover(move |style| style.bg(hover_bg))
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.switch_to_tab(index, window, cx);
                            }))
                            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                show_tab_context_menu(
                                    context_view.clone(),
                                    index,
                                    is_pinned,
                                    event.position,
                                    window,
                                    cx,
                                );
                            })
                            .child(favicon_element)
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(rems(0.75))
                                    .text_color(if is_active {
                                        theme.colors().text
                                    } else {
                                        theme.colors().text_muted
                                    })
                                    .child(display_title),
                            )
                            .when(is_hovered, |this| {
                                let close_hover_view = view.clone();
                                this.child(
                                    div()
                                        .id(SharedString::from(format!("close-tab-{index}")))
                                        .relative()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(16.))
                                        .h(px(16.))
                                        .rounded(
                                            cx.theme().component_radius().button.unwrap_or(px(4.0)),
                                        )
                                        .cursor_pointer()
                                        .when(is_close_hovered, |this| this.bg(hover_bg))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.close_tab_at(index, window, cx);
                                        }))
                                        .child(
                                            native_image_view(SharedString::from(format!(
                                                "close-tab-icon-{index}"
                                            )))
                                            .sf_symbol("xmark")
                                            .w(px(8.))
                                            .h(px(8.)),
                                        )
                                        .child(
                                            native_tracking_view(format!(
                                                "close-tab-track-{index}"
                                            ))
                                            .on_mouse_enter(move |_, _window, cx| {
                                                close_hover_view
                                                    .update(cx, |this, cx| {
                                                        if this.hovered_top_tab_close_index
                                                            != Some(index)
                                                        {
                                                            this.hovered_top_tab_close_index =
                                                                Some(index);
                                                            cx.notify();
                                                        }
                                                    })
                                                    .ok();
                                            })
                                            .on_mouse_exit({
                                                let close_hover_view = view.clone();
                                                move |_, _window, cx| {
                                                    close_hover_view
                                                        .update(cx, |this, cx| {
                                                            if this.hovered_top_tab_close_index
                                                                == Some(index)
                                                            {
                                                                this.hovered_top_tab_close_index =
                                                                    None;
                                                                cx.notify();
                                                            }
                                                        })
                                                        .ok();
                                                }
                                            })
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .size_full(),
                                        ),
                                )
                            })
                            .child(
                                native_tracking_view(format!("browser-tab-track-{index}"))
                                    .on_mouse_enter(move |_, _window, cx| {
                                        hover_view
                                            .update(cx, |this, cx| {
                                                if this.hovered_top_tab_index != Some(index) {
                                                    this.hovered_top_tab_index = Some(index);
                                                    cx.notify();
                                                }
                                            })
                                            .ok();
                                    })
                                    .on_mouse_exit({
                                        let hover_view = view.clone();
                                        move |_, _window, cx| {
                                            hover_view
                                                .update(cx, |this, cx| {
                                                    if this.hovered_top_tab_index == Some(index) {
                                                        this.hovered_top_tab_index = None;
                                                        this.hovered_top_tab_close_index = None;
                                                        cx.notify();
                                                    }
                                                })
                                                .ok();
                                        }
                                    })
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full(),
                            )
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .id("new-tab-button")
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(20.))
                    .h(px(20.))
                    .rounded(cx.theme().component_radius().button.unwrap_or(px(4.0)))
                    .cursor_pointer()
                    .when(self.hovered_top_new_tab_button, |this| {
                        this.bg(theme.colors().text.opacity(0.09))
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_tab(cx);
                        this.update_toolbar_active_tab(window, cx);
                        cx.notify();
                    }))
                    .child(
                        native_image_view("new-tab-plus-icon")
                            .sf_symbol("plus")
                            .w(px(10.))
                            .h(px(10.)),
                    )
                    .child(
                        native_tracking_view("new-tab-button-track")
                            .on_mouse_enter({
                                let view = view.clone();
                                move |_, _window, cx| {
                                    view.update(cx, |this, cx| {
                                        if !this.hovered_top_new_tab_button {
                                            this.hovered_top_new_tab_button = true;
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                                }
                            })
                            .on_mouse_exit({
                                let view = view.clone();
                                move |_, _window, cx| {
                                    view.update(cx, |this, cx| {
                                        if this.hovered_top_new_tab_button {
                                            this.hovered_top_new_tab_button = false;
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                                }
                            })
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    ),
            )
    }

    pub fn ensure_native_sidebar_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Entity<BrowserSidebarPanel> {
        if let Some(sidebar_panel) = &self.native_sidebar_panel {
            return sidebar_panel.clone();
        }

        let browser_view = cx.entity().downgrade();
        let sidebar_panel = cx.new(|cx| BrowserSidebarPanel::new(browser_view, cx));
        self.native_sidebar_panel = Some(sidebar_panel.clone());
        sidebar_panel
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active_index = self.active_tab_index;
        let view = cx.entity().downgrade();

        let pinned_count = self.tabs.iter().filter(|t| t.read(cx).is_pinned()).count();
        let grid_cols = pinned_count.min(3);

        v_flex()
            .h_full()
            .w(px(SIDEBAR_WIDTH_PX))
            .flex_shrink_0()
            .items_stretch()
            .bg(theme.colors().title_bar_background)
            .border_r_1()
            .border_color(theme.colors().border)
            .child(
                v_flex()
                    .id("sidebar-tab-list")
                    .flex_1()
                    .items_stretch()
                    .overflow_y_scroll()
                    .p_1()
                    .gap_1()
                    .when(pinned_count > 0, |this| {
                        let pinned_rows: Vec<Vec<usize>> = (0..pinned_count)
                            .collect::<Vec<_>>()
                            .chunks(grid_cols)
                            .map(|chunk| chunk.to_vec())
                            .collect();

                        this.child(
                            v_flex().gap_1().children(
                                pinned_rows.into_iter().map(|row| {
                                    h_flex()
                                        .gap_1()
                                        .children(row.into_iter().map(|index| {
                                            let tab = &self.tabs[index];
                                            let tab_data = tab.read(cx);
                                            let favicon_url = tab_data.favicon_url();
                                            let is_active = index == active_index;
                                            let is_hovered =
                                                self.hovered_sidebar_tab_index == Some(index);
                                            let selected_bg =
                                                theme.colors().text.opacity(0.14);
                                            let hover_bg = theme.colors().text.opacity(0.09);

                                            let favicon_element = render_tab_favicon(
                                                SharedString::from(format!(
                                                    "sidebar-tab-favicon-{index}"
                                                )),
                                                favicon_url,
                                                cx,
                                            );

                                            let hover_view = view.clone();
                                            let context_view = view.clone();

                                            div()
                                                .id(("sidebar-tab-inner", index))
                                                .relative()
                                                .flex()
                                                .flex_1()
                                                .items_center()
                                                .justify_center()
                                                .h(px(36.))
                                                .flex_shrink_0()
                                                .rounded(cx.theme().component_radius().tab.unwrap_or(px(8.0)))
                                                .cursor_pointer()
                                                .when(is_active, |this| {
                                                    this.bg(selected_bg)
                                                })
                                                .when(is_hovered && !is_active, |this| {
                                                    this.bg(hover_bg)
                                                })
                                                .when(!is_active, |this| {
                                                    this.hover(move |style| {
                                                        style.bg(hover_bg)
                                                    })
                                                })
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.switch_to_tab(
                                                            index, window, cx,
                                                        );
                                                    },
                                                ))
                                                .on_mouse_down(
                                                    MouseButton::Right,
                                                    move |event, window, cx| {
                                                        show_tab_context_menu(
                                                            context_view.clone(),
                                                            index,
                                                            true,
                                                            event.position,
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                )
                                                .child(favicon_element)
                                                .child(
                                                    native_tracking_view(format!(
                                                        "sidebar-tab-track-{index}"
                                                    ))
                                                    .on_mouse_enter(
                                                        move |_, _window, cx| {
                                                            hover_view
                                                                .update(cx, |this, cx| {
                                                                    if this
                                                                        .hovered_sidebar_tab_index
                                                                        != Some(index)
                                                                    {
                                                                        this.hovered_sidebar_tab_index = Some(index);
                                                                        cx.notify();
                                                                    }
                                                                })
                                                                .ok();
                                                        },
                                                    )
                                                    .on_mouse_exit({
                                                        let hover_view = view.clone();
                                                        move |_, _window, cx| {
                                                            hover_view
                                                                .update(cx, |this, cx| {
                                                                    if this
                                                                        .hovered_sidebar_tab_index
                                                                        == Some(index)
                                                                    {
                                                                        this.hovered_sidebar_tab_index = None;
                                                                        this.hovered_sidebar_tab_close_index = None;
                                                                        cx.notify();
                                                                    }
                                                                })
                                                                .ok();
                                                        }
                                                    })
                                                    .absolute()
                                                    .top_0()
                                                    .left_0()
                                                    .size_full(),
                                                )
                                                .into_any_element()
                                        }))
                                        .into_any_element()
                                }),
                            ),
                        )
                    })
                    .children(self.tabs.iter().enumerate().skip(pinned_count).map(
                        |(index, tab)| {
                            let tab_data = tab.read(cx);
                            let title = tab_data.title().to_string();
                            let favicon_url = tab_data.favicon_url();
                            let is_pinned = tab_data.is_pinned();
                            let is_active = index == active_index;
                            let is_hovered = self.hovered_sidebar_tab_index == Some(index);
                            let is_close_hovered =
                                self.hovered_sidebar_tab_close_index == Some(index);
                            let selected_bg = theme.colors().text.opacity(0.14);
                            let hover_bg = theme.colors().text.opacity(0.09);

                            let favicon_element = render_tab_favicon(
                                SharedString::from(format!("sidebar-tab-favicon-{index}")),
                                favicon_url,
                                cx,
                            );

                            let display_title = if title.len() > 24 {
                                let truncated = match title.char_indices().nth(21) {
                                    Some((byte_index, _)) => &title[..byte_index],
                                    None => &title,
                                };
                                format!("{truncated}...")
                            } else {
                                title
                            };

                            let hover_view = view.clone();
                            let context_view = view.clone();
                            let tab_content = div()
                                .id(("sidebar-tab-inner", index))
                                .relative()
                                .flex()
                                .items_center()
                                .w(px(SIDEBAR_WIDTH_PX - 8.0))
                                .h(px(28.))
                                .px_2()
                                .gap_1()
                                .flex_shrink_0()
                                .rounded(cx.theme().component_radius().tab.unwrap_or(px(8.0)))
                                .cursor_pointer()
                                .when(is_active, |this| this.bg(selected_bg))
                                .when(is_hovered && !is_active, |this| this.bg(hover_bg))
                                .when(!is_active, |this| {
                                    this.hover(move |style| style.bg(hover_bg))
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.switch_to_tab(index, window, cx);
                                }))
                                .child(favicon_element)
                                .child(
                                    div()
                                        .flex_1()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(rems(0.75))
                                        .text_color(if is_active {
                                            theme.colors().text
                                        } else {
                                            theme.colors().text_muted
                                        })
                                        .child(display_title),
                                )
                                .when(is_hovered, |this| {
                                    let close_hover_view = view.clone();
                                    this.child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "sidebar-close-tab-{index}"
                                            )))
                                            .relative()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(16.))
                                            .h(px(16.))
                                            .rounded(cx.theme().component_radius().button.unwrap_or(px(4.0)))
                                            .cursor_pointer()
                                            .when(is_close_hovered, |this| this.bg(hover_bg))
                                            .on_click(cx.listener(
                                                move |this, _, window, cx| {
                                                    this.close_tab_at(index, window, cx);
                                                },
                                            ))
                                            .child(
                                                native_image_view(SharedString::from(format!(
                                                    "sidebar-close-tab-icon-{index}"
                                                )))
                                                .sf_symbol("xmark")
                                                .w(px(8.))
                                                .h(px(8.)),
                                            )
                                            .child(
                                                native_tracking_view(format!(
                                                    "sidebar-close-tab-track-{index}"
                                                ))
                                                .on_mouse_enter(move |_, _window, cx| {
                                                    close_hover_view
                                                        .update(cx, |this, cx| {
                                                            if this
                                                                .hovered_sidebar_tab_close_index
                                                                != Some(index)
                                                            {
                                                                this.hovered_sidebar_tab_close_index =
                                                                    Some(index);
                                                                cx.notify();
                                                            }
                                                        })
                                                        .ok();
                                                })
                                                .on_mouse_exit({
                                                    let close_hover_view = view.clone();
                                                    move |_, _window, cx| {
                                                        close_hover_view
                                                            .update(cx, |this, cx| {
                                                                if this
                                                                    .hovered_sidebar_tab_close_index
                                                                    == Some(index)
                                                                {
                                                                    this.hovered_sidebar_tab_close_index =
                                                                        None;
                                                                    cx.notify();
                                                                }
                                                            })
                                                            .ok();
                                                    }
                                                })
                                                .absolute()
                                                .top_0()
                                                .left_0()
                                                .size_full(),
                                            ),
                                    )
                                })
                                .child(
                                    native_tracking_view(format!("sidebar-tab-track-{index}"))
                                        .on_mouse_enter(move |_, _window, cx| {
                                            hover_view
                                                .update(cx, |this, cx| {
                                                    if this.hovered_sidebar_tab_index
                                                        != Some(index)
                                                    {
                                                        this.hovered_sidebar_tab_index =
                                                            Some(index);
                                                        cx.notify();
                                                    }
                                                })
                                                .ok();
                                        })
                                        .on_mouse_exit({
                                            let hover_view = view.clone();
                                            move |_, _window, cx| {
                                                hover_view
                                                    .update(cx, |this, cx| {
                                                        if this.hovered_sidebar_tab_index
                                                            == Some(index)
                                                        {
                                                            this.hovered_sidebar_tab_index = None;
                                                            this.hovered_sidebar_tab_close_index =
                                                                None;
                                                            cx.notify();
                                                        }
                                                    })
                                                    .ok();
                                            }
                                        })
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .size_full(),
                                );

                            div().w_full().child(
                                tab_content.on_mouse_down(
                                    MouseButton::Right,
                                    move |event, window, cx| {
                                        show_tab_context_menu(
                                            context_view.clone(),
                                            index,
                                            is_pinned,
                                            event.position,
                                            window,
                                            cx,
                                        );
                                    },
                                ),
                            )
                        },
                    )),
            )
            .child(
                div()
                    .w_full()
                    .p_1()
                    .child(
                        div()
                            .id("sidebar-new-tab-button")
                            .relative()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(SIDEBAR_WIDTH_PX - 8.0))
                            .h(px(28.))
                            .flex_shrink_0()
                            .rounded(cx.theme().component_radius().tab.unwrap_or(px(8.0)))
                            .cursor_pointer()
                            .when(self.hovered_sidebar_new_tab_button, |this| {
                                this.bg(theme.colors().text.opacity(0.09))
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_tab(cx);
                                this.update_toolbar_active_tab(window, cx);
                                cx.notify();
                            }))
                            .child(
                                native_image_view("sidebar-new-tab-plus-icon")
                                    .sf_symbol("plus")
                                    .w(px(10.))
                                    .h(px(10.)),
                            )
                            .child(
                                native_tracking_view("sidebar-new-tab-button-track")
                                    .on_mouse_enter({
                                        let view = view.clone();
                                        move |_, _window, cx| {
                                            view.update(cx, |this, cx| {
                                                if !this.hovered_sidebar_new_tab_button {
                                                    this.hovered_sidebar_new_tab_button = true;
                                                    cx.notify();
                                                }
                                            })
                                            .ok();
                                        }
                                    })
                                    .on_mouse_exit({
                                        let view = view.clone();
                                        move |_, _window, cx| {
                                            view.update(cx, |this, cx| {
                                                if this.hovered_sidebar_new_tab_button {
                                                    this.hovered_sidebar_new_tab_button = false;
                                                    cx.notify();
                                                }
                                            })
                                            .ok();
                                        }
                                    })
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full(),
                            ),
                    ),
            )
    }
}
