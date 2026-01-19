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
| `Cmd+Shift+J` | Toggle bottom dock | Workspace (has effect in Editor Mode) |

### Terminal Mode Behavior

When entering Terminal Mode:
- If terminal sessions exist: Focus the last active terminal
- If no terminals exist: One is auto-created via `TerminalPanel::set_active(true)`
- If no project is open: Terminal opens in the user's home directory

**Re-opening terminals:** If a user closes all terminals while in Terminal Mode, they can:
- Press `Cmd+N` (or equivalent) to create a new terminal
- Switch back to Editor Mode

Unlike standalone terminal apps, we do NOT auto-recreate a terminal when the last one closes. This mirrors how editor panes work - closing all editors doesn't auto-create a new one.

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
    ├── workspace_modes.rs          # Crate root, init(), ModeId, actions
    ├── mode_switcher.rs            # UI: segmented control component
    └── persistence.rs              # Save/restore mode state (in workspace crate)
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
```

### Simplified Architecture

Rather than adding a separate mode registry/container abstraction, the actual implementation uses **conditional rendering directly in `Workspace`**:

- `Workspace` stores an `active_mode: ModeId` field
- `Workspace::render()` checks `active_mode` and renders differently:
  - **Editor Mode**: Normal dock layout (left/right/bottom docks + center panes)
  - **Terminal Mode**: Terminal panel rendered full-screen, replacing all other content
- `switch_to_mode()` changes the mode and handles focus transitions
- Mode is persisted via the existing workspace serialization system

This approach:
- Avoids circular dependencies (workspace_modes doesn't depend on workspace)
- Reuses existing `TerminalPanel` from bottom dock (no separate terminal mode struct)
- Keeps all mode logic in one place (Workspace)
- Is simpler to understand and maintain

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
│ │ Workspace::render()                                          │ │
│ │                                                              │ │
│ │   ┌─────────────────────────────────────────────────────┐   │ │
│ │   │ Editor Mode                                          │   │ │
│ │   │   - Left/Right/Bottom docks + center panes            │   │ │
│ │   └─────────────────────────────────────────────────────┘   │ │
│ │                                                              │ │
│ │                        ─── OR ───                            │ │
│ │                                                              │ │
│ │   ┌─────────────────────────────────────────────────────┐   │ │
│ │   │ Terminal Mode                                        │   │ │
│ │   │   - `TerminalPanel` rendered full-screen              │   │ │
│ │   └─────────────────────────────────────────────────────┘   │ │
│ └─────────────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ Dock Header + Title Bar Items                                │ │
│ └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Terminal: One System, Two Layouts

The key architectural principle: **one terminal implementation, multiple layouts**.

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
                      ┌───────────────┐
                      │ TerminalPanel │
                      └───────┬───────┘
                              │
                ┌─────────────┴─────────────┐
                │                           │
                ▼                           ▼
      Rendered in bottom dock         Rendered full-screen
         (Editor Mode)                 (Terminal Mode)
```

Terminal sessions are shared because **the same `TerminalPanel` is reused**. Switching modes changes *where* it’s rendered (dock vs full-screen), without recreating sessions.

### Session Sharing

Terminal sessions are owned by the `Project` (via `TerminalProvider`), not by the panel or mode. This allows:

1. Creating a terminal in the dock panel
2. Switching to Terminal Mode
3. Seeing the same terminal session
4. Switching back to Editor Mode
5. The terminal is still there in the dock

Implementation approach:
- `Workspace` renders the existing `TerminalPanel`:
  - In Editor Mode: as the bottom dock panel (normal behavior)
  - In Terminal Mode: full-screen (dock layout is not rendered)
- `Workspace::switch_to_mode(ModeId::TERMINAL, ...)` calls `TerminalPanel::set_active(true)` to ensure a terminal exists and then focuses the panel
- No terminal views are moved between containers; the layout changes, not the underlying session ownership

## Implementation Plan

### Phase 1: Foundation (New Crate + Core Types) ✓

**Files created:**
- `crates/workspace_modes/Cargo.toml`
- `crates/workspace_modes/src/workspace_modes.rs`

**Tasks completed:**
1. Created the `workspace_modes` crate with proper dependencies
2. Defined `ModeId` with EDITOR and TERMINAL constants
3. Defined actions: `SwitchToEditorMode`, `SwitchToTerminalMode`
4. Added crate to workspace `Cargo.toml`

### Phase 5: Mode Switcher UI ✓

