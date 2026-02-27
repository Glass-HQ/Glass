use gpui::{App, Styled, hsla};

use crate::ElevationIndex;
use crate::prelude::*;

fn elevated<E: Styled>(this: E, cx: &App, index: ElevationIndex) -> E {
    let radius = cx.theme().border_radius().large;
    this.bg(cx.theme().colors().elevated_surface_background)
        .rounded(radius)
        .border_1()
        .border_color(cx.theme().colors().border_variant)
        .shadow(index.shadow(cx))
}

fn elevated_borderless<E: Styled>(this: E, cx: &mut App, index: ElevationIndex) -> E {
    let radius = cx.theme().border_radius().large;
    this.bg(cx.theme().colors().elevated_surface_background)
        .rounded(radius)
        .shadow(index.shadow(cx))
}

/// Extends [`gpui::Styled`] with Zed-specific styling methods.
// gate on rust-analyzer so rust-analyzer never needs to expand this macro, it takes up to 10 seconds to expand due to inefficiencies in rust-analyzers proc-macro srv
#[cfg_attr(
    all(debug_assertions, not(rust_analyzer)),
    gpui_macros::derive_inspector_reflection
)]
pub trait StyledExt: Styled + Sized {
    /// Horizontally stacks elements.
    ///
    /// Sets `flex()`, `flex_row()`, `items_center()`
    fn h_flex(self) -> Self {
        self.flex().flex_row().items_center()
    }

    /// Vertically stacks elements.
    ///
    /// Sets `flex()`, `flex_col()`
    fn v_flex(self) -> Self {
        self.flex().flex_col()
    }

    /// The [`Surface`](ElevationIndex::Surface) elevation level, located above the app background, is the standard level for all elements
    ///
    /// Sets `bg()`, `rounded()` (theme border radius), `border()`, `border_color()`, `shadow()`
    ///
    /// Example Elements: Title Bar, Panel, Tab Bar, Editor
    fn elevation_1(self, cx: &App) -> Self {
        elevated(self, cx, ElevationIndex::Surface)
    }

    /// See [`elevation_1`](Self::elevation_1).
    ///
    /// Renders a borderless version [`elevation_1`](Self::elevation_1).
    fn elevation_1_borderless(self, cx: &mut App) -> Self {
        elevated_borderless(self, cx, ElevationIndex::Surface)
    }

    /// Non-Modal Elevated Surfaces appear above the [`Surface`](ElevationIndex::Surface) layer and is used for things that should appear above most UI elements like an editor or panel, but not elements like popovers, context menus, modals, etc.
    ///
    /// Sets `bg()`, `rounded()` (theme border radius), `border()`, `border_color()`, `shadow()`
    ///
    /// Examples: Notifications, Palettes, Detached/Floating Windows, Detached/Floating Panels
    fn elevation_2(self, cx: &App) -> Self {
        elevated(self, cx, ElevationIndex::ElevatedSurface)
    }

    /// See [`elevation_2`](Self::elevation_2).
    ///
    /// Renders a borderless version [`elevation_2`](Self::elevation_2).
    fn elevation_2_borderless(self, cx: &mut App) -> Self {
        elevated_borderless(self, cx, ElevationIndex::ElevatedSurface)
    }

    /// Modal Surfaces are used for elements that should appear above all other UI elements and are located above the wash layer. This is the maximum elevation at which UI elements can be rendered in their default state.
    ///
    /// Elements rendered at this layer should have an enforced behavior: Any interaction outside of the modal will either dismiss the modal or prompt an action (Save your progress, etc) then dismiss the modal.
    ///
    /// If the element does not have this behavior, it should be rendered at the [`Elevated Surface`](ElevationIndex::ElevatedSurface) layer.
    ///
    /// Sets `bg()`, `rounded()` (theme border radius), `border()`, `border_color()`, `shadow()`
    ///
    /// Examples: Settings Modal, Channel Management, Wizards/Setup UI, Dialogs
    fn elevation_3(self, cx: &App) -> Self {
        elevated(self, cx, ElevationIndex::ModalSurface)
    }

    /// See [`elevation_3`](Self::elevation_3).
    ///
    /// Renders a borderless version [`elevation_3`](Self::elevation_3).
    fn elevation_3_borderless(self, cx: &mut App) -> Self {
        elevated_borderless(self, cx, ElevationIndex::ModalSurface)
    }

    /// The theme's primary border color.
    fn border_primary(self, cx: &mut App) -> Self {
        self.border_color(cx.theme().colors().border)
    }

    /// The theme's secondary or muted border color.
    fn border_muted(self, cx: &mut App) -> Self {
        self.border_color(cx.theme().colors().border_variant)
    }

