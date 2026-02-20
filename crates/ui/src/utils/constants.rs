use gpui::{Pixels, Window, px};

// Use pixels here instead of a rem-based size because the macOS traffic
// lights are a static size, and don't scale with the rest of the UI.
//
// Magic number: There is one extra pixel of padding on the left side due to
// the 1px border around the window on macOS apps.
#[cfg(macos_sdk_26)]
pub const TRAFFIC_LIGHT_PADDING: f32 = 78.;

#[cfg(not(macos_sdk_26))]
pub const TRAFFIC_LIGHT_PADDING: f32 = 71.;

/// Returns the platform-appropriate title bar height.
///
/// On macOS, this queries the actual titlebar height from the platform,
/// which accounts for the native toolbar.
/// On Windows, this returns a fixed height of 32px.
/// On other platforms, it scales with the window's rem size (1.75x) with a minimum of 34px.
#[cfg(target_os = "macos")]
pub fn platform_title_bar_height(window: &Window) -> Pixels {
    window.titlebar_height()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn platform_title_bar_height(window: &Window) -> Pixels {
    (1.75 * window.rem_size()).max(px(34.))
}

#[cfg(target_os = "windows")]
pub fn platform_title_bar_height(_window: &Window) -> Pixels {
    // todo(windows) instead of hard coded size report the actual size to the Windows platform API
    px(32.)
}
