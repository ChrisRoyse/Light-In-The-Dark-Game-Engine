use std::path::Path;

use litd_editor::{
    agent_object_binding::{
        AgentObjectBindingRunArgs, AgentObjectBindingRunLedger, BoundAgentObject,
        lower_bound_agent_orders, write_agent_object_binding_ledger,
    },
    mcp_agent_tools::{
        BIND_AGENT_TOOL, SET_PROMPT_MODEL_TOOL, SET_WARD_ENVELOPE_TOOL, call_mcp_agent_tool,
    },
};
use litd_engine::{
    EngineError,
    ai::{
        AiControllerActionBindings, BEHAVIOR_TREE_SCHEMA, BehaviorNode, BehaviorNodeKind,
        BehaviorTree, PerceptionAgentInput, PerceptionEntityInput, sense_perception,
        snapshot_from_sim,
    },
    math::{Fixed, FixedVec3},
    nav::{
        BakedNavMesh, NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput,
        bake_navmesh,
    },
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_replay::{SimReplayOutcome, SimReplayRecord},
};
use serde_json::{Value, json};

use crate::issue788_agent_replay_world::{Issue788Unit, Issue788World};

pub(crate) fn write_agent_state(
    path: &Path,
    root: &Path,
    world: &Issue788World,
    allowed: &[&str],
    forbidden: &[&str],
) -> Result<Value, Box<dyn std::error::Error>> {
    let state_name = path.file_name().unwrap().to_string_lossy();
    let bind = call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{state_name}-bind-ledger")),
            "state_id": "issue788-agent-state",
            "ledger_id": format!("{state_name}_bind"),
            "request_id": format!("{state_name}-bind"),
            "requested_by": "issue788-fsv",
            "binding": {
                "binding_id": world.agent.binding_id,
                "agent_id": world.agent.agent_id,
                "object_id": world.agent.object_id,
                "display_name": world.agent.display_name,
                "deterministic_order_sink": world.agent.deterministic_order_sink,
                "activation": {"mode": "manual", "signal_refs": [], "cooldown_ticks": 1},
                "enabled": true
            }
        }),
    )?;
    let prompt = call_mcp_agent_tool(
        SET_PROMPT_MODEL_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{state_name}-prompt-ledger")),
            "state_id": "issue788-agent-state",
            "ledger_id": format!("{state_name}_prompt"),
            "request_id": format!("{state_name}-prompt"),
            "requested_by": "issue788-fsv",
            "prompt_model": {
                "agent_id": world.agent.agent_id,
                "prompt": world.agent.prompt,
                "model_id": world.agent.model_id,
                "personality": "guardian",
                "memory_scope_refs": ["memory.guardian", world.world_id]
            }
        }),
    )?;
    let ward = call_mcp_agent_tool(
        SET_WARD_ENVELOPE_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{state_name}-ward-ledger")),
            "state_id": "issue788-agent-state",
            "ledger_id": format!("{state_name}_ward"),
            "request_id": format!("{state_name}-ward"),
            "requested_by": "issue788-fsv",
            "envelope": {
                "agent_id": world.agent.agent_id,
                "physics": {
                    "max_move_units_per_tick": 1,
                    "allowed_region_ids": ["region.lane"],
                    "allow_teleport": false
                },
                "visual": {
                    "allowed_effect_ids": ["vfx.guardian"],
                    "max_scale_milli": 1000,
                    "max_emissive_milli": 0
                },
                "behavior": {
                    "allowed_intents": allowed,
                    "forbidden_intents": forbidden,
                    "grounding_refs": world.agent.grounding_refs
                }
            }
        }),
    )?;
    Ok(json!({ "bind": bind, "prompt": prompt, "ward": ward }))
}

