use super::serialize_sha256;
use serde_json::json;

use crate::EngineError;

use super::{
    RPG_UNHALLOWED_KNIGHT_SCHEMA, SoulBoundWeaponState, SoulKillEvent, SoulWeaponActionKind,
    SoulWeaponActionReadback, SoulWeaponBindRequest, SoulWeaponTransferRecord,
    SoulWeaponTransferRequest, UnhallowedKnightCharacter, UnhallowedKnightPolicy,
    UnhallowedKnightReadback, build_ability_catalog_readback, invalid, invalid_error,
    unhallowed_knight_ability_catalog, validate_bind_request, validate_character,
    validate_kill_event, validate_token, validate_transfer_request,
    validate_unhallowed_knight_policy, validate_weapon_state,
};

pub fn derive_unhallowed_knight_readback(
    policy: &UnhallowedKnightPolicy,
) -> Result<UnhallowedKnightReadback, EngineError> {
    validate_unhallowed_knight_policy(policy)?;
    let ability_catalog = unhallowed_knight_ability_catalog();
    let ability_readback = build_ability_catalog_readback(&ability_catalog)?;
    let mut readback = UnhallowedKnightReadback {
        schema: RPG_UNHALLOWED_KNIGHT_SCHEMA.to_string(),
        server_policy_id: policy.server_policy_id.clone(),
        source_issue: policy.source_issue,
        class_id: policy.class_id.clone(),
        display_name: policy.display_name.clone(),
        prime_stat: policy.prime_stat.clone(),
        required_alignment: policy.required_alignment.clone(),
        minimum_strength: policy.minimum_strength,
        ability_count: ability_catalog.abilities.len(),
        ability_readback,
        every_life_claimed_strengthens: policy.soul_weapon.every_life_claimed_strengthens,
        accumulated_power_transferable: policy.soul_weapon.accumulated_power_transferable,
        destroying_weapon_wounds_master: policy.soul_weapon.destroying_weapon_wounds_master,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        policy_sha256: unhallowed_knight_policy_hash(policy)?,
        ability_catalog_sha256: serialize_sha256(&ability_catalog)?,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&readback_fingerprint(&readback))?;
    Ok(readback)
}

pub fn bind_soul_weapon(
    policy: &UnhallowedKnightPolicy,
    character: &UnhallowedKnightCharacter,
    request: &SoulWeaponBindRequest,
) -> Result<SoulWeaponActionReadback, EngineError> {
    validate_unhallowed_knight_policy(policy)?;
    validate_character(policy, character)?;
    validate_bind_request(policy, request)?;
    let state = SoulBoundWeaponState {
        schema: RPG_UNHALLOWED_KNIGHT_SCHEMA.to_string(),
        character_id: character.character_id.clone(),
        character_name: character.character_name.clone(),
        class_id: character.class_id.clone(),
        alignment: character.alignment.clone(),
        strength: character.strength,
        bound_weapon_id: request.weapon_id.clone(),
        weapon_display_name: request.weapon_display_name.clone(),
        souls_siphoned: 0,
        power_rating_milli: policy.soul_weapon.initial_power_milli,
        master_wound_milli: 0,
        destroyed: false,
        kill_event_ids: Vec::new(),
        transfer_history: Vec::new(),
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    };
    action_readback(SoulWeaponActionKind::Bind, None, 0, 0, 0, 0, state)
}

pub fn record_soul_weapon_kill(
    policy: &UnhallowedKnightPolicy,
    state: &SoulBoundWeaponState,
    event: &SoulKillEvent,
) -> Result<SoulWeaponActionReadback, EngineError> {
    validate_unhallowed_knight_policy(policy)?;
    validate_weapon_state(policy, state)?;
    validate_kill_event(event)?;
    if state.destroyed {
        return invalid(
            "state.destroyed",
            "destroyed soul weapons cannot siphon kills",
        );
    }
    if state.kill_event_ids.iter().any(|id| id == &event.event_id) {
        return invalid("event.event_id", "kill event already applied");
    }
    let mut after = state.clone();
    let souls_before = state.souls_siphoned;
    let power_before = state.power_rating_milli;
    after.souls_siphoned = after
        .souls_siphoned
        .checked_add(u32::from(event.soul_value))
        .ok_or_else(|| invalid_error("state.souls_siphoned", "overflowed u32"))?;
    let growth = event
        .soul_value
        .checked_mul(policy.soul_weapon.power_per_soul_milli)
        .ok_or_else(|| invalid_error("event.soul_value", "growth overflowed"))?;
    after.power_rating_milli = (after.power_rating_milli.saturating_add(growth))
        .min(policy.soul_weapon.maximum_power_milli);
    after.kill_event_ids.push(event.event_id.clone());
    validate_weapon_state(policy, &after)?;
    action_readback(
        SoulWeaponActionKind::KillSiphon,
        Some(state.bound_weapon_id.clone()),
        souls_before,
        power_before,
        state.master_wound_milli,
        state.master_wound_milli,
        after,
    )
}

