use std::fs;

use litd_editor::computer_opponents::{
    ComputerOpponentAuthoringArgs, ComputerOpponentDefinition, ComputerOpponentObject,
    ComputerOpponentRunReadback, ComputerOpponentRuntimeArgs, ComputerOpponentScript,
    ComputerOpponentTarget, author_computer_opponents, lower_computer_opponent_orders,
};
use litd_engine::{
    EngineError,
    ai::{AiControllerRunFileReadback, load_ai_controller_run_json},
    math::{Fixed, FixedVec3},
    nav::{
        BakedNavMesh, NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput,
        bake_navmesh,
    },
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_replay::{SimReplayRecord, SimSnapshot},
};
use serde_json::{Value, json};

pub(crate) struct TickReadback {
    pub(crate) controller_path: std::path::PathBuf,
    pub(crate) ledger_path: std::path::PathBuf,
    pub(crate) controller: AiControllerRunFileReadback,
    pub(crate) ledger: ComputerOpponentRunReadback,
}

// FSV harness driver threading the full opponent-tick context; grouping into a struct would only
// re-spell these domain inputs. Scoped allow keeps the harness call sites and fingerprint intact.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_opponent_tick(
    root: &std::path::Path,
    label: &str,
    state_path: &std::path::Path,
    opponents: &[ComputerOpponentDefinition],
    objects: Vec<ComputerOpponentObject>,
    targets: Vec<ComputerOpponentTarget>,
    navmesh: &BakedNavMesh,
    sim: &FixedTickSim<8>,
) -> Result<TickReadback, EngineError> {
    let controller_path = root.join(format!("{label}-controller-aster"));
    let ledger_path = root.join(format!("{label}-computer-opponent-ledger-aster"));
    let ledger = lower_computer_opponent_orders(
        &ComputerOpponentRuntimeArgs {
            run_id: format!("issue852-{label}"),
            state_path: state_path.to_path_buf(),
            controller_run_path: controller_path.clone(),
            ledger_path: ledger_path.clone(),
            opponents: opponents.to_vec(),
            objects,
            targets,
        },
        navmesh,
        sim,
    )?;
    let controller = load_ai_controller_run_json(&controller_path)?;
    Ok(TickReadback {
        controller_path,
        ledger_path,
        controller,
        ledger,
    })
}

pub(crate) fn edge_duplicate_player(
    root: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut duplicate = opponents();
    duplicate[1].player_id = duplicate[0].player_id;
    let state_path = root.join("edge-duplicate-player-state-aster");
    let result = author_computer_opponents(&ComputerOpponentAuthoringArgs {
        state_path: state_path.clone(),
        ledger_dir: root.join("edge-duplicate-authoring-ledgers"),
        state_id: "issue852_edge_duplicate".to_string(),
        requested_by: "issue852_fsv".to_string(),
        opponents: duplicate,
    });
    Ok(edge_report(
        "duplicate_player_rejected",
        result.map(|_| ()),
        "opponent.player_id",
        "duplicate player id",
        json!({"state_path_exists_after": state_path.exists()}),
    ))
}

pub(crate) fn edge_forbidden_chase(
    root: &std::path::Path,
    navmesh: &BakedNavMesh,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut only_attack = vec![opponent(
        2,
        "North Computer",
        "agent.edge_north",
        "unit.edge_north",
        "north",
        8852,
    )];
    only_attack[0].script.allowed_intents = vec!["intent.attack".to_string()];
    let state_path = root.join("edge-forbidden-chase-state-aster");
    let authoring = author_computer_opponents(&ComputerOpponentAuthoringArgs {
        state_path: state_path.clone(),
        ledger_dir: root.join("edge-forbidden-authoring-ledgers"),
        state_id: "issue852_edge_forbidden".to_string(),
        requested_by: "issue852_fsv".to_string(),
        opponents: only_attack.clone(),
    })?;
    let (sim, north, _, human) = duel_sim()?;
    let result = run_opponent_tick(
        root,
        "edge-forbidden-chase",
        &state_path,
        &only_attack,
        vec![ComputerOpponentObject {
            object_id: "unit.edge_north".to_string(),
            body_id: north,
            nav_cell: NavCellCoord::new(0, 0),
            faction: "north".to_string(),
            source_id: 8852,
            alive: true,
        }],
        targets(human),
        navmesh,
        &sim,
    );
    Ok(edge_report(
        "forbidden_chase_rejected",
        result.map(|_| ()),
        "allowed_intents",
        "intent.chase",
        json!({"state_sha256_before_failure": authoring.state_sha256}),
    ))
}

// FSV edge-case driver threading the full opponent context; struct-of-args would duplicate inputs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn edge_dead_opponent(
    root: &std::path::Path,
    state_path: &std::path::Path,
    opponents: &[ComputerOpponentDefinition],
    navmesh: &BakedNavMesh,
    sim: &FixedTickSim<8>,
    north: SimBodyId,
    south: SimBodyId,
    human: SimBodyId,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut dead = objects(north, south, 0, 4);
    dead[0].alive = false;
    let result = run_opponent_tick(
        root,
        "edge-dead-opponent",
        state_path,
        opponents,
        dead,
        targets(human),
        navmesh,
        sim,
    );
    Ok(edge_report(
        "dead_opponent_rejected",
        result.map(|_| ()),
        "object.alive",
        "dead",
        json!({}),
    ))
}

