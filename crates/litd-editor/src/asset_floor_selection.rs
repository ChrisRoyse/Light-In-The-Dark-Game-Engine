//! Per-role FLOOR goal-seeking selection over a candidate catalog (issue #1733).
//!
//! `#1596` delivered the explicit *targeted-list* restore. The remaining half is
//! goal-aware selection: given a catalog of candidate assets and the editor's per-role
//! floor minimums (architecture>=500, character>=100, ...), pick the cheapest-first subset
//! that just meets every floor, then stop -- so an empty vault can be refilled without
//! pulling the whole ~25GB corpus.
//!
//! The authoritative role classifier lives in [`crate::asset_vault::classify_roles`]; this
//! module NEVER re-derives roles. That is the whole point of exposing it as a Rust binary the
//! Python intake calls: one classification implementation, one source of truth. The intake
//! writes the candidate metadata (no blobs) to a catalog JSONL, this selector emits the
//! chosen `source:asset_id` subset as JSON, and the intake downloads exactly that subset.

use std::collections::BTreeMap;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::asset_vault::{AssetRoleInput, ROLE_NAMES, classify_roles, required_role_floors};

/// One candidate asset's classifier-relevant metadata, as one JSONL line. These are exactly
/// the fields [`classify_roles`] reads; no file needs to exist on disk to classify a line.
#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEntry {
    pub source: String,
    pub asset_id: String,
    pub asset_type: String,
    #[serde(default)]
    pub title: String,
    pub relative_path: String,
    pub declared_size: u64,
}

/// One selected candidate, echoed back with the roles the classifier assigned it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SelectedEntry {
    pub source: String,
    pub asset_id: String,
    pub roles: Vec<String>,
    pub declared_size: u64,
}

/// The full selection result the binary serializes to stdout.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FloorSelection {
    pub selected: Vec<SelectedEntry>,
    pub selected_count: usize,
    pub selected_declared_bytes: u64,
    pub role_counts: BTreeMap<String, usize>,
    pub floors: BTreeMap<String, usize>,
    pub catalog_entry_count: usize,
}

fn floor_error<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_floor.{field}"),
        reason: reason.into(),
    })
}

/// Parse a catalog JSONL string into candidate entries. Empty input or any malformed line is a
/// LOUD error naming the offending line -- never a silent skip (the #1596 no-silent-partial rule).
pub fn parse_catalog(text: &str) -> Result<Vec<CatalogEntry>, EngineError> {
    if text.trim().is_empty() {
        return floor_error("catalog.empty", "catalog manifest is empty");
    }
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry = serde_json::from_str::<CatalogEntry>(line)
            .map_err(|error| invalid_line(index + 1, error))?;
        if entry.source.trim().is_empty() || entry.asset_id.trim().is_empty() {
            return floor_error(
                "catalog.fields",
                format!("line {}: source and asset_id must be non-empty", index + 1),
            );
        }
        entries.push(entry);
    }
    if entries.is_empty() {
        return floor_error("catalog.empty", "catalog manifest has no non-blank entries");
    }
    Ok(entries)
}

fn invalid_line(line: usize, error: serde_json::Error) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: "editor.asset_floor.catalog.json".to_string(),
        reason: format!("line {line}: invalid catalog JSONL: {error}"),
    }
}

/// Parse `--floor role=count` override specs. Each role MUST be one of [`ROLE_NAMES`]; an
/// unknown role, a non-numeric / zero count, or a duplicate role is a LOUD error. Passing any
/// overrides REPLACES the default floor set with exactly the named roles (so a small,
/// minimal-sufficient goal can be requested); passing none uses [`required_role_floors`].
pub fn parse_floor_overrides(specs: &[String]) -> Result<BTreeMap<String, usize>, EngineError> {
    if specs.is_empty() {
        return Ok(required_role_floors()
            .iter()
            .map(|(role, minimum)| ((*role).to_string(), *minimum))
            .collect());
    }
    let mut floors: BTreeMap<String, usize> = BTreeMap::new();
    for spec in specs {
        let (role, count) =
            spec.split_once('=')
                .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                    field: "editor.asset_floor.override.form".to_string(),
                    reason: format!("floor override {spec:?} must be 'role=count'"),
                })?;
        let role = role.trim();
        if !ROLE_NAMES.contains(&role) {
            return floor_error(
                "override.role",
                format!(
                    "unknown role {role:?}; known roles: {}",
                    ROLE_NAMES.join(", ")
                ),
            );
        }
        let minimum =
            count
                .trim()
                .parse::<usize>()
                .map_err(|error| EngineError::UiRenderInvalidConfig {
                    field: "editor.asset_floor.override.count".to_string(),
                    reason: format!("floor override {spec:?} count is not a usize: {error}"),
                })?;
        if minimum == 0 {
            return floor_error(
                "override.count",
                format!("floor override {spec:?} minimum must be >= 1 (0 would restore nothing)"),
            );
        }
        if floors.insert(role.to_string(), minimum).is_some() {
            return floor_error(
                "override.duplicate",
                format!("duplicate floor role {role:?}"),
            );
        }
    }
    Ok(floors)
}

