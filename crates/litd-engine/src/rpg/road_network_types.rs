use serde::{Deserialize, Serialize};

/// Road-network spine schema (#1633). CF roads are first-class areas; this spec
/// captures the named road system as data over the area graph: ordered waypoint
/// areas, gate chokepoints (city guards / Tribunal law zones), crossroad
/// landmarks, and the travel-exposure tag each road carries (#1629 doctrine:
/// roads are fast exposed terrain with weak concealment). Every referenced area
/// id is validated against the #1704 registry.
pub const RPG_ROAD_NETWORK_SCHEMA: &str = "litd.rpg.road_network.v1";

/// Travel-exposure classification of a road (#1629). `Exposed` roads are fast but
/// offer little concealment; `Sheltered` roads (forest tracks, sunken lanes) give
/// partial cover; `Chokepoint` roads are dominated by a gate/pass where traffic is
/// funneled and watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadExposure {
    Exposed,
    Sheltered,
    Chokepoint,
}

/// The physical character of a road, matching the CF vocabulary (open road, a
/// mountain pass, a processional holy road, a concealed trail, a sea lane).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadKind {
    Road,
    Pass,
    Trail,
    Path,
    HolyRoad,
    SeaLane,
}

/// A gate chokepoint on a road: where city guards or a Tribunal law zone control
/// passage (#1633). `at_area_id` must be an authored area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoadGate {
    pub gate_id: String,
    pub at_area_id: String,
    pub is_law_zone: bool,
    pub is_guarded: bool,
    pub summary: String,
}

/// A named road: an ordered spine of waypoint areas plus its gates and exposure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedRoad {
    pub road_id: String,
    pub display_name: String,
    pub road_kind: RoadKind,
    pub exposure: RoadExposure,
    pub summary: String,
    /// Ordered areas the road threads through. Every id is validated against the
    /// #1704 registry; consecutive pairs are the road's segments.
    pub waypoint_area_ids: Vec<String>,
    pub gates: Vec<RoadGate>,
}

/// A crossroad landmark where named roads meet (#1633): a meeting point players
/// use to navigate. `at_area_id` is the area the junction sits in; `road_ids`
/// name the roads that converge there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crossroad {
    pub crossroad_id: String,
    pub display_name: String,
    pub at_area_id: String,
    pub road_ids: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoadNetworkSpec {
    pub schema: String,
    pub world_id: String,
    pub display_name: String,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    pub summary: String,
    pub roads: Vec<NamedRoad>,
    pub crossroads: Vec<Crossroad>,
    pub implementation_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoadNetworkReadback {
    pub schema: String,
    pub world_id: String,
    pub source_issue: u32,
    pub road_count: usize,
    pub segment_count: usize,
    pub waypoint_count: usize,
    pub gate_count: usize,
    pub law_zone_gate_count: usize,
    pub crossroad_count: usize,
    pub exposed_road_count: usize,
    pub chokepoint_road_count: usize,
    pub road_network_sha256: String,
}
