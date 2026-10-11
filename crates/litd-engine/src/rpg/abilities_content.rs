use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    AbilityCatalog, AbilityDefinition, build_ability_catalog_readback,
    rpg_authored_payload_without_nulls, validate_rpg_authored_content_records,
};

pub fn ability_content_records(
    catalog: &AbilityCatalog,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = build_ability_catalog_readback(catalog)?;
    let catalog_sha256 = readback.input_sha256.clone();
    let catalog_result_sha256 = readback.result_sha256.clone();
    let mut records = Vec::new();
    records.push(content_record(
        EngineContentKind::Trigger,
        &format!("{}.summary", catalog.catalog_id),
        "Ability Catalog: Ashen Reach",
        "catalog",
        json!({
            "schema": catalog.schema,
            "source_issue": 961,
            "content_migration_issue": 1455,
            "surface_kind": "ability_catalog",
            "catalog_id": catalog.catalog_id,
            "ability_count": readback.ability_count,
            "kind_counts": readback.kind_counts,
            "activation_counts": readback.activation_counts,
            "target_mode_counts": readback.target_mode_counts,
            "effect_kind_counts": readback.effect_kind_counts,
            "cost_resource_totals": readback.cost_resource_totals,
            "prerequisite_edge_count": readback.prerequisite_edge_count,
            "max_prerequisite_depth": readback.max_prerequisite_depth,
            "school_count": readback.school_count,
            "branch_count": readback.branch_count,
            "capstone_count": readback.capstone_count,
            "ui_slot_count": readback.ui_slot_count,
            "trigger_tag_count": readback.trigger_tag_count,
            "ability_ids": readback.ability_ids,
            "ability_catalog_sha256": catalog_sha256,
            "ability_catalog_result_sha256": catalog_result_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);

    for ability in &catalog.abilities {
        records.push(ability_record(
            ability,
            catalog,
            &catalog_sha256,
            &catalog_result_sha256,
        )?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgAbilityInvalidConfig {
            field: "abilities.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn ability_record(
    ability: &AbilityDefinition,
    catalog: &AbilityCatalog,
    catalog_sha256: &str,
    catalog_result_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Ability,
        &ability.ability_id,
        &ability.display_name,
        "abilities",
        json!({
            "schema": catalog.schema,
            "source_issue": 961,
            "content_migration_issue": 1455,
            "surface_kind": "ability_definition",
            "catalog_id": catalog.catalog_id,
            "ability_id": ability.ability_id,
            "display_name": ability.display_name,
            "description": ability.description,
            "kind": ability.kind,
            "activation": ability.activation,
            "category": ability.category,
            "requirements": ability.requirements,
            "costs": ability.costs,
            "timing": ability.timing,
            "targeting": ability.targeting,
            "effects": ability.effects,
            "tags": ability.tags,
            "grants_tags": ability.grants_tags,
            "blocks_tags": ability.blocks_tags,
            "trigger_tags": ability.trigger_tags,
            "ui_slot": ability.ui_slot,
            "ability_sha256": serialize_sha256(ability)?,
            "ability_catalog_sha256": catalog_sha256,
            "ability_catalog_result_sha256": catalog_result_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn content_record(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    collection: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/abilities/{collection}/{id}"),
            author: "editor_authoring.issue1455.abilities".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
