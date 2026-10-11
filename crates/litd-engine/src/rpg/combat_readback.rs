use super::serialize_sha256;

use crate::EngineError;

use super::{
    CombatInput, CombatReadback, CombatRules, RPG_COMBAT_SCHEMA, combat_resolve::CombatResolution,
};

pub(super) fn build_combat_readback(
    input: &CombatInput,
    rules: &CombatRules,
    resolution: CombatResolution,
) -> Result<CombatReadback, EngineError> {
    let input_sha256 = serialize_sha256(input)?;
    let rules_sha256 = serialize_sha256(rules)?;
    let mut readback = CombatReadback {
        schema: RPG_COMBAT_SCHEMA.to_string(),
        encounter_id: input.encounter_id.clone(),
        tick_millis: rules.tick_millis,
        round_ticks: rules.round_ticks,
        round_millis: rules.tick_millis * rules.round_ticks,
        requested_rounds: input.rounds,
        resolved_rounds: resolution.rounds.len() as u16,
        total_damage: resolution.total_damage,
        defense_order: rules
            .defense_order
            .iter()
            .map(|defense| defense.label().to_string())
            .collect(),
        multi_attack_checks: resolution.multi_attack_checks,
        rounds: resolution.rounds,
        final_attacker: resolution.attacker,
        final_defender: resolution.defender,
        rolls_consumed: resolution.rolls_consumed,
        input_sha256,
        rules_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&result_payload(&readback))?;
    Ok(readback)
}

fn result_payload(readback: &CombatReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "encounter_id": readback.encounter_id,
        "tick_millis": readback.tick_millis,
        "round_ticks": readback.round_ticks,
        "round_millis": readback.round_millis,
        "requested_rounds": readback.requested_rounds,
        "resolved_rounds": readback.resolved_rounds,
        "total_damage": readback.total_damage,
        "defense_order": readback.defense_order,
        "multi_attack_checks": readback.multi_attack_checks,
        "rounds": readback.rounds,
        "final_attacker": readback.final_attacker,
        "final_defender": readback.final_defender,
        "rolls_consumed": readback.rolls_consumed,
        "input_sha256": readback.input_sha256,
        "rules_sha256": readback.rules_sha256,
    })
}
