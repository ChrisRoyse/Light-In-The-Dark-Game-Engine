//! Reference-scanning, ref-token and error helpers for the editor object manager (#1466
//! split of `object_manager.rs`).
//!
//! Walk objects/triggers to collect cross-references, tokenize comma/semicolon ref lists,
//! tally reference counts, and mint object-manager errors. The public types, the index
//! builder and the query/rename API stay in `object_manager.rs`; these helpers are
//! `pub(super)` so it calls them unchanged.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;

use super::{ObjectManagerEntry, ObjectManagerReference};
use crate::{
    history::EditorObject,
    mcp_trigger_tools::{EcaGraph, EcaNode},
};

pub(super) fn object_refs(
    object: &EditorObject,
    object_ids: &BTreeSet<String>,
    region_ids: &BTreeSet<String>,
    asset_ids: &BTreeSet<String>,
    references: &mut Vec<ObjectManagerReference>,
) {
    if let Some(parent) = &object.parent
        && object_ids.contains(parent)
    {
        references.push(reference(
            &object.id, "Object", parent, "Object", "parent", None,
        ));
    }
    for (key, value) in &object.properties {
        for token in ref_tokens(value) {
            push_ref(PushRefArgs {
                source_id: &object.id,
                source_kind: "Object",
                field: key,
                token: &token,
                object_ids,
                region_ids,
                asset_ids,
                references,
                trigger_id: None,
            });
        }
    }
}

pub(super) fn trigger_refs(
    graph: &EcaGraph,
    object_ids: &BTreeSet<String>,
    region_ids: &BTreeSet<String>,
    asset_ids: &BTreeSet<String>,
    references: &mut Vec<ObjectManagerReference>,
) {
    for node in graph
        .events
        .iter()
        .chain(graph.conditions.iter())
        .chain(graph.actions.iter())
    {
        node_refs(graph, node, object_ids, region_ids, asset_ids, references);
    }
}

pub(super) fn node_refs(
    graph: &EcaGraph,
    node: &EcaNode,
    object_ids: &BTreeSet<String>,
    region_ids: &BTreeSet<String>,
    asset_ids: &BTreeSet<String>,
    references: &mut Vec<ObjectManagerReference>,
) {
    for (key, value) in &node.args {
        for token in ref_tokens(value) {
            let field = format!("{}.{}", node.operation, key);
            push_ref(PushRefArgs {
                source_id: &graph.trigger_id,
                source_kind: "Trigger",
                field: &field,
                token: &token,
                object_ids,
                region_ids,
                asset_ids,
                references,
                trigger_id: Some(graph.trigger_id.clone()),
            });
        }
    }
}

pub(super) struct PushRefArgs<'a> {
    pub(super) source_id: &'a str,
    pub(super) source_kind: &'a str,
    pub(super) field: &'a str,
    pub(super) token: &'a str,
    pub(super) object_ids: &'a BTreeSet<String>,
    pub(super) region_ids: &'a BTreeSet<String>,
    pub(super) asset_ids: &'a BTreeSet<String>,
    pub(super) references: &'a mut Vec<ObjectManagerReference>,
    pub(super) trigger_id: Option<String>,
}

pub(super) fn push_ref(args: PushRefArgs<'_>) {
    let PushRefArgs {
        source_id,
        source_kind,
        field,
        token,
        object_ids,
        region_ids,
        asset_ids,
        references,
        trigger_id,
    } = args;
    if object_ids.contains(token) {
        references.push(reference(
            source_id,
            source_kind,
            token,
            "Object",
            field,
            trigger_id,
        ));
    } else if region_ids.contains(token) {
        references.push(reference(
            source_id,
            source_kind,
            token,
            "Region",
            field,
            trigger_id,
        ));
    } else if asset_ids.contains(token) {
        references.push(reference(
            source_id,
            source_kind,
            token,
            "Asset",
            field,
            trigger_id,
        ));
    }
}

pub(super) fn reference(
    source_id: &str,
    source_kind: &str,
    target_id: &str,
    target_kind: &str,
    field: &str,
    trigger_id: Option<String>,
) -> ObjectManagerReference {
    ObjectManagerReference {
        source_id: source_id.to_string(),
        source_kind: source_kind.to_string(),
        target_id: target_id.to_string(),
        target_kind: target_kind.to_string(),
        field: field.to_string(),
        trigger_id,
    }
}

pub(super) fn entry(id: &str, kind: &str, label: &str, orphan: bool) -> ObjectManagerEntry {
    ObjectManagerEntry {
        id: id.to_string(),
        kind: kind.to_string(),
        label: label.to_string(),
        reference_count: 0,
        orphan,
    }
}

pub(super) fn reference_counts(references: &[ObjectManagerReference]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for reference in references {
        count(&mut counts, &reference.target_id, 1);
    }
    counts
}

pub(super) fn count(counts: &mut BTreeMap<String, usize>, key: &str, amount: usize) {
    *counts.entry(key.to_string()).or_default() += amount;
}

pub(super) fn ref_tokens(value: &str) -> Vec<String> {
    value
        .split([',', ';'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn replace_ref_tokens(value: &str, old_id: &str, new_id: &str) -> String {
    ref_tokens(value)
        .into_iter()
        .map(|token| {
            if token == old_id {
                new_id.to_string()
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn object_manager_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(object_manager_error_value(field, reason))
}

pub(super) fn object_manager_error_value(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.object_manager.{}", field.into()),
        reason: reason.into(),
    }
}
