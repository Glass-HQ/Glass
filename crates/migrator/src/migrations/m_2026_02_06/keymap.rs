use collections::HashMap;
use std::{ops::Range, sync::LazyLock};
use tree_sitter::{Query, QueryMatch};

use crate::MigrationPatterns;
use crate::patterns::{KEYMAP_ACTION_STRING_PATTERN, KEYMAP_CONTEXT_PATTERN};

pub const KEYMAP_PATTERNS: MigrationPatterns = &[
    (KEYMAP_ACTION_STRING_PATTERN, replace_string_action),
    (KEYMAP_CONTEXT_PATTERN, rename_context_key),
];

fn replace_string_action(
    contents: &str,
    mat: &QueryMatch,
    query: &Query,
) -> Option<(Range<usize>, String)> {
    let action_name_ix = query.capture_index_for_name("action_name")?;
    let action_name_node = mat.nodes_for_capture_index(action_name_ix).next()?;
    let action_name_range = action_name_node.byte_range();
    let action_name = contents.get(action_name_range.clone())?;

    STRING_REPLACE
        .get(action_name)
        .map(|new_action_name| (action_name_range, (*new_action_name).to_string()))
}

fn rename_context_key(
    contents: &str,
    mat: &QueryMatch,
    query: &Query,
) -> Option<(Range<usize>, String)> {
    let context_predicate_ix = query.capture_index_for_name("context_predicate")?;
    let context_predicate_range = mat
        .nodes_for_capture_index(context_predicate_ix)
        .next()?
        .byte_range();
    let old_predicate = contents.get(context_predicate_range.clone())?;
    let new_predicate = old_predicate.replace("OutlinePanel", "ProjectPanel");

    if new_predicate == old_predicate {
        None
    } else {
        Some((context_predicate_range, new_predicate))
    }
}

static STRING_REPLACE: LazyLock<HashMap<&str, &str>> = LazyLock::new(|| {
    HashMap::from_iter([
        ("outline::Toggle", "project_symbols::Toggle"),
        ("outline_panel::ToggleFocus", "project_panel::ToggleFocus"),
        ("outline_panel::Open", "project_panel::Open"),
        ("outline_panel::OpenSelectedEntry", "project_panel::Open"),
        (
            "outline_panel::CollapseSelectedEntry",
            "project_panel::CollapseSelectedEntry",
        ),
        (
            "outline_panel::ExpandSelectedEntry",
            "project_panel::ExpandSelectedEntry",
        ),
        (
            "outline_panel::CollapseAllEntries",
            "project_panel::CollapseAllEntries",
        ),
        (
            "outline_panel::ExpandAllEntries",
            "project_panel::ExpandSelectedEntry",
        ),
        (
            "outline_panel::RevealInFileManager",
            "project_panel::RevealInFileManager",
        ),
        ("outline_panel::FoldDirectory", "project_panel::FoldDirectory"),
        ("outline_panel::UnfoldDirectory", "project_panel::UnfoldDirectory"),
        ("outline_panel::ScrollUp", "project_panel::ScrollUp"),
        ("outline_panel::ScrollDown", "project_panel::ScrollDown"),
        (
            "outline_panel::ScrollCursorCenter",
            "project_panel::ScrollCursorCenter",
        ),
        (
            "outline_panel::ScrollCursorTop",
            "project_panel::ScrollCursorTop",
        ),
        (
            "outline_panel::ScrollCursorBottom",
            "project_panel::ScrollCursorBottom",
        ),
        ("outline_panel::SelectParent", "project_panel::SelectParent"),
        (
            "outline_panel::ToggleActiveEditorPin",
            "project_panel::ToggleFocus",
        ),
        ("outline_panel::CopyPath", "workspace::CopyPath"),
        (
            "outline_panel::CopyRelativePath",
            "workspace::CopyRelativePath",
        ),
    ])
});
