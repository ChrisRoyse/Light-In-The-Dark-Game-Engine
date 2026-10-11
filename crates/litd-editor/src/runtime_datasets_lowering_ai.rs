//! #1786: lower authored `AiProfile` objects into [`LoweredAiProfile`]s for the runtime
//! datasets. An `AiProfile` object (`kind = "AiProfile"`) carries the macro-AI plan in its
//! property bag; the runtime executes those numbers as a deterministic order-source. Fails
//! loud on any malformed field -- no defaults papering over a bad profile.
//!
//! Build-order encoding (one compact `ai.build_order` string): steps joined by `;`, each
//! `struct_id@at_gold:cell_x,cell_z` (e.g. `type_town_hall@0:10,10;type_barracks@400:12,10`).

use litd_engine::EngineError;
use litd_engine::game_runtime::{
    AiBuildStep, AiWaveStep, LoweredAiProfile, lower_ai_profile_with_waves,
};

use crate::history::EditorObject;

use super::{lower_error, owner_to_player_id, str_prop, u16_prop, u32_prop};

pub(super) const AI_PROFILE_KIND: &str = "AiProfile";

/// Lower every authored `AiProfile` object into a lowered profile, remapping its authored
/// owner to the dense player id assigned by the melee owner set.
pub(super) fn lower_ai_profiles(
    objects: &[&EditorObject],
    owners: &[u8],
) -> Result<Vec<LoweredAiProfile>, EngineError> {
    let mut profiles = Vec::new();
    for object in objects {
        if object.kind != AI_PROFILE_KIND {
            continue;
        }
        profiles.push(lower_one(object, owners)?);
    }
    Ok(profiles)
}

fn lower_one(object: &EditorObject, owners: &[u8]) -> Result<LoweredAiProfile, EngineError> {
    let raw_owner = str_prop(object, "ai.controlled_player").ok_or_else(|| {
        EngineError::GamePackageInvalidConfig {
            field: format!(
                "litd_editor.runtime_datasets_lowering.{}.controlled_player",
                object.id
            ),
            reason: "required property ai.controlled_player is absent".to_string(),
        }
    })?;
    let authored_owner = owner_to_player_id(raw_owner)?;
    let dense = owners
        .iter()
        .position(|o| *o == authored_owner)
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: format!(
                "litd_editor.runtime_datasets_lowering.{}.controlled_player",
                object.id
            ),
            reason: format!("owner {raw_owner:?} is not an owning player in this melee"),
        })? as u8;

    let display_name = str_prop(object, "ai.display_name")
        .unwrap_or("Computer")
        .to_string();
    let build_order = parse_build_order(object, str_prop(object, "ai.build_order").unwrap_or(""))?;
    // #1858: optional `ai.waves` = `at_tick:size:regroup;...` (empty = legacy single wave).
    let waves = parse_waves(object, str_prop(object, "ai.waves").unwrap_or(""))?;

    lower_ai_profile_with_waves(
        dense,
        display_name,
        u32_prop(object, "ai.decision_interval_ticks")?,
        str_prop(object, "ai.worker_node_id").ok_or_else(|| {
            EngineError::GamePackageInvalidConfig {
                field: format!(
                    "litd_editor.runtime_datasets_lowering.{}.worker_node_id",
                    object.id
                ),
                reason: "required property ai.worker_node_id is absent".to_string(),
            }
        })?,
        u32_prop(object, "ai.worker_target")?,
        build_order,
        str_prop(object, "ai.army_unit_id").ok_or_else(|| {
            EngineError::GamePackageInvalidConfig {
                field: format!(
                    "litd_editor.runtime_datasets_lowering.{}.army_unit_id",
                    object.id
                ),
                reason: "required property ai.army_unit_id is absent".to_string(),
            }
        })?,
        u32_prop(object, "ai.wave_size")?,
        waves,
        u32_prop(object, "ai.retreat_when_structures_below")?,
        u32_prop(object, "ai.retreat_cooldown_ticks")?,
        u16_prop(object, "ai.handicap_permille")?,
    )
}

fn parse_build_order(object: &EditorObject, raw: &str) -> Result<Vec<AiBuildStep>, EngineError> {
    let malformed = |detail: &str| EngineError::GamePackageInvalidConfig {
        field: format!(
            "litd_editor.runtime_datasets_lowering.{}.build_order",
            object.id
        ),
        reason: format!("malformed ai.build_order step: {detail}"),
    };
    let mut steps = Vec::new();
    for segment in raw.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (struct_id, rest) = segment
            .split_once('@')
            .ok_or_else(|| malformed(&format!("{segment:?} missing '@at_gold'")))?;
        let (gold, cell) = rest
            .split_once(':')
            .ok_or_else(|| malformed(&format!("{segment:?} missing ':cell'")))?;
        let (cell_x, cell_z) = cell
            .split_once(',')
            .ok_or_else(|| malformed(&format!("{segment:?} missing 'x,z'")))?;
        steps.push(AiBuildStep {
            struct_id: struct_id.trim().to_string(),
            at_gold: gold
                .trim()
                .parse()
                .map_err(|error| malformed(&format!("{gold:?} at_gold: {error}")))?,
            cell_x: cell_x
                .trim()
                .parse()
                .map_err(|error| malformed(&format!("{cell_x:?} cell_x: {error}")))?,
            cell_z: cell_z
                .trim()
                .parse()
                .map_err(|error| malformed(&format!("{cell_z:?} cell_z: {error}")))?,
        });
    }
    if steps.is_empty() {
        return lower_error(
            &format!("{}.build_order", object.id),
            "an AI profile needs at least one build-order step",
        );
    }
    Ok(steps)
}

/// #1858: parse `at_tick:size:regroup;...` wave schedule. Empty string → no schedule.
fn parse_waves(object: &EditorObject, raw: &str) -> Result<Vec<AiWaveStep>, EngineError> {
    let malformed = |detail: &str| EngineError::GamePackageInvalidConfig {
        field: format!("litd_editor.runtime_datasets_lowering.{}.waves", object.id),
        reason: format!("malformed ai.waves step: {detail}"),
    };
    let mut waves = Vec::new();
    for segment in raw.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let parts: Vec<&str> = segment.split(':').map(str::trim).collect();
        if parts.len() != 3 {
            return Err(malformed(&format!(
                "{segment:?} expected at_tick:size:regroup"
            )));
        }
        waves.push(AiWaveStep {
            at_tick: parts[0]
                .parse()
                .map_err(|error| malformed(&format!("at_tick {parts:?}: {error}")))?,
            size: parts[1]
                .parse()
                .map_err(|error| malformed(&format!("size {parts:?}: {error}")))?,
            regroup_ticks: parts[2]
                .parse()
                .map_err(|error| malformed(&format!("regroup {parts:?}: {error}")))?,
        });
    }
    Ok(waves)
}
