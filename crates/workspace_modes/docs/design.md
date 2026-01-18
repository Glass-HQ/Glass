# Workspace Modes

## Overview

Workspace Modes introduces a system for switching between different full-screen interfaces within Glass. Rather than being just an IDE with embedded panels, Glass becomes a complete development environment with first-class experiences for editing code, using the terminal, and (in the future) browsing the web.

### Vision

Glass isn't just an IDE—it's a complete development environment.

- **Editor Mode**: Full code editing experience (current default)
- **Terminal Mode**: Full terminal experience (like Ghostty/iTerm, not a watered-down panel)
- **Browser Mode** (future): Full browser experience
- And potentially more modes...

Each mode is a **first-class citizen**, not a dock/panel. The dock versions are convenience features; the modes are the real experience.

## Goals

1. **First-class terminal experience**: A dedicated Terminal Mode that rivals standalone terminal apps
2. **Unified terminal system**: One terminal implementation, two presentation modes (dock panel vs full mode)
3. **Extensible architecture**: Easy to add new modes (Browser Mode, etc.) in the future
4. **Seamless switching**: Instant mode switching with keyboard shortcuts
5. **State persistence**: Remember which mode each workspace was in

## Non-Goals (for v1)

- Session sidebar in Terminal Mode (future feature)
- Animation transitions between modes
- Mode-specific settings UI
- Browser Mode implementation

## User Experience

### Mode Switcher

A segmented control appears at the **far left of the title bar**, before the project/branch selectors:

```
┌─────────────────────────────────────────────────────────────────────┐
│ [Editor][Terminal]  ProjectName  main ▾        ...rest of titlebar │
└─────────────────────────────────────────────────────────────────────┘
```

- Clicking a segment switches to that mode instantly
- The active mode is visually highlighted
- Future modes will appear as additional segments

### Keybindings

| Keybinding | Action | Context |
|------------|--------|---------|
| `Cmd+J` | Switch to Terminal Mode | Global |
| `Cmd+E` | Switch to Editor Mode | Global |
| `Cmd+Shift+J` | Toggle terminal dock panel | Editor Mode only |

### Terminal Mode Behavior

When entering Terminal Mode:
- If terminal sessions exist: Focus the last active terminal
- If no terminals exist: Create one in the project's working directory
- If no project is open: Create one in the user's home directory

There is always at least one terminal in Terminal Mode (like a standalone terminal app).

### Available Features in Terminal Mode

All features available in the terminal dock panel are also available in Terminal Mode:
- Multiple terminal tabs
- Split panes (horizontal/vertical)
- Search within terminal (`Cmd+F`)
- Copy/paste
- VI mode
- Clickable links
- Command palette (`Cmd+Shift+P`)
- File picker (`Cmd+P`)
- Quick open functionality

## Architecture

### Crate Structure

```
crates/workspace_modes/
├── Cargo.toml
├── docs/
│   └── design.md                   # This file
└── src/
    ├── workspace_modes.rs          # Crate root, init(), re-exports
    ├── mode_registry.rs            # ModeRegistry: tracks registered modes
    ├── mode_switcher.rs            # UI: segmented control component
    ├── mode_container.rs           # Container that renders active mode
    ├── persistence.rs              # Save/restore mode state
    └── modes/
        ├── mod.rs
        ├── editor_mode.rs          # Wraps existing workspace center
        └── terminal_mode.rs        # Full terminal experience
```

### Core Types

