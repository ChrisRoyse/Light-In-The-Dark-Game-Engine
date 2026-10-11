use litd_engine::{
    EngineError,
    rpg::{
        InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalBossAgentRole, InfernalCircleId,
        InfernalZonePolicy, validate_infernal_boss_agent_policy,
    },
};
use serde_json::{Value, json};

pub fn boss_agent_edges(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    json!({
        "missing_boss_rejected": missing_boss_edge(policy, zones, attrition),
        "duplicate_circle_rejected": duplicate_circle_edge(policy, zones, attrition),
        "zone_ruler_drift_rejected": zone_ruler_drift_edge(policy, zones, attrition),
        "calyx_required_false_rejected": calyx_required_edge(policy, zones, attrition),
        "live_hot_path_query_rejected": live_hot_path_edge(policy, zones, attrition),
        "low_lethality_rejected": low_lethality_edge(policy, zones, attrition),
        "empty_trigger_terms_rejected": empty_terms_edge(policy, zones, attrition),
        "empty_dialogue_rejected": empty_dialogue_edge(policy, zones, attrition),
        "zero_velocity_rejected": zero_velocity_edge(policy, zones, attrition),
        "wrong_apex_role_rejected": wrong_apex_edge(policy, zones, attrition),
    })
}

fn missing_boss_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.boss_agents.pop();
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn duplicate_circle_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.boss_agents[1].circle_id = InfernalCircleId::Threshold;
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn zone_ruler_drift_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid_zones = zones.clone();
    invalid_zones.zones[2].ruler_ref = "lord.infernal.iron_bastion.false_praetor".to_string();
    edge_from_result(
        validate_infernal_boss_agent_policy(policy, &invalid_zones, attrition),
        true,
    )
}

fn calyx_required_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.calyx_agent_runtime_required = false;
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn live_hot_path_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.calyx_live_query_allowed_in_hot_path = true;
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn low_lethality_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.boss_agents[0].lethality_tier = 6;
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn empty_terms_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.boss_agents[0].tactic_profile.trigger_terms.clear();
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn empty_dialogue_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.boss_agents[0].dialogue_profile.phase_lines.clear();
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn zero_velocity_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.boss_agents[0].tactic_profile.order_velocity = [0, 0, 0];
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn wrong_apex_edge(
    policy: &InfernalBossAgentPolicy,
    zones: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = policy.clone();
    invalid.boss_agents[6].role = InfernalBossAgentRole::CircleLord;
    edge_from_result(
        validate_infernal_boss_agent_policy(&invalid, zones, attrition),
        true,
    )
}

fn edge_from_result<T>(result: Result<T, EngineError>, state_unchanged: bool) -> Value {
    match result {
        Ok(_) => json!({"unexpected_success": true, "state_unchanged": state_unchanged}),
        Err(error) => json!({
            "error_code": error.code(),
            "error": error.to_string(),
            "state_unchanged": state_unchanged,
        }),
    }
}
