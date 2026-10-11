use std::{fs, path::Path};

use litd_editor::{
    agent_object_binding::{AgentObjectBindingRunArgs, BoundAgentObject},
    mcp_agent_tools::{
        AUTHOR_AGENT_TOOL, AgentActivationPolicy, call_mcp_agent_tool, read_mcp_agent_state,
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
use litd_hash::sha256_hex;
use serde_json::{Value, json};

pub(crate) fn write_noncoder_request(
    path: &Path,
    root: &Path,
    state_path: &Path,
    ledger_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let request = author_request(
        state_path,
        ledger_path,
        "issue860-author-happy",
        "tiny",
        "guardian_lane",
        "Guard the lane, greet travelers, and act only through deterministic game orders.",
        manual_activation(),
    );
    let bytes = serde_json::to_vec_pretty(&request)?;
    fs::write(path, &bytes)?;
    let readback = serde_json::from_slice::<Value>(&fs::read(path)?)?;
    if readback != request {
        return Err("issue860 non-coder request readback mismatch".into());
    }
    write_world_source(root, &request)?;
    Ok(readback)
}

fn write_world_source(root: &Path, request: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let world = json!({
        "schema": "litd.editor.issue860.no_code_agent_world.v1",
        "issue": 860,
        "source_of_truth": "non-coder authoring request plus synthetic character/object world",
        "readme_refs": [
            "docs/readme/ED03-object-editor.png",
            "docs/readme/ED04-trigger-editor.png",
            "docs/readme/ED05-ai-coauthor.png",
            "docs/readme/S17-agent-binding.png",
            "docs/readme/03-mcp-agent.png"
        ],
        "object": {
            "object_id": request["object_id"],
            "kind": "Character",
            "position": [0, 0, 0],
            "nav_cell": [0, 0]
        },
        "target": {
            "object_id": "player.one",
            "kind": "Unit",
            "position": [2, 0, 0],
            "nav_cell": [2, 0]
        },
        "visual_binding": request["visual"],
        "model_tier": request["model_tier"],
        "behavior_preset": request["behavior_preset"]
    });
    let path = root.join("no-code-agent-world-source.json");
    fs::write(&path, serde_json::to_vec_pretty(&world)?)?;
    let readback = serde_json::from_slice::<Value>(&fs::read(&path)?)?;
    if readback != world {
        return Err("issue860 world source readback mismatch".into());
    }
    Ok(())
}

pub(crate) fn call_author_agent(request: Value) -> Result<Value, EngineError> {
    serde_json::to_value(call_mcp_agent_tool(AUTHOR_AGENT_TOOL, request)?).map_err(|error| {
        EngineError::SerializationFailed {
            format: "issue860-author-agent-json",
            reason: format!("serialize author-agent output: {error}"),
        }
    })
}

pub(crate) fn author_request(
    state_path: &Path,
    ledger_path: &Path,
    request_id: &str,
    model_tier: &str,
    behavior_preset: &str,
    prompt: &str,
    activation: AgentActivationPolicy,
) -> Value {
    json!({
        "state_path": state_path,
        "ledger_path": ledger_path,
        "state_id": "issue860-agent-state",
        "ledger_id": format!("{request_id}-ledger"),
        "request_id": request_id,
        "requested_by": "noncoder.synapse.fsv",
        "agent_id": "agent.friendly_warden",
        "object_id": "npc.friendly_warden",
        "display_name": "Friendly Warden",
        "prompt": prompt,
        "model_tier": model_tier,
        "behavior_preset": behavior_preset,
        "personality": "curious",
        "region_id": "region.lane",
        "activation": activation,
        "visual": {
            "character_ref": "character.friendly_warden",
            "visual_ref": "vfx.agent_brain_binding",
            "particle_ref": "particle.agent_spark"
        }
    })
}

pub(crate) fn manual_activation() -> AgentActivationPolicy {
    AgentActivationPolicy {
        mode: "manual".to_string(),
        signal_refs: Vec::new(),
        cooldown_ticks: 1,
    }
}

pub(crate) fn signal_activation_without_refs() -> AgentActivationPolicy {
    AgentActivationPolicy {
        mode: "signal".to_string(),
        signal_refs: Vec::new(),
        cooldown_ticks: 1,
    }
}

pub(crate) fn state_summary(path: &Path) -> Value {
    match read_mcp_agent_state(path) {
        Ok(readback) => json!({
            "exists": path.exists(),
            "sha256": readback.sha256,
            "state_fingerprint": readback.state_fingerprint,
            "binding_count": readback.binding_count,
            "prompt_model_count": readback.prompt_model_count,
            "ward_envelope_count": readback.ward_envelope_count,
            "enabled_binding_count": readback.enabled_binding_count
        }),
        Err(error) => json!({
            "exists": path.exists(),
            "error_code": error.code(),
            "error": error.to_string()
        }),
    }
}

pub(crate) fn edge_report(
    label: &str,
    before: Value,
    result: Result<Value, EngineError>,
    after: Value,
    expected_text: &str,
) -> Value {
    match result {
        Err(error) => json!({
            "label": label,
            "matched": before == after
                && error.code() == "LITD-RENDER-UI-INVALID-CONFIG"
                && error.to_string().contains(expected_text),
            "before": before,
            "after": after,
            "error_code": error.code(),
            "error": error.to_string()
        }),
        Ok(value) => json!({
            "label": label,
            "matched": false,
            "before": before,
            "after": after,
            "unexpected": value
        }),
    }
}

pub(crate) fn request_file_readback(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
        "json": serde_json::from_slice::<Value>(&bytes)?
    }))
}

