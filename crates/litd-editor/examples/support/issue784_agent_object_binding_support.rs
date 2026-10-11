use std::path::Path;

use litd_editor::{
    agent_object_binding::{AgentObjectBindingRunArgs, BoundAgentObject, lower_bound_agent_orders},
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
};
use serde_json::{Value, json};

pub(crate) fn edge_no_decision(
    root: &Path,
    state_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (sim, mob, player) = duel_sim(0, 10)?;
    let args = run_args(
        root,
        state_path,
        "edge-no-decision",
        mob,
        NavCellCoord::new(0, 0),
    );
    let result = lower_bound_agent_orders(
        &args,
        &line_navmesh(11),
        &perception_for(&sim, mob, player, NavCellCoord::new(0, 0), 10, 2)?,
        &sim,
    );
    Ok(edge_report(
        "no_decision",
        result,
        "orders",
        "no deterministic order",
    ))
}

pub(crate) fn edge_illegal_order(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let state_path = root.join("edge-illegal-state-aster");
    write_agent_state(&state_path, root, &["intent.attack"], &[])?;
    let (sim, mob, player) = duel_sim(0, 2)?;
    let args = run_args(
        root,
        &state_path,
        "edge-illegal-order",
        mob,
        NavCellCoord::new(0, 0),
    );
    let result = lower_bound_agent_orders(
        &args,
        &line_navmesh(3),
        &perception_for(&sim, mob, player, NavCellCoord::new(0, 0), 2, 8)?,
        &sim,
    );
    Ok(edge_report(
        "illegal_order",
        result,
        "allowed_intents",
        "intent.chase",
    ))
}

pub(crate) fn edge_agent_death(
    root: &Path,
    state_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (mut sim, mob, player) = duel_sim(0, 2)?;
    let perception = perception_for(&sim, mob, player, NavCellCoord::new(0, 0), 2, 8)?;
    sim.despawn_body(mob)?;
    let args = run_args(
        root,
        state_path,
        "edge-agent-death",
        mob,
        NavCellCoord::new(0, 0),
    );
    let result = lower_bound_agent_orders(&args, &line_navmesh(3), &perception, &sim);
    Ok(edge_report(
        "agent_death",
        result,
        "body_id",
        "absent from sim",
    ))
}

fn edge_report<T>(label: &str, result: Result<T, EngineError>, field: &str, reason: &str) -> Value {
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
            "unexpected_error": error.to_string()
        }),
        Ok(_) => json!({"label": label, "matched": false, "unexpected": "operation succeeded"}),
    }
}

pub(crate) fn write_agent_state(
    path: &Path,
    root: &Path,
    allowed: &[&str],
    forbidden: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let state_name = path.file_name().unwrap().to_string_lossy();
    call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{state_name}-bind-ledger")),
            "state_id": "issue784-agent-state",
            "ledger_id": format!("{state_name}_bind"),
            "request_id": format!("{state_name}-bind"),
            "requested_by": "issue784-fsv",
            "binding": {
                "binding_id": "binding.guardian",
                "agent_id": "agent.guardian",
                "object_id": "npc.guardian",
                "display_name": "Guardian",
                "deterministic_order_sink": "orders.guardian",
                "activation": {"mode": "manual", "signal_refs": [], "cooldown_ticks": 1},
                "enabled": true
            }
        }),
    )?;
    call_mcp_agent_tool(
        SET_PROMPT_MODEL_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{state_name}-prompt-ledger")),
            "state_id": "issue784-agent-state",
            "ledger_id": format!("{state_name}_prompt"),
            "request_id": format!("{state_name}-prompt"),
            "requested_by": "issue784-fsv",
            "prompt_model": {
                "agent_id": "agent.guardian",
                "prompt": "Protect the lane and lower decisions to deterministic orders.",
                "model_id": "model.local.bt",
                "personality": "guardian",
                "memory_scope_refs": ["memory.guardian"]
            }
        }),
    )?;
    call_mcp_agent_tool(
        SET_WARD_ENVELOPE_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{state_name}-ward-ledger")),
            "state_id": "issue784-agent-state",
            "ledger_id": format!("{state_name}_ward"),
            "request_id": format!("{state_name}-ward"),
            "requested_by": "issue784-fsv",
            "envelope": {
                "agent_id": "agent.guardian",
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
                    "grounding_refs": ["ward.guardian"]
                }
            }
        }),
    )?;
    Ok(())
}

pub(crate) fn run_args(
    root: &Path,
    state_path: &Path,
    label: &str,
    body_id: SimBodyId,
    nav_cell: NavCellCoord,
) -> AgentObjectBindingRunArgs {
    AgentObjectBindingRunArgs {
        run_id: format!("issue784-{label}"),
        state_path: state_path.to_path_buf(),
        controller_run_path: root.join(format!("{label}-controller-aster")),
        ledger_path: root.join(format!("{label}-binding-ledger-aster")),
        object_bodies: vec![BoundAgentObject {
            object_id: "npc.guardian".to_string(),
            body_id,
            nav_cell,
            source_id: 784,
            faction: "guardian".to_string(),
            alive: true,
        }],
        behavior_tree: combat_tree(),
        action_bindings: AiControllerActionBindings {
            attack: "attack".to_string(),
            chase: "chase".to_string(),
            patrol: "patrol".to_string(),
        },
        path_max_expansions: 64,
        attack_damage_fp: 25,
    }
}

pub(crate) fn duel_sim(
    mob_x: i32,
    player_x: i32,
) -> Result<(FixedTickSim<4>, SimBodyId, SimBodyId), EngineError> {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(1)?, 784)?;
    let mob = sim.spawn_body(v(mob_x, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)?;
    let player = sim.spawn_body(v(player_x, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)?;
    Ok((sim, mob, player))
}

pub(crate) fn perception_for(
    sim: &FixedTickSim<4>,
    mob: SimBodyId,
    player: SimBodyId,
    mob_cell: NavCellCoord,
    player_x: i32,
    sight: i32,
) -> Result<litd_engine::ai::PerceptionRunReport, EngineError> {
    let snapshot = snapshot_from_sim(
        sim,
        &[PerceptionAgentInput {
            id: "agent.guardian".to_string(),
            body_id: mob,
            faction: "guardian".to_string(),
            nav_cell: mob_cell,
            sight_range_fp: Fixed::from_i32(sight).raw(),
            attack_range_fp: Fixed::from_i32(1).raw(),
            threat_range_fp: Fixed::from_i32(sight).raw(),
        }],
        &[PerceptionEntityInput {
            id: "player.one".to_string(),
            body_id: player,
            faction: "player".to_string(),
            nav_cell: NavCellCoord::new(player_x, 0),
            alive: true,
        }],
        &[],
    )?;
    sense_perception(&snapshot, sim.state_hash())
}

pub(crate) fn line_navmesh(width: i32) -> BakedNavMesh {
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

pub(crate) fn sim_state(sim: &FixedTickSim<4>, mob: SimBodyId, player: SimBodyId) -> Value {
    json!({
        "tick": sim.tick(),
        "state_hash": sim.state_hash(),
        "attack_event_count": sim.attack_event_count(),
        "mob": sim.body_state(mob),
        "player": sim.body_state(player)
    })
}

pub(crate) fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
