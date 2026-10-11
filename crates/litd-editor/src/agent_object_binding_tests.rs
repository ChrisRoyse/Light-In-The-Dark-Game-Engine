use std::path::Path;

use litd_engine::{
    EngineError,
    ai::{
        AiControllerAction, AiControllerActionBindings, BEHAVIOR_TREE_SCHEMA, BehaviorNode,
        BehaviorNodeKind, BehaviorTree, PerceptionAgentInput, PerceptionEntityInput,
        load_ai_controller_run_json, sense_perception, snapshot_from_sim,
    },
    fsv_artifacts::begin_issue_run,
    math::{Fixed, FixedVec3},
    nav::{
        BakedNavMesh, NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput,
        bake_navmesh,
    },
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::SimOrderQueue,
};
use serde_json::json;

use crate::{
    agent_object_binding::{
        AgentObjectBindingRunArgs, BoundAgentObject, lower_bound_agent_orders,
        read_agent_object_binding_ledger,
    },
    mcp_agent_tools::{
        BIND_AGENT_TOOL, SET_PROMPT_MODEL_TOOL, SET_WARD_ENVELOPE_TOOL, call_mcp_agent_tool,
    },
};

#[test]
fn agent_object_binding_lowers_from_persisted_state_and_ledgers() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let run = begin_issue_run("issue784_agent_object_binding_unit").unwrap();
    let root = run.run_dir();
    let state_path = root.join("agent-state-aster");
    write_agent_state(&state_path, root, &["intent.attack", "intent.chase"], &[]);
    let navmesh = line_navmesh(3);
    let (mut sim, mob, player) = duel_sim(0, 2);

    let args0 = run_args(root, &state_path, "tick0", mob, NavCellCoord::new(0, 0));
    let perception0 = perception_for(&sim, mob, player, NavCellCoord::new(0, 0), 2, 8);
    let rb0 = lower_bound_agent_orders(&args0, &navmesh, &perception0, &sim).unwrap();
    assert_eq!(rb0.ledger.order_count, 1);
    assert_eq!(rb0.ledger.decisions[0].action, AiControllerAction::Chase);
    assert_eq!(
        rb0,
        read_agent_object_binding_ledger(&args0.ledger_path).unwrap()
    );

    let mut queue = SimOrderQueue::<8>::default();
    let controller0 = load_ai_controller_run_json(&args0.controller_run_path).unwrap();
    for order in &controller0.report.orders {
        queue.enqueue(*order).unwrap();
    }
    queue.step(&mut sim).unwrap();
    assert_eq!(sim.body_state(mob).unwrap().position, v(1, 0, 0));

    let args1 = run_args(root, &state_path, "tick1", mob, NavCellCoord::new(1, 0));
    let perception1 = perception_for(&sim, mob, player, NavCellCoord::new(1, 0), 2, 8);
    let rb1 = lower_bound_agent_orders(&args1, &navmesh, &perception1, &sim).unwrap();
    assert_eq!(rb1.ledger.order_count, 2);
    assert_eq!(rb1.ledger.decisions[0].action, AiControllerAction::Attack);
    let controller1 = load_ai_controller_run_json(&args1.controller_run_path).unwrap();
    for order in &controller1.report.orders {
        queue.enqueue(*order).unwrap();
    }
    queue.step(&mut sim).unwrap();
    assert_eq!(sim.attack_event_count(), 1);
}

