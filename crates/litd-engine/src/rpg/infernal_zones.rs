use std::collections::BTreeSet;

use super::serialize_sha256;

use crate::EngineError;

use super::{
    InfernalAttritionPolicy, InfernalCircleId, InfernalCircleProfile, InfernalRepopProfile,
    InfernalResetProfile, InfernalZoneChainStep, InfernalZoneExit, InfernalZoneExitKind,
    InfernalZoneNode, InfernalZonePolicy, InfernalZoneTopologyReadback, RPG_INFERNAL_ZONES_SCHEMA,
    infernal_attrition_policy_hash, validate_infernal_attrition_policy,
};

const EXPECTED_CHAIN: [InfernalCircleId; 7] = [
    InfernalCircleId::Threshold,
    InfernalCircleId::EmberWastes,
    InfernalCircleId::IronBastion,
    InfernalCircleId::HungeringDark,
    InfernalCircleId::WailingTombs,
    InfernalCircleId::MoltenThroneRoad,
    InfernalCircleId::ObsidianThrone,
];

pub fn derive_infernal_zone_topology(
    policy: &InfernalZonePolicy,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<InfernalZoneTopologyReadback, EngineError> {
    validate_infernal_zone_policy(policy, attrition_policy)?;
    let policy_sha256 = infernal_zone_policy_hash(policy)?;
    let attrition_policy_sha256 = infernal_attrition_policy_hash(attrition_policy)?;
    let mut chain = Vec::with_capacity(policy.zones.len());
    for expected in EXPECTED_CHAIN {
        let zone = zone_for_circle(policy, expected)?;
        let attrition = attrition_profile(attrition_policy, expected)?;
        chain.push(InfernalZoneChainStep {
            depth: zone.depth,
            circle_id: zone.circle_id,
            zone_id: zone.zone_id.clone(),
            display_name: zone.display_name.clone(),
            hazard_profile_id: zone.hazard_profile_id.clone(),
            hazard_kind: zone.hazard_kind,
            attrition_health_damage_per_tick: attrition.health_damage_per_tick,
            attrition_constitution_strain_per_tick: attrition.constitution_strain_per_tick,
            mob_population_count: zone.mob_population_ids.len(),
            ruler_ref: zone.ruler_ref.clone(),
            reset_id: zone.reset_profile.reset_id.clone(),
            repop_id: zone.repop_profile.repop_id.clone(),
            descends_to: zone.exits.first().map(|exit| exit.to_circle_id),
        });
    }
    let mut readback = InfernalZoneTopologyReadback {
        schema: RPG_INFERNAL_ZONES_SCHEMA.to_string(),
        server_policy_id: policy.server_policy_id.clone(),
        source_issue: policy.source_issue,
        zone_count: policy.zones.len(),
        built_in_map_required: policy.built_in_map_required,
        entry_circle: policy.entry_circle,
        terminal_circle: policy.terminal_circle,
        chain,
        wailing_tombs_reagent_source: zone_for_circle(policy, InfernalCircleId::WailingTombs)?
            .withering_reagent_source,
        throne_road_relic_vault_source: zone_for_circle(
            policy,
            InfernalCircleId::MoltenThroneRoad,
        )?
        .relic_vault_source,
        policy_sha256,
        attrition_policy_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&topology_fingerprint(&readback))?;
    Ok(readback)
}

pub fn validate_infernal_zone_policy(
    policy: &InfernalZonePolicy,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<(), EngineError> {
    validate_infernal_attrition_policy(attrition_policy)?;
    if policy.schema != RPG_INFERNAL_ZONES_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_INFERNAL_ZONES_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1067 {
        return invalid("policy.source_issue", "must point at issue 1067");
    }
    if policy.built_in_map_required {
        return invalid(
            "policy.built_in_map_required",
            "Infernal zones cannot require built-in map or minimap access",
        );
    }
    if policy.entry_circle != InfernalCircleId::Threshold {
        return invalid("policy.entry_circle", "entry must start at Threshold");
    }
    if policy.terminal_circle != InfernalCircleId::ObsidianThrone {
        return invalid(
            "policy.terminal_circle",
            "terminal circle must be Obsidian Throne",
        );
    }
    if policy.zones.len() != EXPECTED_CHAIN.len() {
        return invalid("policy.zones", "all seven Infernal circles are required");
    }

    let mut seen_circles = BTreeSet::new();
    let mut seen_depths = BTreeSet::new();
    let mut seen_zone_ids = BTreeSet::new();
    let mut seen_hazards = BTreeSet::new();
    for zone in &policy.zones {
        validate_zone_node(zone, attrition_policy)?;
        if !seen_circles.insert(zone.circle_id) {
            return invalid(
                "policy.zones.circle_id",
                format!("duplicate {}", zone.circle_id.label()),
            );
        }
        if !seen_depths.insert(zone.depth) {
            return invalid(
                "policy.zones.depth",
                format!("duplicate depth {}", zone.depth),
            );
        }
        if !seen_zone_ids.insert(zone.zone_id.as_str()) {
            return invalid(
                "policy.zones.zone_id",
                format!("duplicate {}", zone.zone_id),
            );
        }
        if !seen_hazards.insert(zone.hazard_profile_id.as_str()) {
            return invalid(
                "policy.zones.hazard_profile_id",
                format!("duplicate {}", zone.hazard_profile_id),
            );
        }
    }

    for (index, expected) in EXPECTED_CHAIN.iter().copied().enumerate() {
        let zone = policy.zones.get(index).ok_or_else(|| {
            invalid_error("policy.zones", format!("missing {}", expected.label()))
        })?;
        if zone.circle_id != expected {
            return invalid(
                "policy.zones.circle_id",
                format!(
                    "depth {} must be {}, got {}",
                    index + 1,
                    expected.label(),
                    zone.circle_id.label()
                ),
            );
        }
        let expected_depth = u8::try_from(index + 1).unwrap();
        if zone.depth != expected_depth {
            return invalid(
                "policy.zones.depth",
                format!(
                    "{} must be depth {}, got {}",
                    zone.circle_id.label(),
                    expected_depth,
                    zone.depth
                ),
            );
        }
        validate_zone_exit_chain(policy, index, zone)?;
    }
    validate_special_sources(policy)?;
    Ok(())
}

pub fn zone_for_circle(
    policy: &InfernalZonePolicy,
    circle_id: InfernalCircleId,
) -> Result<&InfernalZoneNode, EngineError> {
    policy
        .zones
        .iter()
        .find(|zone| zone.circle_id == circle_id)
        .ok_or_else(|| {
            invalid_error(
                "policy.zones.circle_id",
                format!("missing {}", circle_id.label()),
            )
        })
}

pub fn infernal_zone_policy_hash(policy: &InfernalZonePolicy) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn validate_zone_node(
    zone: &InfernalZoneNode,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<(), EngineError> {
    validate_token("policy.zones.zone_id", &zone.zone_id)?;
    validate_display_name(&zone.display_name)?;
    if !(1..=7).contains(&zone.depth) {
        return invalid("policy.zones.depth", "must be inside 1..=7");
    }
    validate_token("policy.zones.hazard_profile_id", &zone.hazard_profile_id)?;
    let attrition = attrition_profile(attrition_policy, zone.circle_id)?;
    if zone.hazard_kind != attrition.hazard {
        return invalid(
            "policy.zones.hazard_kind",
            format!(
                "{} must match attrition hazard {:?}",
                zone.circle_id.label(),
                attrition.hazard
            ),
        );
    }
    validate_population(zone)?;
    validate_token("policy.zones.ruler_ref", &zone.ruler_ref)?;
    validate_reset_profile(&zone.reset_profile)?;
    validate_repop_profile(&zone.repop_profile)?;
    for exit in &zone.exits {
        validate_exit(exit)?;
    }
    Ok(())
}

fn validate_population(zone: &InfernalZoneNode) -> Result<(), EngineError> {
    if zone.mob_population_ids.is_empty() || zone.mob_population_ids.len() > 12 {
        return invalid(
            "policy.zones.mob_population_ids",
            "must include 1..=12 mob population ids",
        );
    }
    let mut seen = BTreeSet::new();
    for mob_id in &zone.mob_population_ids {
        validate_token("policy.zones.mob_population_ids", mob_id)?;
        if !seen.insert(mob_id.as_str()) {
            return invalid(
                "policy.zones.mob_population_ids",
                format!("duplicate {}", mob_id),
            );
        }
    }
    Ok(())
}

fn validate_reset_profile(profile: &InfernalResetProfile) -> Result<(), EngineError> {
    validate_token("policy.zones.reset_profile.reset_id", &profile.reset_id)?;
    if !(5..=240).contains(&profile.interval_minutes) {
        return invalid(
            "policy.zones.reset_profile.interval_minutes",
            "must be inside 5..=240",
        );
    }
    if !profile.resets_hazard_emitters {
        return invalid(
            "policy.zones.reset_profile.resets_hazard_emitters",
            "resets must restore per-circle hazards",
        );
    }
    Ok(())
}

fn validate_repop_profile(profile: &InfernalRepopProfile) -> Result<(), EngineError> {
    validate_token("policy.zones.repop_profile.repop_id", &profile.repop_id)?;
    if !(1..=500).contains(&profile.baseline_mob_budget) {
        return invalid(
            "policy.zones.repop_profile.baseline_mob_budget",
            "must be inside 1..=500",
        );
    }
    if !(1..=240).contains(&profile.repop_interval_minutes) {
        return invalid(
            "policy.zones.repop_profile.repop_interval_minutes",
            "must be inside 1..=240",
        );
    }
    if !(1..=10).contains(&profile.max_pressure_tier) {
        return invalid(
            "policy.zones.repop_profile.max_pressure_tier",
            "must be inside 1..=10",
        );
    }
    Ok(())
}

fn validate_exit(exit: &InfernalZoneExit) -> Result<(), EngineError> {
    validate_token("policy.zones.exits.exit_id", &exit.exit_id)?;
    if exit.kind != InfernalZoneExitKind::Descend {
        return invalid("policy.zones.exits.kind", "only descend exits are valid");
    }
    if let Some(lock) = &exit.locked_by_ref {
        validate_token("policy.zones.exits.locked_by_ref", lock)?;
    }
    Ok(())
}

fn validate_zone_exit_chain(
    policy: &InfernalZonePolicy,
    index: usize,
    zone: &InfernalZoneNode,
) -> Result<(), EngineError> {
    if index + 1 == EXPECTED_CHAIN.len() {
        if !zone.exits.is_empty() {
            return invalid(
                "policy.zones.exits",
                "Obsidian Throne must be the terminal circle",
            );
        }
        return Ok(());
    }
    if zone.exits.len() != 1 {
        return invalid(
            "policy.zones.exits",
            format!(
                "{} must have exactly one descending exit",
                zone.circle_id.label()
            ),
        );
    }
    let expected_next = EXPECTED_CHAIN[index + 1];
    let exit = &zone.exits[0];
    if exit.to_circle_id != expected_next {
        return invalid(
            "policy.zones.exits.to_circle_id",
            format!(
                "{} must descend to {}, got {}",
                zone.circle_id.label(),
                expected_next.label(),
                exit.to_circle_id.label()
            ),
        );
    }
    zone_for_circle(policy, expected_next)?;
    Ok(())
}

fn validate_special_sources(policy: &InfernalZonePolicy) -> Result<(), EngineError> {
    for zone in &policy.zones {
        if zone.withering_reagent_source != (zone.circle_id == InfernalCircleId::WailingTombs) {
            return invalid(
                "policy.zones.withering_reagent_source",
                "only Wailing Tombs may hold rare undeath reagents",
            );
        }
        if zone.relic_vault_source != (zone.circle_id == InfernalCircleId::MoltenThroneRoad) {
            return invalid(
                "policy.zones.relic_vault_source",
                "only Molten Throne-road may expose the relic vault source",
            );
        }
    }
    Ok(())
}

fn attrition_profile(
    policy: &InfernalAttritionPolicy,
    circle_id: InfernalCircleId,
) -> Result<&InfernalCircleProfile, EngineError> {
    policy
        .circles
        .iter()
        .find(|profile| profile.circle_id == circle_id)
        .ok_or_else(|| {
            invalid_error(
                "attrition_policy.circles",
                format!("missing {}", circle_id.label()),
            )
        })
}

fn validate_display_name(value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 128 {
        return invalid(
            "policy.zones.display_name",
            "must be non-empty and <= 128 bytes",
        );
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

fn topology_fingerprint(readback: &InfernalZoneTopologyReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "server_policy_id": readback.server_policy_id,
        "source_issue": readback.source_issue,
        "zone_count": readback.zone_count,
        "built_in_map_required": readback.built_in_map_required,
        "entry_circle": readback.entry_circle,
        "terminal_circle": readback.terminal_circle,
        "chain": readback.chain,
        "wailing_tombs_reagent_source": readback.wailing_tombs_reagent_source,
        "throne_road_relic_vault_source": readback.throne_road_relic_vault_source,
        "policy_sha256": readback.policy_sha256,
        "attrition_policy_sha256": readback.attrition_policy_sha256,
    })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("infernal_zones.{}", field.into()),
        reason: reason.to_string(),
    }
}
