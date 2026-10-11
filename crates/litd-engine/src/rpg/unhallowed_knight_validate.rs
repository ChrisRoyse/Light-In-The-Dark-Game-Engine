use std::collections::BTreeSet;

use crate::EngineError;

use super::{
    RPG_UNHALLOWED_KNIGHT_SCHEMA, SoulBoundWeaponState, SoulKillEvent, SoulWeaponBindRequest,
    SoulWeaponRules, SoulWeaponTransferRecord, SoulWeaponTransferRequest,
    UNHALLOWED_KNIGHT_CLASS_ID, UnhallowedKnightCharacter, UnhallowedKnightPolicy,
    build_ability_catalog_readback, unhallowed_knight_ability_catalog,
};

pub fn validate_unhallowed_knight_policy(
    policy: &UnhallowedKnightPolicy,
) -> Result<(), EngineError> {
    if policy.schema != RPG_UNHALLOWED_KNIGHT_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_UNHALLOWED_KNIGHT_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1062 {
        return invalid("policy.source_issue", "must point at issue 1062");
    }
    if policy.class_id != UNHALLOWED_KNIGHT_CLASS_ID {
        return invalid("policy.class_id", "must use the Unhallowed Knight class id");
    }
    validate_text("policy.display_name", &policy.display_name, 96)?;
    if policy.prime_stat != "strength" || policy.required_alignment != "evil" {
        return invalid(
            "policy.class_gate",
            "Unhallowed Knight must be STR-prime and evil-only",
        );
    }
    if !(1..=100).contains(&policy.minimum_strength) {
        return invalid("policy.minimum_strength", "must be inside 1..=100");
    }
    if policy.built_in_map_required || policy.grants_map_or_minimap || policy.grants_gps_markers {
        return invalid("policy.map_flags", "class content cannot grant map/GPS");
    }
    validate_soul_weapon_rules(&policy.soul_weapon)?;
    let ability_readback = build_ability_catalog_readback(&unhallowed_knight_ability_catalog())?;
    if ability_readback.ability_count != 6 || ability_readback.capstone_count != 1 {
        return invalid(
            "ability_catalog",
            "six abilities and one capstone are required",
        );
    }
    Ok(())
}

pub fn validate_weapon_state(
    policy: &UnhallowedKnightPolicy,
    state: &SoulBoundWeaponState,
) -> Result<(), EngineError> {
    if state.schema != RPG_UNHALLOWED_KNIGHT_SCHEMA {
        return invalid(
            "state.schema",
            format!("expected {RPG_UNHALLOWED_KNIGHT_SCHEMA}"),
        );
    }
    validate_character(
        policy,
        &UnhallowedKnightCharacter {
            character_id: state.character_id.clone(),
            character_name: state.character_name.clone(),
            class_id: state.class_id.clone(),
            alignment: state.alignment.clone(),
            strength: state.strength,
        },
    )?;
    validate_token("state.bound_weapon_id", &state.bound_weapon_id)?;
    validate_text("state.weapon_display_name", &state.weapon_display_name, 96)?;
    if state.power_rating_milli > policy.soul_weapon.maximum_power_milli {
        return invalid("state.power_rating_milli", "exceeds soul weapon cap");
    }
    if state.master_wound_milli > 1000 {
        return invalid("state.master_wound_milli", "must be <= 1000");
    }
    if state.destroyed && state.power_rating_milli != 0 {
        return invalid("state.destroyed", "destroyed weapons must have zero power");
    }
    if state.built_in_map_required || state.grants_map_or_minimap || state.grants_gps_markers {
        return invalid("state.map_flags", "soul weapon state cannot grant map/GPS");
    }
    unique_tokens("state.kill_event_ids", &state.kill_event_ids)?;
    for transfer in &state.transfer_history {
        validate_transfer_record(transfer)?;
    }
    Ok(())
}

pub fn validate_character(
    policy: &UnhallowedKnightPolicy,
    character: &UnhallowedKnightCharacter,
) -> Result<(), EngineError> {
    validate_token("character.character_id", &character.character_id)?;
    validate_text("character.character_name", &character.character_name, 128)?;
    if character.class_id != policy.class_id {
        return invalid("character.class_id", "must be Unhallowed Knight");
    }
    if character.alignment != policy.required_alignment {
        return invalid("character.alignment", "Unhallowed Knight must stay evil");
    }
    if character.strength < policy.minimum_strength {
        return invalid("character.strength", "below Unhallowed Knight STR gate");
    }
    Ok(())
}

