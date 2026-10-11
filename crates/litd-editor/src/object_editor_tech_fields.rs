//! #825 tech-tree requirement records for the object-data editor, lowered through the
//! #829 valve. A `Tech` object declares a tier, a `requires` list (unit-type / upgrade
//! prerequisite ids for training / building / researching) and an `equivalents` list of
//! dependency aliases it satisfies (e.g. any town-hall tier). The lowering-time graph
//! validator resolves every requirement against the concrete-id set plus the equivalence
//! aliases; dangling refs fail loud and requirement CYCLES are rejected with a typed
//! `EngineError::ObjectRequirementCycle` naming the cycle. Kept in its own module so the
//! object-editor lowering/validation split stays under the 500-line ratchet.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::EditorObject;

use super::validation::object_editor_error;

/// Highest authored tech tier (build-prerequisite depth model).
pub const TECH_TIER_MAX: u32 = 20;
/// Bound on the prerequisite list of a single tech record.
pub const TECH_MAX_REQUIRES: usize = 32;
/// Bound on the equivalence-alias list of a single tech record.
pub const TECH_MAX_EQUIVALENTS: usize = 32;
/// The overrideable tech-record property keys (used by tests / FSV / persist validation).
pub const TECH_PROPERTY_KEYS: &[&str] = &["tech.tier", "tech.requires", "tech.equivalents"];

/// One authored tech requirement record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechRequirementRecord {
    pub tech_id: String,
    pub rawcode: String,
    pub display_name: String,
    pub tier: u32,
    /// Prerequisite ids (concrete tech ids or equivalence aliases) that must be satisfied.
    pub requires: Vec<String>,
    /// Dependency-equivalence aliases this record satisfies (id A satisfies req for id B).
    pub equivalents: Vec<String>,
}

/// The resolved, graph-validated tech tree emitted by the #829 valve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredTechTree {
    pub records: Vec<TechRequirementRecord>,
    pub node_count: usize,
    pub edge_count: usize,
}

/// Deterministically extract a tech record from an authored `Tech` object. Absent optional
/// fields resolve to empty lists / tier 0; present fields are range / shape validated so a
/// lowering can never emit an out-of-band record.
pub(crate) fn build_tech_record(
    object: &EditorObject,
) -> Result<TechRequirementRecord, EngineError> {
    Ok(TechRequirementRecord {
        tech_id: object.id.clone(),
        rawcode: required_tech(object, "object.rawcode")?,
        display_name: required_tech(object, "object.display_name")?,
        tier: u32_tech(object, "tech.tier", 0, TECH_TIER_MAX, 0)?,
        requires: id_list_tech(object, "tech.requires", TECH_MAX_REQUIRES)?,
        equivalents: id_list_tech(object, "tech.equivalents", TECH_MAX_EQUIVALENTS)?,
    })
}

/// Validate the whole authored tech tree as a lowering-time graph: every requirement must
/// resolve to a concrete tech id or a declared equivalence alias (dangling refs fail loud),
/// and the requirement graph must be acyclic (cycles are rejected naming the cycle).
pub(crate) fn validate_tech_graph(records: &[TechRequirementRecord]) -> Result<usize, EngineError> {
    // Concrete ids + the alias -> provider index. A `requires` entry is satisfiable when it
    // names a concrete tech id or an alias that at least one record declares as an equivalent.
    let mut ids = BTreeSet::new();
    for record in records {
        if !ids.insert(record.tech_id.as_str()) {
            return Err(object_editor_error(
                "tech.tech_id",
                format!("duplicate tech id {}", record.tech_id),
            ));
        }
    }
    let mut alias_providers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for record in records {
        for alias in &record.equivalents {
            if ids.contains(alias.as_str()) && alias != &record.tech_id {
                return Err(object_editor_error(
                    "tech.equivalents",
                    format!(
                        "{} declares equivalent {alias} that collides with another tech id",
                        record.tech_id
                    ),
                ));
            }
            alias_providers
                .entry(alias.as_str())
                .or_default()
                .push(record.tech_id.as_str());
        }
    }
    // Dangling-ref check + adjacency construction (edges: tech -> each concrete satisfier).
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut edge_count = 0usize;
    for record in records {
        let from = record.tech_id.as_str();
        let neighbours = adjacency.entry(from).or_default();
        for req in &record.requires {
            let req = req.as_str();
            if let Some(providers) = alias_providers.get(req) {
                for provider in providers {
                    neighbours.push(*provider);
                    edge_count += 1;
                }
            } else if ids.contains(req) {
                neighbours.push(req);
                edge_count += 1;
            } else {
                return Err(object_editor_error(
                    "tech.requires",
                    format!(
                        "{from} requires {req}, which is not a known tech id or equivalence alias"
                    ),
                ));
            }
        }
        neighbours.sort_unstable();
        neighbours.dedup();
    }
    detect_tech_cycle(records, &adjacency)?;
    Ok(edge_count)
}

