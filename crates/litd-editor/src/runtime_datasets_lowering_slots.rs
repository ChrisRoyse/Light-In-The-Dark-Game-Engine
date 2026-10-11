//! #1788/#836: player-slot + force (team) materialization for runtime datasets lowering.
//!
//! Melee default: each distinct owner is its own force. 2v2 (and larger team games)
//! author `template.force_id` on placed units so teammates share a force and the
//! roster alliance matrix defaults to ally/shared-vision within force.

use std::collections::BTreeMap;

use litd_engine::EngineError;
use litd_engine::game_runtime::players::{PlayerController, PlayerSlot};

use super::lower_error;

/// Map an authored owner string to a dense player id. Accepts `player_one`.. or a number.
pub(super) fn owner_to_player_id(owner: &str) -> Result<u8, EngineError> {
    let normalized = owner.trim();
    let mapped = match normalized {
        "player_one" => Some(0),
        "player_two" => Some(1),
        "player_three" => Some(2),
        "player_four" => Some(3),
        _ => normalized.parse::<u8>().ok(),
    };
    mapped.ok_or_else(|| EngineError::GamePackageInvalidConfig {
        field: "litd_editor.runtime_datasets_lowering.owner".to_string(),
        reason: format!("unrecognized owner player id {owner:?}"),
    })
}

/// Parse optional authored force id (`template.force_id`).
pub(super) fn parse_force_id(raw: &str, object_id: &str) -> Result<u8, EngineError> {
    raw.trim()
        .parse::<u8>()
        .map_err(|error| EngineError::GamePackageInvalidConfig {
            field: format!("litd_editor.runtime_datasets_lowering.{object_id}.template.force_id"),
            reason: format!("expected u8 force id, got {raw:?}: {error}"),
        })
}

/// Record a force assignment for an owner. Fails loud if the same owner is authored
/// on two different forces (split-team bug).
pub(super) fn note_owner_force(
    forces: &mut BTreeMap<u8, u8>,
    owner: u8,
    force: u8,
    object_id: &str,
) -> Result<(), EngineError> {
    match forces.insert(owner, force) {
        Some(prior) if prior != force => Err(EngineError::GamePackageInvalidConfig {
            field: format!("litd_editor.runtime_datasets_lowering.{object_id}.template.force_id"),
            reason: format!(
                "owner {owner} already assigned force {prior}; cannot also assign force {force}"
            ),
        }),
        _ => Ok(()),
    }
}

/// Build dense player slots + force_count from the sorted unique owner set and any
/// authored force hints. Owners without a hint default to force = dense index
/// (classic free-for-all melee).
pub(super) fn build_player_slots(
    owners: &[u8],
    force_hints: &BTreeMap<u8, u8>,
) -> Result<(Vec<PlayerSlot>, u8), EngineError> {
    if owners.len() < 2 {
        return lower_error("players", "a melee match needs at least two owning players");
    }
    let mut forces: Vec<u8> = Vec::with_capacity(owners.len());
    for (index, owner) in owners.iter().enumerate() {
        let force = force_hints.get(owner).copied().unwrap_or(index as u8);
        forces.push(force);
    }
    let force_count = forces.iter().copied().max().unwrap_or(0).saturating_add(1);
    for (index, force) in forces.iter().enumerate() {
        if *force >= force_count {
            return lower_error(
                "players.force",
                format!("slot {index} force {force} >= force_count {force_count}"),
            );
        }
    }
    let slots: Vec<PlayerSlot> = owners
        .iter()
        .enumerate()
        .map(|(index, _owner)| PlayerSlot {
            id: index as u8,
            controller: if index == 0 {
                PlayerController::Human
            } else {
                PlayerController::Computer
            },
            force: forces[index],
            color: index as u8,
            handicap_pct: 100,
        })
        .collect();
    Ok((slots, force_count))
}

/// Remap an authored owner id to its dense slot index.
pub(super) fn dense_owner(owners: &[u8], authored: u8) -> Result<u8, EngineError> {
    owners
        .iter()
        .position(|o| *o == authored)
        .map(|i| i as u8)
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: "litd_editor.runtime_datasets_lowering.owner".to_string(),
            reason: format!("owner {authored} is not an owning player in this melee"),
        })
}
