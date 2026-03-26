mod runtime_actions_modal;

use gpui::{App, actions};
use runtime_actions_modal::RuntimeActionsModal;
use workspace::Workspace;

actions!(
    app_runtime,
    [
        /// Opens runtime actions for detected Apple projects in the current workspace.
        OpenRuntimeActions
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _: Option<&mut gpui::Window>, _: &mut gpui::Context<Workspace>| {
            workspace.register_action(RuntimeActionsModal::toggle);
        },
    )
    .detach();
}
