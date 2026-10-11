//! #835 / #736: Object Manager <-> trigger cross-reference data provider.
//!
//! This is the pure, unit-tested layer behind the Object Manager panel's
//! trigger cross-reference. Given the panel's currently selected entry and the
//! project's trigger graphs, it answers exactly one direction of the
//! cross-reference by delegating to the shared `trigger_cross_ref` pure function
//! over a `McpTriggerRuntimeState` built from those graphs.
//!
//! There is no parallel cross-reference index to drift (#1348): every edge is
//! recomputed from the trigger graphs on demand, so editing a trigger and
//! redrawing the panel changes the answer. The panel draw itself (in
//! `main_editor_object_manager.rs`) stays thin -- it maps the enum below to egui
//! labels and does no computation.

use litd_editor::mcp_trigger_tools::{
    CrossRefObjectKind, CrossRefQuery, EcaGraph, McpTriggerRuntimeState,
};
use litd_editor::trigger_cross_ref::{TriggerObjectRef, answer_cross_ref};

/// The cross-reference answer for the Object Manager's currently selected entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ObjectManagerCrossRef {
    /// The selected entry is a trigger: the concrete objects it references.
    TriggerReferences {
        trigger_id: String,
        objects: Vec<TriggerObjectRef>,
    },
    /// The selected entry is an object: the triggers that reference it.
    ObjectReferencedBy {
        object_kind: CrossRefObjectKind,
        object_id: String,
        trigger_ids: Vec<String>,
    },
    /// The selected entry's kind takes no part in the trigger cross-reference
    /// (e.g. an asset), so there is nothing to show.
    NotCrossReferenceable { kind: String },
    /// The cross-reference computation failed loud (typed error surfaced as text).
    Error(String),
}

/// Map an Object Manager entry `kind` string to the cross-reference object kind
/// it names, if any. Case-insensitive so it tolerates `"Unit"` (the object
/// manager's `EditorObject::kind`) as well as lower-case ids. Kinds with no
/// trigger-referenceable identity (assets, abilities) map to `None`.
pub(super) fn cross_ref_object_kind(kind: &str) -> Option<CrossRefObjectKind> {
    match kind.to_ascii_lowercase().as_str() {
        "unit" => Some(CrossRefObjectKind::Unit),
        "unittype" | "unit_type" | "unit type" => Some(CrossRefObjectKind::UnitType),
        "region" => Some(CrossRefObjectKind::Region),
        "variable" => Some(CrossRefObjectKind::Variable),
        "player" => Some(CrossRefObjectKind::Player),
        "timer" => Some(CrossRefObjectKind::Timer),
        "unitgroup" | "unit_group" | "unit group" => Some(CrossRefObjectKind::UnitGroup),
        _ => None,
    }
}

/// A short ASCII label for a cross-reference object kind, for the panel rows.
pub(super) fn cross_ref_kind_label(kind: CrossRefObjectKind) -> &'static str {
    match kind {
        CrossRefObjectKind::Variable => "variable",
        CrossRefObjectKind::Unit => "unit",
        CrossRefObjectKind::UnitType => "unit-type",
        CrossRefObjectKind::Region => "region",
        CrossRefObjectKind::Player => "player",
        CrossRefObjectKind::Timer => "timer",
        CrossRefObjectKind::UnitGroup => "unit-group",
    }
}

/// Build the in-memory `McpTriggerRuntimeState` the cross-reference reads from.
/// The trigger graphs ARE the source of truth (the same `EcaGraph` values the
/// trigger tools persist), so this is a lossless projection, not a side store.
fn state_from_triggers(triggers: &[EcaGraph]) -> Result<McpTriggerRuntimeState, String> {
    let mut state = McpTriggerRuntimeState::new("object_manager_cross_ref")
        .map_err(|error| error.to_string())?;
    state.graphs = triggers.to_vec();
    Ok(state)
}

/// Answer the Object Manager cross-reference for `selected_id` (of kind
/// `selected_kind`) against the project's trigger `triggers`.
///
/// - A `Trigger` entry yields the objects it references
///   (`ObjectsReferencedByTrigger`).
/// - An object entry whose kind names a cross-referenceable object yields the
///   triggers that reference it (`TriggersReferencingObject`).
/// - Anything else is `NotCrossReferenceable`.
pub(super) fn object_manager_cross_ref(
    triggers: &[EcaGraph],
    selected_id: &str,
    selected_kind: &str,
) -> ObjectManagerCrossRef {
    let state = match state_from_triggers(triggers) {
        Ok(state) => state,
        Err(reason) => return ObjectManagerCrossRef::Error(reason),
    };
    if selected_kind.eq_ignore_ascii_case("trigger") {
        let query = CrossRefQuery::ObjectsReferencedByTrigger {
            trigger_id: selected_id.to_string(),
        };
        return match answer_cross_ref(&state, &query) {
            Ok(report) => ObjectManagerCrossRef::TriggerReferences {
                trigger_id: selected_id.to_string(),
                objects: report.objects,
            },
            Err(error) => ObjectManagerCrossRef::Error(error.to_string()),
        };
    }
    let Some(object_kind) = cross_ref_object_kind(selected_kind) else {
        return ObjectManagerCrossRef::NotCrossReferenceable {
            kind: selected_kind.to_string(),
        };
    };
    let query = CrossRefQuery::TriggersReferencingObject {
        object_kind,
        object_id: selected_id.to_string(),
    };
    match answer_cross_ref(&state, &query) {
        Ok(report) => ObjectManagerCrossRef::ObjectReferencedBy {
            object_kind,
            object_id: selected_id.to_string(),
            trigger_ids: report.trigger_ids,
        },
        Err(error) => ObjectManagerCrossRef::Error(error.to_string()),
    }
}

#[cfg(test)]
#[path = "main_editor_object_manager_crossref_tests.rs"]
mod tests;
