//! Scriptable computer-player opponents lowered through the real AI controller.

use std::{collections::BTreeMap, path::Path};

use litd_engine::{
    EngineError,
    ai::{
        AiControllerAgentInput, PerceptionAgentInput, lower_ai_controller_orders, sense_perception,
        snapshot_from_sim, write_ai_controller_run_json,
    },
    calyx_json_store::{read_calyx_json_bytes, write_calyx_json_bytes},
    math::Fixed,
    nav::BakedNavMesh,
    sim::FixedTickSim,
};
use litd_hash::sha256_hex;

use crate::{
    history::stable_fingerprint, mcp_agent_tools::mcp_agent_tools_store::read_mcp_agent_state,
};

#[path = "computer_opponents_batch.rs"]
mod computer_opponents_batch;
#[path = "computer_opponents_support.rs"]
mod computer_opponents_support;
#[path = "computer_opponents_types.rs"]
mod computer_opponents_types;

pub use computer_opponents_batch::{
    BatchedComputerOpponentLedger, BatchedLedgerReadback, COMPUTER_OPPONENT_BATCH_SCHEMA,
    ComputerOpponentLedgerBatch, read_batch as read_computer_opponent_ledger_batch,
};

use computer_opponents_support::{
    RuntimeStateIndex, action_bindings, behavior_tree, call_agent_mutation, invalid_opponent,
    ledger_spec, opponent_index, sorted_opponents, target_inputs, validate_authoring_args,
    validate_controller_decisions, validate_ledger, validate_live_object, validate_path,
    validate_runtime_args,
};
use computer_opponents_types::COMPUTER_OPPONENT_ISSUE;
pub use computer_opponents_types::{
    COMPUTER_OPPONENT_LEDGER_SCHEMA, ComputerOpponentAuthoringArgs,
    ComputerOpponentAuthoringReadback, ComputerOpponentDecisionLedger, ComputerOpponentDefinition,
    ComputerOpponentObject, ComputerOpponentRunLedger, ComputerOpponentRunReadback,
    ComputerOpponentRuntimeArgs, ComputerOpponentScript, ComputerOpponentTarget,
    ComputerOpponentToolLedgerSummary,
};

pub fn author_computer_opponents(
    args: &ComputerOpponentAuthoringArgs,
) -> Result<ComputerOpponentAuthoringReadback, EngineError> {
    validate_authoring_args(args)?;
    std::fs::create_dir_all(&args.ledger_dir)
        .map_err(|error| invalid_opponent("ledger_dir", error.to_string()))?;
    let mut ledgers = Vec::new();
    for opponent in sorted_opponents(&args.opponents) {
        ledgers.extend(call_agent_mutation(args, opponent)?);
    }
    let state = read_mcp_agent_state(&args.state_path)?;
    Ok(ComputerOpponentAuthoringReadback {
        state_path: state.path,
        state_sha256: state.sha256,
        state_fingerprint: state.state_fingerprint,
        binding_count: state.binding_count,
        prompt_model_count: state.prompt_model_count,
        ward_envelope_count: state.ward_envelope_count,
        operation_count: ledgers.len(),
        ledgers,
    })
}

