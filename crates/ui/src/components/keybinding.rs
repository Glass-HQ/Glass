use std::rc::Rc;

use crate::PlatformStyle;
use crate::{Icon, IconName, IconSize, h_flex, prelude::*};
use gpui::{
    Action, AnyElement, App, FocusHandle, IntoElement, KeybindingKeystroke, Keystroke, Modifiers,
    Window, relative,
};
use itertools::Itertools;
use settings::KeybindSource;

#[derive(Debug)]
enum Source {
    Action {
        action: Box<dyn Action>,
        focus_handle: Option<FocusHandle>,
    },
    Keystrokes {
        /// A keybinding consists of a set of keystrokes,
        /// where each keystroke is a key and a set of modifier keys.
        /// More than one keystroke produces a chord.
        ///
        /// This should always contain at least one keystroke.
        keystrokes: Rc<[KeybindingKeystroke]>,
    },
}

impl Clone for Source {
    fn clone(&self) -> Self {
        match self {
            Source::Action {
                action,
                focus_handle,
            } => Source::Action {
                action: action.boxed_clone(),
                focus_handle: focus_handle.clone(),
            },
            Source::Keystrokes { keystrokes } => Source::Keystrokes {
                keystrokes: keystrokes.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, IntoElement, RegisterComponent)]
pub struct KeyBinding {
    source: Source,
    size: Option<AbsoluteLength>,
    /// The [`PlatformStyle`] to use when displaying this keybinding.
    platform_style: PlatformStyle,
    /// Indicates whether the keybinding is currently disabled.
    disabled: bool,
}

impl KeyBinding {
    /// Returns the highest precedence keybinding for an action. This is the last binding added to
    /// the keymap. User bindings are added after built-in bindings so that they take precedence.
    pub fn for_action(action: &dyn Action, cx: &App) -> Self {
        Self::new(action, None, cx)
    }

    /// Like `for_action`, but lets you specify the context from which keybindings are matched.
    pub fn for_action_in(action: &dyn Action, focus: &FocusHandle, cx: &App) -> Self {
        Self::new(action, Some(focus.clone()), cx)
    }
    pub fn has_binding(&self, window: &Window) -> bool {
        match &self.source {
            Source::Action {
                action,
                focus_handle: Some(focus),
            } => window
                .highest_precedence_binding_for_action_in(action.as_ref(), focus)
                .or_else(|| window.highest_precedence_binding_for_action(action.as_ref()))
                .is_some(),
            _ => false,
        }
    }

    pub fn new(action: &dyn Action, focus_handle: Option<FocusHandle>, _cx: &App) -> Self {
        Self {
            source: Source::Action {
                action: action.boxed_clone(),
                focus_handle,
            },
            size: None,
            platform_style: PlatformStyle::platform(),
            disabled: false,
        }
    }

    pub fn from_keystrokes(keystrokes: Rc<[KeybindingKeystroke]>, _source: KeybindSource) -> Self {
        Self {
            source: Source::Keystrokes { keystrokes },
            size: None,
            platform_style: PlatformStyle::platform(),
            disabled: false,
        }
    }

    /// Sets the [`PlatformStyle`] for this [`KeyBinding`].
    pub fn platform_style(mut self, platform_style: PlatformStyle) -> Self {
        self.platform_style = platform_style;
        self
    }

    /// Sets the size for this [`KeyBinding`].
    pub fn size(mut self, size: impl Into<AbsoluteLength>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Sets whether this keybinding is currently disabled.
    /// Disabled keybinds will be rendered in a dimmed state.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

fn render_key(
    key: &str,
    color: Option<Color>,
    platform_style: PlatformStyle,
    size: impl Into<Option<AbsoluteLength>>,
) -> AnyElement {
    let key_icon = icon_for_key(key, platform_style);
    match key_icon {
        Some(icon) => KeyIcon::new(icon, color).size(size).into_any_element(),
        None => {
            let key = util::capitalize(key);
            Key::new(&key, color).size(size).into_any_element()
        }
    }
}

impl RenderOnce for KeyBinding {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let render_keybinding = |keystrokes: &[KeybindingKeystroke]| {
            let color = self.disabled.then_some(Color::Disabled);

            h_flex()
                .debug_selector(|| {
                    format!(
                        "KEY_BINDING-{}",
                        keystrokes
                            .iter()
                            .map(|k| k.key().to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                })
                .gap(DynamicSpacing::Base04.rems(cx))
                .flex_none()
                .children(keystrokes.iter().map(|keystroke| {
                    h_flex()
                        .flex_none()
                        .py_0p5()
                        .rounded_xs()
                        .text_color(cx.theme().colors().text_muted)
                        .children(render_keybinding_keystroke(
                            keystroke,
                            color,
                            self.size,
                            PlatformStyle::platform(),
                        ))
                }))
                .into_any_element()
        };

        match self.source {
            Source::Action {
                action,
                focus_handle,
            } => focus_handle
                .or_else(|| window.focused(cx))
                .and_then(|focus| {
                    window.highest_precedence_binding_for_action_in(action.as_ref(), &focus)
                })
                .or_else(|| window.highest_precedence_binding_for_action(action.as_ref()))
                .map(|binding| render_keybinding(binding.keystrokes())),
            Source::Keystrokes { keystrokes } => Some(render_keybinding(keystrokes.as_ref())),
        }
        .unwrap_or_else(|| gpui::Empty.into_any_element())
    }
}

pub fn render_keybinding_keystroke(
    keystroke: &KeybindingKeystroke,
    color: Option<Color>,
    size: impl Into<Option<AbsoluteLength>>,
    platform_style: PlatformStyle,
) -> Vec<AnyElement> {
    let use_text = matches!(
        platform_style,
        PlatformStyle::Linux | PlatformStyle::Windows
    );
    let size = size.into();

    if use_text {
        let element = Key::new(
            keystroke_text(keystroke.modifiers(), keystroke.key(), platform_style),
            color,
        )
        .size(size)
        .into_any_element();
        vec![element]
    } else {
        let mut elements = Vec::new();
        elements.extend(render_modifiers(
            keystroke.modifiers(),
            platform_style,
            color,
            size,
            true,
        ));
        elements.push(render_key(keystroke.key(), color, platform_style, size));
        elements
    }
}

fn icon_for_key(key: &str, platform_style: PlatformStyle) -> Option<IconName> {
    match key {
        "left" => Some(IconName::ArrowLeft),
        "right" => Some(IconName::ArrowRight),
        "up" => Some(IconName::ArrowUp),
        "down" => Some(IconName::ArrowDown),
        "backspace" => Some(IconName::Backspace),
        "delete" => Some(IconName::Backspace),
        "return" => Some(IconName::Return),
        "enter" => Some(IconName::Return),
        "tab" => Some(IconName::Tab),
        "space" => Some(IconName::Space),
        "escape" => Some(IconName::Escape),
        "pagedown" => Some(IconName::PageDown),
        "pageup" => Some(IconName::PageUp),
        "shift" if platform_style == PlatformStyle::Mac => Some(IconName::Shift),
        "control" if platform_style == PlatformStyle::Mac => Some(IconName::Control),
        "platform" if platform_style == PlatformStyle::Mac => Some(IconName::Command),
        "function" if platform_style == PlatformStyle::Mac => Some(IconName::Control),
        "alt" if platform_style == PlatformStyle::Mac => Some(IconName::Option),
        _ => None,
    }
}

pub fn render_modifiers(
    modifiers: &Modifiers,
    platform_style: PlatformStyle,
    color: Option<Color>,
    size: Option<AbsoluteLength>,
    trailing_separator: bool,
) -> impl Iterator<Item = AnyElement> {
    #[derive(Clone)]
    enum KeyOrIcon {
        Key(&'static str),
        Plus,
        Icon(IconName),
    }

    struct Modifier {
        enabled: bool,
        mac: KeyOrIcon,
        linux: KeyOrIcon,
        windows: KeyOrIcon,
    }

    let table = {
        use KeyOrIcon::*;

        [
            Modifier {
                enabled: modifiers.function,
                mac: Icon(IconName::Control),
                linux: Key("Fn"),
                windows: Key("Fn"),
            },
            Modifier {
                enabled: modifiers.control,
                mac: Icon(IconName::Control),
                linux: Key("Ctrl"),
                windows: Key("Ctrl"),
            },
            Modifier {
                enabled: modifiers.alt,
                mac: Icon(IconName::Option),
                linux: Key("Alt"),
                windows: Key("Alt"),
            },
            Modifier {
                enabled: modifiers.platform,
                mac: Icon(IconName::Command),
                linux: Key("Super"),
                windows: Key("Win"),
            },
            Modifier {
                enabled: modifiers.shift,
                mac: Icon(IconName::Shift),
                linux: Key("Shift"),
                windows: Key("Shift"),
            },
        ]
    };

    let filtered = table
        .into_iter()
        .filter(|modifier| modifier.enabled)
        .collect::<Vec<_>>();

    let platform_keys = filtered
        .into_iter()
        .map(move |modifier| match platform_style {
            PlatformStyle::Mac => Some(modifier.mac),
            PlatformStyle::Linux => Some(modifier.linux),
            PlatformStyle::Windows => Some(modifier.windows),
        });

    let separator = match platform_style {
        PlatformStyle::Mac => None,
        PlatformStyle::Linux => Some(KeyOrIcon::Plus),
        PlatformStyle::Windows => Some(KeyOrIcon::Plus),
    };

    let platform_keys = itertools::intersperse(platform_keys, separator.clone());

    platform_keys
        .chain(if modifiers.modified() && trailing_separator {
            Some(separator)
        } else {
            None
        })
        .flatten()
        .map(move |key_or_icon| match key_or_icon {
            KeyOrIcon::Key(key) => Key::new(key, color).size(size).into_any_element(),
            KeyOrIcon::Icon(icon) => KeyIcon::new(icon, color).size(size).into_any_element(),
            KeyOrIcon::Plus => "+".into_any_element(),
        })
}

#[derive(IntoElement)]
pub struct Key {
    key: SharedString,
    color: Option<Color>,
    size: Option<AbsoluteLength>,
}

impl RenderOnce for Key {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let single_char = self.key.len() == 1;
        let size = self
            .size
            .unwrap_or_else(|| TextSize::default().rems(cx).into());

        div()
            .py_0()
            .map(|this| {
                if single_char {
                    this.w(size).flex().flex_none().justify_center()
                } else {
                    this.px_0p5()
                }
            })
            .h(size)
            .text_size(size)
            .line_height(relative(1.))
            .text_color(self.color.unwrap_or(Color::Muted).color(cx))
            .child(self.key)
    }
}

impl Key {
    pub fn new(key: impl Into<SharedString>, color: Option<Color>) -> Self {
        Self {
            key: key.into(),
            color,
            size: None,
        }
    }

    pub fn size(mut self, size: impl Into<Option<AbsoluteLength>>) -> Self {
        self.size = size.into();
        self
    }
}

#[derive(IntoElement)]
pub struct KeyIcon {
    icon: IconName,
    color: Option<Color>,
    size: Option<AbsoluteLength>,
}

impl RenderOnce for KeyIcon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let size = self.size.unwrap_or(IconSize::Small.rems().into());

        Icon::new(self.icon)
            .size(IconSize::Custom(size.to_rems(window.rem_size())))
            .color(self.color.unwrap_or(Color::Muted))
    }
}

impl KeyIcon {
    pub fn new(icon: IconName, color: Option<Color>) -> Self {
        Self {
            icon,
            color,
            size: None,
        }
    }

    pub fn size(mut self, size: impl Into<Option<AbsoluteLength>>) -> Self {
        self.size = size.into();
        self
    }
}

/// Returns a textual representation of the key binding for the given [`Action`].
pub fn text_for_action(action: &dyn Action, window: &Window, cx: &App) -> Option<String> {
    let key_binding = window.highest_precedence_binding_for_action(action)?;
    Some(text_for_keybinding_keystrokes(key_binding.keystrokes(), cx))
}

pub fn text_for_keystrokes(keystrokes: &[Keystroke], _cx: &App) -> String {
    let platform_style = PlatformStyle::platform();
    keystrokes
        .iter()
        .map(|keystroke| keystroke_text(&keystroke.modifiers, &keystroke.key, platform_style))
        .join(" ")
}

pub fn text_for_keybinding_keystrokes(keystrokes: &[KeybindingKeystroke], _cx: &App) -> String {
    let platform_style = PlatformStyle::platform();
    keystrokes
        .iter()
        .map(|keystroke| keystroke_text(keystroke.modifiers(), keystroke.key(), platform_style))
        .join(" ")
}

pub fn text_for_keystroke(modifiers: &Modifiers, key: &str, _cx: &App) -> String {
    let platform_style = PlatformStyle::platform();
    keystroke_text(modifiers, key, platform_style)
}

/// Returns a textual representation of the given [`Keystroke`].
fn keystroke_text(modifiers: &Modifiers, key: &str, platform_style: PlatformStyle) -> String {
    let mut text = String::new();
    let delimiter = '-';

    if modifiers.function {
        text.push_str("Fn");
        text.push(delimiter);
    }

    if modifiers.control {
        match platform_style {
            PlatformStyle::Mac => text.push_str("Control"),
            PlatformStyle::Linux | PlatformStyle::Windows => text.push_str("Ctrl"),
        }

        text.push(delimiter);
    }

    if modifiers.platform {
        match platform_style {
            PlatformStyle::Mac => text.push_str("Command"),
            PlatformStyle::Linux => text.push_str("Super"),
            PlatformStyle::Windows => text.push_str("Win"),
        }

        text.push(delimiter);
    }

    if modifiers.alt {
        match platform_style {
            PlatformStyle::Mac => text.push_str("Option"),
            PlatformStyle::Linux | PlatformStyle::Windows => text.push_str("Alt"),
        }

        text.push(delimiter);
    }

    if modifiers.shift {
        text.push_str("Shift");
        text.push(delimiter);
    }

    let key = match key {
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        key => &util::capitalize(key),
    };
    text.push_str(key);

    text
}

impl Component for KeyBinding {
    fn scope() -> ComponentScope {
        ComponentScope::Typography
    }

    fn name() -> &'static str {
        "KeyBinding"
    }

    fn description() -> Option<&'static str> {
        Some("A component that displays a key binding, supporting different platform styles.")
    }

    // fn preview(_window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    //     Some(
    //         v_flex()
    //             .gap_6()
    //             .children(vec![
    //                 example_group_with_title(
    //                     "Basic Usage",
    //                     vec![
    //                         single_example(
    //                             "Default",
    //                             KeyBinding::new_from_gpui(
    //                                 gpui::KeyBinding::new("ctrl-s", gpui::NoAction, None),
    //                                 cx,
    //                             )
    //                             .into_any_element(),
    //                         ),
    //                         single_example(
    //                             "Mac Style",
    //                             KeyBinding::new_from_gpui(
    //                                 gpui::KeyBinding::new("cmd-s", gpui::NoAction, None),
    //                                 cx,
    //                             )
    //                             .platform_style(PlatformStyle::Mac)
    //                             .into_any_element(),
    //                         ),
    //                         single_example(
    //                             "Windows Style",
    //                             KeyBinding::new_from_gpui(
    //                                 gpui::KeyBinding::new("ctrl-s", gpui::NoAction, None),
    //                                 cx,
    //                             )
    //                             .platform_style(PlatformStyle::Windows)
    //                             .into_any_element(),
    //                         ),
    //                     ],
    //                 ),
    //                 example_group_with_title(
    //                     "Complex Bindings",
    //                     vec![
    //                         single_example(
    //                             "Multiple Keys",
    //                             KeyBinding::new_from_gpui(
    //                                 gpui::KeyBinding::new("ctrl-k ctrl-b", gpui::NoAction, None),
    //                                 cx,
    //                             )
    //                             .into_any_element(),
    //                         ),
    //                         single_example(
    //                             "With Shift",
    //                             KeyBinding::new_from_gpui(
    //                                 gpui::KeyBinding::new("shift-cmd-p", gpui::NoAction, None),
    //                                 cx,
    //                             )
    //                             .into_any_element(),
    //                         ),
    //                     ],
    //                 ),
    //             ])
    //             .into_any_element(),
    //     )
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_for_keystroke() {
        let keystroke = Keystroke::parse("cmd-c").unwrap();
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Mac),
            "Command-C".to_string()
        );
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Linux),
            "Super-C".to_string()
        );
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Windows),
            "Win-C".to_string()
        );

        let keystroke = Keystroke::parse("ctrl-alt-delete").unwrap();
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Mac),
            "Control-Option-Delete".to_string()
        );
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Linux),
            "Ctrl-Alt-Delete".to_string()
        );
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Windows),
            "Ctrl-Alt-Delete".to_string()
        );

        let keystroke = Keystroke::parse("shift-pageup").unwrap();
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Mac),
            "Shift-PageUp".to_string()
        );
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Linux),
            "Shift-PageUp".to_string()
        );
        assert_eq!(
            keystroke_text(&keystroke.modifiers, &keystroke.key, PlatformStyle::Windows),
            "Shift-PageUp".to_string()
        );
    }
}
