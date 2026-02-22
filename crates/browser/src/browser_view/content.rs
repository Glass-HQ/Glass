use gpui::{
    Context, Corner, IntoElement, MouseButton, NativeImageScaling, NativeImageSymbolWeight,
    ObjectFit, ParentElement, Styled, anchored, canvas, deferred, div, native_icon_button,
    native_image_view, prelude::*, px, surface,
};
use ui::prelude::*;

use super::BrowserView;
use super::swipe::{SWIPE_INDICATOR_SIZE, SwipePhase};
use crate::new_tab_page;

impl BrowserView {
    pub(super) fn render_placeholder(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(theme.colors().editor_background)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        native_image_view("browser-placeholder-globe")
                            .sf_symbol_config("globe", 96.0, NativeImageSymbolWeight::Regular)
                            .scaling(NativeImageScaling::ScaleUpOrDown)
                            .size(px(96.0)),
                    )
                    .child(
                        div()
                            .text_color(theme.colors().text_muted)
                            .text_size(rems(1.0))
                            .child("Browser"),
                    )
                    .child(
                        div()
                            .text_color(theme.colors().text_muted)
                            .text_size(rems(0.875))
                            .max_w(px(400.))
                            .text_center()
                            .child(
                                "CEF is not initialized. Set CEF_PATH environment variable and restart.",
                            ),
                    ),
            )
    }

    fn render_find_overlay(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.find_visible {
            return None;
        }

        let theme = cx.theme();
        let match_count = self.find_match_count.max(0);
        let active_match = self.find_active_match_ordinal.max(0);
        let match_text = if match_count == 0 {
            String::from("0/0")
        } else {
            format!("{active_match}/{match_count}")
        };

        let overlay = div()
            .id("browser-find-overlay")
            .absolute()
            .top(px(8.))
            .right(px(8.))
            .w(px(320.))
            .key_context("BrowserFindBar")
            .bg(theme.colors().elevated_surface_background)
            .border_1()
            .border_color(theme.colors().border)
            .rounded_md()
            .shadow_md()
            .p_1()
            .flex()
            .items_center()
            .gap_1()
            .when_some(self.find_editor.clone(), |this, editor| {
                this.child(div().flex_1().min_w_0().child(editor))
            })
            .child(
                div()
                    .w(px(52.))
                    .text_size(rems(0.75))
                    .text_color(theme.colors().text_muted)
                    .text_right()
                    .child(match_text),
            )
            .child(
                native_icon_button("find-previous", "chevron.up")
                    .size(px(18.))
                    .tooltip("Previous Match")
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.find_visible {
                            this.find_visible = true;
                            this.focus_find_editor(window, cx);
                        }
                        this.run_find(false, true, cx);
                        cx.notify();
                    })),
            )
            .child(
                native_icon_button("find-next", "chevron.down")
                    .size(px(18.))
                    .tooltip("Next Match")
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.find_visible {
                            this.find_visible = true;
                            this.focus_find_editor(window, cx);
                        }
                        this.run_find(true, true, cx);
                        cx.notify();
                    })),
            )
            .child(
                native_icon_button("find-close", "xmark")
                    .size(px(18.))
                    .tooltip("Close Find")
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(tab) = this.active_tab() {
                            tab.read(cx).stop_finding(true);
                        }
                        this.find_visible = false;
                        this.find_query.clear();
                        this.find_match_count = 0;
                        this.find_active_match_ordinal = 0;
                        this.set_find_editor_text("", window, cx);
                        window.focus(&this.focus_handle, cx);
                        cx.notify();
                    })),
            );

        Some(overlay.into_any_element())
    }

    fn format_download_size(bytes: i64) -> String {
        let safe_bytes = bytes.max(0) as f64;
        if safe_bytes < 1024.0 {
            return format!("{} B", safe_bytes as i64);
        }
        if safe_bytes < 1024.0 * 1024.0 {
            return format!("{:.1} KB", safe_bytes / 1024.0);
        }
        if safe_bytes < 1024.0 * 1024.0 * 1024.0 {
            return format!("{:.1} MB", safe_bytes / (1024.0 * 1024.0));
        }
        format!("{:.1} GB", safe_bytes / (1024.0 * 1024.0 * 1024.0))
    }

    fn download_display_name(download: &super::DownloadItemState) -> String {
        if !download.item.suggested_file_name.is_empty() {
            return download.item.suggested_file_name.clone();
        }
        download
            .item
            .full_path
            .clone()
            .and_then(|path| {
                std::path::Path::new(&path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| String::from("download"))
    }

    fn download_status_line(download: &super::DownloadItemState) -> String {
        if download.item.is_complete {
            return String::from("Complete");
        }
        if download.item.is_canceled {
            return String::from("Canceled");
        }
        if download.item.is_interrupted {
            return String::from("Interrupted");
        }
        if download.item.is_in_progress {
            let received = Self::format_download_size(download.item.received_bytes);
            let total = if download.item.total_bytes > 0 {
                Self::format_download_size(download.item.total_bytes)
            } else {
                String::from("--")
            };
            let percent = download.item.percent_complete.max(0);
            return format!("{percent}% ({received}/{total})");
        }
        String::from("Queued")
    }

    fn render_download_center_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.download_center_visible {
            return None;
        }

        let theme = cx.theme();
        let top_offset = if self.find_visible { px(52.) } else { px(8.) };

        let rows = self.downloads.iter().map(|download| {
            let id = download.item.id;
            let file_name = Self::download_display_name(download);
            let status = Self::download_status_line(download);
            let is_complete = download.item.is_complete;
            let has_path = download.item.full_path.is_some();
            let is_incognito = download.is_incognito;

            div()
                .id(("download-row", id))
                .w_full()
                .p_2()
                .border_b_1()
                .border_color(theme.colors().border_variant)
                .child(
                    div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .text_size(rems(0.8125))
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(file_name),
                                )
                                .child(
                                    div()
                                        .text_size(rems(0.75))
                                        .text_color(theme.colors().text_muted)
                                        .child(status),
                                ),
                        )
                        .when(is_incognito, |this| {
                            this.child(
                                div()
                                    .text_size(rems(0.625))
                                    .text_color(theme.colors().text_muted)
                                    .child("Incognito"),
                            )
                        }),
                )
                .when(is_complete && has_path, |this| {
                    this.child(
                        div()
                            .mt_1()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id(("download-open", id))
                                    .text_size(rems(0.75))
                                    .text_color(theme.colors().text_accent)
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        this.open_download_with_system(id, cx);
                                    }))
                                    .child("Open"),
                            )
                            .child(
                                div()
                                    .id(("download-reveal", id))
                                    .text_size(rems(0.75))
                                    .text_color(theme.colors().text_accent)
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        this.reveal_download_in_finder(id, cx);
                                    }))
                                    .child("Reveal"),
                            ),
                    )
                })
        });

        let overlay = div()
            .id("browser-download-center-overlay")
            .absolute()
            .top(top_offset)
            .right(px(8.))
            .w(px(360.))
            .max_h(px(360.))
            .overflow_y_scroll()
            .bg(theme.colors().elevated_surface_background)
            .border_1()
            .border_color(theme.colors().border)
            .rounded_md()
            .shadow_md()
            .child(
                div()
                    .w_full()
                    .p_2()
                    .border_b_1()
                    .border_color(theme.colors().border_variant)
                    .text_size(rems(0.8125))
                    .text_color(theme.colors().text)
                    .child("Downloads"),
            )
            .when(self.downloads.is_empty(), |this| {
                this.child(
                    div()
                        .w_full()
                        .p_3()
                        .text_size(rems(0.75))
                        .text_color(theme.colors().text_muted)
                        .child("No downloads yet."),
                )
            })
            .when(!self.downloads.is_empty(), |this| this.children(rows));

        Some(overlay.into_any_element())
    }

    pub(super) fn render_browser_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let find_overlay = self.render_find_overlay(cx);
        let download_center_overlay = self.render_download_center_overlay(cx);
        let theme = cx.theme();

        let is_new_tab_page = self
            .active_tab()
            .map(|t| t.read(cx).is_new_tab_page())
            .unwrap_or(false);

        if is_new_tab_page {
            let browser_view = cx.entity();
            return div()
                .id("browser-content")
                .relative()
                .flex_1()
                .w_full()
                .child(new_tab_page::render_new_tab_page(
                    browser_view,
                    self.new_tab_search_text().to_string(),
                    self.is_incognito_window,
                    cx,
                ))
                .when_some(find_overlay, |this, overlay| this.child(overlay))
                .when_some(download_center_overlay, |this, overlay| this.child(overlay))
                .into_any_element();
        }

        let current_frame = self.active_tab().and_then(|t| t.read(cx).current_frame());

        let has_frame = current_frame.is_some();

        let this = cx.entity();
        let bounds_tracker = canvas(
            move |bounds, _window, cx| {
                this.update(cx, |view, _| {
                    view.content_bounds = bounds;
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let context_menu_overlay = self.context_menu.as_ref().map(|cm| {
            deferred(
                anchored()
                    .position(cm.position)
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(cm.menu.clone()),
            )
            .with_priority(1)
        });

        let swipe_indicator = if self.swipe_state.is_active() {
            let progress = self.swipe_state.progress();
            let fired = self.swipe_state.phase == SwipePhase::Fired;
            let swiping_back = self.swipe_state.is_swiping_back();
            let can_navigate = if swiping_back {
                self.active_tab()
                    .map(|t| t.read(cx).can_go_back())
                    .unwrap_or(false)
            } else {
                self.active_tab()
                    .map(|t| t.read(cx).can_go_forward())
                    .unwrap_or(false)
            };

            let icon = if swiping_back {
                IconName::ArrowLeft
            } else {
                IconName::ArrowRight
            };

            let committed = can_navigate && (self.swipe_state.threshold_crossed() || fired);
            let indicator_size = px(SWIPE_INDICATOR_SIZE);

            let visible_inset = px(8.);
            let slide_offset = if fired {
                visible_inset
            } else {
                let ease = progress * progress * (3.0 - 2.0 * progress);
                px(-SWIPE_INDICATOR_SIZE) + (px(SWIPE_INDICATOR_SIZE) + visible_inset) * ease
            };

            let opacity = if fired { 0.6 } else { progress * 0.9 };

            Some(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .when(swiping_back, |this| this.left(slide_offset))
                    .when(!swiping_back, |this| this.right(slide_offset))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(indicator_size)
                            .h(indicator_size)
                            .rounded_full()
                            .bg(theme.colors().element_background)
                            .border_1()
                            .border_color(theme.colors().border)
                            .opacity(opacity)
                            .child(Icon::new(icon).size(IconSize::Small).color(if committed {
                                ui::Color::Default
                            } else {
                                ui::Color::Muted
                            })),
                    ),
            )
        } else {
            None
        };

        div()
            .id("browser-content")
            .relative()
            .flex_1()
            .w_full()
            .overflow_hidden()
            .bg(theme.colors().editor_background)
            .child(bounds_tracker)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::handle_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::handle_mouse_up))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_scroll_wheel(cx.listener(Self::handle_scroll))
            .when_some(current_frame, |this, frame| {
                this.child(surface(frame).size_full().object_fit(ObjectFit::Fill))
            })
            .when(!has_frame, |this| {
                this.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_color(theme.colors().text_muted)
                                .child("Loading..."),
                        ),
                )
            })
            .when_some(context_menu_overlay, |this, overlay| this.child(overlay))
            .when_some(swipe_indicator, |this, indicator| this.child(indicator))
            .when_some(find_overlay, |this, overlay| this.child(overlay))
            .when_some(download_center_overlay, |this, overlay| this.child(overlay))
            .into_any_element()
    }
}