/// Greedily select the cheapest-first subset of `entries` that meets every floor in `floors`.
///
/// Deterministic: entries are considered in ascending `(declared_size, source, asset_id)` order.
/// An entry is selected when it advances at least one still-unmet floor role; selecting it counts
/// ALL of its floor roles. Selection stops the instant every floor is met. If the whole catalog
/// is exhausted with a floor still unmet, this fails LOUD naming each unmet role (achieved vs
/// required) -- it never returns a partial selection that would silently under-restore.
pub fn select_floor(
    entries: &[CatalogEntry],
    floors: &BTreeMap<String, usize>,
) -> Result<FloorSelection, EngineError> {
    if entries.is_empty() {
        return floor_error("select.empty", "no catalog entries to select from");
    }
    if floors.is_empty() {
        return floor_error("select.floors", "no role floors requested");
    }
    let classified: Vec<(usize, Vec<String>)> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (index, classify_entry(entry)))
        .collect();
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| {
        entries[a]
            .declared_size
            .cmp(&entries[b].declared_size)
            .then_with(|| entries[a].source.cmp(&entries[b].source))
            .then_with(|| entries[a].asset_id.cmp(&entries[b].asset_id))
    });

    let mut role_counts: BTreeMap<String, usize> =
        floors.keys().map(|role| (role.clone(), 0usize)).collect();
    let mut selected = Vec::new();
    let mut selected_bytes = 0u64;
    for index in order {
        if floors_met(floors, &role_counts) {
            break;
        }
        let roles = &classified[index].1;
        let advances = roles.iter().any(|role| {
            floors
                .get(role)
                .is_some_and(|minimum| role_counts.get(role).copied().unwrap_or(0) < *minimum)
        });
        if !advances {
            continue;
        }
        for role in roles {
            if let Some(count) = role_counts.get_mut(role) {
                *count += 1;
            }
        }
        let entry = &entries[index];
        selected_bytes += entry.declared_size;
        selected.push(SelectedEntry {
            source: entry.source.clone(),
            asset_id: entry.asset_id.clone(),
            roles: roles.clone(),
            declared_size: entry.declared_size,
        });
    }

    if !floors_met(floors, &role_counts) {
        let unmet = floors
            .iter()
            .filter(|(role, minimum)| role_counts.get(*role).copied().unwrap_or(0) < **minimum)
            .map(|(role, minimum)| {
                format!(
                    "{role}={}<{minimum}",
                    role_counts.get(role).copied().unwrap_or(0)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        return floor_error(
            "select.unmet",
            format!(
                "catalog of {} entries cannot satisfy floors: unmet={unmet} (selected {} entries before exhaustion)",
                entries.len(),
                selected.len()
            ),
        );
    }

    Ok(FloorSelection {
        selected_count: selected.len(),
        selected_declared_bytes: selected_bytes,
        selected,
        role_counts,
        floors: floors.clone(),
        catalog_entry_count: entries.len(),
    })
}

fn classify_entry(entry: &CatalogEntry) -> Vec<String> {
    classify_roles(&AssetRoleInput {
        source: &entry.source,
        asset_id: &entry.asset_id,
        asset_type: &entry.asset_type,
        title: &entry.title,
        relative_path: &entry.relative_path,
    })
}

fn floors_met(floors: &BTreeMap<String, usize>, role_counts: &BTreeMap<String, usize>) -> bool {
    floors
        .iter()
        .all(|(role, minimum)| role_counts.get(role).copied().unwrap_or(0) >= *minimum)
}

#[cfg(test)]
#[path = "asset_floor_selection_tests.rs"]
mod tests;
