use gpui::{App, ClickEvent, CursorStyle, NativeButton, NativeIconButton, Window};

/// A trait for elements that can be clicked. Enables the use of the `on_click` method.
pub trait Clickable {
    /// Sets the click handler that will fire whenever the element is clicked.
    fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self;
    /// Sets the cursor style when hovering over the element.
    fn cursor_style(self, cursor_style: CursorStyle) -> Self;
}

impl Clickable for NativeButton {
    fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click(handler)
    }

    fn cursor_style(self, _cursor_style: CursorStyle) -> Self {
        self
    }
}

impl Clickable for NativeIconButton {
    fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click(handler)
    }

    fn cursor_style(self, _cursor_style: CursorStyle) -> Self {
        self
    }
}