pub(crate) fn duel_sim() -> Result<(FixedTickSim<4>, SimBodyId, SimBodyId), EngineError> {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(1)?, 860)?;
    let agent = sim.spawn_body(v(0, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)?;
    let target = sim.spawn_body(v(2, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)?;
    Ok((sim, agent, target))
}

pub(crate) fn run_args(
    root: &Path,
    state_path: &Path,
    label: &str,
    body_id: SimBodyId,
    nav_cell: NavCellCoord,
) -> AgentObjectBindingRunArgs {
    AgentObjectBindingRunArgs {
        run_id: format!("issue860-{label}"),
        state_path: state_path.to_path_buf(),
        controller_run_path: root.join(format!("{label}-controller-aster")),
        ledger_path: root.join(format!("{label}-binding-ledger-aster")),
        object_bodies: vec![BoundAgentObject {
            object_id: "npc.friendly_warden".to_string(),
            body_id,
            nav_cell,
            source_id: 860,
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

pub(crate) fn perception_for(
    sim: &FixedTickSim<4>,
    agent: SimBodyId,
    target: SimBodyId,
    agent_cell: NavCellCoord,
) -> Result<litd_engine::ai::PerceptionRunReport, EngineError> {
    let target_cell = NavCellCoord::new(2, 0);
    let snapshot = snapshot_from_sim(
        sim,
        &[PerceptionAgentInput {
            id: "agent.friendly_warden".to_string(),
            body_id: agent,
            faction: "guardian".to_string(),
            nav_cell: agent_cell,
            sight_range_fp: Fixed::from_i32(8).raw(),
            attack_range_fp: Fixed::from_i32(1).raw(),
            threat_range_fp: Fixed::from_i32(8).raw(),
        }],
        &[PerceptionEntityInput {
            id: "player.one".to_string(),
            body_id: target,
            faction: "player".to_string(),
            nav_cell: target_cell,
            alive: true,
        }],
        &[],
    )?;
    sense_perception(&snapshot, sim.state_hash())
}

pub(crate) fn line_navmesh() -> BakedNavMesh {
    let cells = (0..3)
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

pub(crate) fn sim_state(sim: &FixedTickSim<4>, agent: SimBodyId, target: SimBodyId) -> Value {
    json!({
        "tick": sim.tick(),
        "state_hash": sim.state_hash(),
        "attack_event_count": sim.attack_event_count(),
        "agent": sim.body_state(agent),
        "target": sim.body_state(target)
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

pub(crate) fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
