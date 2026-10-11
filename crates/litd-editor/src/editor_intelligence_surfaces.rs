use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    history::stable_fingerprint,
    mcp_tools::{
        StructuredIntelligenceMcpSurfaceManifest,
        validate_structured_intelligence_mcp_surface_manifest,
    },
};

#[path = "editor_intelligence_surfaces_store.rs"]
mod editor_intelligence_surfaces_store;
#[path = "editor_intelligence_surfaces_support.rs"]
mod editor_intelligence_surfaces_support;

pub use editor_intelligence_surfaces_store::{
    EditorIntelligenceSurfaceWriteback, parse_editor_intelligence_surface_deck,
    read_editor_intelligence_surface_deck, write_editor_intelligence_surface_deck,
};
use editor_intelligence_surfaces_support::{
    build_panel, invalid, normalize_observations, surface_specs, validate_observation,
};

pub const EDITOR_INTELLIGENCE_SURFACES_SCHEMA: &str = "litd.editor_intelligence_surfaces.deck.v1";
pub const EDITOR_INTELLIGENCE_SURFACES_ISSUE: u32 = 1420;
pub const EDITOR_INTELLIGENCE_SURFACES_PARENT_ISSUE: u32 = 1350;
pub const EDITOR_INTELLIGENCE_SURFACES_SOURCE_ISSUE: u32 = 1419;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorIntelligenceSurfaceKind {
    Abundance,
    Bits,
    AssociationGraph,
    Kernel,
    WhatIf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorIntelligenceDock {
    RightInspector,
    BottomPanel,
    CenterOverlay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorIntelligenceObservation {
    pub tool_name: String,
    pub source_ref: String,
    pub payload: Value,
}

impl EditorIntelligenceObservation {
    pub fn new(
        tool_name: impl Into<String>,
        source_ref: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            source_ref: source_ref.into(),
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorIntelligenceMetric {
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorIntelligenceSurfacePanel {
    pub kind: EditorIntelligenceSurfaceKind,
    pub panel_id: String,
    pub title: String,
    pub dock: EditorIntelligenceDock,
    pub order: u8,
    pub min_width_px: u16,
    pub min_height_px: u16,
    pub source_tools: Vec<String>,
    pub readback_refs: Vec<String>,
    pub metrics: Vec<EditorIntelligenceMetric>,
    pub focus_cx_ids: Vec<String>,
    pub warnings: Vec<String>,
    pub payload_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorIntelligenceSurfaceDeck {
    pub schema: String,
    pub issue: u32,
    pub parent_issue: u32,
    pub source_issue: u32,
    pub deck_id: String,
    pub vault_name: String,
    pub source_of_truth: String,
    pub surface_count: usize,
    pub observation_count: usize,
    pub manifest_families: Vec<String>,
    pub manifest_fingerprint: String,
    pub observation_fingerprint: String,
    pub surfaces: Vec<EditorIntelligenceSurfacePanel>,
    pub observations: Vec<EditorIntelligenceObservation>,
}

pub fn build_editor_intelligence_surface_deck(
    deck_id: impl Into<String>,
    vault_name: impl Into<String>,
    source_of_truth: impl Into<String>,
    manifest: &StructuredIntelligenceMcpSurfaceManifest,
    observations: Vec<EditorIntelligenceObservation>,
) -> Result<EditorIntelligenceSurfaceDeck, EngineError> {
    validate_structured_intelligence_mcp_surface_manifest(manifest)?;
    let deck_id = deck_id.into();
    let vault_name = vault_name.into();
    let source_of_truth = source_of_truth.into();
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(&deck_id)
        || vault_name.trim().is_empty()
        || source_of_truth.trim().is_empty()
    {
        return invalid(
            "binding",
            "deck id, vault name, and source of truth are required",
        );
    }

    let advertised = manifest
        .families
        .iter()
        .flat_map(|family| family.required_tools.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let observations = normalize_observations(observations, &advertised)?;
    let by_tool = observations
        .iter()
        .map(|observation| (observation.tool_name.as_str(), observation))
        .collect::<BTreeMap<_, _>>();
    let surfaces = surface_specs()
        .into_iter()
        .map(|spec| build_panel(spec, &by_tool))
        .collect::<Result<Vec<_>, _>>()?;
    let deck = EditorIntelligenceSurfaceDeck {
        schema: EDITOR_INTELLIGENCE_SURFACES_SCHEMA.to_string(),
        issue: EDITOR_INTELLIGENCE_SURFACES_ISSUE,
        parent_issue: EDITOR_INTELLIGENCE_SURFACES_PARENT_ISSUE,
        source_issue: EDITOR_INTELLIGENCE_SURFACES_SOURCE_ISSUE,
        deck_id,
        vault_name,
        source_of_truth,
        surface_count: surfaces.len(),
        observation_count: observations.len(),
        manifest_families: manifest
            .families
            .iter()
            .map(|family| family.family.clone())
            .collect(),
        manifest_fingerprint: stable_fingerprint(manifest)?,
        observation_fingerprint: stable_fingerprint(&observations)?,
        surfaces,
        observations,
    };
    validate_editor_intelligence_surface_deck(&deck)?;
    Ok(deck)
}

pub fn validate_editor_intelligence_surface_deck(
    deck: &EditorIntelligenceSurfaceDeck,
) -> Result<(), EngineError> {
    if deck.schema != EDITOR_INTELLIGENCE_SURFACES_SCHEMA
        || deck.issue != EDITOR_INTELLIGENCE_SURFACES_ISSUE
        || deck.parent_issue != EDITOR_INTELLIGENCE_SURFACES_PARENT_ISSUE
        || deck.source_issue != EDITOR_INTELLIGENCE_SURFACES_SOURCE_ISSUE
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&deck.deck_id)
        || deck.vault_name.trim().is_empty()
        || deck.source_of_truth.trim().is_empty()
        || deck.surface_count != deck.surfaces.len()
        || deck.observation_count != deck.observations.len()
    {
        return invalid("deck", "editor intelligence deck binding drifted");
    }
    if deck.surface_count != surface_specs().len() {
        return invalid(
            "deck.surfaces",
            "all required intelligence panels must be present",
        );
    }
    let mut kinds = BTreeSet::new();
    let mut panels = BTreeSet::new();
    for (idx, panel) in deck.surfaces.iter().enumerate() {
        if !kinds.insert(panel.kind)
            || !panels.insert(panel.panel_id.as_str())
            || panel.order as usize != idx + 1
            || panel.source_tools.is_empty()
            || panel.readback_refs.is_empty()
            || panel.metrics.is_empty()
            || panel.payload_fingerprint.trim().is_empty()
        {
            return invalid(
                "panel",
                "panel ids, order, source tools, and metrics are required",
            );
        }
    }
    for observation in &deck.observations {
        validate_observation(observation)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "editor_intelligence_surfaces_tests.rs"]
mod tests;
