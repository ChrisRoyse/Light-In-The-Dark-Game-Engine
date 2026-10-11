use serde::{Deserialize, Serialize};

use crate::world_graph_navigation::{WorldGraphTraversalReport, WorldKnowledgeGraph};

pub const WAYFINDING_AUTHORING_SCHEMA: &str = "litd.rpg.wayfinding_authoring.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WayfindingMarkerPolicy {
    HiddenNoAutomaticMarkers,
    AssistedOptIn,
    EditorOnly,
}

impl WayfindingMarkerPolicy {
    pub fn requires_diegetic_chain(self) -> bool {
        matches!(self, Self::HiddenNoAutomaticMarkers)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WayfindingCueKind {
    RoadCue,
    Signpost,
    Landmark,
    NpcRumor,
    JournalClue,
    MapLandmark,
    EnvironmentalSilhouette,
    DangerNote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WayfindingValidationStatus {
    Validated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WayfindingObjective {
    pub quest_id: String,
    pub objective_id: String,
    pub title: String,
    pub description: String,
    pub target_ref: String,
    pub marker_policy: WayfindingMarkerPolicy,
    pub start_anchor_id: Option<String>,
    pub destination_anchor_id: Option<String>,
    pub chain_id: Option<String>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WayfindingRouteStep {
    pub step_id: String,
    pub from_anchor_id: String,
    pub to_anchor_id: String,
    pub cue_kind: WayfindingCueKind,
    pub direction_text: String,
    pub landmark_refs: Vec<String>,
    pub signpost_text: Option<String>,
    pub dialogue_text: Option<String>,
    pub journal_text: Option<String>,
    pub environmental_text: Option<String>,
    pub reliability_milli: u16,
    pub ambiguity_note: Option<String>,
    pub danger_notes: Vec<String>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WayfindingChain {
    pub chain_id: String,
    pub quest_id: String,
    pub objective_id: String,
    pub start_anchor_id: String,
    pub destination_anchor_id: String,
    pub route_steps: Vec<WayfindingRouteStep>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WayfindingAuthoringRequest {
    pub request_id: String,
    pub requested_by: String,
    pub objective: WayfindingObjective,
    pub chain: Option<WayfindingChain>,
    pub graph: WorldKnowledgeGraph,
    pub max_hops: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WayfindingUiProjection {
    pub quest_panel_title: String,
    pub objective_text: String,
    pub journal_steps: Vec<String>,
    pub map_landmark_refs: Vec<String>,
    pub map_surface_source_required: bool,
    pub gps_marker_count: usize,
    pub quest_marker_count: usize,
    pub player_position_indicator: bool,
    pub route_line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WayfindingValidationReport {
    pub schema: String,
    pub request_id: String,
    pub quest_id: String,
    pub objective_id: String,
    pub status: WayfindingValidationStatus,
    pub code: String,
    pub reasons: Vec<String>,
    pub route_anchor_path: Vec<String>,
    pub cue_kinds: Vec<WayfindingCueKind>,
    pub traversal: WorldGraphTraversalReport,
    pub ui_projection: WayfindingUiProjection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WayfindingAuthoringLedger {
    pub schema: String,
    pub ledger_id: String,
    pub graph: WorldKnowledgeGraph,
    pub report_count: usize,
    pub reports: Vec<WayfindingValidationReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WayfindingAuthoringReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: WayfindingAuthoringLedger,
}
