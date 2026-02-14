use gpui::App;
use zed_actions::toast;

use crate::Workspace;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|_workspace, _: &toast::RunAction, window, cx| {
            let workspace = cx.entity();
            let window = window.window_handle();
            cx.defer(move |cx| {
                let action = workspace
                    .read(cx)
                    .toast_layer
                    .read(cx)
                    .active_toast_action();

                if let Some(on_click) = action.and_then(|action| action.on_click) {
                    window
                        .update(cx, |_, window, cx| {
                            on_click(window, cx);
                        })
                        .ok();
                }
            });
        });
    })
    .detach();
}