**Files modified:**
- `crates/workspace_modes/src/mode_switcher.rs`
- `crates/title_bar/src/title_bar.rs`
- `crates/title_bar/Cargo.toml`

**Tasks completed:**
1. Created `ModeSwitcher` segmented control component using `ToggleButtonGroup`
2. Integrated into `TitleBar` at far left position
3. Click events dispatch `SwitchToEditorMode` / `SwitchToTerminalMode` actions

### Phase 6: Workspace Integration ✓

**Files modified:**
- `crates/workspace/src/workspace.rs`
- `crates/workspace/Cargo.toml`

**Tasks completed:**
1. Added `active_mode: ModeId` field to `Workspace` struct
2. Implemented conditional rendering in `Workspace::render()`:
   - Editor Mode: normal dock layout
   - Terminal Mode: terminal panel full-screen
3. Registered mode switch action handlers
4. Added `switch_to_mode()` method with proper focus management

### Phase 7: Keybindings

**Files to modify:**
- `assets/keymaps/default-macos.json`
- `assets/keymaps/default-linux.json`
- `assets/keymaps/default-windows.json`

**Tasks:**
1. Add `Cmd+J` / `Ctrl+J` → `workspace_modes::SwitchToTerminalMode`
2. Add `Cmd+E` / `Ctrl+E` → `workspace_modes::SwitchToEditorMode`
3. Move `workspace::ToggleBottomDock` to `Cmd+Shift+J` / `Ctrl+Shift+J` (freeing up `Cmd+J` / `Ctrl+J`)
4. Resolve any keybinding conflicts in default keymaps (comment out or remap as needed)

### Phase 8: Persistence ✓

**Files modified:**
- `crates/workspace/src/persistence.rs`
- `crates/workspace/src/persistence/model.rs`