#[test]
fn agent_object_binding_edges_fail_loud() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let run = begin_issue_run("issue784_agent_object_binding_edges").unwrap();
    let root = run.run_dir();
    let state_path = root.join("agent-state-aster");
    write_agent_state(&state_path, root, &["intent.attack", "intent.chase"], &[]);
    let (sim, mob, player) = duel_sim(0, 10);
    let no_decision = lower_bound_agent_orders(
        &run_args(
            root,
            &state_path,
            "no-decision",
            mob,
            NavCellCoord::new(0, 0),
        ),
        &line_navmesh(11),
        &perception_for(&sim, mob, player, NavCellCoord::new(0, 0), 10, 2),
        &sim,
    );
    assert_edge(no_decision, "orders", "no deterministic order");

    let illegal_state = root.join("illegal-state-aster");
    write_agent_state(&illegal_state, root, &["intent.attack"], &[]);
    let (sim, mob, player) = duel_sim(0, 2);
    let illegal = lower_bound_agent_orders(
        &run_args(
            root,
            &illegal_state,
            "illegal-order",
            mob,
            NavCellCoord::new(0, 0),
        ),
        &line_navmesh(3),
        &perception_for(&sim, mob, player, NavCellCoord::new(0, 0), 2, 8),
        &sim,
    );
    assert_edge(illegal, "allowed_intents", "intent.chase");

    let (mut dead_sim, dead_mob, dead_player) = duel_sim(0, 2);
    let dead_perception = perception_for(
        &dead_sim,
        dead_mob,
        dead_player,
        NavCellCoord::new(0, 0),
        2,
        8,
    );
    dead_sim.despawn_body(dead_mob).unwrap();
    let dead = lower_bound_agent_orders(
        &run_args(
            root,
            &state_path,
            "agent-death",
            dead_mob,
            NavCellCoord::new(0, 0),
        ),
        &line_navmesh(3),
        &dead_perception,
        &dead_sim,
    );
    assert_edge(dead, "body_id", "absent from sim");
}

fn write_agent_state(path: &Path, root: &Path, allowed: &[&str], forbidden: &[&str]) {
    call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{}-bind-ledger", path.file_name().unwrap().to_string_lossy())),
            "state_id": "issue784-agent-state",
            "ledger_id": format!("{}_bind", path.file_name().unwrap().to_string_lossy()),
            "request_id": "issue784-bind",
            "requested_by": "issue784-test",
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
    )
    .unwrap();
    call_mcp_agent_tool(
        SET_PROMPT_MODEL_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{}-prompt-ledger", path.file_name().unwrap().to_string_lossy())),
            "state_id": "issue784-agent-state",
            "ledger_id": format!("{}_prompt", path.file_name().unwrap().to_string_lossy()),
            "request_id": "issue784-prompt",
            "requested_by": "issue784-test",
            "prompt_model": {
                "agent_id": "agent.guardian",
                "prompt": "Protect the lane and lower decisions to deterministic orders.",
                "model_id": "model.local.bt",
                "personality": "guardian",
                "memory_scope_refs": ["memory.guardian"]
            }
        }),
    )
    .unwrap();
    call_mcp_agent_tool(
        SET_WARD_ENVELOPE_TOOL,
        json!({
            "state_path": path,
            "ledger_path": root.join(format!("{}-ward-ledger", path.file_name().unwrap().to_string_lossy())),
            "state_id": "issue784-agent-state",
            "ledger_id": format!("{}_ward", path.file_name().unwrap().to_string_lossy()),
            "request_id": "issue784-ward",
            "requested_by": "issue784-test",
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
    )
    .unwrap();
}

fn run_args(
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

fn duel_sim(mob_x: i32, player_x: i32) -> (FixedTickSim<4>, SimBodyId, SimBodyId) {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(1).unwrap(), 784).unwrap();
    let mob = sim
        .spawn_body(v(mob_x, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)
        .unwrap();
    let player = sim
        .spawn_body(v(player_x, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)
        .unwrap();
    (sim, mob, player)
}

fn perception_for(
    sim: &FixedTickSim<4>,
    mob: SimBodyId,
    player: SimBodyId,
    mob_cell: NavCellCoord,
    player_x: i32,
    sight: i32,
) -> litd_engine::ai::PerceptionRunReport {
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
    )
    .unwrap();
    sense_perception(&snapshot, sim.state_hash()).unwrap()
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

fn assert_edge<T>(result: Result<T, EngineError>, field: &str, reason: &str) {
    match result {
        Err(EngineError::AiInvalidConfig {
            field: actual,
            reason: actual_reason,
        }) => {
            assert!(actual.contains(field), "{actual} did not contain {field}");
            assert!(
                actual_reason.contains(reason),
                "{actual_reason} did not contain {reason}"
            );
        }
        Err(error) => panic!("expected AiInvalidConfig, got {error:?}"),
        Ok(_) => panic!("expected AiInvalidConfig, got Ok"),
    }
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
