use std::sync::Arc;

use client::User;
use cloud_api_types::Plan;
use encoding_selector::Toggle as ToggleEncoding;
use gpui::{
    Action, App, NativeToolbarButton, NativeToolbarItem, NativeToolbarLabel,
    NativeToolbarMenuButton, NativeToolbarMenuItem,
};
use settings::Settings;
use workspace::notifications::NotifyResultExt;

use crate::{TitleBar, title_bar_settings::TitleBarSettings};

pub(super) fn plan_label(plan: Plan) -> &'static str {
    match plan {
        Plan::ZedFree => "Free",
        Plan::ZedPro => "Pro",
        Plan::ZedProTrial => "Pro Trial",
        Plan::ZedBusiness => "Business",
        Plan::ZedStudent => "Student",
    }
}

fn account_menu_label(user_login: &str, plan: Option<Plan>) -> String {
    format!(
        "{} ({})",
        user_login,
        plan_label(plan.unwrap_or(Plan::ZedFree))
    )
}

impl TitleBar {
    pub(crate) fn build_toolchain_item(&self, toolchain: String) -> NativeToolbarItem {
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.status.toolchain", toolchain)
                .tool_tip("Select Toolchain")
                .on_click(|_, window, cx| {
                    window.dispatch_action(toolchain_selector::Select.boxed_clone(), cx);
                }),
        )
    }

    pub(crate) fn build_encoding_item(&self, encoding: String) -> NativeToolbarItem {
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.status.encoding", encoding)
                .tool_tip("Reopen with Encoding")
                .on_click(|_, window, cx| {
                    window.dispatch_action(ToggleEncoding.boxed_clone(), cx);
                }),
        )
    }

    pub(crate) fn build_line_ending_item(&self, line_ending: String) -> NativeToolbarItem {
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.status.line_ending", line_ending)
                .tool_tip("Select Line Ending")
                .on_click(|_, window, cx| {
                    window.dispatch_action(line_ending_selector::Toggle.boxed_clone(), cx);
                }),
        )
    }

    pub(crate) fn build_image_info_item(&self, image_info: String) -> NativeToolbarItem {
        NativeToolbarItem::Label(NativeToolbarLabel::new(
            "glass.status.image_info",
            image_info,
        ))
    }

    pub(crate) fn build_sign_in_item(&self) -> NativeToolbarItem {
        let client = self.client.clone();
        let workspace = self.workspace.clone();
        NativeToolbarItem::Button(
            NativeToolbarButton::new("glass.sign_in", "Sign In")
                .tool_tip("Sign In")
                .icon("person.crop.circle.badge.plus")
                .on_click(move |_, window, cx| {
                    let client = client.clone();
                    let workspace = workspace.clone();
                    window
                        .spawn(cx, async move |mut cx| {
                            client
                                .sign_in_with_optional_connect(true, cx)
                                .await
                                .notify_workspace_async_err(workspace, &mut cx);
                        })
                        .detach();
                }),
        )
    }

    pub(crate) fn build_user_menu_item(
        &self,
        user: &Option<Arc<User>>,
        plan: Option<Plan>,
        cx: &App,
    ) -> NativeToolbarItem {
        let show_update = self.update_version.read(cx).show_update_in_menu_bar();
        let signed_in = user.is_some();
        let user_login = user
            .as_ref()
            .map(|user| user.github_login.to_string())
            .unwrap_or_else(|| "Account".to_string());

        let mut menu_items = Vec::new();
        if signed_in {
            menu_items.push(
                NativeToolbarMenuItem::action(account_menu_label(&user_login, plan)).enabled(false),
            );
            menu_items.push(NativeToolbarMenuItem::separator());
        }
        if show_update {
            menu_items
                .push(NativeToolbarMenuItem::action("Restart to Update").icon("arrow.down.circle"));
            menu_items.push(NativeToolbarMenuItem::separator());
        }
        menu_items.push(NativeToolbarMenuItem::action("Settings").icon("gearshape"));
        menu_items.push(NativeToolbarMenuItem::action("Keymap").icon("keyboard"));
        menu_items.push(NativeToolbarMenuItem::action("Themes…").icon("paintbrush"));
        menu_items.push(NativeToolbarMenuItem::action("Icon Themes…").icon("photo"));
        menu_items.push(NativeToolbarMenuItem::action("Extensions").icon("puzzlepiece.extension"));
        if signed_in {
            menu_items.push(NativeToolbarMenuItem::separator());
            menu_items.push(
                NativeToolbarMenuItem::action("Sign Out")
                    .icon("rectangle.portrait.and.arrow.right"),
            );
        }

        let mut menu_button =
            NativeToolbarMenuButton::new("glass.user_menu", "Account", menu_items)
                .tool_tip("User Menu")
                .shows_indicator(false);
        if TitleBarSettings::get_global(cx).show_user_picture
            && let Some(user) = user
        {
            let avatar_url = user.avatar_uri.to_string();
            if !avatar_url.is_empty() {
                menu_button = menu_button.image_url(avatar_url).image_circular(true);
            } else {
                menu_button = menu_button.icon("person.crop.circle");
            }
        } else {
            menu_button = menu_button.icon("person.crop.circle");
        }

        let client = self.client.clone();
        NativeToolbarItem::MenuButton(menu_button.on_select(move |event, window, cx| {
            let show_update_offset = usize::from(show_update);
            let signed_in_offset = usize::from(signed_in);
            let base = show_update_offset + signed_in_offset;

            if signed_in && event.index == 0 {
                cx.open_url(&client::zed_urls::account_url(cx));
                return;
            }

            if show_update && event.index == signed_in_offset {
                workspace::reload(cx);
                return;
            }

            match event.index.saturating_sub(base) {
                0 => window.dispatch_action(zed_actions::OpenSettings.boxed_clone(), cx),
                1 => window.dispatch_action(zed_actions::OpenKeymap.boxed_clone(), cx),
                2 => window.dispatch_action(
                    zed_actions::theme_selector::Toggle::default().boxed_clone(),
                    cx,
                ),
                3 => window.dispatch_action(
                    zed_actions::icon_theme_selector::Toggle::default().boxed_clone(),
                    cx,
                ),
                4 => window.dispatch_action(zed_actions::Extensions::default().boxed_clone(), cx),
                5 if signed_in => {
                    let client = client.clone();
                    window
                        .spawn(cx, async move |mut cx| {
                            client.sign_out(&mut cx).await;
                        })
                        .detach();
                }
                _ => {}
            }
        }))
    }
}
