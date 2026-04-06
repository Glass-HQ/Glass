mod browser;
mod browser_items;
mod editor_items;
mod items;
mod project_items;
mod session_items;
mod state;
mod status;

use self::editor_items::plan_label;
use crate::{TitleBar, show_menus, title_bar_settings::TitleBarSettings};
use client::Status as ClientStatus;
use gpui::{
    Action, AnyElement, Context, IntoElement, NativeToolbar, NativeToolbarDisplayMode,
    NativeToolbarItem, NativeToolbarSizeMode, Window,
};
use settings::Settings;
use workspace::ToggleSidebar;
use workspace_modes::ModeId;

pub(crate) use state::NativeToolbarState;

impl TitleBar {
    pub(crate) fn render_macos_title_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if show_menus(cx) {
            window.set_native_toolbar(None);
            return self.render_gpui_title_bar(window, cx).into_any_element();
        }

        self.update_native_toolbar(window, cx);
        let button_layout = TitleBarSettings::get_global(cx).button_layout;
        let background_color = if self.browser_surface_active(cx) {
            let active_url = self.active_tab_url(cx);
            let live_color = self.active_tab_page_chrome_color(cx);

            if let (Some(url), Some(color)) = (active_url.clone(), live_color) {
                self.native_toolbar_state.active_page_chrome_url = Some(url);
                self.native_toolbar_state.active_page_chrome_color = Some(color);
                Some(color)
            } else if active_url.is_some()
                && active_url == self.native_toolbar_state.active_page_chrome_url
            {
                self.native_toolbar_state.active_page_chrome_color
            } else {
                self.native_toolbar_state.active_page_chrome_url = active_url;
                self.native_toolbar_state.active_page_chrome_color = None;
                None
            }
        } else {
            self.native_toolbar_state.active_page_chrome_url = None;
            self.native_toolbar_state.active_page_chrome_color = None;
            None
        };
        self.platform_titlebar.update(cx, |titlebar, _| {
            titlebar.set_button_layout(button_layout);
            titlebar.set_background_color(background_color);
            titlebar.set_children(None::<AnyElement>);
        });
        window.set_background_color(background_color);

