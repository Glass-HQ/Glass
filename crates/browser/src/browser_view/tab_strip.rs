use gpui::{
    Context, IntoElement, ParentElement, SharedString, SharedUri, Styled, div, img,
    native_icon_button, prelude::*, px, rems,
};
use ui::{Icon, IconName, IconSize, prelude::*};

use super::BrowserView;

const SIDEBAR_WIDTH_PX: f32 = 200.0;

impl BrowserView {
    pub(super) fn render_tab_strip(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active_index = self.active_tab_index;
        let view = cx.entity().downgrade();

        h_flex()
            .w_full()
            .h(px(30.))
            .flex_shrink_0()
            .bg(theme.colors().title_bar_background)
            .border_b_1()
            .border_color(theme.colors().border)
            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                let tab_data = tab.read(cx);
                let title = tab_data.title().to_string();
                let favicon_url = tab_data.favicon_url().map(|s| s.to_string());
                let is_pinned = tab_data.is_pinned();
                let is_active = index == active_index;

                let favicon_element = if let Some(ref url) = favicon_url {
                    img(SharedUri::from(url.clone()))
                        .size(px(14.))
                        .rounded_sm()
                        .flex_shrink_0()
                        .into_any_element()
                } else {
                    Icon::new(IconName::Globe)
                        .size(IconSize::XSmall)
                        .color(Color::Muted)
                        .into_any_element()
                };

                let tab_content = if is_pinned {
                    div()
                        .id(("browser-tab-inner", index))
                        .flex()
                        .items_center()
                        .justify_center()
                        .h_full()
                        .w(px(36.))
                        .flex_shrink_0()
                        .border_r_1()
                        .border_color(theme.colors().border)
                        .cursor_pointer()
                        .when(is_active, |this| this.bg(theme.colors().editor_background))
                        .when(!is_active, |this| {
                            this.hover(|style| style.bg(theme.colors().ghost_element_hover))
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.switch_to_tab(index, window, cx);
                        }))
                        .child(favicon_element)
                        .into_any_element()
                } else {
                    let display_title = if title.len() > 24 {
                        let truncated = match title.char_indices().nth(21) {
                            Some((byte_index, _)) => &title[..byte_index],
                            None => &title,
                        };
                        format!("{truncated}...")
                    } else {
                        title
                    };

                    div()
                        .id(("browser-tab-inner", index))
                        .flex()
                        .items_center()
                        .h_full()
                        .px_2()
                        .gap_1()
                        .min_w(px(80.))
                        .max_w(px(200.))
                        .border_r_1()
                        .border_color(theme.colors().border)
                        .cursor_pointer()
                        .when(is_active, |this| this.bg(theme.colors().editor_background))
                        .when(!is_active, |this| {
                            this.hover(|style| style.bg(theme.colors().ghost_element_hover))
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
                        .child(
                            native_icon_button(
                                SharedString::from(format!("close-tab-{index}")),
                                "xmark",
                            )
                            .size(px(16.))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_tab_at(index, window, cx);
                            }))
                            .tooltip("Close Tab"),
                        )
                        .into_any_element()
                };

                let view = view.clone();
                ui::right_click_menu(("tab-ctx-menu", index))
                    .trigger(move |_, _, _| tab_content)
                    .menu(move |window, cx| {
                        let view = view.clone();
                        ui::ContextMenu::build(window, cx, move |mut menu, _window, _cx| {
                            if is_pinned {
                                let view = view.clone();
                                menu = menu.entry("Unpin Tab", None, move |_window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.unpin_tab_at(index, cx);
                                    })
                                    .ok();
                                });
                            } else {
                                let view = view.clone();
                                menu = menu.entry("Pin Tab", None, move |_window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.pin_tab_at(index, cx);
                                    })
                                    .ok();
                                });
                            }
                            menu = menu.separator();
                            {
                                let view = view.clone();
                                menu = menu.entry("Close Tab", None, move |_window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.close_tab_at_inner(index, cx);
                                    })
                                    .ok();
                                });
                            }
                            {
                                let view = view.clone();
                                menu = menu.entry("Close Other Tabs", None, move |_window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.close_other_tabs_at(index, cx);
                                    })
                                    .ok();
                                });
                            }
                            if !is_pinned {
                                menu = menu.separator();
                                menu =
                                    menu.entry("Bookmark This Page", None, move |_window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.toggle_bookmark_at(index, cx);
                                        })
                                        .ok();
                                    });
                            }
                            menu
                        })
                    })
            }))
            .child(
                native_icon_button("new-tab-button", "plus")
                    .size(px(20.))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_tab(cx);
                        this.update_toolbar_active_tab(window, cx);
                        cx.notify();
                    }))
                    .tooltip("New Tab"),
            )
    }

    pub(super) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active_index = self.active_tab_index;
        let view = cx.entity().downgrade();

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
                    .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                        let tab_data = tab.read(cx);
                        let title = tab_data.title().to_string();
                        let favicon_url = tab_data.favicon_url().map(|s| s.to_string());
                        let is_pinned = tab_data.is_pinned();
                        let is_active = index == active_index;

                        let favicon_element = if let Some(ref url) = favicon_url {
                            img(SharedUri::from(url.clone()))
                                .size(px(14.))
                                .rounded_sm()
                                .flex_shrink_0()
                                .into_any_element()
                        } else {
                            Icon::new(IconName::Globe)
                                .size(IconSize::XSmall)
                                .color(Color::Muted)
                                .into_any_element()
                        };

                        let tab_content = if is_pinned {
                            div()
                                .id(("sidebar-tab-inner", index))
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(SIDEBAR_WIDTH_PX))
                                .h(px(30.))
                                .flex_shrink_0()
                                .border_b_1()
                                .border_color(theme.colors().border)
                                .cursor_pointer()
                                .when(is_active, |this| this.bg(theme.colors().editor_background))
                                .when(!is_active, |this| {
                                    this.hover(|style| style.bg(theme.colors().ghost_element_hover))
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.switch_to_tab(index, window, cx);
                                }))
                                .child(favicon_element)
                                .into_any_element()
                        } else {
                            let display_title = if title.len() > 24 {
                                let truncated = match title.char_indices().nth(21) {
                                    Some((byte_index, _)) => &title[..byte_index],
                                    None => &title,
                                };
                                format!("{truncated}...")
                            } else {
                                title
                            };

                            div()
                                .id(("sidebar-tab-inner", index))
                                .flex()
                                .items_center()
                                .w(px(SIDEBAR_WIDTH_PX))
                                .h(px(30.))
                                .px_2()
                                .gap_1()
                                .flex_shrink_0()
                                .border_b_1()
                                .border_color(theme.colors().border)
                                .cursor_pointer()
                                .when(is_active, |this| this.bg(theme.colors().editor_background))
                                .when(!is_active, |this| {
                                    this.hover(|style| style.bg(theme.colors().ghost_element_hover))
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
                                .child(
                                    native_icon_button(
                                        SharedString::from(format!("sidebar-close-tab-{index}")),
                                        "xmark",
                                    )
                                    .size(px(16.))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.close_tab_at(index, window, cx);
                                    }))
                                    .tooltip("Close Tab"),
                                )
                                .into_any_element()
                        };

                        let view = view.clone();
                        div().w_full().child(
                            ui::right_click_menu(("sidebar-tab-ctx-menu", index))
                                .trigger(move |_, _, _| {
                                    div().w(px(SIDEBAR_WIDTH_PX)).child(tab_content)
                                })
                                .menu(move |window, cx| {
                                    let view = view.clone();
                                    ui::ContextMenu::build(
                                        window,
                                        cx,
                                        move |mut menu, _window, _cx| {
                                            if is_pinned {
                                                let view = view.clone();
                                                menu = menu.entry(
                                                    "Unpin Tab",
                                                    None,
                                                    move |_window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.unpin_tab_at(index, cx);
                                                        })
                                                        .ok();
                                                    },
                                                );
                                            } else {
                                                let view = view.clone();
                                                menu = menu.entry(
                                                    "Pin Tab",
                                                    None,
                                                    move |_window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.pin_tab_at(index, cx);
                                                        })
                                                        .ok();
                                                    },
                                                );
                                            }
                                            menu = menu.separator();
                                            {
                                                let view = view.clone();
                                                menu = menu.entry(
                                                    "Close Tab",
                                                    None,
                                                    move |_window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.close_tab_at_inner(index, cx);
                                                        })
                                                        .ok();
                                                    },
                                                );
                                            }
                                            {
                                                let view = view.clone();
                                                menu = menu.entry(
                                                    "Close Other Tabs",
                                                    None,
                                                    move |_window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.close_other_tabs_at(index, cx);
                                                        })
                                                        .ok();
                                                    },
                                                );
                                            }
                                            if !is_pinned {
                                                menu = menu.separator();
                                                menu = menu.entry(
                                                    "Bookmark This Page",
                                                    None,
                                                    move |_window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.toggle_bookmark_at(index, cx);
                                                        })
                                                        .ok();
                                                    },
                                                );
                                            }
                                            menu
                                        },
                                    )
                                }),
                        )
                    })),
            )
            .child(
                div()
                    .w_full()
                    .p_1()
                    .border_t_1()
                    .border_color(theme.colors().border)
                    .child(
                        native_icon_button("sidebar-new-tab-button", "plus")
                            .size(px(20.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_tab(cx);
                                this.update_toolbar_active_tab(window, cx);
                                cx.notify();
                            }))
                            .tooltip("New Tab"),
                    ),
            )
    }
}
