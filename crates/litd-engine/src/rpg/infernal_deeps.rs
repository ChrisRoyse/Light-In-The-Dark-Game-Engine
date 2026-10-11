use std::collections::BTreeSet;

use super::serialize_sha256;
use serde_json::json;

use crate::EngineError;

use super::{
    InfernalAccessPolicy, InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalCircleId,
    InfernalDeepsChildHashes, InfernalDeepsPackagePolicy, InfernalDeepsPackageReadback,
    InfernalRelicVaultPolicy, InfernalZonePolicy, RPG_INFERNAL_DEEPS_SCHEMA, UndeathPolicy,
    WitheringRaidPathPolicy, derive_infernal_boss_agent_hierarchy,
    derive_infernal_relic_vault_readback, derive_infernal_zone_topology,
    derive_withering_raid_path_readback, infernal_access_policy_hash,
    infernal_attrition_policy_hash, validate_infernal_access_policy,
};

const REQUIRED_CHILD_ISSUES: [u32; 6] = [1066, 1070, 1067, 1068, 1069, 1064];

pub struct DeriveInfernalDeepsPackageArgs<'a> {
    pub package: &'a InfernalDeepsPackagePolicy,
    pub access: &'a InfernalAccessPolicy,
    pub attrition: &'a InfernalAttritionPolicy,
    pub zones: &'a InfernalZonePolicy,
    pub bosses: &'a InfernalBossAgentPolicy,
    pub relics: &'a InfernalRelicVaultPolicy,
    pub withering: &'a WitheringRaidPathPolicy,
    pub undeath: &'a UndeathPolicy,
}

pub fn derive_infernal_deeps_package(
    args: DeriveInfernalDeepsPackageArgs<'_>,
) -> Result<InfernalDeepsPackageReadback, EngineError> {
    let DeriveInfernalDeepsPackageArgs {
        package,
        access,
        attrition,
        zones,
        bosses,
        relics,
        withering,
        undeath,
    } = args;
    validate_infernal_deeps_package_policy(package)?;
    validate_infernal_access_policy(access)?;
    let topology = derive_infernal_zone_topology(zones, attrition)?;
    let hierarchy = derive_infernal_boss_agent_hierarchy(bosses, zones, attrition)?;
    let vaults = derive_infernal_relic_vault_readback(relics, zones, bosses, attrition)?;
    let raid = derive_withering_raid_path_readback(withering, undeath, zones, bosses, attrition)?;
    validate_cross_child_contract(access, &topology, &hierarchy, &vaults, &raid)?;
    let child_hashes = InfernalDeepsChildHashes {
        access_policy_sha256: infernal_access_policy_hash(access)?,
        attrition_policy_sha256: infernal_attrition_policy_hash(attrition)?,
        zone_topology_sha256: topology.result_sha256.clone(),
        boss_hierarchy_sha256: hierarchy.result_sha256.clone(),
        relic_vaults_sha256: vaults.result_sha256.clone(),
        withering_raid_path_sha256: raid.result_sha256.clone(),
    };
    let mut readback = InfernalDeepsPackageReadback {
        schema: RPG_INFERNAL_DEEPS_SCHEMA.to_string(),
        package_id: package.package_id.clone(),
        source_issue: package.source_issue,
        display_name: package.display_name.clone(),
        ordered_hell_not_outer_dark: package.ordered_hell_not_outer_dark,
        group_required: package.group_required,
        no_recall_inside: access.no_recall_inside,
        entry_circle: topology.entry_circle,
        terminal_circle: topology.terminal_circle,
        access_path_count: access.entry_rules.len(),
        denied_escape_kind_count: access.denied_escape_kinds.len(),
        zone_count: topology.zone_count,
        hazard_kind_count: topology
            .chain
            .iter()
            .map(|step| step.hazard_kind)
            .collect::<BTreeSet<_>>()
            .len(),
        boss_count: hierarchy.boss_count,
        apex_agent_id: hierarchy.apex_agent_id.clone(),
        relic_vault_count: vaults.vault_count,
        hellforged_item_count: vaults.item_count,
        world_finite_item_total: vaults.world_finite_item_total,
        highest_power_rating_milli: vaults.highest_power_rating_milli,
        withering_raid_phase_count: raid.phase_count,
        withering_minimum_party_size: raid.minimum_party_size,
        withering_minimum_character_level: raid.minimum_character_level,
        wailing_tombs_reagent_source: topology.wailing_tombs_reagent_source
            && raid.source_zone_has_reagents,
        throne_road_relic_vault_source: topology.throne_road_relic_vault_source,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        child_hashes,
        policy_sha256: infernal_deeps_package_policy_hash(package)?,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&readback_fingerprint(&readback))?;
    validate_infernal_deeps_readback(package, &readback)?;
    Ok(readback)
}