pub(crate) fn instantiate_world(
    world: &Issue788World,
) -> Result<(FixedTickSim<4>, SimBodyId, SimBodyId), EngineError> {
    let agent = unit_by_role(world, "agent_body")?;
    let target = unit_by_role(world, "target")?;
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(world.ticks_per_second)?, world.sim_seed)?;
    let agent_body = sim.spawn_body(pos(agent.position), FixedVec3::ZERO, FixedVec3::ZERO)?;
    let target_body = sim.spawn_body(pos(target.position), FixedVec3::ZERO, FixedVec3::ZERO)?;
    Ok((sim, agent_body, target_body))
}

pub(crate) fn run_args(
    root: &Path,
    state_path: &Path,
    label: &str,
    world: &Issue788World,
    body_id: SimBodyId,
    nav_cell: NavCellCoord,
) -> AgentObjectBindingRunArgs {
    AgentObjectBindingRunArgs {
        run_id: format!("issue788-{label}"),
        state_path: state_path.to_path_buf(),
        controller_run_path: root.join(format!("{label}-controller-aster")),
        ledger_path: root.join(format!("{label}-binding-ledger-aster")),
        object_bodies: vec![BoundAgentObject {
            object_id: world.agent.object_id.clone(),
            body_id,
            nav_cell,
            source_id: world.agent.source_id,
            faction: world.agent.faction.clone(),
            alive: true,
        }],
        behavior_tree: combat_tree(),
        action_bindings: AiControllerActionBindings {
            attack: "attack".to_string(),
            chase: "chase".to_string(),
            patrol: "patrol".to_string(),
        },
        path_max_expansions: world.path_max_expansions,
        attack_damage_fp: world.attack_damage_fp,
    }
}

pub(crate) fn navmesh_from_world(world: &Issue788World) -> BakedNavMesh {
    line_navmesh(world.nav_width)
}

pub(crate) fn perception_for(
    world: &Issue788World,
    sim: &FixedTickSim<4>,
    agent_body: SimBodyId,
    target_body: SimBodyId,
    agent_cell: NavCellCoord,
    target_cell: NavCellCoord,
    sight_range: i32,
) -> Result<litd_engine::ai::PerceptionRunReport, EngineError> {
    let target = unit_by_role(world, "target")?;
    let snapshot = snapshot_from_sim(
        sim,
        &[PerceptionAgentInput {
            id: world.agent.agent_id.clone(),
            body_id: agent_body,
            faction: world.agent.faction.clone(),
            nav_cell: agent_cell,
            sight_range_fp: Fixed::from_i32(sight_range).raw(),
            attack_range_fp: Fixed::from_i32(1).raw(),
            threat_range_fp: Fixed::from_i32(sight_range).raw(),
        }],
        &[PerceptionEntityInput {
            id: target.object_id.clone(),
            body_id: target_body,
            faction: target.faction.clone(),
            nav_cell: target_cell,
            alive: target.alive,
        }],
        &[],
    )?;
    sense_perception(&snapshot, sim.state_hash())
}

pub(crate) fn edge_no_decision(
    root: &Path,
    state_path: &Path,
    world: &Issue788World,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (sim, agent, target) = duel_sim(world, 0, 10)?;
    let result = lower_bound_agent_orders(
        &run_args(
            root,
            state_path,
            "edge-no-decision",
            world,
            agent,
            NavCellCoord::new(0, 0),
        ),
        &line_navmesh(11),
        &perception_for(
            world,
            &sim,
            agent,
            target,
            NavCellCoord::new(0, 0),
            NavCellCoord::new(10, 0),
            2,
        )?,
        &sim,
    );
    Ok(ai_edge(
        "no_decision",
        result,
        "orders",
        "no deterministic order",
    ))
}

