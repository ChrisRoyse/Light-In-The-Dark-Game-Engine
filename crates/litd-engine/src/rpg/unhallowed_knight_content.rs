use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    AbilityCatalog, RPG_UNHALLOWED_KNIGHT_SCHEMA, UnhallowedKnightPolicy,
    derive_unhallowed_knight_readback, rpg_authored_payload_without_nulls,
    unhallowed_knight_ability_catalog, validate_rpg_authored_content_records,
    validate_unhallowed_knight_policy,
};

pub fn unhallowed_knight_content_records(
    policy: &UnhallowedKnightPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_unhallowed_knight_policy(policy)?;
    let readback = derive_unhallowed_knight_readback(policy)?;
    let catalog = unhallowed_knight_ability_catalog();
    let mut records = vec![
        content_record(
            EngineContentKind::Lore,
            &policy.class_id,
            &policy.display_name,
            json!({
                "schema": RPG_UNHALLOWED_KNIGHT_SCHEMA,
                "source_issue": 1062,
                "content_migration_issue": 1455,
                "surface_kind": "class_spec",
                "class_id": policy.class_id,
                "prime_stat": policy.prime_stat,
                "required_alignment": policy.required_alignment,
                "minimum_strength": policy.minimum_strength,
                "ability_count": readback.ability_count,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Item,
            "item.unhallowed.soul_bound_weapon.prototype",
            "Soul-Bound Weapon Prototype",
            json!({
                "schema": RPG_UNHALLOWED_KNIGHT_SCHEMA,
                "source_issue": 1062,
                "content_migration_issue": 1455,
                "surface_kind": "soul_weapon_rules",
                "rules": policy.soul_weapon,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
    ];
    records.extend(ability_records(&catalog)?);
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "unhallowed_knight.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn ability_records(catalog: &AbilityCatalog) -> Result<Vec<EngineContentRecord>, EngineError> {
    catalog
        .abilities
        .iter()
        .map(|ability| {
            content_record(
                EngineContentKind::Ability,
                &ability.ability_id,
                &ability.display_name,
                json!({
                "schema": RPG_UNHALLOWED_KNIGHT_SCHEMA,
                "source_issue": 1062,
                "content_migration_issue": 1455,
                "surface_kind": "ability",
                "ability": ability,
                "built_in_map_required": false,
                    "grants_map_or_minimap": false,
                    "grants_gps_markers": false,
                }),
            )
        })
        .collect()
}

fn content_record(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/unhallowed_knight/{id}"),
            author: "editor_authoring.issue1455.unhallowed_knight".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
