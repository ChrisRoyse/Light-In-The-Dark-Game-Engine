use std::collections::BTreeSet;

use crate::EngineError;

use super::area_registry::AreaRegistry;
use super::road_network_types::*;
use super::serialize_sha256;

pub fn road_network_readback(spec: &RoadNetworkSpec) -> Result<RoadNetworkReadback, EngineError> {
    validate_road_network(spec)?;
    let segment_count: usize = spec
        .roads
        .iter()
        .map(|r| r.waypoint_area_ids.len().saturating_sub(1))
        .sum();
    let waypoint_count: usize = spec.roads.iter().map(|r| r.waypoint_area_ids.len()).sum();
    let gate_count: usize = spec.roads.iter().map(|r| r.gates.len()).sum();
    let law_zone_gate_count: usize = spec
        .roads
        .iter()
        .flat_map(|r| r.gates.iter())
        .filter(|g| g.is_law_zone)
        .count();
    let exposed_road_count = spec
        .roads
        .iter()
        .filter(|r| r.exposure == RoadExposure::Exposed)
        .count();
    let chokepoint_road_count = spec
        .roads
        .iter()
        .filter(|r| r.exposure == RoadExposure::Chokepoint)
        .count();
    Ok(RoadNetworkReadback {
        schema: spec.schema.clone(),
        world_id: spec.world_id.clone(),
        source_issue: spec.source_issue,
        road_count: spec.roads.len(),
        segment_count,
        waypoint_count,
        gate_count,
        law_zone_gate_count,
        crossroad_count: spec.crossroads.len(),
        exposed_road_count,
        chokepoint_road_count,
        road_network_sha256: serialize_sha256(spec)?,
    })
}

pub fn validate_road_network(spec: &RoadNetworkSpec) -> Result<(), EngineError> {
    if spec.schema != RPG_ROAD_NETWORK_SCHEMA {
        return invalid("schema", format!("expected {RPG_ROAD_NETWORK_SCHEMA}"));
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
    if spec.roads.is_empty() {
        return invalid("roads", "at least one road is required".to_string());
    }

    let mut road_ids = BTreeSet::new();
    let mut gate_ids = BTreeSet::new();
    for road in &spec.roads {
        validate_token("roads.road_id", &road.road_id)?;
        validate_text("roads.display_name", &road.display_name)?;
        validate_text("roads.summary", &road.summary)?;
        if !road_ids.insert(road.road_id.clone()) {
            return invalid("roads.road_id", format!("duplicate road {}", road.road_id));
        }
        if road.waypoint_area_ids.len() < 2 {
            return invalid(
                "roads.waypoint_area_ids",
                format!("road {} needs at least two waypoints", road.road_id),
            );
        }
        for area_id in &road.waypoint_area_ids {
            validate_token("roads.waypoint_area_ids", area_id)?;
        }
        for gate in &road.gates {
            validate_token("roads.gates.gate_id", &gate.gate_id)?;
            validate_token("roads.gates.at_area_id", &gate.at_area_id)?;
            validate_text("roads.gates.summary", &gate.summary)?;
            if !gate_ids.insert(gate.gate_id.clone()) {
                return invalid(
                    "roads.gates.gate_id",
                    format!("duplicate gate {}", gate.gate_id),
                );
            }
            if !road.waypoint_area_ids.contains(&gate.at_area_id) {
                return invalid(
                    "roads.gates.at_area_id",
                    format!(
                        "gate {} sits at {} which is not a waypoint of road {}",
                        gate.gate_id, gate.at_area_id, road.road_id
                    ),
                );
            }
        }
    }

    let mut crossroad_ids = BTreeSet::new();
    for crossroad in &spec.crossroads {
        validate_token("crossroads.crossroad_id", &crossroad.crossroad_id)?;
        validate_text("crossroads.display_name", &crossroad.display_name)?;
        validate_token("crossroads.at_area_id", &crossroad.at_area_id)?;
        validate_text("crossroads.summary", &crossroad.summary)?;
        if !crossroad_ids.insert(crossroad.crossroad_id.clone()) {
            return invalid(
                "crossroads.crossroad_id",
                format!("duplicate crossroad {}", crossroad.crossroad_id),
            );
        }
        if crossroad.road_ids.len() < 2 {
            return invalid(
                "crossroads.road_ids",
                format!(
                    "crossroad {} must join at least two roads",
                    crossroad.crossroad_id
                ),
            );
        }
        for road_id in &crossroad.road_ids {
            if !road_ids.contains(road_id) {
                return invalid(
                    "crossroads.road_ids",
                    format!(
                        "crossroad {} references unknown road {road_id}",
                        crossroad.crossroad_id
                    ),
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

/// Cross-area referential integrity against the #1704 registry: every area a road
/// threads through, every gate location, and every crossroad location must be an
/// authored area. Collects ALL dangling references and fails loud.
pub fn validate_road_network_against_registry(
    spec: &RoadNetworkSpec,
    registry: &AreaRegistry,
) -> Result<(), EngineError> {
    validate_road_network(spec)?;
    let mut dangling: Vec<String> = Vec::new();
    for road in &spec.roads {
        for area_id in &road.waypoint_area_ids {
            if !registry.contains(area_id) {
                dangling.push(format!("{area_id} (waypoint of road {})", road.road_id));
            }
        }
        for gate in &road.gates {
            if !registry.contains(&gate.at_area_id) {
                dangling.push(format!("{} (gate {})", gate.at_area_id, gate.gate_id));
            }
        }
    }
    for crossroad in &spec.crossroads {
        if !registry.contains(&crossroad.at_area_id) {
            dangling.push(format!(
                "{} (crossroad {})",
                crossroad.at_area_id, crossroad.crossroad_id
            ));
        }
    }
    if !dangling.is_empty() {
        return invalid(
            "road_network.area_refs",
            format!(
                "{} road area reference(s) are unknown: {}",
                dangling.len(),
                dangling.join("; ")
            ),
        );
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
