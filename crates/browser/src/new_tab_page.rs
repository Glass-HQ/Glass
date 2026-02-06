use gpui::{App, IntoElement, Styled, prelude::*};
use ui::{Icon, IconName, IconSize, prelude::*};

pub fn render_new_tab_page(cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .bg(theme.colors().editor_background)
        .child(
            Icon::new(IconName::Globe)
                .size(IconSize::Custom(rems(6.0)))
                .color(Color::Muted),
        )
}
