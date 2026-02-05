mod app_store_connect;
mod build_logs;
pub mod panel;

pub use app_store_connect::AppStoreConnectTab;
pub use build_logs::BuildLogsView;
pub use panel::NativePlatformsPanel;

use gpui::App;

pub fn init(cx: &mut App) {
    panel::init(cx);
    app_store_connect::init(cx);
}
