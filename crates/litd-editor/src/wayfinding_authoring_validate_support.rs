//! UI projection, cue-kind sorting and the in-world text/ref validators for wayfinding
//! authoring (#1466 split of `wayfinding_authoring_validate.rs`).
//!
//! Leaf helpers with no cross-validator orchestration: the quest-panel projection, the
//! direction-text "stay in-world" guard, and the single-line / ref-list validators. The
//! request/ledger/chain orchestration and `wayfinding_error` stay in
//! `wayfinding_authoring_validate.rs`; these helpers are `pub(super)` so it calls them unchanged.

use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    MAX_LIST_LEN, MAX_TEXT_LEN, WayfindingChain, WayfindingCueKind, WayfindingObjective,
    WayfindingRouteStep, WayfindingUiProjection, wayfinding_error,
};

pub(super) fn ui_projection(
    objective: &WayfindingObjective,
    chain: &WayfindingChain,
) -> WayfindingUiProjection {
    WayfindingUiProjection {
        quest_panel_title: objective.title.clone(),
        objective_text: objective.description.clone(),
        journal_steps: chain
            .route_steps
            .iter()
            .map(|step| step.direction_text.clone())
            .collect(),
        map_landmark_refs: chain
            .route_steps
            .iter()
            .flat_map(|step| step.landmark_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        map_surface_source_required: true,
        gps_marker_count: 0,
        quest_marker_count: 0,
        player_position_indicator: false,
        route_line_count: 0,
    }
}

pub(super) fn sorted_cue_kinds(chain: &WayfindingChain) -> Vec<WayfindingCueKind> {
    chain
        .route_steps
        .iter()
        .map(|step| step.cue_kind)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn validate_direction_text(
    field: &str,
    value: &str,
    step: &WayfindingRouteStep,
) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    let lower = value.to_ascii_lowercase();
    for denied in [
        "gps",
        "minimap",
        "waypoint",
        "quest marker",
        "player dot",
        "x=",
        "y=",
        "z=",
        "coordinate",
        "coords",
    ] {
        if lower.contains(denied) {
            return Err(wayfinding_error(
                field,
                format!("direction text must stay in-world, rejected {denied}"),
            ));
        }
    }
    let vague = [
        "somewhere nearby",
        "around here",
        "near there",
        "over there",
    ];
    let has_vague = vague.iter().any(|needle| lower.contains(needle));
    let has_concrete_clue = !step.landmark_refs.is_empty()
        || step.signpost_text.is_some()
        || step.dialogue_text.is_some()
        || step.journal_text.is_some()
        || step.environmental_text.is_some();
    if has_vague && !has_concrete_clue {
        return Err(wayfinding_error(
            field,
            "vague direction needs a known place, landmark, sign, rumor, journal, or clue",
        ));
    }
    Ok(())
}

pub(super) fn validate_optional(field: &str, value: &Option<String>) -> Result<(), EngineError> {
    if let Some(text) = value {
        validate_single_line(field, text)?;
    }
    Ok(())
}

pub(super) fn require_some(field: &str, value: &Option<String>) -> Result<(), EngineError> {
    if value.as_deref().unwrap_or("").trim().is_empty() {
        return Err(wayfinding_error(
            field,
            "cue kind requires matching in-world clue text",
        ));
    }
    Ok(())
}

pub(super) fn validate_refs(field: &str, values: &[String]) -> Result<(), EngineError> {
    if values.is_empty() {
        return Err(wayfinding_error(field, "at least one ref is required"));
    }
    validate_refs_allow_empty(field, values)
}

pub(super) fn validate_refs_allow_empty(field: &str, values: &[String]) -> Result<(), EngineError> {
    if values.len() > MAX_LIST_LEN {
        return Err(wayfinding_error(
            field,
            "ref list is larger than the authoring cap",
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_single_line(field, value)?;
        if !seen.insert(value.as_str()) {
            return Err(wayfinding_error(field, format!("duplicate ref {value}")));
        }
    }
    Ok(())
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty()
        || value.len() > MAX_TEXT_LEN
        || value.contains(['\n', '\r', '\t', '\0'])
    {
        return Err(wayfinding_error(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}
