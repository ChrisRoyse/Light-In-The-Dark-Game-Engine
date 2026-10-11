use super::serialize_sha256;
use serde_json::json;

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalRelicItem, InfernalRelicVault,
    InfernalRelicVaultPolicy, InfernalZonePolicy, RPG_INFERNAL_RELIC_VAULTS_SCHEMA,
    infernal_relic_vaults::vault_for_id, rpg_authored_payload_without_nulls,
    validate_infernal_relic_vault_policy, validate_rpg_authored_content_records,
};

pub fn infernal_relic_item_content_records(
    policy: &InfernalRelicVaultPolicy,
    zone_policy: &InfernalZonePolicy,
    boss_policy: &InfernalBossAgentPolicy,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_infernal_relic_vault_policy(policy, zone_policy, boss_policy, attrition_policy)?;
    let records = policy
        .item_set
        .iter()
        .map(|item| content_record(item, vault_for_id(policy, &item.source_vault_id)?))
        .collect::<Result<Vec<_>, _>>()?;
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "infernal_relic_vaults.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn content_record(
    item: &InfernalRelicItem,
    vault: &InfernalRelicVault,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(json!({
        "schema": RPG_INFERNAL_RELIC_VAULTS_SCHEMA,
        "source_issue": 1069,
        "content_migration_issue": 1455,
        "surface_kind": "infernal_relic_item",
        "item_id": item.item_id,
        "slot": item.slot,
        "rarity": item.rarity,
        "source_vault_id": item.source_vault_id,
        "source_circle_id": vault.circle_id,
        "guarded_by_ruler_ref": item.guarded_by_ruler_ref,
        "world_finite_count": item.world_finite_count,
        "power_rating_milli": item.power_rating_milli,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
        "bypasses_infernal_no_recall": false,
        "perks": item.perks,
    }));
    Ok(EngineContentRecord {
        kind: EngineContentKind::Item,
        id: item.item_id.clone(),
        display_name: item.display_name.clone(),
        source: EngineContentSource {
            path: format!(
                "editor://rpg/ashen_reach/infernal_relic_vaults/{}",
                item.item_id
            ),
            author: "editor_authoring.issue1455.infernal_relic_vaults".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