```rust
/// Unique identifier for a workspace mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModeId(&'static str);

impl ModeId {
    pub const EDITOR: ModeId = ModeId("editor");
    pub const TERMINAL: ModeId = ModeId("terminal");
}

/// Trait that all workspace modes must implement
pub trait WorkspaceMode: Render + Focusable + EventEmitter<ModeEvent> {
    /// Unique identifier for this mode
    fn id(&self) -> ModeId;
    
    /// Display name shown in the mode switcher
    fn name(&self) -> &'static str;
    
    /// Key context for mode-specific keybindings
    fn key_context(&self) -> KeyContext;
    
    /// Called when switching TO this mode
    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>);
    
    /// Called when switching AWAY from this mode
    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>);
    
    /// Whether this mode can be activated
    fn can_activate(&self, cx: &App) -> bool;
}

/// Events emitted by modes
pub enum ModeEvent {
    /// Request to switch to another mode (e.g., when clicking a file link in terminal)
    RequestSwitchTo(ModeId),
}

/// Registry that tracks all available modes
pub struct ModeRegistry {
    modes: HashMap<ModeId, Box<dyn Fn(&mut Window, &mut App) -> AnyView>>,
    order: Vec<ModeId>,  // Display order in switcher
}

/// Container that manages mode instances and renders the active one
pub struct ModeContainer {
    workspace: WeakEntity<Workspace>,
    active_mode_id: ModeId,
    modes: HashMap<ModeId, AnyView>,
    registry: Arc<ModeRegistry>,
}
```

### Integration Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│ Window                                                           │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ TitleBar                                                     │ │
│ │ ┌──────────────────┐                                        │ │
│ │ │   ModeSwitcher   │  ProjectName  main ▾  ...              │ │
│ │ │ [Editor][Terminal]                                         │ │
│ │ └──────────────────┘                                        │ │
│ └─────────────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ ModeContainer                                                │ │
│ │                                                              │ │
│ │   ┌─────────────────────────────────────────────────────┐   │ │
│ │   │ EditorMode                                           │   │ │
│ │   │   - Left dock (Project Panel, etc.)                  │   │ │
│ │   │   - Center (Editor Panes)                            │   │ │
│ │   │   - Right dock (Outline, etc.)                       │   │ │
│ │   │   - Bottom dock (Terminal Panel, etc.)               │   │ │
│ │   └─────────────────────────────────────────────────────┘   │ │
│ │                                                              │ │
│ │                        ─── OR ───                            │ │
│ │                                                              │ │
│ │   ┌─────────────────────────────────────────────────────┐   │ │
│ │   │ TerminalMode                                         │   │ │
│ │   │   - Full-window terminal PaneGroup                   │   │ │
│ │   │   - Tab bar with terminal tabs                       │   │ │
│ │   │   - Supports splits, search, all terminal features   │   │ │
│ │   └─────────────────────────────────────────────────────┘   │ │
│ │                                                              │ │
│ └─────────────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ StatusBar                                                    │ │
│ └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Terminal: One System, Two Presentations

The key architectural principle: **one terminal implementation, two presentation contexts**.

```
                    ┌─────────────────────┐
                    │   Terminal Core     │
                    │   (crates/terminal) │
                    │   - PTY management  │
                    │   - Alacritty       │
                    │   - Shell handling  │
                    └──────────┬──────────┘
                               │
                    ┌──────────┴──────────┐
                    │    TerminalView     │
                    │ (crates/terminal_   │
                    │      view)          │
                    │ - Rendering         │
                    │ - Input handling    │
                    └──────────┬──────────┘
                               │
              ┌────────────────┴────────────────┐
              │                                 │
              ▼                                 ▼
    ┌─────────────────┐               ┌─────────────────┐
    │  TerminalPanel  │               │  TerminalMode   │
    │ (dock in Editor │◄─── shared ──►│ (full window in │
    │     Mode)       │    sessions   │  Terminal Mode) │
    └─────────────────┘               └─────────────────┘
```

Both `TerminalPanel` and `TerminalMode` access the same terminal sessions. When you switch modes, you're switching the container, not recreating terminals.

### Session Sharing

Terminal sessions are owned by the `Project` (via `TerminalProvider`), not by the panel or mode. This allows:

1. Creating a terminal in the dock panel
2. Switching to Terminal Mode
3. Seeing the same terminal session
4. Switching back to Editor Mode
5. The terminal is still there in the dock

Implementation approach:
- `TerminalPanel` continues to own its `PaneGroup` for dock presentation
- `TerminalMode` has its own `PaneGroup` for full-screen presentation
- Both can display the same `TerminalView` instances
- When switching modes, active terminal views can be "moved" between containers

## Implementation Plan

### Phase 1: Foundation (New Crate + Core Types)

