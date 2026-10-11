use serde::{Deserialize, Serialize};

/// Generic area/city content schema (#987). Generalized from the infernal-only
/// `litd.rpg.infernal_zones.v1` zone emitter so any City/Town/Wilderness/Dungeon/
/// Lair/CabalHall/Road/Region can be authored as editor content and lowered to the
/// hot path. Topology is captured as discoverable connections (respecting the
/// acquired-map doctrine #944), never as an omniscient built-in minimap.
pub const RPG_AREA_SPEC_SCHEMA: &str = "litd.rpg.area_spec.v1";
pub const GALADON_AREA_ID: &str = "area.thera.galadon";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaKind {
    City,
    Town,
    Wilderness,
    Dungeon,
    Lair,
    CabalHall,
    Road,
    Region,
}

/// A room/sub-location inside an area. Rooms are the discrete places a player can
/// occupy; connections wire them together (and to other areas) as topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaRoom {
    pub room_id: String,
    pub display_name: String,
    pub room_kind: RoomKind,
    pub summary: String,
    pub concealment: RoomConcealment,
    pub npc_population_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomKind {
    Gate,
    Intersection,
    Street,
    Market,
    Guildhall,
    Temple,
    Inn,
    Shop,
    Academy,
    Sewer,
    Wilderness,
    Chamber,
    Landmark,
    Tribunal,
    Cartographer,
}

/// Stealth surface per #1629. `None` is exposed ground; the others are the
/// concealment affordances the design calls out (alley shadow volumes, crowd
/// cover, hidden sewer layer, gate chokepoints).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomConcealment {
    None,
    AlleyShadow,
    CrowdCover,
    HiddenLayer,
    GateChokepoint,
}

/// A directed connection out of a room. `to_area_id` may reference this area
/// (internal move) or another area (a world-graph edge, #1632). `discovery`
/// encodes the acquired-map doctrine (#944): a Hidden/ConcealedTrail connection
/// exists in topology but is not revealed for free on any minimap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaConnection {
    pub connection_id: String,
    pub from_room_id: String,
    pub to_area_id: String,
    pub to_room_id: Option<String>,
    pub direction: ConnectionDirection,
    pub travel_kind: TravelKind,
    pub discovery: ConnectionDiscovery,
    pub locked_by_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionDirection {
    North,
    South,
    East,
    West,
    Northeast,
    Northwest,
    Southeast,
    Southwest,
    Up,
    Down,
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelKind {
    Gate,
    Road,
    Path,
    Stair,
    Descend,
    Portal,
    ConcealedTrail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionDiscovery {
    /// Plainly visible from the room (open gate, main road).
    Visible,
    /// Present but must be found (hidden door, sewer grate).
    Hidden,
    /// A concealed off-road trail (e.g. Goblin Village approach).
    ConcealedTrail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaLevelBand {
    pub min_level: u8,
    pub max_level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaResetProfile {
    pub reset_id: String,
    pub interval_minutes: u16,
    pub clears_unlooted_corpses: bool,
    pub resets_doors_and_locks: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaRepopProfile {
    pub repop_id: String,
    pub baseline_population_budget: u16,
    pub repop_interval_minutes: u16,
    pub max_pressure_tier: u8,
}

/// Recall/hometown wiring per #1634. `recall_room_id`, when set, must name a room
/// in this area (the temple/altar recall point). `hometown_access` gates who may
/// choose this area as a hometown (e.g. evil races cannot start in Galadon).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaRecallProfile {
    pub is_hometown_option: bool,
    pub recall_room_id: Option<String>,
    pub has_academy: bool,
    pub hometown_access: AreaAccessRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaAccessRule {
    Any,
    Restricted { values: Vec<String> },
}

/// Visual contract for area geometry, mirroring `RaceVisualContract` but for
/// places: the tileset/model that renders the area, the props it must ship, and
/// a silhouette/material note. Renderer is pinned to Rust/wgpu (#782).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaVisualContract {
    pub renderer: String,
    pub tileset_id: String,
    pub model_id: String,
    pub required_props: Vec<String>,
    pub silhouette: String,
    pub material_notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaSpec {
    pub schema: String,
    pub area_id: String,
    pub display_name: String,
    pub player_facing_canon_name: String,
    pub area_kind: AreaKind,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    pub source_history_label: String,
    pub summary: String,
    pub level_band: AreaLevelBand,
    pub rooms: Vec<AreaRoom>,
    pub connections: Vec<AreaConnection>,
    pub resets: AreaResetProfile,
    pub repop: AreaRepopProfile,
    pub recall: AreaRecallProfile,
    pub visual: AreaVisualContract,
    /// Region-variant / era link (#1725). When set, this area is a variant --
    /// the same geography in a different era or state -- of the base area named
    /// here (e.g. the Past-era Maethien is a variant of the present-day Ruins of
    /// Maethien). The base area MUST be authored: the #1704 corpus registry
    /// resolves this reference and fails loud on a dangling `variant_of_area_id`.
    /// `None` for a base/standalone area.
    pub variant_of_area_id: Option<String>,
    /// Human-facing label for the variant relationship (e.g. "past",
    /// "flashback"). Required iff `variant_of_area_id` is set; a linked variant
    /// must say what kind of variant it is.
    pub variant_label: Option<String>,
    pub implementation_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaSpecReadback {
    pub schema: String,
    pub area_id: String,
    pub display_name: String,
    pub player_facing_canon_name: String,
    pub area_kind: AreaKind,
    pub source_issue: u32,
    pub level_band_min: u8,
    pub level_band_max: u8,
    pub room_count: usize,
    pub connection_count: usize,
    pub gate_count: usize,
    pub guildhall_count: usize,
    pub external_connection_count: usize,
    pub hidden_connection_count: usize,
    pub has_sewers: bool,
    pub is_hometown_option: bool,
    pub has_academy: bool,
    pub has_recall_point: bool,
    pub visual_renderer: String,
    pub prop_count: usize,
    /// #1725 region-variant link, echoed for FSV readback: `Some(base_area_id)`
    /// when this area is an era/state variant of another, else `None`.
    pub variant_of_area_id: Option<String>,
    pub variant_label: Option<String>,
    pub area_spec_sha256: String,
}