    /// Sets the background color to red for debugging when building UI.
    fn debug_bg_red(self) -> Self {
        self.bg(hsla(0. / 360., 1., 0.5, 1.))
    }

    /// Sets the background color to green for debugging when building UI.
    fn debug_bg_green(self) -> Self {
        self.bg(hsla(120. / 360., 1., 0.5, 1.))
    }

    /// Sets the background color to blue for debugging when building UI.
    fn debug_bg_blue(self) -> Self {
        self.bg(hsla(240. / 360., 1., 0.5, 1.))
    }

    /// Sets the background color to yellow for debugging when building UI.
    fn debug_bg_yellow(self) -> Self {
        self.bg(hsla(60. / 360., 1., 0.5, 1.))
    }

    /// Sets the background color to cyan for debugging when building UI.
    fn debug_bg_cyan(self) -> Self {
        self.bg(hsla(160. / 360., 1., 0.5, 1.))
    }

    /// Sets the background color to magenta for debugging when building UI.
    fn debug_bg_magenta(self) -> Self {
        self.bg(hsla(300. / 360., 1., 0.5, 1.))
    }

    /// Theme-aware extra-small border radius (default 2px).
    fn theme_rounded_xs(self, cx: &App) -> Self {
        self.rounded(cx.theme().border_radius().extra_small)
    }

    /// Theme-aware small border radius (default 4px).
    fn theme_rounded_sm(self, cx: &App) -> Self {
        self.rounded(cx.theme().border_radius().small)
    }

    /// Theme-aware medium border radius (default 6px).
    fn theme_rounded_md(self, cx: &App) -> Self {
        self.rounded(cx.theme().border_radius().medium)
    }

    /// Theme-aware large border radius (default 8px).
    fn theme_rounded_lg(self, cx: &App) -> Self {
        self.rounded(cx.theme().border_radius().large)
    }

    /// Theme-aware extra-large border radius (default 12px).
    fn theme_rounded_xl(self, cx: &App) -> Self {
        self.rounded(cx.theme().border_radius().extra_large)
    }

    /// Theme-aware small top border radius.
    fn theme_rounded_t_sm(self, cx: &App) -> Self {
        self.rounded_t(cx.theme().border_radius().small)
    }

    /// Theme-aware medium top border radius.
    fn theme_rounded_t_md(self, cx: &App) -> Self {
        self.rounded_t(cx.theme().border_radius().medium)
    }

    /// Theme-aware large top border radius.
    fn theme_rounded_t_lg(self, cx: &App) -> Self {
        self.rounded_t(cx.theme().border_radius().large)
    }

    /// Theme-aware small bottom border radius.
    fn theme_rounded_b_sm(self, cx: &App) -> Self {
        self.rounded_b(cx.theme().border_radius().small)
    }

    /// Theme-aware medium bottom border radius.
    fn theme_rounded_b_md(self, cx: &App) -> Self {
        self.rounded_b(cx.theme().border_radius().medium)
    }

    /// Theme-aware large bottom border radius.
    fn theme_rounded_b_lg(self, cx: &App) -> Self {
        self.rounded_b(cx.theme().border_radius().large)
    }

    /// Theme-aware small left border radius.
    fn theme_rounded_l_sm(self, cx: &App) -> Self {
        self.rounded_l(cx.theme().border_radius().small)
    }

    /// Theme-aware medium left border radius.
    fn theme_rounded_l_md(self, cx: &App) -> Self {
        self.rounded_l(cx.theme().border_radius().medium)
    }

    /// Theme-aware small right border radius.
    fn theme_rounded_r_sm(self, cx: &App) -> Self {
        self.rounded_r(cx.theme().border_radius().small)
    }

    /// Theme-aware medium right border radius.
    fn theme_rounded_r_md(self, cx: &App) -> Self {
        self.rounded_r(cx.theme().border_radius().medium)
    }

    /// Theme-aware small top-left corner border radius.
    fn theme_rounded_tl_sm(self, cx: &App) -> Self {
        self.rounded_tl(cx.theme().border_radius().small)
    }

    /// Theme-aware small top-right corner border radius.
    fn theme_rounded_tr_sm(self, cx: &App) -> Self {
        self.rounded_tr(cx.theme().border_radius().small)
    }

    /// Theme-aware small bottom-left corner border radius.
    fn theme_rounded_bl_sm(self, cx: &App) -> Self {
        self.rounded_bl(cx.theme().border_radius().small)
    }

    /// Theme-aware small bottom-right corner border radius.
    fn theme_rounded_br_sm(self, cx: &App) -> Self {
        self.rounded_br(cx.theme().border_radius().small)
    }
}

impl<E: Styled> StyledExt for E {}