**Files to create:**
- `crates/workspace_modes/Cargo.toml` ✓
- `crates/workspace_modes/src/workspace_modes.rs` ✓
- `crates/workspace_modes/src/mode_registry.rs` ✓

**Tasks:**
1. ✓ Create the `workspace_modes` crate with proper dependencies
2. ✓ Define `ModeId`, `WorkspaceMode` trait, `ModeEvent` enum
3. Implement `ModeRegistry` for tracking modes (flesh out stub)
4. Add crate to workspace `Cargo.toml`
5. Write unit tests for registry

### Phase 2: Mode Container

**Files to modify:**
- `crates/workspace_modes/src/mode_container.rs`

**Tasks:**
1. Implement `ModeContainer` struct
2. Handle mode instantiation (lazy, on first switch)
3. Handle mode activation/deactivation lifecycle
4. Implement `Render` for `ModeContainer`
5. Handle focus management when switching modes

### Phase 3: Editor Mode

**Files to modify:**
- `crates/workspace_modes/src/modes/editor_mode.rs`

**Tasks:**
1. Create `EditorMode` that wraps existing workspace center + docks
2. Implement `WorkspaceMode` trait fully
3. Ensure all existing functionality works unchanged
4. This is largely a "pass-through" to existing workspace rendering

### Phase 4: Terminal Mode

**Files to modify:**
- `crates/workspace_modes/src/modes/terminal_mode.rs`
- `crates/terminal_view/src/terminal_view.rs` (export utilities)
- `crates/terminal_view/src/terminal_panel.rs` (share session management)

**Tasks:**
1. Create `TerminalMode` struct with its own `PaneGroup`
2. Implement `WorkspaceMode` trait fully
3. Reuse existing `TerminalView`, pane splitting, tab bar logic
4. Ensure at least one terminal always exists
5. Handle terminal creation with correct working directory
6. Wire up all terminal features (search, splits, etc.)

### Phase 5: Mode Switcher UI

**Files to modify:**
- `crates/workspace_modes/src/mode_switcher.rs`
- `crates/title_bar/src/title_bar.rs`
- `crates/title_bar/Cargo.toml` (add dependency)

**Tasks:**
1. Create `ModeSwitcher` segmented control component
2. Style to match Glass design system
3. Handle click events to dispatch mode switch actions
4. Integrate into `TitleBar` at far left position
5. Ensure proper visual feedback for active mode

### Phase 6: Workspace Integration

**Files to modify:**
- `crates/workspace/src/workspace.rs`
- `crates/workspace/Cargo.toml`

**Tasks:**
1. Add `ModeContainer` to `Workspace` struct
2. Modify `Workspace::render()` to delegate to `ModeContainer`
3. Register mode switch actions (`SwitchToEditorMode`, `SwitchToTerminalMode`)
4. Ensure existing workspace serialization still works

### Phase 7: Keybindings

**Files to modify:**
- `assets/keymaps/default-macos.json`
- `assets/keymaps/default-linux.json`
- `assets/keymaps/default-windows.json`

**Tasks:**
1. Add `Cmd+J` / `Ctrl+J` → `workspace_modes::SwitchToTerminalMode`
2. Add `Cmd+E` / `Ctrl+E` → `workspace_modes::SwitchToEditorMode`
3. Change `Cmd+Shift+J` → `terminal_panel::Toggle` (dock panel in Editor Mode)
4. Add key context conditions so dock toggle only works in Editor Mode

### Phase 8: Persistence

**Files to modify:**
- `crates/workspace_modes/src/persistence.rs`
- `crates/workspace/src/persistence.rs` (or integrate there)

**Tasks:**
1. Serialize active mode ID per workspace
2. Restore mode on workspace load
3. Handle migration (existing workspaces default to Editor Mode)

### Phase 9: Polish & Testing

**Tasks:**
1. Ensure command palette works in Terminal Mode
2. Ensure file picker works in Terminal Mode
3. Test mode switching under various conditions
4. Test with no project open (terminal opens in home dir)
5. Test with remote/SSH projects
6. Performance testing (mode switch should be instant)
7. Write integration tests

## File Changes Summary

