//! Mode View Registry
//!
//! This module provides a global registry for mode views. Mode-specific crates
//! register their views here, and workspace queries this registry to render
//! the appropriate view for each mode.

use crate::ModeId;
use collections::HashMap;
use gpui::{AnyView, App, FocusHandle, Global};

/// A view that can be displayed for a workspace mode.
///
/// Mode views are registered with the `ModeViewRegistry` and retrieved by
/// workspace when switching modes.
pub struct RegisteredModeView {
    /// The view to render for this mode
    pub view: AnyView,
    /// The focus handle for this view
    pub focus_handle: FocusHandle,
}

/// Global registry for mode views.
///
/// This registry allows mode-specific crates to register their views without
/// creating cyclic dependencies with workspace.
///
/// ## Usage
///
/// ```ignore
/// // In browser crate's init:
/// let browser_view = cx.new(|cx| BrowserView::new(cx));
/// ModeViewRegistry::global_mut(cx).register(
///     ModeId::BROWSER,
///     RegisteredModeView {
///         view: browser_view.clone().into(),
///         focus_handle: browser_view.focus_handle(cx),
///     },
/// );
///
/// // In workspace when rendering:
/// if let Some(mode_view) = ModeViewRegistry::global(cx).get(ModeId::BROWSER) {
///     // render mode_view.view
/// }
/// ```
#[derive(Default)]
pub struct ModeViewRegistry {
    views: HashMap<ModeId, RegisteredModeView>,
}

impl Global for ModeViewRegistry {}

impl ModeViewRegistry {
    /// Initialize the global registry
    pub fn init(cx: &mut App) {
        cx.set_global(Self::default());
    }

    /// Get a reference to the global registry
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// Get a mutable reference to the global registry
    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    /// Try to get the global registry, returns None if not initialized
    pub fn try_global(cx: &App) -> Option<&Self> {
        cx.try_global::<Self>()
    }

    /// Register a view for a mode
    pub fn register(&mut self, mode_id: ModeId, view: RegisteredModeView) {
        self.views.insert(mode_id, view);
    }

    /// Get the registered view for a mode
    pub fn get(&self, mode_id: ModeId) -> Option<&RegisteredModeView> {
        self.views.get(&mode_id)
    }

    /// Check if a mode has a registered view
    pub fn has_view(&self, mode_id: ModeId) -> bool {
        self.views.contains_key(&mode_id)
    }
}
