use crate::browser_view::BrowserView;
use gpui::{
    App, Entity, IntoElement, SearchChangeEvent, SearchSubmitEvent, Styled, native_search_field,
    prelude::*, px,
};
use ui::prelude::*;

pub fn render_new_tab_page(
    browser_view: Entity<BrowserView>,
    search_text: String,
    is_incognito_window: bool,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let browser_view_for_change = browser_view.downgrade();
    let browser_view_for_submit = browser_view.downgrade();

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
                .child(
                    native_search_field("new-tab-search")
                        .placeholder("Search or enter URL")
                        .value(search_text)
                        .on_change(move |event: &SearchChangeEvent, _window, cx| {
                            if let Err(error) = browser_view_for_change.update(cx, |browser_view, cx| {
                                browser_view.set_new_tab_search_text(event.text.clone(), cx);
                            }) {
                                log::debug!(
                                    "[browser] failed to update new tab search text: {}",
                                    error
                                );
                            }
                        })
                        .on_submit(move |event: &SearchSubmitEvent, _window, cx| {
                            if let Err(error) = browser_view_for_submit.update(cx, |browser_view, cx| {
                                browser_view.submit_new_tab_search(&event.text, cx);
                            }) {
                                log::debug!(
                                    "[browser] failed to submit new tab search text: {}",
                                    error
                                );
                            }
                        })
                        .w(px(500.)),
                ),
        )
}
