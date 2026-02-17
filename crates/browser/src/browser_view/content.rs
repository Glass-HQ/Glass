use gpui::{
    Context, Corner, IntoElement, MouseButton, ObjectFit, ParentElement, Styled, anchored, canvas,
    deferred, div, prelude::*, surface,
};
use ui::{Icon, IconName, IconSize, prelude::*};

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
                        Icon::new(IconName::Globe)
                            .size(IconSize::Custom(rems(6.0)))
                            .color(Color::Muted),
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

    pub(super) fn render_browser_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let is_new_tab_page = self
            .active_tab()
            .map(|t| t.read(cx).is_new_tab_page())
            .unwrap_or(false);

        if is_new_tab_page {
            let omnibox = self
                .toolbar
                .as_ref()
                .map(|toolbar| toolbar.read(cx).omnibox().clone());
            return div()
                .id("browser-content")
                .relative()
                .flex_1()
                .w_full()
                .child(new_tab_page::render_new_tab_page(omnibox.as_ref(), cx))
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
            .into_any_element()
    }
}
