use crate::omnibox::Omnibox;
use gpui::{App, Entity, IntoElement, Styled, prelude::*};
use ui::{Icon, IconName, IconSize, prelude::*};

pub fn render_new_tab_page(omnibox: Option<&Entity<Omnibox>>, cx: &App) -> impl IntoElement {
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
                .gap_6()
                .child(
                    Icon::new(IconName::Globe)
                        .size(IconSize::Custom(rems(4.0)))
                        .color(Color::Muted),
                )
                .when_some(omnibox.cloned(), |this, omnibox| {
                    this.child(div().w(px(500.)).child(omnibox))
                }),
        )
}
