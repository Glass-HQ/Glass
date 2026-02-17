use crate::omnibox::Omnibox;
use gpui::{App, Entity, IntoElement, Styled, prelude::*};
use ui::{Icon, IconName, IconSize, prelude::*};

pub fn render_new_tab_page(
    omnibox: Option<&Entity<Omnibox>>,
    is_incognito_window: bool,
    cx: &App,
) -> impl IntoElement {
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
                .when(is_incognito_window, |this| {
                    this.child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.colors().border)
                            .bg(theme.colors().element_background)
                            .text_size(rems(0.8125))
                            .text_color(theme.colors().text)
                            .child("Incognito Window"),
                    )
                    .child(
                        div()
                            .text_size(rems(0.75))
                            .text_color(theme.colors().text_muted)
                            .text_center()
                            .max_w(px(500.))
                            .child("Your browsing activity in this window is not saved to browser history or session restore."),
                    )
                })
                .when_some(omnibox.cloned(), |this, omnibox| {
                    this.child(div().w(px(500.)).child(omnibox))
                }),
        )
}