pub(crate) fn edge_illegal_order(
    root: &Path,
    world: &Issue788World,
) -> Result<Value, Box<dyn std::error::Error>> {
    let state_path = root.join("edge-illegal-state-aster");
    write_agent_state(&state_path, root, world, &["intent.attack"], &[])?;
    let (sim, agent, target) = duel_sim(world, 0, 2)?;
    let result = lower_bound_agent_orders(
        &run_args(
            root,
            &state_path,
            "edge-illegal-order",
            world,
            agent,
            NavCellCoord::new(0, 0),
        ),
        &line_navmesh(3),
        &perception_for(
            world,
            &sim,
            agent,
            target,
            NavCellCoord::new(0, 0),
            NavCellCoord::new(2, 0),
            world.sight_range,
        )?,
        &sim,
    );
    Ok(ai_edge(
        "illegal_ward_order",
        result,
        "allowed_intents",
        "intent.chase",
    ))
}

pub(crate) fn edge_agent_death(
    root: &Path,
    state_path: &Path,
    world: &Issue788World,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (mut sim, agent, target) = duel_sim(world, 0, 2)?;
    let perception = perception_for(
        world,
        &sim,
        agent,
        target,
        NavCellCoord::new(0, 0),
        NavCellCoord::new(2, 0),
        world.sight_range,
    )?;
    sim.despawn_body(agent)?;
    let result = lower_bound_agent_orders(
        &run_args(
            root,
            state_path,
            "edge-agent-death",
            world,
            agent,
            NavCellCoord::new(0, 0),
        ),
        &line_navmesh(3),
        &perception,
        &sim,
    );
    Ok(ai_edge("agent_death", result, "body_id", "absent from sim"))
}

pub(crate) fn edge_tampered_binding_ledger(
    root: &Path,
    ledger: &AgentObjectBindingRunLedger,
) -> Value {
    let mut tampered = ledger.clone();
    tampered.decisions[0].intent_id = "intent.tampered".to_string();
    let result = write_agent_object_binding_ledger(
        root.join("edge-tampered-binding-ledger-aster"),
        &tampered,
    );
    ai_edge(
        "tampered_binding_ledger",
        result,
        "fingerprint",
        "fingerprint mismatch",
    )
}

pub(crate) fn edge_replay_hash_mismatch(record: &SimReplayRecord) -> Value {
    let mut tampered = record.clone();
    tampered.expected_final_hash ^= 1;
    engine_edge(
        "replay_hash_mismatch",
        tampered.replay::<4, 8>(),
        "LITD-REFLECT-FINGERPRINT-MISMATCH",
        "fingerprint mismatch",
    )
}

fn ai_edge<T>(label: &str, result: Result<T, EngineError>, field: &str, reason: &str) -> Value {
    match result {
        Err(EngineError::AiInvalidConfig {
            field: actual,
            reason: actual_reason,
        }) => json!({
            "label": label,
            "matched": actual.contains(field) && actual_reason.contains(reason),
            "error_code": "LITD-AI-INVALID-CONFIG",
            "field": actual,
            "reason": actual_reason
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error_code": error.code(),
            "unexpected_error": error.to_string()
        }),
        Ok(_) => json!({"label": label, "matched": false, "unexpected": "operation succeeded"}),
    }
}

fn engine_edge<T>(label: &str, result: Result<T, EngineError>, code: &str, text: &str) -> Value {
    match result {
        Err(error) => json!({
            "label": label,
            "matched": error.code() == code && error.to_string().contains(text),
            "error_code": error.code(),
            "message": error.to_string()
        }),
        Ok(_) => json!({"label": label, "matched": false, "unexpected": "operation succeeded"}),
    }
}

pub(crate) fn replay_json(outcome: &SimReplayOutcome) -> Value {
    json!({
        "initial_hash": outcome.initial_hash,
        "final_hash": outcome.final_hash,
        "final_tick": outcome.final_tick,
        "steps": outcome.steps,
        "orders_applied": outcome.orders_applied,
        "final_snapshot_hash": outcome.final_snapshot.state_hash,
        "attack_events": outcome.final_snapshot.attack_events
    })
}