pub fn transfer_soul_weapon_power(
    policy: &UnhallowedKnightPolicy,
    state: &SoulBoundWeaponState,
    request: &SoulWeaponTransferRequest,
) -> Result<SoulWeaponActionReadback, EngineError> {
    validate_unhallowed_knight_policy(policy)?;
    validate_weapon_state(policy, state)?;
    validate_transfer_request(policy, request)?;
    if state.destroyed {
        return invalid(
            "state.destroyed",
            "destroyed soul weapons cannot transfer power",
        );
    }
    let power_before = state.power_rating_milli;
    let power_after = scaled_milli(power_before, policy.soul_weapon.transfer_efficiency_milli)?;
    let mut after = state.clone();
    let from_weapon_id = after.bound_weapon_id.clone();
    after.bound_weapon_id = request.new_weapon_id.clone();
    after.weapon_display_name = request.new_weapon_display_name.clone();
    after.power_rating_milli = power_after;
    after.transfer_history.push(SoulWeaponTransferRecord {
        transfer_id: request.transfer_id.clone(),
        from_weapon_id,
        to_weapon_id: request.new_weapon_id.clone(),
        power_before_milli: power_before,
        power_after_milli: power_after,
        efficiency_milli: policy.soul_weapon.transfer_efficiency_milli,
    });
    validate_weapon_state(policy, &after)?;
    action_readback(
        SoulWeaponActionKind::Transfer,
        Some(state.bound_weapon_id.clone()),
        state.souls_siphoned,
        power_before,
        state.master_wound_milli,
        state.master_wound_milli,
        after,
    )
}

pub fn destroy_soul_bound_weapon(
    policy: &UnhallowedKnightPolicy,
    state: &SoulBoundWeaponState,
    destruction_id: &str,
) -> Result<SoulWeaponActionReadback, EngineError> {
    validate_unhallowed_knight_policy(policy)?;
    validate_weapon_state(policy, state)?;
    validate_token("destruction_id", destruction_id)?;
    if state.destroyed {
        return invalid("state.destroyed", "soul weapon is already destroyed");
    }
    let mut after = state.clone();
    let power_before = state.power_rating_milli;
    let wound_before = state.master_wound_milli;
    after.destroyed = true;
    after.power_rating_milli = 0;
    after.master_wound_milli = after
        .master_wound_milli
        .saturating_add(policy.soul_weapon.destruction_wound_milli)
        .min(1000);
    validate_weapon_state(policy, &after)?;
    action_readback(
        SoulWeaponActionKind::Destroy,
        Some(state.bound_weapon_id.clone()),
        state.souls_siphoned,
        power_before,
        wound_before,
        after.master_wound_milli,
        after,
    )
}

pub fn unhallowed_knight_policy_hash(
    policy: &UnhallowedKnightPolicy,
) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn action_readback(
    action: SoulWeaponActionKind,
    weapon_id_before: Option<String>,
    souls_before: u32,
    power_before_milli: u16,
    wound_before_milli: u16,
    wound_after_milli: u16,
    after_state: SoulBoundWeaponState,
) -> Result<SoulWeaponActionReadback, EngineError> {
    let mut readback = SoulWeaponActionReadback {
        schema: RPG_UNHALLOWED_KNIGHT_SCHEMA.to_string(),
        action,
        character_id: after_state.character_id.clone(),
        weapon_id_before,
        weapon_id_after: Some(after_state.bound_weapon_id.clone()),
        souls_before,
        souls_after: after_state.souls_siphoned,
        power_before_milli,
        power_after_milli: after_state.power_rating_milli,
        master_wound_before_milli: wound_before_milli,
        master_wound_after_milli: wound_after_milli,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        state_sha256: serialize_sha256(&after_state)?,
        result_sha256: String::new(),
        after_state,
    };
    readback.result_sha256 = serialize_sha256(&action_fingerprint(&readback))?;
    Ok(readback)
}

fn scaled_milli(value: u16, milli: u16) -> Result<u16, EngineError> {
    let product = u32::from(value)
        .checked_mul(u32::from(milli))
        .ok_or_else(|| invalid_error("transfer.power", "overflowed"))?;
    Ok((product / 1000).try_into().unwrap_or(u16::MAX))
}

fn readback_fingerprint(readback: &UnhallowedKnightReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "server_policy_id": readback.server_policy_id,
        "source_issue": readback.source_issue,
        "class_id": readback.class_id,
        "prime_stat": readback.prime_stat,
        "required_alignment": readback.required_alignment,
        "ability_count": readback.ability_count,
        "every_life_claimed_strengthens": readback.every_life_claimed_strengthens,
        "accumulated_power_transferable": readback.accumulated_power_transferable,
        "destroying_weapon_wounds_master": readback.destroying_weapon_wounds_master,
        "built_in_map_required": readback.built_in_map_required,
        "policy_sha256": readback.policy_sha256,
        "ability_catalog_sha256": readback.ability_catalog_sha256,
    })
}

fn action_fingerprint(readback: &SoulWeaponActionReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "action": readback.action,
        "character_id": readback.character_id,
        "weapon_id_after": readback.weapon_id_after,
        "souls_after": readback.souls_after,
        "power_after_milli": readback.power_after_milli,
        "master_wound_after_milli": readback.master_wound_after_milli,
        "built_in_map_required": readback.built_in_map_required,
        "state_sha256": readback.state_sha256,
    })
}