pub fn validate_bind_request(
    policy: &UnhallowedKnightPolicy,
    request: &SoulWeaponBindRequest,
) -> Result<(), EngineError> {
    validate_token("request.weapon_id", &request.weapon_id)?;
    validate_text(
        "request.weapon_display_name",
        &request.weapon_display_name,
        96,
    )?;
    if request.rite_id != policy.soul_weapon.binding_rite_id {
        return invalid("request.rite_id", "wrong binding rite");
    }
    if !request.accepted_irreversible_binding {
        return invalid(
            "request.accepted_irreversible_binding",
            "binding cost required",
        );
    }
    Ok(())
}

pub fn validate_kill_event(event: &SoulKillEvent) -> Result<(), EngineError> {
    validate_token("event.event_id", &event.event_id)?;
    validate_token("event.victim_id", &event.victim_id)?;
    if event.soul_value == 0 || event.soul_value > 20 {
        return invalid("event.soul_value", "must be inside 1..=20");
    }
    if !event.witnessed {
        return invalid(
            "event.witnessed",
            "soul kills must be witnessed by combat state",
        );
    }
    Ok(())
}

pub fn validate_transfer_request(
    policy: &UnhallowedKnightPolicy,
    request: &SoulWeaponTransferRequest,
) -> Result<(), EngineError> {
    validate_token("request.transfer_id", &request.transfer_id)?;
    validate_token("request.new_weapon_id", &request.new_weapon_id)?;
    validate_text(
        "request.new_weapon_display_name",
        &request.new_weapon_display_name,
        96,
    )?;
    if request.rite_id != policy.soul_weapon.transfer_rite_id {
        return invalid("request.rite_id", "wrong transfer rite");
    }
    if !request.accepted_soul_scarring_cost {
        return invalid(
            "request.accepted_soul_scarring_cost",
            "transfer cost required",
        );
    }
    Ok(())
}

pub(crate) fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

pub(crate) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

pub(crate) fn invalid_error(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("unhallowed_knight.{}", field.into()),
        reason: reason.to_string(),
    }
}

fn validate_soul_weapon_rules(rules: &SoulWeaponRules) -> Result<(), EngineError> {
    validate_token("soul_weapon.binding_rite_id", &rules.binding_rite_id)?;
    validate_token("soul_weapon.transfer_rite_id", &rules.transfer_rite_id)?;
    validate_token("soul_weapon.soul_resource_id", &rules.soul_resource_id)?;
    if !rules.every_life_claimed_strengthens
        || !rules.accumulated_power_transferable
        || !rules.destroying_weapon_wounds_master
    {
        return invalid(
            "soul_weapon.flags",
            "kill growth, transfer, and destruction wound are required",
        );
    }
    if rules.initial_power_milli == 0
        || rules.power_per_soul_milli == 0
        || rules.maximum_power_milli <= rules.initial_power_milli
        || rules.maximum_power_milli > 1000
    {
        return invalid(
            "soul_weapon.power",
            "power values must be bounded and growing",
        );
    }
    if !(1..=1000).contains(&rules.transfer_efficiency_milli) {
        return invalid("soul_weapon.transfer_efficiency_milli", "must be 1..=1000");
    }
    if rules.destruction_wound_milli == 0 || rules.destruction_wound_milli > 1000 {
        return invalid("soul_weapon.destruction_wound_milli", "must be 1..=1000");
    }
    if rules.grants_map_or_minimap || rules.grants_gps_markers {
        return invalid("soul_weapon.map_flags", "soul weapons cannot grant map/GPS");
    }
    Ok(())
}

fn validate_transfer_record(record: &SoulWeaponTransferRecord) -> Result<(), EngineError> {
    validate_token("transfer.transfer_id", &record.transfer_id)?;
    validate_token("transfer.from_weapon_id", &record.from_weapon_id)?;
    validate_token("transfer.to_weapon_id", &record.to_weapon_id)?;
    if record.power_after_milli > record.power_before_milli || record.efficiency_milli > 1000 {
        return invalid("transfer.power", "transfer cannot create power");
    }
    Ok(())
}

fn unique_tokens(field: &str, values: &[String]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for value in values {
        validate_token(field, value)?;
        if !seen.insert(value.as_str()) {
            return invalid(field, "duplicate value");
        }
    }
    Ok(())
}

fn validate_text(field: &str, value: &str, max_len: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.len() > max_len {
        return invalid(field, format!("must be <= {max_len} bytes"));
    }
    Ok(())
}