        self.platform_titlebar.clone().into_any_element()
    }

    pub(crate) fn invalidate_native_toolbar(&mut self, cx: &mut Context<Self>) {
        self.native_toolbar_state.last_toolbar_key.clear();
        cx.notify();
    }

    fn update_native_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_mode = self
            .workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).active_mode_id())
            .unwrap_or(ModeId::BROWSER);
        let is_browser_surface_active = self.browser_surface_active(cx);
        let is_terminal_mode = active_mode == ModeId::TERMINAL;
        let is_new_tab_page = is_browser_surface_active && self.active_tab_is_new_tab_page(cx);
        let title_bar_settings = *TitleBarSettings::get_global(cx);

        self.sync_omnibox_url(cx);
        self.refresh_status_data(window, cx);

        if self.native_toolbar_state.omnibox_panel_dirty {
            self.native_toolbar_state.omnibox_panel_dirty = false;
            if self.native_toolbar_state.omnibox_focused
                && !self.native_toolbar_state.omnibox_text.is_empty()
            {
                self.show_search_suggestion_menu(window);
            }
        }

        if self.native_toolbar_state.omnibox_focused {
            return;
        }

        let has_restricted_worktrees = self.has_restricted_worktrees(cx);
        let is_remote = self.project.read(cx).is_via_remote_server();
        let (user, menu_plan) = {
            let user_store = self.user_store.read(cx);
            let user = user_store.current_user();
            let has_subscription_period = user_store.subscription_period().is_some();
            let plan = user_store.plan().filter(|_| {
                // Preserve the existing GPUI menu behavior for legacy free accounts.
                has_subscription_period
            });
            let menu_plan = user
                .as_ref()
                .map(|_| plan.unwrap_or(cloud_api_types::Plan::ZedFree));

            (user, menu_plan)
        };
        let user_login = user
            .as_ref()
            .map(|user| user.github_login.to_string())
            .unwrap_or_default();
        let current_plan_label = menu_plan.map(plan_label);
        let show_update = self.update_version.read(cx).show_update_in_menu_bar();
        let connection_status_key = match &*self.client.status().borrow() {
            ClientStatus::ConnectionError => "connection_error",
            ClientStatus::ConnectionLost => "connection_lost",
            ClientStatus::Reauthenticating => "reauthenticating",
            ClientStatus::Reconnecting => "reconnecting",
            ClientStatus::ReconnectionError { .. } => "reconnection_error",
            ClientStatus::UpgradeRequired => "upgrade_required",
            _ => "ok",
        };
        let toolbar_key = format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{:?}:{:?}:{:?}:{:?}",
            active_mode.0,
            is_browser_surface_active,
            "",
            "",
            self.native_toolbar_state.omnibox_text,
            is_new_tab_page,
            has_restricted_worktrees,
            is_remote,
            title_bar_settings.show_project_items,
            title_bar_settings.show_branch_name,
            user_login,
            current_plan_label.unwrap_or_default(),
            connection_status_key,
            show_update,
            self.native_toolbar_state.status_encoding,
            self.native_toolbar_state.status_line_ending,
            self.native_toolbar_state.status_toolchain,
            self.native_toolbar_state.status_image_info,
        );

        if self.native_toolbar_state.last_toolbar_key == toolbar_key {
            return;
        }
        self.native_toolbar_state.last_toolbar_key = toolbar_key;

        let mut toolbar = NativeToolbar::new("glass.main.toolbar")
            .title("Glass")
            .display_mode(NativeToolbarDisplayMode::IconOnly)
            .size_mode(NativeToolbarSizeMode::Regular)
            .shows_baseline_separator(false)
            // Sidebar zone: FlexibleSpace + toggle + TrackingSeparator — always in this order.
            // TrackingSeparator is the boundary between sidebar and content zones in the toolbar.
            // FlexibleSpace fills the sidebar column, pinning the toggle to its trailing edge.
            //
            // We use a custom button (not NativeToolbarItem::SidebarToggle) because
            // NSToolbarToggleSidebarItemIdentifier tracks sidebar state via the responder
            // chain to NSSplitViewController. Glass opts out of manage_window_chrome, so the
            // split view is a subview — not contentViewController — and AppKit can't find it
            // after the first collapse, permanently breaking the button's visual state.
            // A custom button with a stable SF Symbol has consistent appearance always.
            .item(NativeToolbarItem::FlexibleSpace)
            .item(self.build_simple_action_button(
                "glass.sidebar.toggle",
                "sidebar.left",
                "Toggle Sidebar",
                |window, cx| window.dispatch_action(ToggleSidebar.boxed_clone(), cx),
            ))
            .item(NativeToolbarItem::SidebarTrackingSeparator)
            .item(self.build_mode_switcher_item(active_mode));

        if let Some(item) = self.build_restricted_mode_item(cx) {
            toolbar = toolbar.item(item);
        }

        toolbar = toolbar.item(NativeToolbarItem::FlexibleSpace);

        if is_browser_surface_active {
            if !is_new_tab_page {
                toolbar = toolbar
                    .item(self.build_back_item())
                    .item(self.build_forward_item())
                    .item(self.build_reload_item())
                    .item(self.build_omnibox_item());
            }

            toolbar = toolbar
                .item(NativeToolbarItem::FlexibleSpace)
                .item(self.build_downloads_item());
        }

        if !is_browser_surface_active && !is_terminal_mode {
            if let Some(toolchain) = self.native_toolbar_state.status_toolchain.clone() {
                toolbar = toolbar.item(self.build_toolchain_item(toolchain));
            }
            if let Some(encoding) = self.native_toolbar_state.status_encoding.clone() {
                toolbar = toolbar.item(self.build_encoding_item(encoding));
            }
            if let Some(line_ending) = self.native_toolbar_state.status_line_ending.clone() {
                toolbar = toolbar.item(self.build_line_ending_item(line_ending));
            }
            if let Some(image_info) = self.native_toolbar_state.status_image_info.clone() {
                toolbar = toolbar.item(self.build_image_info_item(image_info));
            }
        }

        if let Some(item) = self.build_connection_status_item(cx) {
            toolbar = toolbar.item(item);
        }

        if show_update {
            toolbar = toolbar.item(self.build_update_item());
        }

        if user.is_none() && title_bar_settings.show_sign_in {
            toolbar = toolbar.item(self.build_sign_in_item());
        }

        toolbar = toolbar
            .item(NativeToolbarItem::Space)
            .item(self.build_user_menu_item(&user, menu_plan, cx));

        window.set_native_toolbar(Some(toolbar));
    }
}
