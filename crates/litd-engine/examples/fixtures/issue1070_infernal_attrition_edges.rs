use litd_engine::{
    EngineError,
    rpg::{
        AttributeScores, InfernalAttritionPolicy, InfernalAttritionState, InfernalAttritionStep,
        InfernalCircleId, MortalityCharacterState, MortalityPolicy, RPG_INFERNAL_ATTRITION_SCHEMA,
        RPG_MORTALITY_SCHEMA, apply_infernal_attrition, infernal_attrition_state_hash,
        validate_infernal_attrition_policy, validate_infernal_attrition_state,
        validate_infernal_attrition_step,
    },
};
use serde_json::{Value, json};

pub fn attrition_edges(attrition: &InfernalAttritionPolicy, mortality: &MortalityPolicy) -> Value {
    json!({
        "duplicate_circle_rejected": duplicate_circle_edge(attrition),
        "missing_circle_rejected": missing_circle_edge(attrition),
        "zero_tick_rejected": zero_tick_edge(attrition),
        "over_mitigation_rejected": over_mitigation_edge(attrition),
        "over_max_tick_rejected": over_max_tick_edge(attrition),
        "state_strain_boundary_rejected": state_strain_edge(attrition, mortality),
        "circle_mismatch_rejected": circle_mismatch_edge(attrition, mortality),
    })
}

fn duplicate_circle_edge(policy: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.circles[1].circle_id = InfernalCircleId::Threshold;
    let before = value_hash(&invalid);
    let result = validate_infernal_attrition_policy(&invalid);
    edge_from_result(result, before, value_hash(&invalid))
}

fn missing_circle_edge(policy: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.circles.pop();
    let before = value_hash(&invalid);
    let result = validate_infernal_attrition_policy(&invalid);
    edge_from_result(result, before, value_hash(&invalid))
}

fn zero_tick_edge(policy: &InfernalAttritionPolicy) -> Value {
    let invalid = step("edge.zero", InfernalCircleId::EmberWastes, 0, 0);
    let before = value_hash(&json!({"policy": policy, "step": &invalid}));
    let result = validate_infernal_attrition_step(&invalid, policy);
    let after = value_hash(&json!({"policy": policy, "step": &invalid}));
    edge_from_result(result, before, after)
}

fn over_mitigation_edge(policy: &InfernalAttritionPolicy) -> Value {
    let invalid = step("edge.mitigation", InfernalCircleId::EmberWastes, 1, 90);
    let before = value_hash(&json!({"policy": policy, "step": &invalid}));
    let result = validate_infernal_attrition_step(&invalid, policy);
    let after = value_hash(&json!({"policy": policy, "step": &invalid}));
    edge_from_result(result, before, after)
}

fn over_max_tick_edge(policy: &InfernalAttritionPolicy) -> Value {
    let invalid = step("edge.max_tick", InfernalCircleId::EmberWastes, 241, 0);
    let before = value_hash(&json!({"policy": policy, "step": &invalid}));
    let result = validate_infernal_attrition_step(&invalid, policy);
    let after = value_hash(&json!({"policy": policy, "step": &invalid}));
    edge_from_result(result, before, after)
}

fn state_strain_edge(attrition: &InfernalAttritionPolicy, mortality: &MortalityPolicy) -> Value {
    let state = state(InfernalCircleId::EmberWastes, 100, 16, 100);
    let before = infernal_attrition_state_hash(&state).unwrap();
    let result = validate_infernal_attrition_state(&state, attrition, mortality);
    let after = infernal_attrition_state_hash(&state).unwrap();
    edge_from_result(result, before, after)
}

fn circle_mismatch_edge(attrition: &InfernalAttritionPolicy, mortality: &MortalityPolicy) -> Value {
    let state = state(InfernalCircleId::EmberWastes, 100, 16, 0);
    let before = infernal_attrition_state_hash(&state).unwrap();
    let result = apply_infernal_attrition(
        &state,
        attrition,
        mortality,
        &step("edge.circle", InfernalCircleId::WailingTombs, 1, 0),
    )
    .map(|_| ());
    let after = infernal_attrition_state_hash(&state).unwrap();
    edge_from_result(result, before, after)
}

fn edge_from_result<T>(result: Result<T, EngineError>, before: String, after: String) -> Value {
    let state_unchanged = before == after;
    match result {
        Ok(_) => {
            json!({"unexpected_success": true, "before_sha256": before, "after_sha256": after, "state_unchanged": state_unchanged})
        }
        Err(error) => json!({
            "error_code": error.code(),
            "error": error.to_string(),
            "before_sha256": before,
            "after_sha256": after,
            "state_unchanged": state_unchanged,
        }),
    }
}

fn value_hash(value: &impl serde::Serialize) -> String {
    litd_hash::sha256_hex(&serde_json::to_vec(value).expect("edge input must serialize"))
}

fn state(
    circle_id: InfernalCircleId,
    hit_points: i32,
    current_constitution: i16,
    constitution_strain: i32,
) -> InfernalAttritionState {
    InfernalAttritionState {
        schema: RPG_INFERNAL_ATTRITION_SCHEMA.to_string(),
        run_id: "run.infernal.deeps.1070".to_string(),
        party_member_id: "party.vanguard".to_string(),
        circle_id,
        hit_points,
        max_hit_points: 220,
        constitution_strain,
        total_exposure_ticks: 0,
        mortality_state: MortalityCharacterState {
            schema: RPG_MORTALITY_SCHEMA.to_string(),
            character_id: "char.hellrunner".to_string(),
            character_name: "Ira Cindermark".to_string(),
            race_id: "race.ashen_human".to_string(),
            level: 51,
            age_years: 39,
            base_attributes: AttributeScores::new(16, 13, 12, 14, 16, 11),
            current_constitution,
            deaths_since_constitution_loss: 0,
            total_deaths: 0,
            permanently_dead: false,
            mourning_wall_entry_id: None,
        },
    }
}

fn step(
    step_id: &str,
    circle_id: InfernalCircleId,
    tick_count: u16,
    mitigation_percent: u8,
) -> InfernalAttritionStep {
    InfernalAttritionStep {
        step_id: step_id.to_string(),
        circle_id,
        tick_count,
        mitigation_percent,
    }
}