pub fn validate_infernal_deeps_package_policy(
    package: &InfernalDeepsPackagePolicy,
) -> Result<(), EngineError> {
    if package.schema != RPG_INFERNAL_DEEPS_SCHEMA {
        return invalid(
            "package.schema",
            format!("expected {RPG_INFERNAL_DEEPS_SCHEMA}"),
        );
    }
    validate_token("package.package_id", &package.package_id)?;
    validate_display("package.display_name", &package.display_name)?;
    if package.source_issue != 1063 {
        return invalid("package.source_issue", "must point at issue 1063");
    }
    if !package.ordered_hell_not_outer_dark || !package.group_required || !package.no_recall_inside
    {
        return invalid(
            "package.flags",
            "Infernal Deeps must be ordered hell, group-required, and no-recall",
        );
    }
    if package.built_in_map_required || package.grants_map_or_minimap || package.grants_gps_markers
    {
        return invalid("package.map_flags", "package cannot grant map/GPS");
    }
    if package.required_child_issues != REQUIRED_CHILD_ISSUES {
        return invalid(
            "package.required_child_issues",
            "must list the six verified child issues in dependency order",
        );
    }
    Ok(())
}

pub fn validate_infernal_deeps_readback(
    package: &InfernalDeepsPackagePolicy,
    readback: &InfernalDeepsPackageReadback,
) -> Result<(), EngineError> {
    validate_infernal_deeps_package_policy(package)?;
    if readback.schema != RPG_INFERNAL_DEEPS_SCHEMA {
        return invalid(
            "readback.schema",
            format!("expected {RPG_INFERNAL_DEEPS_SCHEMA}"),
        );
    }
    if readback.package_id != package.package_id || readback.source_issue != 1063 {
        return invalid(
            "readback.identity",
            "must match package policy and issue 1063",
        );
    }
    if readback.policy_sha256 != infernal_deeps_package_policy_hash(package)? {
        return invalid("readback.policy_sha256", "does not match package policy");
    }
    if readback.entry_circle != InfernalCircleId::Threshold
        || readback.terminal_circle != InfernalCircleId::ObsidianThrone
        || readback.access_path_count != 5
        || readback.denied_escape_kind_count != 5
        || readback.zone_count != 7
        || readback.hazard_kind_count != 3
        || readback.boss_count != 7
        || readback.relic_vault_count != 2
        || readback.hellforged_item_count != 6
        || readback.world_finite_item_total != 6
        || readback.highest_power_rating_milli != 1000
        || readback.withering_raid_phase_count != 6
        || readback.withering_minimum_party_size < 8
        || !readback.wailing_tombs_reagent_source
        || !readback.throne_road_relic_vault_source
    {
        return invalid(
            "readback.coverage",
            "does not satisfy parent Infernal Deeps scope",
        );
    }
    if !readback.no_recall_inside
        || readback.built_in_map_required
        || readback.grants_map_or_minimap
        || readback.grants_gps_markers
    {
        return invalid(
            "readback.flags",
            "readback cannot allow recall or map/GPS grants",
        );
    }
    validate_hash(
        "child_hashes.access_policy_sha256",
        &readback.child_hashes.access_policy_sha256,
    )?;
    validate_hash(
        "child_hashes.attrition_policy_sha256",
        &readback.child_hashes.attrition_policy_sha256,
    )?;
    validate_hash(
        "child_hashes.zone_topology_sha256",
        &readback.child_hashes.zone_topology_sha256,
    )?;
    validate_hash(
        "child_hashes.boss_hierarchy_sha256",
        &readback.child_hashes.boss_hierarchy_sha256,
    )?;
    validate_hash(
        "child_hashes.relic_vaults_sha256",
        &readback.child_hashes.relic_vaults_sha256,
    )?;
    validate_hash(
        "child_hashes.withering_raid_path_sha256",
        &readback.child_hashes.withering_raid_path_sha256,
    )?;
    Ok(())
}