### New Files

| File | Purpose |
|------|---------|
| `crates/workspace_modes/Cargo.toml` | Crate manifest |
| `crates/workspace_modes/src/workspace_modes.rs` | Crate root |
| `crates/workspace_modes/src/mode_registry.rs` | Mode registration |
| `crates/workspace_modes/src/mode_container.rs` | Active mode rendering |
| `crates/workspace_modes/src/mode_switcher.rs` | UI component |
| `crates/workspace_modes/src/persistence.rs` | State persistence |
| `crates/workspace_modes/src/modes/mod.rs` | Modes module |
| `crates/workspace_modes/src/modes/editor_mode.rs` | Editor Mode |
| `crates/workspace_modes/src/modes/terminal_mode.rs` | Terminal Mode |
| `crates/workspace_modes/docs/design.md` | This design document |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` (root) | Add workspace_modes to members |
| `crates/workspace/Cargo.toml` | Add workspace_modes dependency |
| `crates/workspace/src/workspace.rs` | Integrate ModeContainer |
| `crates/title_bar/Cargo.toml` | Add workspace_modes dependency |
| `crates/title_bar/src/title_bar.rs` | Add ModeSwitcher |
| `crates/terminal_view/src/terminal_panel.rs` | Share session utilities |
| `crates/zed/Cargo.toml` | Add workspace_modes dependency |
| `crates/zed/src/zed.rs` | Initialize modes |
| `assets/keymaps/default-*.json` | New keybindings |

## Testing Strategy

### Unit Tests
- `ModeRegistry` registration and lookup
- `ModeId` equality and hashing
- Persistence serialization/deserialization

### Integration Tests
- Mode switching preserves terminal sessions
- Keybindings trigger correct actions
- Terminal Mode creates terminal when none exists
- Working directory is correct (project dir vs home dir)

### Manual Testing
- Visual appearance of mode switcher
- Mode switching performance
- All terminal features work in Terminal Mode
- Command palette/file picker work in Terminal Mode

## Edge Cases

### Opening Terminal Without a Project
When Glass is opened without a project folder:
- `default_working_directory()` in `terminal_view.rs` falls back to `dirs::home_dir()`
- Terminal Mode will use this same logic
- A terminal will open in the user's home directory

### File Links in Terminal
When a file link is clicked in Terminal Mode:
- Emit `ModeEvent::RequestSwitchTo(ModeId::EDITOR)`
- Switch to Editor Mode
- Open the file in the editor

### Remote/SSH Projects
- If the terminal dock panel supports SSH, Terminal Mode supports it too
- Same terminal system, same capabilities
- No special handling needed

## Future Considerations

### Browser Mode
When implementing Browser Mode, follow the same pattern:
1. Create `browser_mode.rs` in `modes/`
2. Implement `WorkspaceMode` trait
3. Register with `ModeRegistry`
4. Add to `ModeSwitcher`

### Session Sidebar (Terminal Mode)
Future enhancement for Terminal Mode:
- Vertical sidebar showing terminal sessions
- Drag-and-drop to reorder
- Right-click context menu
- Session naming/renaming

### Mode-Specific Settings
Future enhancement:
- Settings that only apply to specific modes
- E.g., Terminal Mode font size independent of Editor font size

## Design Decisions

### Why a New Crate?

1. **Separation of concerns**: The mode system is a distinct concept from workspace layout/panes
2. **Future extensibility**: Browser Mode and other modes will have a clean home
3. **Testability**: Mode switching logic can be tested in isolation
4. **Clarity**: Clear ownership of the mode switching feature

### Why Shared Terminal Sessions?

1. **Consistency**: Same terminal whether in dock or full mode
2. **No duplication**: One terminal implementation, not two
3. **User expectation**: Switching modes shouldn't kill terminals
4. **Resource efficiency**: No need to recreate PTY processes

### Why Segmented Control (Not Tabs)?

1. **Clarity**: Modes are fundamentally different from tabs
2. **Always visible**: User always knows which mode they're in
3. **Future-proof**: Works well with 2, 3, or more modes
4. **Compact**: Doesn't take much title bar space
