use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::FixedVec3,
    nav::NavCellCoord,
    sim::{FixedTickSim, SimBodyId, SimBodyState},
};

use super::{
    AgentBlackboard, BehaviorLeafState, BehaviorStatus, BlackboardValue,
    blackboard::invalid_config,
    perception_hash::{compute_perception_run_fnv1a64, compute_snapshot_fnv1a64},
    perception_spatial::{PerceptionBlockerGrid, line_of_sight, manhattan_xz_fp},
    perception_validate::{body_state_index, validate_inputs},
    perception_verify::verify_run_semantics,
    verify_blackboard_integrity,
};

pub const AI_PERCEPTION_SNAPSHOT_SCHEMA: &str = "litd.ai.perception.snapshot.v1";
pub const AI_PERCEPTION_RUN_SCHEMA: &str = "litd.ai.perception.run.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptionAgentInput {
    pub id: String,
    pub body_id: SimBodyId,
    pub faction: String,
    pub nav_cell: NavCellCoord,
    pub sight_range_fp: i64,
    pub attack_range_fp: i64,
    pub threat_range_fp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptionEntityInput {
    pub id: String,
    pub body_id: SimBodyId,
    pub faction: String,
    pub nav_cell: NavCellCoord,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptionAgent {
    pub id: String,
    pub body_id: SimBodyId,
    pub faction: String,
    pub position: FixedVec3,
    pub nav_cell: NavCellCoord,
    pub sight_range_fp: i64,
    pub attack_range_fp: i64,
    pub threat_range_fp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptionEntity {
    pub id: String,
    pub body_id: SimBodyId,
    pub faction: String,
    pub position: FixedVec3,
    pub nav_cell: NavCellCoord,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptionBlocker {
    pub id: String,
    pub min: NavCellCoord,
    pub max: NavCellCoord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptionSnapshot {
    pub schema: String,
    pub tick: u64,
    pub sim_state_hash: u64,
    pub agents: Vec<PerceptionAgent>,
    pub entities: Vec<PerceptionEntity>,
    pub blockers: Vec<PerceptionBlocker>,
    pub fnv1a64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceivedEnemy {
    pub id: String,
    pub body_id: SimBodyId,
    pub nav_cell: NavCellCoord,
    pub distance_fp: i64,
    pub in_sight_range: bool,
    pub in_attack_range: bool,
    pub has_line_of_sight: bool,
    pub threat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPerception {
    pub agent_id: String,
    pub body_id: SimBodyId,
    pub nearest_enemy_id: Option<String>,
    pub nearest_enemy_distance_fp: Option<i64>,
    pub nearest_visible_enemy_id: Option<String>,
    pub nearest_visible_enemy_distance_fp: Option<i64>,
    pub enemies_in_sight_range: Vec<String>,
    pub enemies_in_attack_range: Vec<String>,
    pub enemies_with_line_of_sight: Vec<String>,
    pub threat_enemy_ids: Vec<String>,
    pub enemies: Vec<PerceivedEnemy>,
    pub blackboard: AgentBlackboard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptionRunReport {
    pub schema: String,
    pub tick: u64,
    pub snapshot_fnv1a64: String,
    pub sim_state_hash_before: u64,
    pub sim_state_hash_after: u64,
    pub sim_state_unchanged: bool,
    pub agent_count: usize,
    pub entity_count: usize,
    pub blocker_count: usize,
    pub agents: Vec<AgentPerception>,
    pub fnv1a64: String,
}

pub fn snapshot_from_sim<const MAX_BODIES: usize>(
    sim: &FixedTickSim<MAX_BODIES>,
    agents: &[PerceptionAgentInput],
    entities: &[PerceptionEntityInput],
    blockers: &[PerceptionBlocker],
) -> Result<PerceptionSnapshot, EngineError> {
    let body_states = sim.body_states().collect::<Vec<_>>();
    snapshot_from_sim_states(
        sim.tick(),
        sim.state_hash(),
        &body_states,
        agents,
        entities,
        blockers,
    )
}

pub fn snapshot_from_sim_states(
    tick: u64,
    sim_state_hash: u64,
    body_states: &[SimBodyState],
    agents: &[PerceptionAgentInput],
    entities: &[PerceptionEntityInput],
    blockers: &[PerceptionBlocker],
) -> Result<PerceptionSnapshot, EngineError> {
    validate_inputs(agents, entities, blockers)?;
    let states = body_state_index(body_states)?;
    let mut snapshot_agents = Vec::with_capacity(agents.len());
    for agent in sorted_agents(agents) {
        let state =
            states
                .get(&agent.body_id.index())
                .ok_or_else(|| EngineError::AiInvalidConfig {
                    field: format!("agents.{}.body_id", agent.id),
                    reason: "agent body id is absent from sim snapshot".to_string(),
                })?;
        snapshot_agents.push(PerceptionAgent {
            id: agent.id.clone(),
            body_id: agent.body_id,
            faction: agent.faction.clone(),
            position: state.position,
            nav_cell: agent.nav_cell,
            sight_range_fp: agent.sight_range_fp,
            attack_range_fp: agent.attack_range_fp,
            threat_range_fp: agent.threat_range_fp,
        });
    }
    let mut snapshot_entities = Vec::with_capacity(entities.len());
    for entity in sorted_entities(entities) {
        let state =
            states
                .get(&entity.body_id.index())
                .ok_or_else(|| EngineError::AiInvalidConfig {
                    field: format!("entities.{}.body_id", entity.id),
                    reason: "entity body id is absent from sim snapshot".to_string(),
                })?;
        snapshot_entities.push(PerceptionEntity {
            id: entity.id.clone(),
            body_id: entity.body_id,
            faction: entity.faction.clone(),
            position: state.position,
            nav_cell: entity.nav_cell,
            alive: entity.alive,
        });
    }
    let mut blockers = blockers.to_vec();
    blockers.sort_by(|left, right| left.id.cmp(&right.id));
    let mut snapshot = PerceptionSnapshot {
        schema: AI_PERCEPTION_SNAPSHOT_SCHEMA.to_string(),
        tick,
        sim_state_hash,
        agents: snapshot_agents,
        entities: snapshot_entities,
        blockers,
        fnv1a64: String::new(),
    };
    snapshot.fnv1a64 = compute_snapshot_fnv1a64(&snapshot);
    // Do NOT re-verify here (#1486 #4): the snapshot is well-formed by construction
    // (validate_inputs above + a freshly-computed hash), and its only consumer,
    // sense_perception, re-verifies it at that trust boundary. Re-hashing the snapshot we
    // just hashed was pure O(agents+entities) waste.
    Ok(snapshot)
}

pub fn sense_perception(
    snapshot: &PerceptionSnapshot,
    sim_state_hash_after: u64,
) -> Result<PerceptionRunReport, EngineError> {
    verify_perception_snapshot_integrity(snapshot)?;
    if snapshot.sim_state_hash != sim_state_hash_after {
        return invalid_config(
            "sim_state_hash_after",
            "perception sensing must read the sim snapshot without mutation",
        );
    }
    // Rasterise the blockers into an occupancy grid ONCE per run instead of scanning every
    // blocker AABB per line-of-sight cell for every agent x entity pair (#1486 #3).
    let blocker_grid = PerceptionBlockerGrid::build(&snapshot.blockers)?;
    let mut agents = Vec::with_capacity(snapshot.agents.len());
    for agent in &snapshot.agents {
        agents.push(sense_agent(snapshot, &blocker_grid, agent)?);
    }
    let mut report = PerceptionRunReport {
        schema: AI_PERCEPTION_RUN_SCHEMA.to_string(),
        tick: snapshot.tick,
        snapshot_fnv1a64: snapshot.fnv1a64.clone(),
        sim_state_hash_before: snapshot.sim_state_hash,
        sim_state_hash_after,
        sim_state_unchanged: true,
        agent_count: agents.len(),
        entity_count: snapshot.entities.len(),
        blocker_count: snapshot.blockers.len(),
        agents,
        fnv1a64: String::new(),
    };
    report.fnv1a64 = compute_perception_run_fnv1a64(&report);
    // As above (#1486 #4): the run is well-formed by construction. The controller
    // (verify_perception_run_integrity) and write_perception_run_json both verify it at
    // their boundaries, so re-hashing + re-running verify_run_semantics on the run we just
    // built here was redundant with those checks.
    Ok(report)
}

pub fn verify_perception_snapshot_integrity(
    snapshot: &PerceptionSnapshot,
) -> Result<(), EngineError> {
    if snapshot.schema != AI_PERCEPTION_SNAPSHOT_SCHEMA {
        return invalid_config(
            "perception.snapshot.schema",
            "unsupported perception schema",
        );
    }
    if snapshot.agents.is_empty() {
        return invalid_config(
            "perception.snapshot.agents",
            "must contain at least one agent",
        );
    }
    if snapshot.entities.is_empty() {
        return invalid_config(
            "perception.snapshot.entities",
            "must contain at least one entity",
        );
    }
    let actual = compute_snapshot_fnv1a64(snapshot);
    if snapshot.fnv1a64 != actual {
        return Err(EngineError::AiReadbackMismatch {
            field: "perception_snapshot.fnv1a64".to_string(),
            expected: snapshot.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

pub fn verify_perception_run_integrity(report: &PerceptionRunReport) -> Result<(), EngineError> {
    if report.schema != AI_PERCEPTION_RUN_SCHEMA {
        return invalid_config("perception.run.schema", "unsupported perception run schema");
    }
    if !report.sim_state_unchanged || report.sim_state_hash_before != report.sim_state_hash_after {
        return invalid_config(
            "perception.run.sim_state_hash",
            "sim state must be unchanged",
        );
    }
    if report.agent_count != report.agents.len() || report.agents.is_empty() {
        return invalid_config(
            "perception.run.agent_count",
            "must match non-empty agent list",
        );
    }
    for agent in &report.agents {
        verify_blackboard_integrity(&agent.blackboard)?;
    }
    verify_run_semantics(report)?;
    let actual = compute_perception_run_fnv1a64(report);
    if report.fnv1a64 != actual {
        return Err(EngineError::AiReadbackMismatch {
            field: "perception_run.fnv1a64".to_string(),
            expected: report.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

fn sense_agent(
    snapshot: &PerceptionSnapshot,
    blocker_grid: &PerceptionBlockerGrid,
    agent: &PerceptionAgent,
) -> Result<AgentPerception, EngineError> {
    let mut enemies = Vec::new();
    for entity in &snapshot.entities {
        if !entity.alive || entity.body_id == agent.body_id || entity.faction == agent.faction {
            continue;
        }
        let distance_fp = manhattan_xz_fp(agent.position, entity.position)?;
        let in_sight_range = distance_fp <= agent.sight_range_fp;
        let in_attack_range = distance_fp <= agent.attack_range_fp;
        let has_line_of_sight =
            in_sight_range && line_of_sight(agent.nav_cell, entity.nav_cell, blocker_grid)?;
        let threat = distance_fp <= agent.threat_range_fp;
        enemies.push(PerceivedEnemy {
            id: entity.id.clone(),
            body_id: entity.body_id,
            nav_cell: entity.nav_cell,
            distance_fp,
            in_sight_range,
            in_attack_range,
            has_line_of_sight,
            threat,
        });
    }
    enemies.sort_by(|left, right| {
        (left.distance_fp, left.id.as_str()).cmp(&(right.distance_fp, right.id.as_str()))
    });
    let nearest_enemy = enemies.first();
    let nearest_visible = enemies
        .iter()
        .find(|enemy| enemy.in_sight_range && enemy.has_line_of_sight);
    let sight = ids_where(&enemies, |enemy| enemy.in_sight_range);
    let attack = ids_where(&enemies, |enemy| enemy.in_attack_range);
    let los = ids_where(&enemies, |enemy| {
        enemy.in_sight_range && enemy.has_line_of_sight
    });
    let threat = ids_where(&enemies, |enemy| enemy.threat);
    let blackboard = blackboard_for_agent(BlackboardForAgentArgs {
        agent,
        tick: snapshot.tick,
        sim_state_hash: snapshot.sim_state_hash,
        nearest_visible,
        sight: &sight,
        attack: &attack,
        los: &los,
        threat: &threat,
    })?;
    Ok(AgentPerception {
        agent_id: agent.id.clone(),
        body_id: agent.body_id,
        nearest_enemy_id: nearest_enemy.map(|enemy| enemy.id.clone()),
        nearest_enemy_distance_fp: nearest_enemy.map(|enemy| enemy.distance_fp),
        nearest_visible_enemy_id: nearest_visible.map(|enemy| enemy.id.clone()),
        nearest_visible_enemy_distance_fp: nearest_visible.map(|enemy| enemy.distance_fp),
        enemies_in_sight_range: sight,
        enemies_in_attack_range: attack,
        enemies_with_line_of_sight: los,
        threat_enemy_ids: threat,
        enemies,
        blackboard,
    })
}

struct BlackboardForAgentArgs<'a> {
    agent: &'a PerceptionAgent,
    tick: u64,
    sim_state_hash: u64,
    nearest_visible: Option<&'a PerceivedEnemy>,
    sight: &'a [String],
    attack: &'a [String],
    los: &'a [String],
    threat: &'a [String],
}

fn blackboard_for_agent(args: BlackboardForAgentArgs<'_>) -> Result<AgentBlackboard, EngineError> {
    let BlackboardForAgentArgs {
        agent,
        tick,
        sim_state_hash,
        nearest_visible,
        sight,
        attack,
        los,
        threat,
    } = args;
    let has_enemy = nearest_visible.is_some();
    let in_attack_range = !attack.is_empty();
    let has_los = !los.is_empty();
    let threat_present = !threat.is_empty();
    let mut values = BTreeMap::new();
    values.insert(
        "perception.has_enemy".to_string(),
        BlackboardValue::Bool(has_enemy),
    );
    values.insert(
        "perception.sees_enemy".to_string(),
        BlackboardValue::Bool(has_enemy),
    );
    values.insert(
        "perception.in_attack_range".to_string(),
        BlackboardValue::Bool(in_attack_range),
    );
    values.insert(
        "perception.has_line_of_sight".to_string(),
        BlackboardValue::Bool(has_los),
    );
    values.insert(
        "perception.threat_present".to_string(),
        BlackboardValue::Bool(threat_present),
    );
    values.insert(
        "perception.nearest_enemy_id".to_string(),
        nearest_visible
            .map(|enemy| BlackboardValue::Text(enemy.id.clone()))
            .unwrap_or(BlackboardValue::None),
    );
    values.insert(
        "perception.nearest_enemy_distance_fp".to_string(),
        nearest_visible
            .map(|enemy| BlackboardValue::Int(enemy.distance_fp))
            .unwrap_or(BlackboardValue::None),
    );
    values.insert(
        "perception.enemies_in_sight_range".to_string(),
        BlackboardValue::TextList(sight.to_vec()),
    );
    values.insert(
        "perception.enemies_in_attack_range".to_string(),
        BlackboardValue::TextList(attack.to_vec()),
    );
    values.insert(
        "perception.enemies_with_line_of_sight".to_string(),
        BlackboardValue::TextList(los.to_vec()),
    );
    values.insert(
        "perception.threat_enemy_ids".to_string(),
        BlackboardValue::TextList(threat.to_vec()),
    );
    let leaf_state = BehaviorLeafState {
        conditions: BTreeMap::from([
            ("has_enemy".to_string(), bool_status(has_enemy)),
            ("sees_enemy".to_string(), bool_status(has_enemy)),
            ("in_attack_range".to_string(), bool_status(in_attack_range)),
            ("line_of_sight".to_string(), bool_status(has_los)),
            ("threat_present".to_string(), bool_status(threat_present)),
        ]),
        actions: BTreeMap::new(),
    };
    AgentBlackboard::new(agent.id.clone(), tick, sim_state_hash, values, leaf_state)
}

fn sorted_agents(agents: &[PerceptionAgentInput]) -> Vec<&PerceptionAgentInput> {
    let mut sorted = agents.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));
    sorted
}

fn sorted_entities(entities: &[PerceptionEntityInput]) -> Vec<&PerceptionEntityInput> {
    let mut sorted = entities.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));
    sorted
}

fn ids_where(
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

fn bool_status(value: bool) -> BehaviorStatus {
    if value {
        BehaviorStatus::Success
    } else {
        BehaviorStatus::Failure
    }
}