pub fn infernal_deeps_package_policy_hash(
    package: &InfernalDeepsPackagePolicy,
) -> Result<String, EngineError> {
    serialize_sha256(package)
}

fn validate_cross_child_contract(
    access: &InfernalAccessPolicy,
    topology: &super::InfernalZoneTopologyReadback,
    hierarchy: &super::InfernalBossAgentHierarchyReadback,
    vaults: &super::InfernalRelicVaultReadback,
    raid: &super::WitheringRaidPathReadback,
) -> Result<(), EngineError> {
    if !access.no_recall_inside
        || access.destination_circle != topology.entry_circle
        || topology.built_in_map_required
        || hierarchy.built_in_map_required
        || vaults.built_in_map_required
        || raid.built_in_map_required
    {
        return invalid(
            "children.flags",
            "child contracts must preserve no-recall and no built-in maps",
        );
    }
    if hierarchy.apex_agent_id != "agent.infernal.obsidian_throne.maldrithax" {
        return invalid(
            "children.apex_agent_id",
            "Maldrithax must remain apex agent",
        );
    }
    if raid.minimum_party_size < 8 || !raid.source_zone_has_reagents {
        return invalid(
            "children.withering",
            "Withering raid must remain elite and Wailing-Tombs sourced",
        );
    }
    Ok(())
}

fn readback_fingerprint(readback: &InfernalDeepsPackageReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "package_id": readback.package_id,
        "source_issue": readback.source_issue,
        "ordered_hell_not_outer_dark": readback.ordered_hell_not_outer_dark,
        "group_required": readback.group_required,
        "no_recall_inside": readback.no_recall_inside,
        "entry_circle": readback.entry_circle,
        "terminal_circle": readback.terminal_circle,
        "access_path_count": readback.access_path_count,
        "denied_escape_kind_count": readback.denied_escape_kind_count,
        "zone_count": readback.zone_count,
        "hazard_kind_count": readback.hazard_kind_count,
        "boss_count": readback.boss_count,
        "apex_agent_id": readback.apex_agent_id,
        "relic_vault_count": readback.relic_vault_count,
        "hellforged_item_count": readback.hellforged_item_count,
        "world_finite_item_total": readback.world_finite_item_total,
        "highest_power_rating_milli": readback.highest_power_rating_milli,
        "withering_raid_phase_count": readback.withering_raid_phase_count,
        "wailing_tombs_reagent_source": readback.wailing_tombs_reagent_source,
        "throne_road_relic_vault_source": readback.throne_road_relic_vault_source,
        "built_in_map_required": readback.built_in_map_required,
        "child_hashes": readback.child_hashes,
        "policy_sha256": readback.policy_sha256,
    })
}

fn validate_hash(field: &str, value: &str) -> Result<(), EngineError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid(field, "must be a SHA-256 hex digest");
    }
    Ok(())
}

fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 128 {
        return invalid(field, "must be non-empty and <= 128 bytes");
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
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

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: format!("infernal_deeps.{}", field.into()),
        reason: reason.to_string(),
    })
}