/// Depth-first cycle detection over the requirement adjacency. On a back-edge the cycle path
/// is reconstructed from the DFS stack and reported as `EngineError::ObjectRequirementCycle`.
fn detect_tech_cycle(
    records: &[TechRequirementRecord],
    adjacency: &BTreeMap<&str, Vec<&str>>,
) -> Result<(), EngineError> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        White,
        Gray,
        Black,
    }
    let mut state: BTreeMap<&str, Mark> = records
        .iter()
        .map(|record| (record.tech_id.as_str(), Mark::White))
        .collect();
    // Iterative DFS with an explicit stack so deep trees cannot overflow the call stack.
    for record in records {
        if state.get(record.tech_id.as_str()) != Some(&Mark::White) {
            continue;
        }
        let mut path: Vec<&str> = Vec::new();
        let mut stack: Vec<(&str, usize)> = vec![(record.tech_id.as_str(), 0)];
        state.insert(record.tech_id.as_str(), Mark::Gray);
        path.push(record.tech_id.as_str());
        while let Some(&(node, index)) = stack.last() {
            let neighbours = adjacency.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if index < neighbours.len() {
                stack.last_mut().unwrap().1 += 1;
                let next = neighbours[index];
                match state.get(next).copied().unwrap_or(Mark::White) {
                    Mark::Gray => {
                        // Back-edge: the cycle is the suffix of `path` starting at `next`.
                        let start = path.iter().position(|n| *n == next).unwrap_or(0);
                        let mut cycle: Vec<&str> = path[start..].to_vec();
                        cycle.push(next);
                        return Err(EngineError::ObjectRequirementCycle {
                            cycle: cycle.join(" -> "),
                        });
                    }
                    Mark::White => {
                        state.insert(next, Mark::Gray);
                        path.push(next);
                        stack.push((next, 0));
                    }
                    Mark::Black => {}
                }
            } else {
                state.insert(node, Mark::Black);
                path.pop();
                stack.pop();
            }
        }
    }
    Ok(())
}

/// Validate an already-lowered tech tree on TOML readback so a corrupted or hand-edited
/// artifact fails loud (re-runs the same range + graph checks the lowering ran).
pub(crate) fn validate_lowered_tech_tree(tree: &LoweredTechTree) -> Result<(), EngineError> {
    if tree.records.len() != tree.node_count {
        return Err(object_editor_error(
            "tech.node_count",
            format!(
                "node_count {} does not match {} records",
                tree.node_count,
                tree.records.len()
            ),
        ));
    }
    for record in &tree.records {
        if record.rawcode.len() != 4 {
            return Err(object_editor_error(
                "tech.rawcode",
                format!("{} rawcode must be four bytes", record.tech_id),
            ));
        }
        check_tech_range("tech.tier", &record.tech_id, record.tier, 0, TECH_TIER_MAX)?;
        check_tech_list_len(
            "tech.requires",
            &record.tech_id,
            record.requires.len(),
            TECH_MAX_REQUIRES,
        )?;
        check_tech_list_len(
            "tech.equivalents",
            &record.tech_id,
            record.equivalents.len(),
            TECH_MAX_EQUIVALENTS,
        )?;
    }
    let edge_count = validate_tech_graph(&tree.records)?;
    if edge_count != tree.edge_count {
        return Err(object_editor_error(
            "tech.edge_count",
            format!(
                "edge_count {} does not match recomputed {edge_count}",
                tree.edge_count
            ),
        ));
    }
    Ok(())
}

fn required_tech(object: &EditorObject, key: &str) -> Result<String, EngineError> {
    object
        .properties
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| object_editor_error(key, "required tech property is missing"))
}

fn u32_tech(
    object: &EditorObject,
    key: &str,
    min: u32,
    max: u32,
    default: u32,
) -> Result<u32, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(default);
    };
    let parsed = raw
        .parse::<u32>()
        .map_err(|_| object_editor_error(key, format!("must be an integer in {min}..={max}")))?;
    check_tech_range(key, &object.id, parsed, min, max)?;
    Ok(parsed)
}

fn id_list_tech(
    object: &EditorObject,
    key: &str,
    max_len: usize,
) -> Result<Vec<String>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let ids = raw
        .split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if ids.len() > max_len {
        return Err(object_editor_error(
            key,
            format!("at most {max_len} ids allowed; got {}", ids.len()),
        ));
    }
    let mut seen = BTreeSet::new();
    for id in &ids {
        if !seen.insert(id.as_str()) {
            return Err(object_editor_error(key, format!("duplicate id {id}")));
        }
    }
    Ok(ids)
}

fn check_tech_range(
    key: &str,
    tech_id: &str,
    value: u32,
    min: u32,
    max: u32,
) -> Result<(), EngineError> {
    if value < min || value > max {
        return Err(object_editor_error(
            key,
            format!("{tech_id}: must be in {min}..={max}; got {value}"),
        ));
    }
    Ok(())
}

fn check_tech_list_len(
    key: &str,
    tech_id: &str,
    len: usize,
    max_len: usize,
) -> Result<(), EngineError> {
    if len > max_len {
        return Err(object_editor_error(
            key,
            format!("{tech_id}: at most {max_len} ids allowed; got {len}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "object_editor_tech_fields_tests.rs"]
mod tests;