pub(crate) fn controller_summary(report: &litd_engine::ai::AiControllerRunReport) -> Value {
    json!({
        "tick": report.tick,
        "fnv1a64": report.fnv1a64,
        "agent_count": report.agent_count,
        "order_count": report.order_count,
        "actions": report.decisions.iter().map(|decision| decision.action).collect::<Vec<_>>(),
        "order_fingerprints": report
            .decisions
            .iter()
            .flat_map(|decision| decision.order_fingerprints.clone())
            .collect::<Vec<_>>()
    })
}

pub(crate) fn step_json(step: litd_engine::sim_order::SimOrderStepReport) -> Value {
    json!({
        "tick": step.tick,
        "orders_applied": step.orders_applied,
        "bodies_updated": step.bodies_updated,
        "state_hash": step.state_hash,
        "pending_orders": step.pending_orders
    })
}

pub(crate) fn sim_state(sim: &FixedTickSim<4>, agent: SimBodyId, target: SimBodyId) -> Value {
    json!({
        "tick": sim.tick(),
        "state_hash": sim.state_hash(),
        "attack_event_count": sim.attack_event_count(),
        "agent": sim.body_state(agent),
        "target": sim.body_state(target)
    })
}

pub(crate) fn nav_cell(cell: [i32; 2]) -> NavCellCoord {
    NavCellCoord::new(cell[0], cell[1])
}

pub(crate) fn unit_by_role<'a>(
    world: &'a Issue788World,
    role: &str,
) -> Result<&'a Issue788Unit, EngineError> {
    world
        .units
        .iter()
        .find(|unit| unit.role == role)
        .ok_or_else(|| EngineError::AiInvalidConfig {
            field: format!("issue788.world.units.{role}"),
            reason: "missing unit role".to_string(),
        })
}

fn duel_sim(
    world: &Issue788World,
    agent_x: i32,
    target_x: i32,
) -> Result<(FixedTickSim<4>, SimBodyId, SimBodyId), EngineError> {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(world.ticks_per_second)?, world.sim_seed)?;
    let agent = sim.spawn_body(pos([agent_x, 0, 0]), FixedVec3::ZERO, FixedVec3::ZERO)?;
    let target = sim.spawn_body(pos([target_x, 0, 0]), FixedVec3::ZERO, FixedVec3::ZERO)?;
    Ok((sim, agent, target))
}

fn line_navmesh(width: i32) -> BakedNavMesh {
    let cells = (0..width)
        .map(|x| NavTerrainCellInput {
            id: format!("cell.{x:02}"),
            x,
            z: 0,
            height_fp: 0,
            flags: NavPathingFlags::default(),
        })
        .collect::<Vec<_>>();
    bake_navmesh(&NavBakeInput::new(Fixed::ONE.raw(), cells, Vec::new())).unwrap()
}

fn combat_tree() -> BehaviorTree {
    BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes: vec![
            node(
                "root",
                BehaviorNodeKind::Selector,
                &["attack_seq", "chase"],
                None,
            ),
            node(
                "attack_seq",
                BehaviorNodeKind::Sequence,
                &["in_range", "los", "attack"],
                None,
            ),
            node(
                "in_range",
                BehaviorNodeKind::Condition,
                &[],
                Some("in_attack_range"),
            ),
            node(
                "los",
                BehaviorNodeKind::Condition,
                &[],
                Some("line_of_sight"),
            ),
            node("attack", BehaviorNodeKind::Action, &[], Some("attack")),
            node("chase", BehaviorNodeKind::Action, &[], Some("chase")),
        ],
    }
}

fn node(
    id: &str,
    kind: BehaviorNodeKind,
    children: &[&str],
    binding: Option<&str>,
) -> BehaviorNode {
    BehaviorNode {
        id: id.to_string(),
        kind,
        children: children.iter().map(|child| (*child).to_string()).collect(),
        binding: binding.map(str::to_string),
    }
}

fn pos(value: [i32; 3]) -> FixedVec3 {
    FixedVec3::from_i32s(value[0], value[1], value[2])
}
