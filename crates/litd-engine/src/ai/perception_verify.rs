use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::{
    AgentPerception, BehaviorStatus, BlackboardValue, PerceivedEnemy, PerceptionRunReport,
    blackboard::{invalid_config, validate_id},
};

pub(super) fn verify_run_semantics(report: &PerceptionRunReport) -> Result<(), EngineError> {
    if report.snapshot_fnv1a64.is_empty() {
        return invalid_config(
            "perception.run.snapshot_fnv1a64",
            "snapshot hash must be non-empty",
        );
    }
    if report.entity_count == 0 {
        return invalid_config("perception.run.entity_count", "must be non-zero");
    }
    let mut agent_ids = BTreeSet::new();
    let mut agent_body_ids = BTreeSet::new();
    for agent in &report.agents {
        validate_id("perception.run.agent_id", &agent.agent_id)?;
        if !agent_ids.insert(agent.agent_id.as_str()) {
            return invalid_config(
                "perception.run.agent_id",
                format!("duplicate agent id {}", agent.agent_id),
            );
        }
        if !agent_body_ids.insert(agent.body_id.index()) {
            return invalid_config("perception.run.body_id", "duplicate agent body id");
        }
        verify_agent(report, agent)?;
    }
    Ok(())
}

fn verify_agent(report: &PerceptionRunReport, agent: &AgentPerception) -> Result<(), EngineError> {
    if agent.blackboard.agent_id != agent.agent_id {
        return invalid_config(
            "perception.blackboard.agent_id",
            "blackboard agent id must match perception agent id",
        );
    }
    if agent.blackboard.sim_tick != report.tick
        || agent.blackboard.sim_state_hash != report.sim_state_hash_before
    {
        return invalid_config(
            "perception.blackboard.sim_source",
            "blackboard must reference the same sim tick and hash as the perception run",
        );
    }
    let enemies_by_id = enemy_index(&agent.enemies)?;
    let nearest_enemy = agent.enemies.first();
    verify_optional_enemy(
        "perception.nearest_enemy",
        nearest_enemy,
        &agent.nearest_enemy_id,
        agent.nearest_enemy_distance_fp,
        &enemies_by_id,
    )?;
    let nearest_visible = agent
        .enemies
        .iter()
        .find(|enemy| enemy.in_sight_range && enemy.has_line_of_sight);
    verify_optional_enemy(
        "perception.nearest_visible_enemy",
        nearest_visible,
        &agent.nearest_visible_enemy_id,
        agent.nearest_visible_enemy_distance_fp,
        &enemies_by_id,
    )?;
    let sight = sorted_ids(&agent.enemies, |enemy| enemy.in_sight_range);
    let attack = sorted_ids(&agent.enemies, |enemy| enemy.in_attack_range);
    let los = sorted_ids(&agent.enemies, |enemy| {
        enemy.in_sight_range && enemy.has_line_of_sight
    });
    let threat = sorted_ids(&agent.enemies, |enemy| enemy.threat);
    verify_list(
        "perception.enemies_in_sight_range",
        &agent.enemies_in_sight_range,
        &sight,
    )?;
    verify_list(
        "perception.enemies_in_attack_range",
        &agent.enemies_in_attack_range,
        &attack,
    )?;
    verify_list(
        "perception.enemies_with_line_of_sight",
        &agent.enemies_with_line_of_sight,
        &los,
    )?;
    verify_list(
        "perception.threat_enemy_ids",
        &agent.threat_enemy_ids,
        &threat,
    )?;
    verify_blackboard(agent, nearest_visible, &sight, &attack, &los, &threat)
}

fn enemy_index(enemies: &[PerceivedEnemy]) -> Result<BTreeMap<&str, &PerceivedEnemy>, EngineError> {
    let mut index = BTreeMap::new();
    let mut previous: Option<(i64, &str)> = None;
    for enemy in enemies {
        validate_id("perception.enemy_id", &enemy.id)?;
        if let Some(previous) = previous
            && previous > (enemy.distance_fp, enemy.id.as_str())
        {
            return invalid_config(
                "perception.enemies",
                "enemies must be sorted by distance then id",
            );
        }
        previous = Some((enemy.distance_fp, enemy.id.as_str()));
        if index.insert(enemy.id.as_str(), enemy).is_some() {
            return invalid_config(
                "perception.enemy_id",
                format!("duplicate enemy id {}", enemy.id),
            );
        }
    }
    Ok(index)
}

fn verify_optional_enemy(
    field: &str,
    expected: Option<&PerceivedEnemy>,
    actual_id: &Option<String>,
    actual_distance: Option<i64>,
    enemies_by_id: &BTreeMap<&str, &PerceivedEnemy>,
) -> Result<(), EngineError> {
    match (expected, actual_id, actual_distance) {
        (Some(expected), Some(id), Some(distance))
            if id == &expected.id && distance == expected.distance_fp =>
        {
            if !enemies_by_id.contains_key(id.as_str()) {
                return invalid_config(field, "enemy id must exist in enemy detail list");
            }
            Ok(())
        }
        (None, None, None) => Ok(()),
        _ => invalid_config(
            field,
            "nearest enemy id and distance do not match enemy details",
        ),
    }
}

