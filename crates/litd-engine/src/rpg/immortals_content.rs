use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    ImmortalActionKind, ImmortalCatalog, ImmortalDefinition, ImmortalRules, RPG_IMMORTAL_SCHEMA,
    immortals_validate::{validate_immortal_catalog, validate_immortal_rules},
    rpg_authored_payload_without_nulls, validate_rpg_authored_content_records,
};

const IMMORTAL_RULES_ID: &str = "immortal.rules.mud_defaults";

pub fn immortal_content_records(
    catalog: &ImmortalCatalog,
    rules: &ImmortalRules,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_immortal_rules(rules)?;
    validate_immortal_catalog(catalog)?;
    let catalog_sha256 = serialize_sha256(catalog)?;
    let rules_sha256 = serialize_sha256(rules)?;
    let mut records = vec![
        rules_record(rules, catalog, &catalog_sha256, &rules_sha256)?,
        catalog_record(catalog, &catalog_sha256, &rules_sha256)?,
    ];

    for immortal in &catalog.immortals {
        records.push(immortal_record(
            immortal,
            catalog,
            &catalog_sha256,
            &rules_sha256,
        )?);
        for action in &immortal.allowed_actions {
            records.push(action_policy_record(
                immortal,
                *action,
                catalog,
                &catalog_sha256,
                &rules_sha256,
            )?);
        }
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgImmortalInvalidConfig {
            field: "immortals.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn rules_record(
    rules: &ImmortalRules,
    catalog: &ImmortalCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Trigger,
        IMMORTAL_RULES_ID,
        "Immortal Rules: MUD Defaults",
        "rules",
        json!({
            "schema": RPG_IMMORTAL_SCHEMA,
            "source_issue": 965,
            "content_migration_issue": 1455,
            "surface_kind": "immortal_rules",
            "catalog_id": catalog.catalog_id,
            "min_signal_strength_milli": rules.min_signal_strength_milli,
            "min_rp_quality_milli": rules.min_rp_quality_milli,
            "max_empowerment_tier": rules.max_empowerment_tier,
            "immortal_catalog_sha256": catalog_sha256,
            "immortal_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn catalog_record(
    catalog: &ImmortalCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Agent,
        &format!("{}.summary", catalog.catalog_id),
        "Immortal Catalog: Ashen Reach",
        "catalog",
        json!({
            "schema": catalog.schema,
            "source_issue": 965,
            "content_migration_issue": 1455,
            "surface_kind": "immortal_catalog",
            "catalog_id": catalog.catalog_id,
            "immortal_count": catalog.immortals.len(),
            "immortal_ids": catalog
                .immortals
                .iter()
                .map(|immortal| immortal.immortal_id.as_str())
                .collect::<Vec<_>>(),
            "immortal_catalog_sha256": catalog_sha256,
            "immortal_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn immortal_record(
    immortal: &ImmortalDefinition,
    catalog: &ImmortalCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Agent,
        &immortal.immortal_id,
        &format!("Immortal Agent: {}", immortal.display_name),
        "immortals",
        json!({
            "schema": catalog.schema,
            "source_issue": 965,
            "content_migration_issue": 1455,
            "surface_kind": "immortal_definition",
            "catalog_id": catalog.catalog_id,
            "immortal_id": immortal.immortal_id,
            "display_name": immortal.display_name,
            "religion_id": immortal.religion_id,
            "allowed_actions": immortal.allowed_actions,
            "ward_tags": immortal.ward_tags,
            "lowered_agent_id": immortal.lowered_agent_id,
            "max_empowerment_tier": immortal.max_empowerment_tier,
            "immortal_definition_sha256": serialize_sha256(immortal)?,
            "immortal_catalog_sha256": catalog_sha256,
            "immortal_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn action_policy_record(
    immortal: &ImmortalDefinition,
    action: ImmortalActionKind,
    catalog: &ImmortalCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    let id = format!("{}.action.{}", immortal.immortal_id, action.as_str());
    content_record(
        EngineContentKind::Trigger,
        &id,
        &format!(
            "Immortal Action Policy: {} {}",
            immortal.display_name,
            action.as_str()
        ),
        "action_policies",
        json!({
            "schema": catalog.schema,
            "source_issue": 965,
            "content_migration_issue": 1455,
            "surface_kind": "immortal_action_policy",
            "catalog_id": catalog.catalog_id,
            "immortal_id": immortal.immortal_id,
            "display_name": immortal.display_name,
            "religion_id": immortal.religion_id,
            "action_kind": action,
            "accepted_signal_kinds": accepted_signal_kinds(action),
            "ward_tags": immortal.ward_tags,
            "lowered_agent_id": immortal.lowered_agent_id,
            "max_empowerment_tier": immortal.max_empowerment_tier,
            "immortal_catalog_sha256": catalog_sha256,
            "immortal_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn accepted_signal_kinds(action: ImmortalActionKind) -> &'static [&'static str] {
    match action {
        ImmortalActionKind::GrantEmpowerment => &["prayer", "shrine_visit"],
        ImmortalActionKind::IssueQuest => &["quest_opportunity", "roleplay_scene"],
        ImmortalActionKind::RunEvent => &["event_opportunity", "roleplay_scene"],
        ImmortalActionKind::EnforceRoleplay => &["rule_break"],
        ImmortalActionKind::PossessMob => &["possession_request"],
    }
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
            path: format!("editor://rpg/ashen_reach/immortals/{collection}/{id}"),
            author: "editor_authoring.issue1455.immortals".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
