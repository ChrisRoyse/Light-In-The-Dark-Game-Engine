use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    AbilityCatalog, AdvancementRules, PracticeAwardBand, RPG_ADVANCEMENT_SCHEMA, StudyAbilityCost,
    StudyGrantRule, advancement_validate::validate_advancement_rules,
    build_ability_catalog_readback, rpg_authored_payload_without_nulls,
    validate_rpg_authored_content_records,
};

const ADVANCEMENT_RULES_ID: &str = "advancement.rules.mud_defaults";

pub fn advancement_content_records(
    rules: &AdvancementRules,
    catalog: &AbilityCatalog,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_advancement_rules(rules)?;
    let catalog_readback = build_ability_catalog_readback(catalog)?;
    validate_study_costs_match_catalog(rules, catalog)?;
    let rules_sha256 = serialize_sha256(rules)?;
    let catalog_sha256 = catalog_readback.input_sha256;

    let mut records = vec![content_record(
        EngineContentKind::Lore,
        ADVANCEMENT_RULES_ID,
        "Advancement Rules: MUD Defaults",
        "rules",
        json!({
            "schema": RPG_ADVANCEMENT_SCHEMA,
            "source_issue": 962,
            "content_migration_issue": 1455,
            "surface_kind": "advancement_rules",
            "rules_id": ADVANCEMENT_RULES_ID,
            "ability_catalog_id": catalog.catalog_id,
            "max_level": rules.max_level,
            "train_sessions_per_level": rules.train_sessions_per_level,
            "train_stat_increment": rules.train_stat_increment,
            "train_hit_points": rules.train_hit_points,
            "train_mana": rules.train_mana,
            "train_movement": rules.train_movement,
            "practice_gain_base": rules.practice_gain_base,
            "practice_intelligence_divisor": rules.practice_intelligence_divisor,
            "practice_cap": rules.practice_cap,
            "use_gain_per_success": rules.use_gain_per_success,
            "focused_skill_learn_bonus": rules.focused_skill_learn_bonus,
            "level_cap_base": rules.level_cap_base,
            "level_cap_per_level": rules.level_cap_per_level,
            "practice_band_count": rules.practice_bands.len(),
            "study_grant_count": rules.study_grants.len(),
            "study_cost_count": rules.study_costs.len(),
            "rules_sha256": rules_sha256,
            "ability_catalog_sha256": catalog_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];

    for band in &rules.practice_bands {
        records.push(practice_band_record(band, &rules_sha256, &catalog_sha256)?);
    }
    for grant in &rules.study_grants {
        records.push(study_grant_record(grant, &rules_sha256, &catalog_sha256)?);
    }
    for cost in &rules.study_costs {
        records.push(study_cost_record(cost, &rules_sha256, &catalog_sha256)?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgAdvancementInvalidConfig {
            field: "advancement.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn practice_band_record(
    band: &PracticeAwardBand,
    rules_sha256: &str,
    catalog_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Lore,
        &format!("advancement.practice_band.wisdom_{}", band.min_wisdom),
        &format!("Advancement Practice Band: WIS {}", band.min_wisdom),
        "practice_bands",
        json!({
            "schema": RPG_ADVANCEMENT_SCHEMA,
            "source_issue": 962,
            "content_migration_issue": 1455,
            "surface_kind": "advancement_practice_band",
            "rules_id": ADVANCEMENT_RULES_ID,
            "min_wisdom": band.min_wisdom,
            "practices_per_level": band.practices_per_level,
            "rules_sha256": rules_sha256,
            "ability_catalog_sha256": catalog_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn study_grant_record(
    grant: &StudyGrantRule,
    rules_sha256: &str,
    catalog_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Lore,
        &format!("advancement.study_grant.{}", grant.class_id),
        &format!("Advancement Study Grant: {}", grant.class_id),
        "study_grants",
        json!({
            "schema": RPG_ADVANCEMENT_SCHEMA,
            "source_issue": 962,
            "content_migration_issue": 1455,
            "surface_kind": "advancement_study_grant",
            "rules_id": ADVANCEMENT_RULES_ID,
            "class_id": grant.class_id,
            "max_level": grant.max_level,
            "points_per_level": grant.points_per_level,
            "milestone_level": grant.milestone_level,
            "milestone_points": grant.milestone_points,
            "rules_sha256": rules_sha256,
            "ability_catalog_sha256": catalog_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn study_cost_record(
    cost: &StudyAbilityCost,
    rules_sha256: &str,
    catalog_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Ability,
        &format!("advancement.study_cost.{}", cost.ability_id),
        &format!("Advancement Study Cost: {}", cost.ability_id),
        "study_costs",
        json!({
            "schema": RPG_ADVANCEMENT_SCHEMA,
            "source_issue": 962,
            "content_migration_issue": 1455,
            "surface_kind": "advancement_study_cost",
            "rules_id": ADVANCEMENT_RULES_ID,
            "ability_id": cost.ability_id,
            "points": cost.points,
            "rules_sha256": rules_sha256,
            "ability_catalog_sha256": catalog_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn validate_study_costs_match_catalog(
    rules: &AdvancementRules,
    catalog: &AbilityCatalog,
) -> Result<(), EngineError> {
    for cost in &rules.study_costs {
        if !catalog
            .abilities
            .iter()
            .any(|ability| ability.ability_id == cost.ability_id)
        {
            return Err(EngineError::RpgAdvancementInvalidConfig {
                field: "study_costs.ability_id".to_string(),
                reason: format!("missing ability {} in catalog", cost.ability_id),
            });
        }
    }
    Ok(())
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
            path: format!("editor://rpg/ashen_reach/advancement/{collection}/{id}"),
            author: "editor_authoring.issue1455.advancement".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
