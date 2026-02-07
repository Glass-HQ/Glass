//! Mode View Registry
//!
//! This module provides a global registry for mode views. Mode-specific crates
//! register their views here, and workspace queries this registry to render
//! the appropriate view for each mode.

use crate::ModeId;
use collections::HashMap;
use gpui::{AnyView, App, FocusHandle, Global};
use std::sync::Arc;

/// A view that can be displayed for a workspace mode.
///
/// Mode views are registered with the `ModeViewRegistry` and retrieved by
/// workspace when switching modes.
pub struct RegisteredModeView {
    /// The view to render for this mode
    pub view: AnyView,
    /// The focus handle for this view
    pub focus_handle: FocusHandle,
    /// Optional view to render in the title bar center when this mode is active
    pub titlebar_center_view: Option<AnyView>,
}

/// Factory function that creates a new mode view instance.
pub type ModeViewFactory = Arc<dyn Fn(&mut App) -> RegisteredModeView + Send + Sync>;

/// Global registry for mode views.
///
/// This registry allows mode-specific crates to register their views without
/// creating cyclic dependencies with workspace.
///
/// Modes can register either a concrete view (via `register`) or a factory
/// (via `register_factory`) for per-workspace instances.
#[derive(Default)]
pub struct ModeViewRegistry {
    views: HashMap<ModeId, RegisteredModeView>,
    factories: HashMap<ModeId, ModeViewFactory>,
    titlebar_center_views: HashMap<ModeId, AnyView>,
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

    /// Register a concrete view for a mode (shared across all windows)
    pub fn register(&mut self, mode_id: ModeId, view: RegisteredModeView) {
        self.views.insert(mode_id, view);
    }

    /// Register a factory that creates per-workspace view instances.
    /// Each workspace will call this factory to get its own independent view.
    pub fn register_factory(&mut self, mode_id: ModeId, factory: ModeViewFactory) {
        self.factories.insert(mode_id, factory);
    }

    /// Get the factory for a mode, if one is registered.
    pub fn factory(&self, mode_id: ModeId) -> Option<&ModeViewFactory> {
        self.factories.get(&mode_id)
    }

    /// Get the registered view for a mode (concrete, shared view)
    pub fn get(&self, mode_id: ModeId) -> Option<&RegisteredModeView> {
        self.views.get(&mode_id)
    }

    /// Check if a mode has a registered view or factory
    pub fn has_view(&self, mode_id: ModeId) -> bool {
        self.views.contains_key(&mode_id) || self.factories.contains_key(&mode_id)
    }

    /// Set the title bar center view for a mode
    pub fn set_titlebar_center_view(&mut self, mode_id: ModeId, view: AnyView) {
        self.titlebar_center_views.insert(mode_id, view);
    }

    /// Get the title bar center view for a mode
    pub fn titlebar_center_view(&self, mode_id: ModeId) -> Option<&AnyView> {
        self.titlebar_center_views
            .get(&mode_id)
            .or_else(|| {
                self.views
                    .get(&mode_id)
                    .and_then(|v| v.titlebar_center_view.as_ref())
            })
    }
}