pub(crate) fn opponents() -> Vec<ComputerOpponentDefinition> {
    vec![
        opponent(
            2,
            "North Computer",
            "agent.computer_north",
            "unit.computer_north",
            "north",
            8522,
        ),
        opponent(
            3,
            "South Computer",
            "agent.computer_south",
            "unit.computer_south",
            "south",
            8523,
        ),
    ]
}

pub(crate) fn objects(
    north: SimBodyId,
    south: SimBodyId,
    north_x: i32,
    south_x: i32,
) -> Vec<ComputerOpponentObject> {
    vec![
        ComputerOpponentObject {
            object_id: "unit.computer_north".to_string(),
            body_id: north,
            nav_cell: NavCellCoord::new(north_x, 0),
            faction: "north".to_string(),
            source_id: 8522,
            alive: true,
        },
        ComputerOpponentObject {
            object_id: "unit.computer_south".to_string(),
            body_id: south,
            nav_cell: NavCellCoord::new(south_x, 0),
            faction: "south".to_string(),
            source_id: 8523,
            alive: true,
        },
    ]
}

pub(crate) fn targets(human: SimBodyId) -> Vec<ComputerOpponentTarget> {
    vec![ComputerOpponentTarget {
        entity_id: "player.human".to_string(),
        body_id: human,
        nav_cell: NavCellCoord::new(2, 0),
        faction: "human".to_string(),
        alive: true,
    }]
}

pub(crate) fn duel_sim() -> Result<(FixedTickSim<8>, SimBodyId, SimBodyId, SimBodyId), EngineError>
{
    let mut sim = FixedTickSim::<8>::new(SimConfig::new(1)?, 852)?;
    let north = sim.spawn_body(v(0, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)?;
    let south = sim.spawn_body(v(4, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)?;
    let human = sim.spawn_body(v(2, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)?;
    Ok((sim, north, south, human))
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

pub(crate) fn sim_state_after_step(
    step: Value,
    sim: &FixedTickSim<8>,
    north: SimBodyId,
    south: SimBodyId,
    human: SimBodyId,
) -> Value {
    let mut state = sim_state(sim, north, south, human);
    state["step"] = step;
    state
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

pub(crate) fn sim_state(
    sim: &FixedTickSim<8>,
    north: SimBodyId,
    south: SimBodyId,
    human: SimBodyId,
) -> Value {
    json!({
        "tick": sim.tick(),
        "state_hash": sim.state_hash(),
        "attack_event_count": sim.attack_event_count(),
        "north": sim.body_state(north),
        "south": sim.body_state(south),
        "human": sim.body_state(human)
    })
}

pub(crate) fn write_replay(
    replay_path: &std::path::Path,
    replay: &SimReplayRecord,
) -> Result<SimReplayRecord, Box<dyn std::error::Error>> {
    fs::write(replay_path, replay.to_ron_string()?)?;
    Ok(SimReplayRecord::from_ron_str(&fs::read_to_string(
        replay_path,
    )?)?)
}

pub(crate) fn write_snapshot(
    snapshot_path: &std::path::Path,
    snapshot: &SimSnapshot,
) -> Result<SimSnapshot, Box<dyn std::error::Error>> {
    fs::write(snapshot_path, snapshot.to_ron_string()?)?;
    Ok(SimSnapshot::from_ron_str(&fs::read_to_string(
        snapshot_path,
    )?)?)
}

fn opponent(
    player_id: u8,
    player_name: &str,
    agent_id: &str,
    object_id: &str,
    faction: &str,
    source_id: u32,
) -> ComputerOpponentDefinition {
    ComputerOpponentDefinition {
        player_id,
        player_name: player_name.to_string(),
        agent_id: agent_id.to_string(),
        object_id: object_id.to_string(),
        display_name: player_name.to_string(),
        faction: faction.to_string(),
        source_id,
        region_id: "region.arena".to_string(),
        enabled: true,
        script: ComputerOpponentScript {
            script_id: format!("script.{faction}.melee"),
            behavior_preset: "melee_guardian".to_string(),
            prompt: format!("{player_name} guards its lane and attacks hostile units."),
            model_id: "model.local.tiny_agent".to_string(),
            personality: format!("{faction}_melee"),
            allowed_intents: vec!["intent.attack".to_string(), "intent.chase".to_string()],
            forbidden_intents: vec!["intent.attack_civilian".to_string()],
            sight_range_cells: 8,
            attack_range_cells: 1,
            threat_range_cells: 8,
            path_max_expansions: 64,
            attack_damage_fp: 21,
        },
    }
}

fn edge_report<T>(
    label: &str,
    result: Result<T, EngineError>,
    field: &str,
    reason: &str,
    extra: Value,
) -> Value {
    match result {
        Err(EngineError::AiInvalidConfig {
            field: actual,
            reason: actual_reason,
        }) => json!({
            "label": label,
            "matched": actual.contains(field) && actual_reason.contains(reason),
            "error_code": "LITD-AI-INVALID-CONFIG",
            "field": actual,
            "reason": actual_reason,
            "extra": extra
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
            "extra": extra
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded",
            "extra": extra
        }),
    }
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
