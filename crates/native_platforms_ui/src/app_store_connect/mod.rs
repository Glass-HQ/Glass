mod tab;

pub use tab::AppStoreConnectTab;

use gpui::{App, AppContext};
use workspace::Workspace;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(
            |workspace, _: &crate::panel::Deploy, window, cx| {
                let tab = cx.new(|cx| AppStoreConnectTab::new(window, cx));
                workspace.add_item_to_active_pane(Box::new(tab), None, true, window, cx);
            },
        );
    })
    .detach();
}
