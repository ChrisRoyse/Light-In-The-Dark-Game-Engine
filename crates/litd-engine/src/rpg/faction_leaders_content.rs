use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    FactionLeaderActionKind, FactionLeaderCatalog, FactionLeaderRules,
    FactionOrganizationDefinition, FactionRankDefinition, RPG_FACTION_LEADER_SCHEMA,
    faction_leaders_validate::{validate_catalog, validate_faction_rules},
    rpg_authored_payload_without_nulls, validate_rpg_authored_content_records,
};

const FACTION_RULES_ID: &str = "faction_leader.rules.mud_defaults";

pub fn faction_leader_content_records(
    catalog: &FactionLeaderCatalog,
    rules: &FactionLeaderRules,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_faction_rules(rules)?;
    validate_catalog(catalog)?;
    let catalog_sha256 = serialize_sha256(catalog)?;
    let rules_sha256 = serialize_sha256(rules)?;
    let mut records = vec![
        rules_record(rules, catalog, &catalog_sha256, &rules_sha256)?,
        catalog_record(catalog, &catalog_sha256, &rules_sha256)?,
    ];

    for organization in &catalog.organizations {
        records.push(organization_record(
            organization,
            catalog,
            &catalog_sha256,
            &rules_sha256,
        )?);
        for rank in &organization.ranks {
            records.push(rank_record(
                organization,
                rank,
                catalog,
                &catalog_sha256,
                &rules_sha256,
            )?);
        }
        for action in &organization.allowed_actions {
            records.push(action_policy_record(
                organization,
                *action,
                catalog,
                &catalog_sha256,
                &rules_sha256,
            )?);
        }
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgFactionInvalidConfig {
            field: "faction_leaders.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn rules_record(
    rules: &FactionLeaderRules,
    catalog: &FactionLeaderCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Trigger,
        FACTION_RULES_ID,
        "Faction Leader Rules: MUD Defaults",
        "rules",
        json!({
            "schema": RPG_FACTION_LEADER_SCHEMA,
            "source_issue": 966,
            "content_migration_issue": 1455,
            "surface_kind": "faction_leader_rules",
            "catalog_id": catalog.catalog_id,
            "min_signal_strength_milli": rules.min_signal_strength_milli,
            "min_rp_quality_milli": rules.min_rp_quality_milli,
            "min_loyalty_milli": rules.min_loyalty_milli,
            "min_warfare_pressure_milli": rules.min_warfare_pressure_milli,
            "max_rank": rules.max_rank,
            "faction_catalog_sha256": catalog_sha256,
            "faction_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn catalog_record(
    catalog: &FactionLeaderCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Agent,
        &format!("{}.summary", catalog.catalog_id),
        "Faction Leader Catalog: Ashen Reach",
        "catalog",
        json!({
            "schema": catalog.schema,
            "source_issue": 966,
            "content_migration_issue": 1455,
            "surface_kind": "faction_leader_catalog",
            "catalog_id": catalog.catalog_id,
            "organization_count": catalog.organizations.len(),
            "organization_ids": catalog
                .organizations
                .iter()
                .map(|organization| organization.organization_id.as_str())
                .collect::<Vec<_>>(),
            "faction_catalog_sha256": catalog_sha256,
            "faction_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn organization_record(
    organization: &FactionOrganizationDefinition,
    catalog: &FactionLeaderCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Agent,
        &organization.organization_id,
        &format!("Faction Leader Organization: {}", organization.display_name),
        "organizations",
        json!({
            "schema": catalog.schema,
            "source_issue": 966,
            "content_migration_issue": 1455,
            "surface_kind": "faction_leader_organization",
            "catalog_id": catalog.catalog_id,
            "organization_id": organization.organization_id,
            "display_name": organization.display_name,
            "organization_kind": organization.organization_kind,
            "leader_agent_id": organization.leader_agent_id,
            "leader_display_name": organization.leader_display_name,
            "religion_id": organization.religion_id,
            "home_region_id": organization.home_region_id,
            "item_of_power_id": organization.item_of_power_id,
            "allowed_actions": organization.allowed_actions,
            "ward_tags": organization.ward_tags,
            "required_theme_tags": organization.required_theme_tags,
            "forbidden_theme_tags": organization.forbidden_theme_tags,
            "rank_count": organization.ranks.len(),
            "enemy_organization_ids": organization.enemy_organization_ids,
            "organization_sha256": serialize_sha256(organization)?,
            "faction_catalog_sha256": catalog_sha256,
            "faction_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn rank_record(
    organization: &FactionOrganizationDefinition,
    rank: &FactionRankDefinition,
    catalog: &FactionLeaderCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    let id = format!("{}.rank.{}", organization.organization_id, rank.rank);
    content_record(
        EngineContentKind::Lore,
        &id,
        &format!("Faction Rank: {} {}", organization.display_name, rank.title),
        "ranks",
        json!({
            "schema": catalog.schema,
            "source_issue": 966,
            "content_migration_issue": 1455,
            "surface_kind": "faction_rank",
            "catalog_id": catalog.catalog_id,
            "organization_id": organization.organization_id,
            "display_name": organization.display_name,
            "rank": rank.rank,
            "title": rank.title,
            "min_rp_quality_milli": rank.min_rp_quality_milli,
            "min_loyalty_milli": rank.min_loyalty_milli,
            "power_ids": rank.power_ids,
            "rank_sha256": serialize_sha256(rank)?,
            "faction_catalog_sha256": catalog_sha256,
            "faction_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn action_policy_record(
    organization: &FactionOrganizationDefinition,
    action: FactionLeaderActionKind,
    catalog: &FactionLeaderCatalog,
    catalog_sha256: &str,
    rules_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    let id = format!(
        "{}.action.{}",
        organization.organization_id,
        action.as_str()
    );
    content_record(
        EngineContentKind::Trigger,
        &id,
        &format!(
            "Faction Leader Action Policy: {} {}",
            organization.display_name,
            action.as_str()
        ),
        "action_policies",
        json!({
            "schema": catalog.schema,
            "source_issue": 966,
            "content_migration_issue": 1455,
            "surface_kind": "faction_leader_action_policy",
            "catalog_id": catalog.catalog_id,
            "organization_id": organization.organization_id,
            "display_name": organization.display_name,
            "leader_agent_id": organization.leader_agent_id,
            "action_kind": action,
            "accepted_signal_kinds": accepted_signal_kinds(action),
            "ward_tags": organization.ward_tags,
            "required_theme_tags": organization.required_theme_tags,
            "forbidden_theme_tags": organization.forbidden_theme_tags,
            "enemy_organization_ids": organization.enemy_organization_ids,
            "faction_catalog_sha256": catalog_sha256,
            "faction_rules_sha256": rules_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn accepted_signal_kinds(action: FactionLeaderActionKind) -> &'static [&'static str] {
    match action {
        FactionLeaderActionKind::Recruit => &["application"],
        FactionLeaderActionKind::Induct => &["sponsor_support"],
        FactionLeaderActionKind::SetObjective => &["world_threat"],
        FactionLeaderActionKind::WageWarfare => &["item_of_power_threat"],
        FactionLeaderActionKind::RewardLoyalty => &["loyalty_report"],
        FactionLeaderActionKind::AdvanceRank => &["rank_review"],
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
            path: format!("editor://rpg/ashen_reach/faction_leaders/{collection}/{id}"),
            author: "editor_authoring.issue1455.faction_leaders".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
