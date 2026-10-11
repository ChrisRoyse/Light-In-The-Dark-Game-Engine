use serde::{Deserialize, Serialize};

use crate::render::ui::{MapOverlayPolicy, MapOverlayPolicyReadback, MinimapMarker};

pub const RPG_MAP_ACCESS_SCHEMA: &str = "litd.rpg.map_access.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpgMapSourceKind {
    CartographerPurchase,
    FoundOrLooted,
    QuestReward,
    ArchiveCopy,
    ExplorationKnowledge,
    NpcKnowledge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpgMapSourceStatus {
    Available,
    Removed,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpgMapFidelity {
    Complete,
    Incomplete,
    Stale,
    Inaccurate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpgMapFeatureKind {
    Landmark,
    Danger,
    Resource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpgMapRuntimeState {
    pub schema: String,
    pub player_id: String,
    pub sources: Vec<RpgMapSource>,
}

impl RpgMapRuntimeState {
    pub fn fresh(player_id: &str) -> Self {
        Self {
            schema: RPG_MAP_ACCESS_SCHEMA.to_string(),
            player_id: player_id.to_string(),
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpgMapSource {
    pub source_id: String,
    pub kind: RpgMapSourceKind,
    pub status: RpgMapSourceStatus,
    pub scope_id: String,
    pub display_name: String,
    pub acquired_from: String,
    pub quality_tier: u8,
    pub fidelity: RpgMapFidelity,
    pub features: Vec<RpgMapFeature>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpgMapFeature {
    pub feature_id: String,
    pub kind: RpgMapFeatureKind,
    pub label: String,
    pub world_position: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpgMapOpenRequest {
    pub requested_source_id: Option<String>,
}

impl RpgMapOpenRequest {
    pub fn no_map_screen() -> Self {
        Self {
            requested_source_id: None,
        }
    }

    pub fn open_source(source_id: &str) -> Self {
        Self {
            requested_source_id: Some(source_id.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpgMapAccessState {
    NoMapAccess,
    DiegeticMapVisible,
    SourceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpgMapAccessReadback {
    pub player_id: String,
    pub access: RpgMapAccessState,
    pub requested_source_id: Option<String>,
    pub active_source_id: Option<String>,
    pub scope_id: Option<String>,
    pub source_kind: Option<RpgMapSourceKind>,
    pub source_status: Option<RpgMapSourceStatus>,
    pub fidelity: Option<RpgMapFidelity>,
    pub map_surface_visible: bool,
    pub degraded_reason: Option<String>,
    pub gps_indicators_disabled: bool,
    pub source_count: usize,
    pub available_source_count: usize,
    pub policy: MapOverlayPolicyReadback,
    pub state_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpgMapRenderPlan {
    pub overlay_policy: MapOverlayPolicy,
    pub markers: Vec<MinimapMarker>,
    pub readback: RpgMapAccessReadback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpgMapRuntimeStateReadback {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
    pub state: RpgMapRuntimeState,
}