pub fn lower_computer_opponent_orders<const MAX_BODIES: usize>(
    args: &ComputerOpponentRuntimeArgs,
    navmesh: &BakedNavMesh,
    sim: &FixedTickSim<MAX_BODIES>,
) -> Result<ComputerOpponentRunReadback, EngineError> {
    validate_runtime_args(args)?;
    let state = read_mcp_agent_state(&args.state_path)?;
    let opponents = opponent_index(&args.opponents)?;
    let state_index = RuntimeStateIndex::new(&state.state);
    let objects = runtime_object_index(&args.objects)?;
    let mut controller_inputs = Vec::new();
    let mut perception_inputs = Vec::new();

    for opponent in sorted_opponents(&args.opponents)
        .into_iter()
        .filter(|opponent| opponent.enabled)
    {
        state_index.require_complete(opponent)?;
        let object = objects.get(opponent.object_id.as_str()).ok_or_else(|| {
            invalid_opponent(
                format!("opponents.{}.object_id", opponent.agent_id),
                "computer opponent object is missing from runtime body list",
            )
        })?;
        validate_live_object(opponent, object, sim)?;
        perception_inputs.push(PerceptionAgentInput {
            id: opponent.agent_id.clone(),
            body_id: object.body_id,
            faction: object.faction.clone(),
            nav_cell: object.nav_cell,
            sight_range_fp: Fixed::from_i32(opponent.script.sight_range_cells).raw(),
            attack_range_fp: Fixed::from_i32(opponent.script.attack_range_cells).raw(),
            threat_range_fp: Fixed::from_i32(opponent.script.threat_range_cells).raw(),
        });
        controller_inputs.push(AiControllerAgentInput {
            id: opponent.agent_id.clone(),
            source_id: opponent.source_id,
            body_id: object.body_id,
            nav_cell: object.nav_cell,
            behavior_tree: behavior_tree(&opponent.script)?,
            path_max_expansions: opponent.script.path_max_expansions,
            attack_damage_fp: opponent.script.attack_damage_fp,
            action_bindings: action_bindings(),
        });
    }

    let entity_inputs = target_inputs(&args.targets, sim)?;
    let snapshot = snapshot_from_sim(sim, &perception_inputs, &entity_inputs, &[])?;
    let perception = sense_perception(&snapshot, sim.state_hash())?;
    let controller = lower_ai_controller_orders(navmesh, &perception, &controller_inputs)?;
    let decisions =
        validate_controller_decisions(&controller.decisions, &opponents, &state.state, sim)?;
    let controller_readback = write_ai_controller_run_json(&args.controller_run_path, &controller)?;

    let mut ledger = ComputerOpponentRunLedger {
        schema: COMPUTER_OPPONENT_LEDGER_SCHEMA.to_string(),
        issue: COMPUTER_OPPONENT_ISSUE,
        run_id: args.run_id.clone(),
        source_of_truth:
            "MCP agent computer-player state + AI controller run + computer opponent ledger row"
                .to_string(),
        mcp_agent_state_path: state.path,
        mcp_agent_state_sha256: state.sha256,
        mcp_agent_state_fingerprint: state.state_fingerprint,
        controller_run_path: controller_readback.path.display().to_string(),
        controller_sha256: controller_readback.sha256,
        controller_fnv1a64: controller_readback.fnv1a64,
        perception_fnv1a64: perception.fnv1a64,
        navmesh_fnv1a64: navmesh.fnv1a64.clone(),
        tick: controller.tick,
        opponent_count: controller_inputs.len(),
        target_count: args.targets.len(),
        decision_count: decisions.len(),
        order_count: controller.order_count,
        decisions,
        ledger_fingerprint: String::new(),
    };
    ledger.ledger_fingerprint = ledger_fingerprint(&ledger)?;
    write_computer_opponent_ledger(&args.ledger_path, &ledger)
}

pub fn read_computer_opponent_ledger(
    path: impl AsRef<Path>,
) -> Result<ComputerOpponentRunReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&ledger_spec(path))
        .map_err(|error| invalid_opponent("ledger.read", error.to_string()))?
        .ok_or_else(|| {
            invalid_opponent(
                "ledger.read",
                format!("missing computer opponent ledger at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let ledger = serde_json::from_slice::<ComputerOpponentRunLedger>(&bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "computer-opponent-ledger-json",
            reason: format!("parse computer opponent ledger: {error}"),
        }
    })?;
    validate_ledger(&ledger)?;
    Ok(ComputerOpponentRunReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

// Tool-cadence write path (issue #1524 item 3): serialize -> write -> re-read ->
// re-parse -> re-hash -> deep-compare on EVERY call. This full-state verification is
// the CORRECT behavior for a tool invocation and is deliberately kept. It must NOT be
// promoted to a per-AI-decision tick as-is; the per-tick-safe path with in-memory
// accumulation and a bounded cadence flush is `BatchedComputerOpponentLedger` in
// `computer_opponents_batch.rs`, which amortizes the durable write + full verification
// across a batch without dropping durability.
fn write_computer_opponent_ledger(
    path: impl AsRef<Path>,
    ledger: &ComputerOpponentRunLedger,
) -> Result<ComputerOpponentRunReadback, EngineError> {
    validate_ledger(ledger)?;
    let path = path.as_ref();
    let bytes =
        serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
            format: "computer-opponent-ledger-json",
            reason: format!("serialize computer opponent ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&ledger_spec(path), &bytes)
        .map_err(|error| invalid_opponent("ledger.write", error.to_string()))?;
    if stored.bytes != bytes {
        return Err(invalid_opponent(
            "ledger.write",
            "Aster computer opponent ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_computer_opponent_ledger(path)?;
    if readback.ledger != *ledger {
        return Err(invalid_opponent("ledger.readback", "ledger mismatch"));
    }
    Ok(readback)
}

fn runtime_object_index(
    objects: &[ComputerOpponentObject],
) -> Result<BTreeMap<&str, &ComputerOpponentObject>, EngineError> {
    let mut index = BTreeMap::new();
    for object in objects {
        if index.insert(object.object_id.as_str(), object).is_some() {
            return Err(invalid_opponent("object.object_id", "duplicate object"));
        }
    }
    Ok(index)
}

fn ledger_fingerprint(ledger: &ComputerOpponentRunLedger) -> Result<String, EngineError> {
    let mut expected = ledger.clone();
    expected.ledger_fingerprint.clear();
    stable_fingerprint(&expected)
}

#[cfg(test)]
#[path = "computer_opponents_tests.rs"]
mod tests;