fn sorted_ids(
    enemies: &[PerceivedEnemy],
    predicate: impl Fn(&PerceivedEnemy) -> bool,
) -> Vec<String> {
    let mut ids = enemies
        .iter()
        .filter(|enemy| predicate(enemy))
        .map(|enemy| enemy.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn verify_list(field: &str, actual: &[String], expected: &[String]) -> Result<(), EngineError> {
    if actual != expected {
        return invalid_config(field, "ids must match derived enemy facts");
    }
    Ok(())
}

fn verify_blackboard(
    agent: &AgentPerception,
    nearest_visible: Option<&PerceivedEnemy>,
    sight: &[String],
    attack: &[String],
    los: &[String],
    threat: &[String],
) -> Result<(), EngineError> {
    let values = &agent.blackboard.values;
    expect_bool(values, "perception.has_enemy", nearest_visible.is_some())?;
    expect_bool(values, "perception.sees_enemy", nearest_visible.is_some())?;
    expect_bool(values, "perception.in_attack_range", !attack.is_empty())?;
    expect_bool(values, "perception.has_line_of_sight", !los.is_empty())?;
    expect_bool(values, "perception.threat_present", !threat.is_empty())?;
    match nearest_visible {
        Some(enemy) => {
            expect_text(values, "perception.nearest_enemy_id", &enemy.id)?;
            expect_int(
                values,
                "perception.nearest_enemy_distance_fp",
                enemy.distance_fp,
            )?;
        }
        None => {
            expect_none(values, "perception.nearest_enemy_id")?;
            expect_none(values, "perception.nearest_enemy_distance_fp")?;
        }
    }
    expect_text_list(values, "perception.enemies_in_sight_range", sight)?;
    expect_text_list(values, "perception.enemies_in_attack_range", attack)?;
    expect_text_list(values, "perception.enemies_with_line_of_sight", los)?;
    expect_text_list(values, "perception.threat_enemy_ids", threat)?;
    expect_condition(
        agent,
        "has_enemy",
        status_from_bool(nearest_visible.is_some()),
    )?;
    expect_condition(
        agent,
        "sees_enemy",
        status_from_bool(nearest_visible.is_some()),
    )?;
    expect_condition(
        agent,
        "in_attack_range",
        status_from_bool(!attack.is_empty()),
    )?;
    expect_condition(agent, "line_of_sight", status_from_bool(!los.is_empty()))?;
    expect_condition(
        agent,
        "threat_present",
        status_from_bool(!threat.is_empty()),
    )
}

fn expect_bool(
    values: &BTreeMap<String, BlackboardValue>,
    key: &str,
    expected: bool,
) -> Result<(), EngineError> {
    match values.get(key) {
        Some(BlackboardValue::Bool(actual)) if *actual == expected => Ok(()),
        _ => invalid_config(key, "blackboard bool does not match perception facts"),
    }
}

fn expect_text(
    values: &BTreeMap<String, BlackboardValue>,
    key: &str,
    expected: &str,
) -> Result<(), EngineError> {
    match values.get(key) {
        Some(BlackboardValue::Text(actual)) if actual == expected => Ok(()),
        _ => invalid_config(key, "blackboard text does not match perception facts"),
    }
}

fn expect_int(
    values: &BTreeMap<String, BlackboardValue>,
    key: &str,
    expected: i64,
) -> Result<(), EngineError> {
    match values.get(key) {
        Some(BlackboardValue::Int(actual)) if *actual == expected => Ok(()),
        _ => invalid_config(key, "blackboard int does not match perception facts"),
    }
}

fn expect_none(values: &BTreeMap<String, BlackboardValue>, key: &str) -> Result<(), EngineError> {
    match values.get(key) {
        Some(BlackboardValue::None) => Ok(()),
        _ => invalid_config(key, "blackboard value must be explicit none"),
    }
}

fn expect_text_list(
    values: &BTreeMap<String, BlackboardValue>,
    key: &str,
    expected: &[String],
) -> Result<(), EngineError> {
    match values.get(key) {
        Some(BlackboardValue::TextList(actual)) if actual == expected => Ok(()),
        _ => invalid_config(key, "blackboard id list does not match perception facts"),
    }
}

fn expect_condition(
    agent: &AgentPerception,
    key: &str,
    expected: BehaviorStatus,
) -> Result<(), EngineError> {
    match agent.blackboard.leaf_state.conditions.get(key) {
        Some(actual) if *actual == expected => Ok(()),
        _ => invalid_config(key, "blackboard condition does not match perception facts"),
    }
}

fn status_from_bool(value: bool) -> BehaviorStatus {
    if value {
        BehaviorStatus::Success
    } else {
        BehaviorStatus::Failure
    }
}
