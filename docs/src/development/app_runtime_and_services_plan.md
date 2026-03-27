# App Runtime And Services Plan

## Context

This document replaces the previous `native_platforms` and App Store Connect prototype direction.

The old implementation is being deleted in this branch before release. The goal is to remove the incomplete model completely, then rebuild on a clearer architecture.

## Prototype History

The deleted prototype landed incrementally and without a stable long-term model:

- `062005c7d7` added early Apple project, scheme, simulator, and device support.
- `1b62d52b76` and `c6a3c0c7e5` expanded the App Store Connect prototype.
- `1b778b5508` and `db6032ef36` refactored build pipeline details.
- `2e7c24c7b4` and `0092913517` moved the feature into the workspace sidebar model.
- `cc6caaabc2` redesigned the sidebar panel with native GPUI components.

That work proved user interest, but it also revealed that the implementation was organized around the wrong abstractions.

## Why The Prototype Was Deleted

The deleted implementation had three core problems:

1. It treated native and mobile development as a special sidebar feature instead of normal project execution.
2. It mixed local runtime tooling and remote service management into one product area.
3. It encoded Apple-specific UI and workflow choices too early, before Glass had a general model for project detection, targets, devices, execution, and services.

The result was a split design:

- build and device controls lived in a dock panel
- App Store Connect already behaved like a workspace item
- the naming and crate boundaries implied a permanent product direction that is no longer desired

## Agreed Direction

Glass should support development for web, desktop, mobile, and cross-platform projects from one environment.

The right architectural axis is:

- project detection
- capabilities
- targets and devices
- build and run execution
- remote services and release management

This is explicitly not organized around a `native platforms` concept.

## Product Model

Glass should have three distinct layers:

### 1. Action Layer

This is the lightweight entry point for project execution.

It should:

- be reachable from the title bar or command palette
- open a dialog with target, device, and action controls
- stay out of the user’s way in monorepos and non-runnable workspaces

This layer is for fast actions, not for rich dashboards.

### 2. Execution Surfaces

These appear only after the user does something.

Examples:

- running opens or updates a run session item
- building opens or updates build output

The user should keep editing code normally and only see these surfaces when an action produces output.

### 3. Service Management

This is a separate product area from local runtime tooling.

Examples:

- App Store Connect
- Vercel
- Convex
- Supabase

These should be modeled as providers behind an internal service abstraction. The internal abstraction may later become a public protocol once the model has been proven across multiple providers.

## Capability Model

Glass should detect what a workspace can do, then expose the relevant controls.

Capabilities can include:

- discover targets
- discover devices
- run on simulator
- run on physical device
- build artifacts
- upload release artifacts
- manage service metadata

The UI should respond to capabilities, not to hardcoded framework names.

## Tooling Model

Local runtime tooling and remote services are different layers.

Local runtime tooling includes:

- Xcode
- simulators
- physical device tooling

Remote services include:

- App Store Connect
- Vercel
- Convex
- Supabase

Xcode is not analogous to App Store Connect. The model must preserve that distinction.

## Language Model

Language support should continue to live in Glass’s existing language/editor/LSP architecture.

What gets added here is not a separate native-language subsystem. What gets added is orchestration on top of existing language support:

- project detection
- runtime capability detection
- target and device selection
- execution and output routing
- service provider integration

## Planned Crate Boundaries

The replacement direction should use new names and new boundaries rather than extending the deleted crates.

Proposed shape:

- `app_runtime`
- `app_runtime_ui`
- `service_hub`
- provider crates such as `apple_tooling`, `gpui_tooling`

These names are placeholders. The important decision is the separation of responsibilities.

## Steps

Status snapshot on `feature/app-runtime-next-steps` as of 2026-03-27.

### Step 1: Delete The Prototype

Status: Done in this branch.

- [x] remove `native_platforms`
- [x] remove `native_platforms_ui`
- [x] remove workspace integration and marketing references

### Step 2: Define Detection And Capability Interfaces

Status: Done in this worktree.

- [x] detect Apple runnable project types in the workspace
- [x] detect GPUI runnable project types in the workspace
- [x] map detected projects to capability sets
- [x] keep the model independent from UI concerns

### Step 3: Build The Action Dialog

Status: Done in this worktree.

- [x] add a title bar button and command palette entry
- [x] open a dialog for target selection, device selection, and execution actions

### Step 4: Add Execution Surfaces

Status: Done in this worktree.

- [x] route Apple and GPUI run/build actions into reusable center-pane execution surfaces
- [x] separate runtime action selection from execution dispatch
- [x] create dedicated non-terminal run/build output items

### Step 5: Add Apple As The First Provider Set

Status: Done in this worktree.

- [x] implement Apple runtime capabilities against the new model, including simulator and macOS desktop local destinations
- [x] reintroduce App Store Connect as a service provider, not as part of a `native platforms` feature

### Step 6: Add GPUI Support

Status: Done in this worktree.

- [x] implement GPUI project detection and execution support

### Step 7: Introduce Internal Service Provider Abstractions

Status: Done in this worktree.

- [x] introduce an internal `service_hub` model
- [x] add a real App Store Connect provider backed by `asc` command planning
- [x] validate explicit artifact handoff for ASC build upload operations
- [x] surface ASC authentication, app browsing, build browsing, upload, and release operations in a workspace item

### Step 8: Consider Protocol Extraction Later

Status: Done for this phase.

- [x] do not publish a protocol early
- [x] first prove the model inside Glass
- [x] keep protocol extraction explicitly deferred until a later multi-provider phase

## Non-Goals For The Rebuild

- no recreation of a sidebar-native feature area
- no Apple-first naming that constrains future architecture
- no large dashboard as the default interaction model
- no public protocol until the internal abstraction is proven