**Tasks completed:**
1. Added `active_mode` column to workspaces table
2. Mode is serialized when workspace is serialized
3. Mode is restored when workspace is loaded
4. Existing workspaces default to Editor Mode

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
| `crates/workspace_modes/src/workspace_modes.rs` | Crate root, ModeId, actions |
| `crates/workspace_modes/src/mode_switcher.rs` | UI component (segmented control) |
| `crates/workspace_modes/docs/design.md` | This design document |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` (root) | Add workspace_modes to members |
| `crates/workspace/Cargo.toml` | Add workspace_modes dependency |
| `crates/workspace/src/workspace.rs` | Track `active_mode` and render conditionally |
| `crates/title_bar/Cargo.toml` | Add workspace_modes dependency |
| `crates/title_bar/src/title_bar.rs` | Add ModeSwitcher |
| `crates/workspace/src/persistence.rs` | Persist `active_mode` in workspace DB |
| `crates/workspace/src/persistence/model.rs` | Serialize `active_mode` with workspace state |
| `crates/zed/Cargo.toml` | Add workspace_modes dependency |
| `crates/zed/src/zed.rs` | Initialize modes |
| `assets/keymaps/default-*.json` | New keybindings |

## Testing Strategy

### Unit Tests
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
- The existing `terminal_path_like_target.rs` handles this via `workspace.open_paths()`
- Files open in the editor pane automatically (mode-agnostic)
- No special handling needed - the same terminal panel code works in both dock and full-screen mode

### Remote/SSH Projects
- If the terminal dock panel supports SSH, Terminal Mode supports it too
- Same terminal system, same capabilities
- No special handling needed

## Future Considerations

### Browser Mode
When implementing Browser Mode, follow the same pattern:
1. Add a new `ModeId` constant (e.g. `ModeId::BROWSER`)
2. Add a new segment to `ModeSwitcher`
3. Update `Workspace::render()` to render browser UI when `active_mode` matches
4. Extend persistence so the mode string can be restored for the new mode

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

---

## Implementation Progress

This section tracks the actual implementation progress and decisions made.

### Completed (2026-01-19)

#### Phase 1: Foundation ✓
- Created `workspace_modes` crate with proper structure
- Implemented `ModeId` with `EDITOR` and `TERMINAL` constants
- Defined actions: `SwitchToEditorMode`, `SwitchToTerminalMode`

#### Phase 5: ModeSwitcher UI ✓
- Implemented `ModeSwitcher` component using `ToggleButtonGroup`
- Segmented control with "Editor" and "Terminal" buttons
- Tooltips on each button
- Dispatches `SwitchToEditorMode` / `SwitchToTerminalMode` actions

#### Phase 6: Basic Workspace Integration ✓
- Added `workspace_modes` dependency to `workspace`, `title_bar`, `zed` crates
- Added `ModeSwitcher` to title bar (after application menu, before project items)
- Called `workspace_modes::init(cx)` in zed initialization

#### Phase 8: Persistence Types ✓
- Persisted `active_mode` as part of workspace serialization and workspace DB state

#### Phase 7: Keybindings ✓
**Status**: Completed across all platforms.

**macOS** (`default-macos.json`):
- `Cmd+J` → `workspace_modes::SwitchToTerminalMode`
- `Cmd+E` → `workspace_modes::SwitchToEditorMode`
- `Cmd+Shift+J` → `workspace::ToggleBottomDock` (moved from Cmd+J)
- Commented out conflicting `Cmd+E` bindings (buffer_search, keymap_editor)

**Linux** (`default-linux.json`):
- `Ctrl+J` → `workspace_modes::SwitchToTerminalMode`
- `Ctrl+E` → `workspace_modes::SwitchToEditorMode`
- `Ctrl+Shift+J` → `workspace::ToggleBottomDock` (moved from Ctrl+J)
- `Ctrl+Shift+Down` → `pane::SplitDown` in FileFinder (moved from Ctrl+J)
- `Ctrl+Shift+K` → `zed::OpenKeymapFile` in KeymapEditor (moved from Ctrl+E)
- Commented out conflicting terminal `Ctrl+E` SendKeystroke

**Windows** (`default-windows.json`):
- `Ctrl+J` → `workspace_modes::SwitchToTerminalMode`
- `Ctrl+E` → `workspace_modes::SwitchToEditorMode`
- `Ctrl+Shift+J` → `workspace::ToggleBottomDock` (moved from Ctrl+J)
- `Ctrl+Shift+Down` → `pane::SplitDown` in FileFinder (moved from Ctrl+J)
- `Ctrl+Shift+K` → `zed::OpenKeymapFile` in KeymapEditor (moved from Ctrl+E)
- Commented out conflicting file_finder `Ctrl+E` and terminal `Ctrl+E` SendKeystroke

#### Phase 8: Persistence ✓
**Status**: Completed.

- Added `active_mode` column to `workspaces` table in workspace persistence schema
- Migration added for existing workspaces (default to "editor" via `ModeId::from_str`)
- Active mode is saved when workspace is serialized (`serialize_workspace_internal`)
- Active mode is restored when workspace is loaded (`new_local` function)

#### Phase 6: Full Workspace Integration ✓ (2026-01-19 - Second Session)
**Status**: Completed - properly implemented mode switching in workspace.

**What was fixed:**
1. Removed the broken overlay hack that rendered terminal panel on top of editor content
2. Implemented proper conditional rendering in `Workspace::render()`:
   - When in Editor mode: renders normal dock layout (left/right/bottom docks + center panes)
   - When in Terminal mode: renders terminal panel full-screen, replacing all other content
3. Added proper focus management in `switch_to_mode()`:
   - Terminal mode: focuses the terminal panel
   - Editor mode: focuses the active editor pane
4. Mode switching now serializes workspace state for persistence
5. Active mode is restored from serialized workspace on load

**Technical approach:**
- Used conditional rendering with `if self.active_mode == ModeId::TERMINAL { ... } else { ... }` 
- Terminal mode uses the existing `TerminalPanel` from bottom dock, rendered full-screen
- No separate container entity needed - modes are tracked directly in `Workspace`
- This approach is simpler and avoids circular dependencies

#### Phase 9: Testing ✓ (2026-01-19)
**Status**: Integration tests completed.

**Tests added:**
- `test_mode_switching_basic` - Verifies switching between Editor and Terminal modes
- `test_mode_switching_idempotent` - Verifies repeated switches to same mode are no-ops
- `test_mode_switch_actions` - Verifies action dispatch works correctly
- `test_active_mode_persistence` - Verifies mode is saved/loaded from database correctly

All 116 workspace tests pass.

### Remaining Work

- **Manual Testing**: See manual testing checklist below
- **Future**: Browser Mode can be added following the same pattern

### Architecture Notes

#### Avoiding Circular Dependencies
- `workspace_modes` does NOT depend on `workspace`
- `workspace` depends on `workspace_modes`
- This keeps the dependency graph clean

#### Terminal Session Sharing
Terminal Mode reuses the existing `TerminalPanel` from the bottom dock, rendered full-screen. This means:
- Same terminal sessions in dock and full-screen mode
- No separate terminal mode struct needed
- All existing terminal features (tabs, splits, search) work automatically
- No PTY process recreation when switching modes
