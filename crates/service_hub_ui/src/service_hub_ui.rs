mod app_store_connect_page;

use app_store_connect_page::AppStoreConnectPage;
use gpui::{App, actions};
use workspace::Workspace;

actions!(
    service_hub,
    [
        /// Opens App Store Connect service management for the current workspace.
        OpenAppStoreConnect
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace,
         window: Option<&mut gpui::Window>,
         _cx: &mut gpui::Context<Workspace>| {
            let Some(_) = window else {
                return;
            };

            workspace.register_action(move |workspace, _: &OpenAppStoreConnect, window, cx| {
                if let Some(existing) = workspace.item_of_type::<AppStoreConnectPage>(cx) {
                    workspace.activate_item(&existing, true, true, window, cx);
                    return;
                }

                let page = AppStoreConnectPage::new(workspace, window, cx);
                workspace.add_item_to_active_pane(Box::new(page), None, true, window, cx);
            });
        },
    )
    .detach();
}
