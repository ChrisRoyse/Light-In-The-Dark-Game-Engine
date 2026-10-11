//! #812: deterministic region enter/leave events.
//!
//! Named region rects live on the `EditorDocument` (`EditorRegion`, an axis-aligned
//! box: `center` +/- `half_extents`). This module turns a single unit movement
//! (a `previous` -> `next` world position) into an ordered list of enter/leave
//! events the trigger system can consume as deterministic simulation events.
//!
//! Determinism contract (the reason this is a pure function, not a live probe):
//! membership is recomputed from the CURRENT region geometry on every call (so a
//! moved/resized region is always honored -- unlike WarCraft 3's init-time region
//! registration, which silently stops firing after `Region - Move`), and the event
//! order is fixed: all LEAVE events first, then all ENTER events, each group sorted
//! by `region_id`. Overlapping regions are first-class: a point inside N regions is
//! a member of all N, and a move that changes membership of several overlapping
//! regions at once emits one event per region, in the fixed order. Given the same
//! document and the same two points the event list is byte-identical every run.

use std::collections::BTreeSet;

use litd_engine::{EngineError, math::Vec3};
use serde::{Deserialize, Serialize};

use crate::history::{EditorDocument, EditorRegion};

/// One deterministic region-membership transition for a single movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionTransitionKind {
    Leave,
    Enter,
}

/// A consumable sim event: a named region was entered or left by the movement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionTransitionEvent {
    pub region_id: String,
    pub region_name: String,
    pub kind: RegionTransitionKind,
}

/// Full, self-describing readback of a region-transition evaluation (FSV surface).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionTransitionReport {
    pub previous: Vec3,
    pub next: Vec3,
    pub previous_region_ids: Vec<String>,
    pub next_region_ids: Vec<String>,
    pub events: Vec<RegionTransitionEvent>,
}

/// Inclusive axis-aligned containment test: a point on the boundary counts as
/// inside so a start/goal exactly on a region edge is deterministic (no epsilon
/// jitter deciding membership).
pub fn point_in_region(region: &EditorRegion, point: Vec3) -> bool {
    let delta = point - region.center;
    delta.x.abs() <= region.half_extents.x
        && delta.y.abs() <= region.half_extents.y
        && delta.z.abs() <= region.half_extents.z
}

/// Deterministically ordered set of region ids that contain `point`.
pub fn regions_containing(document: &EditorDocument, point: Vec3) -> BTreeSet<String> {
    document
        .regions
        .values()
        .filter(|region| point_in_region(region, point))
        .map(|region| region.id.clone())
        .collect()
}

/// Compute the deterministic enter/leave events for a `previous` -> `next` move.
///
/// Fails loud if the document is invalid or either endpoint is non-finite -- a
/// NaN coordinate must never silently produce an empty (or arbitrary) event list.
pub fn evaluate_region_transitions(
    document: &EditorDocument,
    previous: Vec3,
    next: Vec3,
) -> Result<RegionTransitionReport, EngineError> {
    document.validate()?;
    if !previous.is_finite() || !next.is_finite() {
        return Err(invalid_region_transition(
            "endpoints",
            "previous and next positions must be finite",
        ));
    }
    let before = regions_containing(document, previous);
    let after = regions_containing(document, next);

    let mut events = Vec::new();
    // Leaves first (member before, not after), then enters (member after, not
    // before) -- each already sorted because `before`/`after` are BTreeSets.
    for region_id in before.difference(&after) {
        events.push(transition_event(
            document,
            region_id,
            RegionTransitionKind::Leave,
        )?);
    }
    for region_id in after.difference(&before) {
        events.push(transition_event(
            document,
            region_id,
            RegionTransitionKind::Enter,
        )?);
    }

    Ok(RegionTransitionReport {
        previous,
        next,
        previous_region_ids: before.into_iter().collect(),
        next_region_ids: after.into_iter().collect(),
        events,
    })
}

fn transition_event(
    document: &EditorDocument,
    region_id: &str,
    kind: RegionTransitionKind,
) -> Result<RegionTransitionEvent, EngineError> {
    let region = document.regions.get(region_id).ok_or_else(|| {
        invalid_region_transition("region", format!("missing region {region_id}"))
    })?;
    Ok(RegionTransitionEvent {
        region_id: region.id.clone(),
        region_name: region.name.clone(),
        kind,
    })
}

fn invalid_region_transition(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.region_transitions.{field}"),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "region_transitions_tests.rs"]
mod tests;
