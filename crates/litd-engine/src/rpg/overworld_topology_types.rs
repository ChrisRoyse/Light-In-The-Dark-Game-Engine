use serde::{Deserialize, Serialize};

use super::area_specs_types::AreaLevelBand;

/// Overworld topology graph schema (#1632). The Thera continental layout as
/// authoritative data: regions (nodes) that group authored areas, plus the
/// region-level road/sea/portal spine (edges). Every `member_area_id` must
/// resolve to an authored area in the #1704 registry, so the topology can never
/// drift from the area corpus it summarizes.
pub const RPG_OVERWORLD_TOPOLOGY_SCHEMA: &str = "litd.rpg.overworld_topology.v1";
pub const THERA_WORLD_ID: &str = "world.thera";

/// The compass sector of the continent a region sits in, matching the CF QHCF
/// WorldMap organization (center hub, then the cardinal spokes, the Underdark
/// below, and the portal/plane/island regions beyond the ocean).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldSector {
    Center,
    North,
    East,
    South,
    West,
    Northwest,
    Below,
    Beyond,
}

/// The kind of region-level spine an edge represents. Roads are exposed
/// fast-travel terrain (#1629); seas need a vessel; the Underdark spine is the
/// surface-to-deep descent; portals are The Past / planar links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionSpine {
    Road,
    Sea,
    Portal,
    Underdark,
    Trail,
}

/// A region node: a named area of the continent grouping the authored areas that
/// sit within it, with the level band the ranking guides assign to that stretch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyRegion {
    pub region_id: String,
    pub display_name: String,
    pub sector: WorldSector,
    pub summary: String,
    pub level_band: AreaLevelBand,
    /// Authored `area_id`s that belong to this region. Validated against the
    /// #1704 registry; an area may belong to at most one region (a partition).
    pub member_area_ids: Vec<String>,
}

/// A region-level spine edge: how you cross from one region to another and by
/// what travel mode. Undirected in spirit but authored with a from/to for a
/// stable id; the graph treats them as bidirectional connectivity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionEdge {
    pub edge_id: String,
    pub from_region_id: String,
    pub to_region_id: String,
    pub spine: RegionSpine,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverworldTopologySpec {
    pub schema: String,
    pub world_id: String,
    pub display_name: String,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    pub summary: String,
    pub regions: Vec<TopologyRegion>,
    pub edges: Vec<RegionEdge>,
    pub implementation_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverworldTopologyReadback {
    pub schema: String,
    pub world_id: String,
    pub source_issue: u32,
    pub region_count: usize,
    pub edge_count: usize,
    pub member_area_count: usize,
    pub sector_count: usize,
    pub road_edge_count: usize,
    pub sea_edge_count: usize,
    pub portal_edge_count: usize,
    pub underdark_edge_count: usize,
    pub is_connected: bool,
    pub topology_sha256: String,
}
