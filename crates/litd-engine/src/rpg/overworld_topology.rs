use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::area_registry::AreaRegistry;
use super::area_specs_types::AreaLevelBand;
use super::overworld_topology_types::*;
use super::serialize_sha256;

pub fn overworld_topology_readback(
    spec: &OverworldTopologySpec,
) -> Result<OverworldTopologyReadback, EngineError> {
    validate_overworld_topology(spec)?;
    let member_area_count = spec.regions.iter().map(|r| r.member_area_ids.len()).sum();
    let sector_count = spec
        .regions
        .iter()
        .map(|r| r.sector)
        .collect::<BTreeSet<_>>()
        .len();
    let road_edge_count = count_spine(spec, RegionSpine::Road);
    let sea_edge_count = count_spine(spec, RegionSpine::Sea);
    let portal_edge_count = count_spine(spec, RegionSpine::Portal);
    let underdark_edge_count = count_spine(spec, RegionSpine::Underdark);
    Ok(OverworldTopologyReadback {
        schema: spec.schema.clone(),
        world_id: spec.world_id.clone(),
        source_issue: spec.source_issue,
        region_count: spec.regions.len(),
        edge_count: spec.edges.len(),
        member_area_count,
        sector_count,
        road_edge_count,
        sea_edge_count,
        portal_edge_count,
        underdark_edge_count,
        is_connected: topology_is_connected(spec),
        topology_sha256: serialize_sha256(spec)?,
    })
}

fn count_spine(spec: &OverworldTopologySpec, spine: RegionSpine) -> usize {
    spec.edges.iter().filter(|e| e.spine == spine).count()
}

pub fn validate_overworld_topology(spec: &OverworldTopologySpec) -> Result<(), EngineError> {
    if spec.schema != RPG_OVERWORLD_TOPOLOGY_SCHEMA {
        return invalid(
            "schema",
            format!("expected {RPG_OVERWORLD_TOPOLOGY_SCHEMA}"),
        );
    }
    validate_token("world_id", &spec.world_id)?;
    validate_text("display_name", &spec.display_name)?;
    if spec.source_issue == 0 {
        return invalid("source_issue", "must be non-zero".to_string());
    }
    if spec.canon_issues.is_empty() || !spec.canon_issues.contains(&spec.source_issue) {
        return invalid("canon_issues", "must include the source issue".to_string());
    }
    if !spec.mechanics_source_url.starts_with("http") {
        return invalid(
            "mechanics_source_url",
            "must cite the factual layout source".to_string(),
        );
    }
    validate_text("summary", &spec.summary)?;
    if spec.regions.is_empty() {
        return invalid("regions", "at least one region is required".to_string());
    }

    // Region ids unique; membership is a partition (no area in two regions).
    let mut region_ids = BTreeSet::new();
    let mut seen_areas = BTreeSet::new();
    for region in &spec.regions {
        validate_token("regions.region_id", &region.region_id)?;
        validate_text("regions.display_name", &region.display_name)?;
        validate_text("regions.summary", &region.summary)?;
        validate_level_band(&region.level_band)?;
        if !region_ids.insert(region.region_id.clone()) {
            return invalid(
                "regions.region_id",
                format!("duplicate region {}", region.region_id),
            );
        }
        if region.member_area_ids.is_empty() {
            return invalid(
                "regions.member_area_ids",
                format!("region {} has no member areas", region.region_id),
            );
        }
        for area_id in &region.member_area_ids {
            validate_token("regions.member_area_ids", area_id)?;
            if !seen_areas.insert(area_id.clone()) {
                return invalid(
                    "regions.member_area_ids",
                    format!("area {area_id} belongs to more than one region"),
                );
            }
        }
    }

    // Edges reference existing regions, unique ids, no self-loops.
    let mut edge_ids = BTreeSet::new();
    for edge in &spec.edges {
        validate_token("edges.edge_id", &edge.edge_id)?;
        validate_text("edges.summary", &edge.summary)?;
        if !edge_ids.insert(edge.edge_id.clone()) {
            return invalid("edges.edge_id", format!("duplicate edge {}", edge.edge_id));
        }
        if edge.from_region_id == edge.to_region_id {
            return invalid(
                "edges.from_region_id",
                format!("edge {} is a self-loop", edge.edge_id),
            );
        }
        for (field, region) in [
            ("edges.from_region_id", &edge.from_region_id),
            ("edges.to_region_id", &edge.to_region_id),
        ] {
            if !region_ids.contains(region) {
                return invalid(
                    field,
                    format!("edge {} references unknown region {region}", edge.edge_id),
                );
            }
        }
    }

    if spec.implementation_notes.is_empty() {
        return invalid(
            "implementation_notes",
            "must include source notes".to_string(),
        );
    }
    Ok(())
}

/// Cross-area referential integrity against the authoritative #1704 registry:
/// every `member_area_id` must resolve to an authored area. Collects EVERY
/// dangling membership and fails loud listing all of them.
pub fn validate_topology_against_registry(
    spec: &OverworldTopologySpec,
    registry: &AreaRegistry,
) -> Result<(), EngineError> {
    validate_overworld_topology(spec)?;
    let mut dangling: Vec<String> = Vec::new();
    for region in &spec.regions {
        for area_id in &region.member_area_ids {
            if !registry.contains(area_id) {
                dangling.push(format!("{} in region {}", area_id, region.region_id));
            }
        }
    }
    if !dangling.is_empty() {
        return invalid(
            "regions.member_area_ids",
            format!(
                "{} region member(s) reference unknown areas: {}",
                dangling.len(),
                dangling.join("; ")
            ),
        );
    }
    Ok(())
}

/// Is the region graph one connected component over its edges? A disconnected
/// overworld would mean an unreachable region; the FSV asserts connectivity.
fn topology_is_connected(spec: &OverworldTopologySpec) -> bool {
    if spec.regions.len() <= 1 {
        return true;
    }
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for region in &spec.regions {
        adjacency.entry(region.region_id.as_str()).or_default();
    }
    for edge in &spec.edges {
        adjacency
            .entry(edge.from_region_id.as_str())
            .or_default()
            .push(edge.to_region_id.as_str());
        adjacency
            .entry(edge.to_region_id.as_str())
            .or_default()
            .push(edge.from_region_id.as_str());
    }
    let start = spec.regions[0].region_id.as_str();
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        if !visited.insert(node) {
            continue;
        }
        if let Some(neighbors) = adjacency.get(node) {
            for &n in neighbors {
                if !visited.contains(n) {
                    stack.push(n);
                }
            }
        }
    }
    visited.len() == spec.regions.len()
}

fn validate_level_band(band: &AreaLevelBand) -> Result<(), EngineError> {
    if band.min_level == 0 || band.max_level == 0 {
        return invalid("level_band", "levels must be >= 1".to_string());
    }
    if band.min_level > band.max_level {
        return invalid("level_band", "min_level must be <= max_level".to_string());
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes".to_string());
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash".to_string(),
        );
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty".to_string());
    }
    Ok(())
}

fn invalid<T>(field: &str, reason: String) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    })
}
