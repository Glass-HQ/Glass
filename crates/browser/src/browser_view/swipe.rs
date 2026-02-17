use crate::input;
use gpui::{Context, ScrollDelta, TouchPhase, Window, point};
use std::time::Duration;

use super::BrowserView;

const SWIPE_AXIS_LOCK_THRESHOLD: f32 = 25.0;
pub(super) const SWIPE_NAV_THRESHOLD: f32 = 150.0;
pub(super) const SWIPE_INDICATOR_SIZE: f32 = 36.0;

#[derive(Default, Clone, Copy, PartialEq)]
pub(super) enum SwipePhase {
    #[default]
    Idle,
    Undecided,
    Horizontal,
    Vertical,
    Fired,
}

#[derive(Default)]
pub(super) struct SwipeNavigationState {
    accumulated_x: f32,
    accumulated_y: f32,
    pub(super) phase: SwipePhase,
}

impl SwipeNavigationState {
    pub(super) fn reset(&mut self) {
        self.accumulated_x = 0.0;
        self.accumulated_y = 0.0;
        self.phase = SwipePhase::Idle;
    }

    pub(super) fn progress(&self) -> f32 {
        (self.accumulated_x.abs() / SWIPE_NAV_THRESHOLD).clamp(0.0, 1.0)
    }

    pub(super) fn is_active(&self) -> bool {
        self.phase == SwipePhase::Horizontal || self.phase == SwipePhase::Fired
    }

    pub(super) fn is_swiping_back(&self) -> bool {
        self.is_active() && self.accumulated_x > 0.0
    }

    pub(super) fn threshold_crossed(&self) -> bool {
        self.accumulated_x.abs() >= SWIPE_NAV_THRESHOLD
    }
}

impl BrowserView {
    pub(super) fn handle_scroll(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let ScrollDelta::Pixels(delta) = event.delta {
            let delta_x = f32::from(delta.x);
            let delta_y = f32::from(delta.y);

            match event.touch_phase {
                TouchPhase::Started => {
                    self._swipe_dismiss_task = None;
                    self.swipe_state.reset();
                    self.swipe_state.phase = SwipePhase::Undecided;
                }
                TouchPhase::Moved if self.swipe_state.phase == SwipePhase::Undecided => {
                    self.swipe_state.accumulated_x += delta_x;
                    self.swipe_state.accumulated_y += delta_y;

                    let abs_x = self.swipe_state.accumulated_x.abs();
                    let abs_y = self.swipe_state.accumulated_y.abs();
                    let total = abs_x + abs_y;

                    if total >= SWIPE_AXIS_LOCK_THRESHOLD {
                        if abs_x > abs_y * 2.0 {
                            self.swipe_state.phase = SwipePhase::Horizontal;
                            cx.notify();
                            return;
                        } else {
                            self.swipe_state.phase = SwipePhase::Vertical;
                        }
                    } else {
                        return;
                    }
                }
                TouchPhase::Moved if self.swipe_state.phase == SwipePhase::Horizontal => {
                    self.swipe_state.accumulated_x += delta_x;
                    cx.notify();
                    return;
                }
                TouchPhase::Ended if self.swipe_state.phase == SwipePhase::Horizontal => {
                    if self.swipe_state.threshold_crossed() {
                        if let Some(tab) = self.active_tab().cloned() {
                            if self.swipe_state.is_swiping_back() {
                                tab.update(cx, |tab, _| tab.go_back());
                            } else {
                                tab.update(cx, |tab, _| tab.go_forward());
                            }
                        }
                        self.swipe_state.phase = SwipePhase::Fired;
                        cx.notify();
                        self._swipe_dismiss_task = Some(cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(300))
                                .await;
                            this.update(cx, |this, cx| {
                                this.swipe_state.reset();
                                cx.notify();
                            })
                            .ok();
                        }));
                    } else {
                        self.swipe_state.reset();
                        cx.notify();
                    }
                    return;
                }
                TouchPhase::Ended => {
                    if self.swipe_state.phase == SwipePhase::Undecided {
                        self.swipe_state.reset();
                    }
                }
                _ => {}
            }
        }

        if let Some(tab) = self.active_tab() {
            let offset = point(self.content_bounds.origin.x, self.content_bounds.origin.y);
            input::handle_scroll_wheel(&tab.read(cx), event, offset);
        }
    }
}
