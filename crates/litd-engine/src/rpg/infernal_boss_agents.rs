use std::collections::BTreeSet;

use super::serialize_sha256;
use serde_json::json;

use crate::{EngineError, calyx_agent_runtime::ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY};

use super::{
    InfernalAttritionPolicy, InfernalBossAgentHierarchyReadback, InfernalBossAgentPolicy,
    InfernalBossAgentReadback, InfernalBossAgentRole, InfernalBossAgentSpec, InfernalCircleId,
    InfernalZoneNode, InfernalZonePolicy, RPG_INFERNAL_BOSS_AGENTS_SCHEMA,
    infernal_zone_policy_hash, validate_infernal_zone_policy, zone_for_circle,
};

pub(super) const EXPECTED_BOSSES: [InfernalCircleId; 7] = [
    InfernalCircleId::Threshold,
    InfernalCircleId::EmberWastes,
    InfernalCircleId::IronBastion,
    InfernalCircleId::HungeringDark,
    InfernalCircleId::WailingTombs,
    InfernalCircleId::MoltenThroneRoad,
    InfernalCircleId::ObsidianThrone,
];

pub fn derive_infernal_boss_agent_hierarchy(
    policy: &InfernalBossAgentPolicy,
    zone_policy: &InfernalZonePolicy,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<InfernalBossAgentHierarchyReadback, EngineError> {
    validate_infernal_boss_agent_policy(policy, zone_policy, attrition_policy)?;
    let policy_sha256 = infernal_boss_agent_policy_hash(policy)?;
    let zone_policy_sha256 = infernal_zone_policy_hash(zone_policy)?;
    let mut chain = Vec::with_capacity(policy.boss_agents.len());
    for (index, circle_id) in EXPECTED_BOSSES.iter().copied().enumerate() {
        let boss = boss_for_circle(policy, circle_id)?;
        let zone = zone_for_circle(zone_policy, circle_id)?;
        chain.push(InfernalBossAgentReadback {
            depth: u8::try_from(index + 1).unwrap(),
            circle_id,
            zone_id: zone.zone_id.clone(),
            ruler_ref: boss.ruler_ref.clone(),
            boss_id: boss.boss_id.clone(),
            calyx_agent_id: boss.calyx_agent_id.clone(),
            display_name: boss.display_name.clone(),
            role: boss.role,
            lethality_tier: boss.lethality_tier,
            tactic_id: boss.tactic_profile.tactic_id.clone(),
            trigger_term_count: boss.tactic_profile.trigger_terms.len(),
            ability_count: boss.tactic_profile.ability_ids.len(),
            dialogue_line_count: dialogue_line_count(boss),
            min_strength_milli: boss.tactic_profile.min_strength_milli,
            target_body: boss.target_body,
        });
    }
    let apex_agent_id = boss_for_circle(policy, InfernalCircleId::ObsidianThrone)?
        .calyx_agent_id
        .clone();
    let mut readback = InfernalBossAgentHierarchyReadback {
        schema: RPG_INFERNAL_BOSS_AGENTS_SCHEMA.to_string(),
        server_policy_id: policy.server_policy_id.clone(),
        source_issue: policy.source_issue,
        boss_count: policy.boss_agents.len(),
        calyx_agent_runtime_required: policy.calyx_agent_runtime_required,
        hot_path_policy: policy.hot_path_policy.clone(),
        calyx_live_query_allowed_in_hot_path: policy.calyx_live_query_allowed_in_hot_path,
        built_in_map_required: zone_policy.built_in_map_required,
        apex_agent_id,
        chain,
        policy_sha256,
        zone_policy_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&hierarchy_fingerprint(&readback))?;
    Ok(readback)
}

pub fn validate_infernal_boss_agent_policy(
    policy: &InfernalBossAgentPolicy,
    zone_policy: &InfernalZonePolicy,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<(), EngineError> {
    validate_infernal_zone_policy(zone_policy, attrition_policy)?;
    if policy.schema != RPG_INFERNAL_BOSS_AGENTS_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_INFERNAL_BOSS_AGENTS_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1068 {
        return invalid("policy.source_issue", "must point at issue 1068");
    }
    if !policy.calyx_agent_runtime_required {
        return invalid(
            "policy.calyx_agent_runtime_required",
            "boss hierarchy must use Calyx agent runtime",
        );
    }
    if policy.hot_path_policy != ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY {
        return invalid(
            "policy.hot_path_policy",
            format!("expected {ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY}"),
        );
    }
    if policy.calyx_live_query_allowed_in_hot_path {
        return invalid(
            "policy.calyx_live_query_allowed_in_hot_path",
            "live Calyx queries are forbidden in deterministic combat ticks",
        );
    }
    if policy.boss_agents.len() != EXPECTED_BOSSES.len() {
        return invalid(
            "policy.boss_agents",
            "one Infernal boss-agent is required for each of the seven circles",
        );
    }
    validate_unique_boss_fields(policy)?;
    for (index, circle_id) in EXPECTED_BOSSES.iter().copied().enumerate() {
        let boss = policy.boss_agents.get(index).ok_or_else(|| {
            invalid_error(
                "policy.boss_agents",
                format!("missing {}", circle_id.label()),
            )
        })?;
        if boss.circle_id != circle_id {
            return invalid(
                "policy.boss_agents.circle_id",
                format!(
                    "slot {} must be {}, got {}",
                    index + 1,
                    circle_id.label(),
                    boss.circle_id.label()
                ),
            );
        }
        let zone = zone_for_circle(zone_policy, circle_id)?;
        validate_boss_agent(boss, zone, index)?;
    }
    Ok(())
}

pub fn boss_for_circle(
    policy: &InfernalBossAgentPolicy,
    circle_id: InfernalCircleId,
) -> Result<&InfernalBossAgentSpec, EngineError> {
    policy
        .boss_agents
        .iter()
        .find(|boss| boss.circle_id == circle_id)
        .ok_or_else(|| {
            invalid_error(
                "policy.boss_agents.circle_id",
                format!("missing {}", circle_id.label()),
            )
        })
}

pub fn infernal_boss_agent_policy_hash(
    policy: &InfernalBossAgentPolicy,
) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn validate_unique_boss_fields(policy: &InfernalBossAgentPolicy) -> Result<(), EngineError> {
    let mut circles = BTreeSet::new();
    let mut rulers = BTreeSet::new();
    let mut boss_ids = BTreeSet::new();
    let mut agent_ids = BTreeSet::new();
    let mut target_bodies = BTreeSet::new();
    let mut tactic_ids = BTreeSet::new();
    let mut openings = BTreeSet::new();
    for boss in &policy.boss_agents {
        unique(&mut circles, boss.circle_id, "policy.boss_agents.circle_id")?;
        unique(
            &mut rulers,
            boss.ruler_ref.as_str(),
            "policy.boss_agents.ruler_ref",
        )?;
        unique(
            &mut boss_ids,
            boss.boss_id.as_str(),
            "policy.boss_agents.boss_id",
        )?;
        unique(
            &mut agent_ids,
            boss.calyx_agent_id.as_str(),
            "policy.boss_agents.calyx_agent_id",
        )?;
        unique(
            &mut target_bodies,
            boss.target_body,
            "policy.boss_agents.target_body",
        )?;
        unique(
            &mut tactic_ids,
            boss.tactic_profile.tactic_id.as_str(),
            "policy.boss_agents.tactic_id",
        )?;
        unique(
            &mut openings,
            boss.dialogue_profile.opening_line.as_str(),
            "policy.boss_agents.dialogue.opening_line",
        )?;
    }
    Ok(())
}

fn validate_boss_agent(
    boss: &InfernalBossAgentSpec,
    zone: &InfernalZoneNode,
    index: usize,
) -> Result<(), EngineError> {
    validate_token("policy.boss_agents.ruler_ref", &boss.ruler_ref)?;
    validate_token("policy.boss_agents.boss_id", &boss.boss_id)?;
    validate_token("policy.boss_agents.calyx_agent_id", &boss.calyx_agent_id)?;
    validate_display_name(&boss.display_name)?;
    if boss.ruler_ref != zone.ruler_ref {
        return invalid(
            "policy.boss_agents.ruler_ref",
            format!(
                "{} must match zone ruler {}",
                boss.ruler_ref, zone.ruler_ref
            ),
        );
    }
    let expected_role = if boss.circle_id == InfernalCircleId::ObsidianThrone {
        InfernalBossAgentRole::ApexTyrant
    } else {
        InfernalBossAgentRole::CircleLord
    };
    if boss.role != expected_role {
        return invalid(
            "policy.boss_agents.role",
            format!("{} must be {:?}", boss.circle_id.label(), expected_role),
        );
    }
    if !(7..=10).contains(&boss.lethality_tier) {
        return invalid(
            "policy.boss_agents.lethality_tier",
            "boss lethality must be high-tier inside 7..=10",
        );
    }
    if boss.circle_id == InfernalCircleId::ObsidianThrone && boss.lethality_tier != 10 {
        return invalid(
            "policy.boss_agents.lethality_tier",
            "Maldrithax must be the apex tier 10 boss",
        );
    }
    if index > 0 {
        let previous_min = 7 + u8::try_from(index.saturating_sub(1) / 2).unwrap();
        if boss.lethality_tier < previous_min {
            return invalid(
                "policy.boss_agents.lethality_tier",
                "lethality must not regress through the descending hierarchy",
            );
        }
    }
    if boss.target_body == 0 {
        return invalid("policy.boss_agents.target_body", "must be non-zero");
    }
    validate_tactics(boss)?;
    validate_dialogue(boss)?;
    Ok(())
}

fn validate_tactics(boss: &InfernalBossAgentSpec) -> Result<(), EngineError> {
    let tactics = &boss.tactic_profile;
    validate_token("policy.boss_agents.tactic_id", &tactics.tactic_id)?;
    if tactics.trigger_terms.len() < 3 || tactics.trigger_terms.len() > 12 {
        return invalid(
            "policy.boss_agents.trigger_terms",
            "each boss needs 3..=12 trigger terms",
        );
    }
    let mut terms = BTreeSet::new();
    for term in &tactics.trigger_terms {
        validate_token("policy.boss_agents.trigger_terms", term)?;
        unique(
            &mut terms,
            term.as_str(),
            "policy.boss_agents.trigger_terms",
        )?;
    }
    if tactics.ability_ids.len() < 3 || tactics.ability_ids.len() > 10 {
        return invalid(
            "policy.boss_agents.ability_ids",
            "each boss needs 3..=10 tactical abilities",
        );
    }
    let mut abilities = BTreeSet::new();
    for ability in &tactics.ability_ids {
        validate_token("policy.boss_agents.ability_ids", ability)?;
        unique(
            &mut abilities,
            ability.as_str(),
            "policy.boss_agents.ability_ids",
        )?;
    }
    if tactics.order_velocity == [0, 0, 0]
        || tactics
            .order_velocity
            .iter()
            .any(|value| !(-32..=32).contains(value))
    {
        return invalid(
            "policy.boss_agents.order_velocity",
            "must be non-zero and each component must be inside -32..=32",
        );
    }
    if !(500..=1000).contains(&tactics.min_strength_milli) {
        return invalid(
            "policy.boss_agents.min_strength_milli",
            "must be inside 500..=1000",
        );
    }
    for placeholder in ["{agent}", "{signal}", "{world}", "{terms}"] {
        if !tactics.prompt_template.contains(placeholder) {
            return invalid(
                "policy.boss_agents.prompt_template",
                format!("missing placeholder {placeholder}"),
            );
        }
    }
    Ok(())
}

fn validate_dialogue(boss: &InfernalBossAgentSpec) -> Result<(), EngineError> {
    validate_line(
        "policy.boss_agents.dialogue.opening_line",
        &boss.dialogue_profile.opening_line,
    )?;
    validate_line(
        "policy.boss_agents.dialogue.aggro_line",
        &boss.dialogue_profile.aggro_line,
    )?;
    validate_line(
        "policy.boss_agents.dialogue.defeat_line",
        &boss.dialogue_profile.defeat_line,
    )?;
    if boss.dialogue_profile.phase_lines.len() < 2 || boss.dialogue_profile.phase_lines.len() > 6 {
        return invalid(
            "policy.boss_agents.dialogue.phase_lines",
            "each boss needs 2..=6 phase lines",
        );
    }
    for line in &boss.dialogue_profile.phase_lines {
        validate_line("policy.boss_agents.dialogue.phase_lines", line)?;
    }
    Ok(())
}

fn dialogue_line_count(boss: &InfernalBossAgentSpec) -> usize {
    3 + boss.dialogue_profile.phase_lines.len()
}
fn validate_display_name(value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 128 {
        return invalid(
            "policy.boss_agents.display_name",
            "must be non-empty and <= 128 bytes",
        );
    }
    Ok(())
}
fn validate_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 180 {
        return invalid(field, "must be non-empty and <= 180 bytes");
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
fn unique<T: Ord + Copy>(seen: &mut BTreeSet<T>, value: T, field: &str) -> Result<(), EngineError> {
    if !seen.insert(value) {
        return invalid(field, "duplicate value");
    }
    Ok(())
}
fn hierarchy_fingerprint(readback: &InfernalBossAgentHierarchyReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "server_policy_id": readback.server_policy_id,
        "source_issue": readback.source_issue,
        "boss_count": readback.boss_count,
        "calyx_agent_runtime_required": readback.calyx_agent_runtime_required,
        "hot_path_policy": readback.hot_path_policy,
        "calyx_live_query_allowed_in_hot_path": readback.calyx_live_query_allowed_in_hot_path,
        "built_in_map_required": readback.built_in_map_required,
        "apex_agent_id": readback.apex_agent_id,
        "chain": readback.chain,
        "policy_sha256": readback.policy_sha256,
        "zone_policy_sha256": readback.zone_policy_sha256,
    })
}
fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}
fn invalid_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("infernal_boss_agents.{}", field.into()),
        reason: reason.to_string(),
    }
}
