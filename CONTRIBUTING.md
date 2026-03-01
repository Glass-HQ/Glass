# Contributing to Glass

Thank you for helping us make Glass better!

> Glass is a fork of [Zed](https://github.com/zed-industries/zed). When contributing, please keep in mind that some crates and patterns originate from upstream Zed.

## Contribution ideas

Glass is in active development. We welcome PRs that are:

- Fixing or extending the docs.
- Fixing bugs.
- Small enhancements to existing features to make them work for more people.
- Small extra features, like keybindings or actions you miss from other editors or extensions.

If you're looking for concrete ideas, check the [issues](https://github.com/Glass-HQ/Glass/issues) page.

## Sending changes

The best way to get us to take a look at a proposed change is to send a pull
request. We will get back to you (though this sometimes takes longer than we'd
like, sorry).

- Make sure the change is **desired**: we're always happy to accept bugfixes,
  but features should be confirmed with us first if you aim to avoid wasted
  effort. If there isn't already a GitHub issue for your feature with
  confirmation that we want it, start with a GitHub discussion rather than a PR.
- Include a clear description of **what you're solving**, and why it's important.
- Include **tests**. For UI changes, consider updating visual regression tests (see [Building Glass for macOS](./docs/src/development/macos.md#visual-regression-tests)).
- If it changes the UI, attach **screenshots** or screen recordings.
- Make the PR about **one thing only**, e.g. if it's a bugfix, don't add two
  features and a refactoring on top of that.
- Keep AI assistance under your judgement and responsibility: it's unlikely
  we'll merge a vibe-coded PR that the author doesn't understand.

### UI/UX checklist

When your changes affect UI, consult this checklist:

**Accessibility / Ergonomics**
- Do all keyboard shortcuts work as intended?
- Are shortcuts discoverable (tooltips, menus, docs)?
- Do all mouse actions work (drag, context menus, resizing, scrolling)?
- Does the feature look great in light mode and dark mode?
- Are hover states, focus rings, and active states clear and consistent?
- Is it usable without a mouse (keyboard-only navigation)?

**Responsiveness**
- Does the UI scale gracefully on:
    - Narrow panes (e.g., side-by-side split views)?
    - Short panes (e.g., laptops with 13" displays)?
    - High-DPI / Retina displays?
- Does resizing panes or windows keep the UI usable and attractive?
- Do dialogs or modals stay centered and within viewport bounds?

**Performance**
- All user interactions must have instant feedback.
    - If the user requests something slow (e.g. an LLM generation) there should be some indication of the work in progress.
- Does it handle large files, big projects, or heavy workloads without degrading?
- Frames must take no more than 8ms (120fps)

**Consistency**
- Does it match Glass's design language (spacing, typography, icons)?
- Are terminology, labels, and tone consistent with the rest of Glass?
- Are interactions consistent (e.g., how tabs close, how modals dismiss, how errors show)?

**Internationalization & Text**
- Are strings concise, clear, and unambiguous?

**User Paths & Edge Cases**
- What does the happy path look like?
- What does the unhappy path look like? (errors, rejections, invalid states)
- How does it behave if data is missing, corrupted, or delayed?
- Are error messages actionable and consistent with Glass's voice?

**Discoverability & Learning**
- Can a first-time user figure it out without docs?
- Is there an intuitive way to undo/redo actions?
- Are power features discoverable but not intrusive?
- Is there a path from beginner to expert usage (progressive disclosure)?

## Things we will (probably) not merge

Although there are few hard and fast rules, typically we don't merge:

- Anything that can be provided by an extension. For adding themes or support for a new language, check out the [docs on developing extensions](https://zed.dev/docs/extensions/developing-extensions).
- Features where (in our subjective opinion) the extra complexity isn't worth it for the number of people who will benefit.
- Giant refactorings.
- Non-trivial changes with no tests.
- Stylistic code changes that do not alter any app logic.
- Anything that seems AI-generated without understanding the output.

## Bird's-eye view of Glass

Glass is built on top of Zed's crate architecture. Here are the crates you're most likely to interact with:

- [`gpui`](/crates/gpui) is a GPU-accelerated UI framework which provides all of the building blocks for Glass. We maintain a [standalone fork](https://github.com/Obsydian-HQ/gpui) with native iOS/macOS component extensions. **We recommend familiarizing yourself with the root level GPUI documentation.**
- [`editor`](/crates/editor) contains the core `Editor` type that drives both the code editor and all various input fields. It also handles a display layer for LSP features such as Inlay Hints or code completions.
- [`project`](/crates/project) manages files and navigation within the filetree. It is also the app's side of communication with LSP.
- [`workspace`](/crates/workspace) handles local state serialization and groups projects together.
- [`browser`](/crates/browser) provides the integrated browser powered by CEF.
- [`lsp`](/crates/lsp) handles communication with external LSP servers.
- [`language`](/crates/language) drives `editor`'s understanding of language — from providing a list of symbols to the syntax map.
- [`theme`](/crates/theme) defines the theme system and provides default themes.
- [`ui`](/crates/ui) is a collection of UI components and common patterns used throughout Glass.
- [`zed`](/crates/zed) is where all things come together, and the `main` entry point for Glass.
