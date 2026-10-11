use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    ActionProfile, ArmorMitigation, CombatRules, MultiAttackSlot, RPG_COMBAT_SCHEMA, WeaponMatchup,
    combat_validate::validate_combat_rules, rpg_authored_payload_without_nulls,
    validate_rpg_authored_content_records,
};

const COMBAT_RULES_ID: &str = "combat.rules.mud_defaults";

pub fn combat_content_records(
    rules: &CombatRules,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_combat_rules(rules)?;
    let rules_sha256 = serialize_sha256(rules)?;
    let mut records = vec![content_record(
        COMBAT_RULES_ID,
        "Combat Rules: MUD Defaults",
        "rules",
        json!({
            "schema": RPG_COMBAT_SCHEMA,
            "source_issue": 963,
            "content_migration_issue": 1455,
            "surface_kind": "combat_rules",
            "rules_id": COMBAT_RULES_ID,
            "tick_millis": rules.tick_millis,
            "round_ticks": rules.round_ticks,
            "max_rounds": rules.max_rounds,
            "hit_base": rules.hit_base,
            "hit_target": rules.hit_target,
            "armor_class_divisor": rules.armor_class_divisor,
            "min_damage": rules.min_damage,
            "max_defense_chance": rules.max_defense_chance,
            "weapon_specialization_min": rules.weapon_specialization_min,
            "weapon_specialization_hit_bonus": rules.weapon_specialization_hit_bonus,
            "parry_specialization_bonus": rules.parry_specialization_bonus,
            "defense_order_count": rules.defense_order.len(),
            "multi_attack_slot_count": rules.multi_attack_slots.len(),
            "weapon_matchup_count": rules.weapon_matchups.len(),
            "armor_mitigation_count": rules.armor_mitigations.len(),
            "action_profile_count": rules.action_profiles.len(),
            "rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];

    for (position, defense) in rules.defense_order.iter().enumerate() {
        records.push(content_record(
            &format!("combat.defense_order.{position}.{}", defense.label()),
            &format!("Combat Defense Order {}: {}", position + 1, defense.label()),
            "defense_order",
            json!({
                "schema": RPG_COMBAT_SCHEMA,
                "source_issue": 963,
                "content_migration_issue": 1455,
                "surface_kind": "combat_defense_order",
                "rules_id": COMBAT_RULES_ID,
                "position": position,
                "defense": defense.label(),
                "rules_sha256": rules_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }
    for slot in &rules.multi_attack_slots {
        records.push(multi_attack_slot_record(slot, &rules_sha256)?);
    }
    for matchup in &rules.weapon_matchups {
        records.push(weapon_matchup_record(matchup, &rules_sha256)?);
    }
    for mitigation in &rules.armor_mitigations {
        records.push(armor_mitigation_record(mitigation, &rules_sha256)?);
    }
    for profile in &rules.action_profiles {
        records.push(action_profile_record(profile, &rules_sha256)?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgCombatInvalidConfig {
            field: "combat.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn multi_attack_slot_record(
    slot: &MultiAttackSlot,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        &format!("combat.multi_attack_slot.{}", slot.attack_slot),
        &format!("Combat Multi-Attack Slot {}", slot.attack_slot),
        "multi_attack_slots",
        json!({
            "schema": RPG_COMBAT_SCHEMA,
            "source_issue": 963,
            "content_migration_issue": 1455,
            "surface_kind": "combat_multi_attack_slot",
            "rules_id": COMBAT_RULES_ID,
            "attack_slot": slot.attack_slot,
            "skill_id": slot.skill_id,
            "minimum_rating": slot.minimum_rating,
            "rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn weapon_matchup_record(
    matchup: &WeaponMatchup,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        &format!(
            "combat.weapon_matchup.{}_vs_{}",
            matchup.attacker.label(),
            matchup.defender.label()
        ),
        &format!(
            "Combat Weapon Matchup: {} vs {}",
            matchup.attacker.label(),
            matchup.defender.label()
        ),
        "weapon_matchups",
        json!({
            "schema": RPG_COMBAT_SCHEMA,
            "source_issue": 963,
            "content_migration_issue": 1455,
            "surface_kind": "combat_weapon_matchup",
            "rules_id": COMBAT_RULES_ID,
            "attacker": matchup.attacker.label(),
            "defender": matchup.defender.label(),
            "hit_modifier": matchup.hit_modifier,
            "parry_modifier": matchup.parry_modifier,
            "rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn armor_mitigation_record(
    mitigation: &ArmorMitigation,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        &format!("combat.armor_mitigation.{}", mitigation.armor_type.label()),
        &format!("Combat Armor Mitigation: {}", mitigation.armor_type.label()),
        "armor_mitigations",
        json!({
            "schema": RPG_COMBAT_SCHEMA,
            "source_issue": 963,
            "content_migration_issue": 1455,
            "surface_kind": "combat_armor_mitigation",
            "rules_id": COMBAT_RULES_ID,
            "armor_type": mitigation.armor_type.label(),
            "damage_reduction_percent": mitigation.damage_reduction_percent,
            "rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn action_profile_record(
    profile: &ActionProfile,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        &format!("combat.action_profile.{}", profile.action.label()),
        &format!("Combat Action Profile: {}", profile.action.label()),
        "action_profiles",
        json!({
            "schema": RPG_COMBAT_SCHEMA,
            "source_issue": 963,
            "content_migration_issue": 1455,
            "surface_kind": "combat_action_profile",
            "rules_id": COMBAT_RULES_ID,
            "action": profile.action.label(),
            "skill_id": profile.skill_id,
            "hit_modifier": profile.hit_modifier,
            "damage_bonus": profile.damage_bonus,
            "lag_ticks": profile.lag_ticks,
            "rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn content_record(
    id: &str,
    display_name: &str,
    collection: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind: EngineContentKind::Lore,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/combat/{collection}/{id}"),
            author: "editor_authoring.issue1455.combat".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
