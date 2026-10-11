use super::serialize_sha256;
use serde_json::json;

use crate::{
    EngineError,
    calyx_agent_runtime::ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalBossAgentSpec, InfernalZoneNode,
    InfernalZonePolicy, RPG_INFERNAL_BOSS_AGENTS_SCHEMA, boss_for_circle,
    infernal_boss_agents::EXPECTED_BOSSES, rpg_authored_payload_without_nulls,
    validate_infernal_boss_agent_policy, validate_rpg_authored_content_records, zone_for_circle,
};

pub fn infernal_boss_agent_content_records(
    policy: &InfernalBossAgentPolicy,
    zone_policy: &InfernalZonePolicy,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_infernal_boss_agent_policy(policy, zone_policy, attrition_policy)?;
    let records = EXPECTED_BOSSES
        .iter()
        .copied()
        .map(|circle_id| {
            let boss = boss_for_circle(policy, circle_id)?;
            let zone = zone_for_circle(zone_policy, circle_id)?;
            content_record(boss, zone)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "infernal_boss_agents.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn content_record(
    boss: &InfernalBossAgentSpec,
    zone: &InfernalZoneNode,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(json!({
        "schema": RPG_INFERNAL_BOSS_AGENTS_SCHEMA,
        "source_issue": 1068,
        "content_migration_issue": 1455,
        "surface_kind": "infernal_boss_agent",
        "agent_id": boss.calyx_agent_id,
        "boss_id": boss.boss_id,
        "circle_id": boss.circle_id,
        "zone_id": zone.zone_id,
        "ruler_ref": boss.ruler_ref,
        "role": boss.role,
        "lethality_tier": boss.lethality_tier,
        "trigger_terms": boss.tactic_profile.trigger_terms,
        "ability_ids": boss.tactic_profile.ability_ids,
        "tactic_id": boss.tactic_profile.tactic_id,
        "dialogue": boss.dialogue_profile,
        "min_strength_milli": boss.tactic_profile.min_strength_milli,
        "prompt_template": boss.tactic_profile.prompt_template,
        "target_body": boss.target_body,
        "order_velocity": boss.tactic_profile.order_velocity,
        "hot_path_policy": ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY,
        "calyx_live_query_allowed_in_hot_path": false,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
    }));
    Ok(EngineContentRecord {
        kind: EngineContentKind::Agent,
        id: boss.calyx_agent_id.clone(),
        display_name: boss.display_name.clone(),
        source: EngineContentSource {
            path: format!(
                "editor://rpg/ashen_reach/infernal_boss_agents/{}",
                boss.boss_id
            ),
            author: "editor_authoring.issue1455.infernal_boss_agents".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
