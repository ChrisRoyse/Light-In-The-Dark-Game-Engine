use std::fs;

use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    net_authority::build_net_authority_proof,
    sim::{FixedTickSim, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource, SimOrderSourceKind},
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();

    let run = begin_issue_run("issue783_agent_lowering_boundary")?;
    let config = SimConfig::new(64)?;
    let mut sim = FixedTickSim::<3>::new(config, 0x783)?;
    let agent_body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    let human_body = sim.spawn_body(v(4, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    sim.spawn_body(v(-4, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    let initial_snapshot = SimSnapshot::capture(&sim);

    let agent_order = SimOrder::new(
        1,
        SimOrderSource::lowered_agent(783),
        1,
        agent_body,
        SimOrderKind::SetVelocity(v(64, 0, 0)),
    );
    let human_order = SimOrder::new(
        1,
        SimOrderSource::human(1),
        1,
        human_body,
        SimOrderKind::SetVelocity(v(0, 0, 64)),
    );
    let orders = vec![agent_order, human_order];

    let mut queue = SimOrderQueue::<8>::default();
    for order in &orders {
        queue.enqueue(*order)?;
    }
    let run_report = queue.run_ticks(&mut sim, 4)?;
    let final_snapshot = SimSnapshot::capture(&sim);

    let replay_record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: initial_snapshot.clone(),
        orders: orders.clone(),
        run_ticks: 4,
        expected_final_hash: run_report.state_hash,
    };
    let replay_path = run.artifact_path("agent-lowering-replay.ron")?;
    fs::write(&replay_path, replay_record.to_ron_string()?)?;
    let replay_bytes = fs::read(&replay_path)?;
    let replay_readback = SimReplayRecord::from_ron_str(std::str::from_utf8(&replay_bytes)?)?;
    let replay_outcome = replay_readback.replay::<3, 8>()?;

    let authority_proof = build_net_authority_proof::<3, 8>(&initial_snapshot, 1, human_body)?;
    let edge_replay_mismatch = replay_mismatch_code(&replay_record);
    let edge_capacity = order_capacity_code(agent_order);
    let edge_agent_source = agent_source_code(human_order);

    let source_kinds = orders
        .iter()
        .map(|order| format!("{:?}", order.source.kind))
        .collect::<Vec<_>>();
    let accepted = replay_outcome.final_hash == run_report.state_hash
        && replay_outcome.final_hash == final_snapshot.state_hash
        && agent_order.source.kind == SimOrderSourceKind::LoweredAgent
        && human_order.source.kind == SimOrderSourceKind::Human
        && authority_proof.proves_server_authority()
        && edge_replay_mismatch == "LITD-REFLECT-FINGERPRINT-MISMATCH"
        && edge_capacity == "LITD-SIM-ORDER-CAPACITY-EXCEEDED"
        && edge_agent_source == "LITD-SIM-INVALID-CONFIG";

    if !accepted {
        return Err("issue783 agent lowering boundary acceptance failed".into());
    }

    let report = json!({
        "schema": "litd.issue783.agent_lowering_boundary_fsv.v1",
        "issue": 783,
        "adr_path": "docs/adr/adr-783-agent-deterministic-lowering.md",
        "decision": "AI agents and humans submit deterministic server inputs; agents never mutate the sim directly.",
        "supporting_doctrine": {
            "agent_epic": "#781",
            "server_authority": "#1318",
            "operator_resolution_comment": "https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/783#issuecomment-4872029733"
        },
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "replay_record_path": replay_path.display().to_string(),
            "manual_gate": "manual Synapse/browser readback must inspect this JSON, the replay record path/hash, source kinds, replay hashes, server-authority proof, and edge codes"
        },
        "sim": {
            "ticks_per_second": config.ticks_per_second,
            "run_ticks": replay_record.run_ticks,
            "initial_hash": initial_snapshot.state_hash,
            "expected_final_hash": run_report.state_hash,
            "replay_final_hash": replay_outcome.final_hash,
            "final_snapshot_hash": final_snapshot.state_hash,
            "replay_hash_match": replay_outcome.final_hash == run_report.state_hash,
            "orders_applied": replay_outcome.orders_applied,
            "source_kinds": source_kinds,
            "agent_order_fingerprint": agent_order.fingerprint(),
            "human_order_fingerprint": human_order.fingerprint(),
            "agent_order_source_kind": "lowered_agent",
            "human_order_source_kind": "human"
        },
        "server_authority": {
            "schema": authority_proof.schema,
            "proves_server_authority": authority_proof.proves_server_authority(),
            "authority_mode": authority_proof.authority_mode,
            "client_contract": authority_proof.client_contract,
            "connected_clients": authority_proof.connected_clients,
            "accepted_inputs": authority_proof.accepted_inputs,
            "rejected_inputs": authority_proof.rejected_inputs,
            "corrected_forged_states": authority_proof.corrected_forged_states,
            "rejected_forged_positions": authority_proof.rejected_forged_positions,
            "rejected_forged_hit_points": authority_proof.rejected_forged_hit_points,
            "rejected_forged_items": authority_proof.rejected_forged_items,
            "snapshot_delta_changed_bodies": authority_proof.snapshot_delta_changed_bodies,
            "lockstep_hash_checks": authority_proof.lockstep_hash_checks,
            "lockstep_divergence_rejections": authority_proof.lockstep_divergence_rejections,
            "server_truth_count": authority_proof.server_truth.len()
        },
        "edges": [
            {
                "label": "replay_expected_hash_mismatch_fails_closed",
                "code": edge_replay_mismatch
            },
            {
                "label": "order_queue_capacity_fails_closed",
                "code": edge_capacity
            },
            {
                "label": "client_input_cannot_use_lowered_agent_source",
                "code": edge_agent_source
            }
        ],
        "sufficiency_argument": "The smallest proof uses one lowered-agent order and one human order in the same deterministic replay, plus a two-client server-authority proof. It covers the #783 boundary: out-of-band agent decisions lower to fingerprinted inputs, human inputs use the same queue, replay hashes match, and forged/non-authoritative inputs fail closed. Larger scenes would add volume but not a distinct decision state."
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE783_AGENT_LOWERING_FSV report={} bytes={} sha256={} replay={} replay_hash_match={} authority={} edges=3",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        replay_path.display(),
        replay_outcome.final_hash == run_report.state_hash,
        authority_proof.proves_server_authority()
    );
    Ok(())
}

fn replay_mismatch_code(record: &SimReplayRecord) -> &'static str {
    let mut bad = record.clone();
    bad.expected_final_hash = 0;
    match bad.replay::<3, 8>() {
        Err(error @ EngineError::SerializedFingerprintMismatch { .. }) => error.code(),
        other => panic!("expected SerializedFingerprintMismatch, got {other:?}"),
    }
}

fn order_capacity_code(order: SimOrder) -> &'static str {
    let mut queue = SimOrderQueue::<1>::default();
    queue.enqueue(order).expect("first order fits");
    match queue.enqueue(order) {
        Err(error @ EngineError::SimOrderCapacityExceeded { .. }) => error.code(),
        other => panic!("expected SimOrderCapacityExceeded, got {other:?}"),
    }
}

fn agent_source_code(mut human_order: SimOrder) -> &'static str {
    use litd_engine::net_authority::{
        AuthoritativeSimServer, AuthorityClientBinding, AuthorityServerConfig, ClientInput,
    };

    let config = SimConfig::new(64).expect("valid sim config");
    let mut sim = FixedTickSim::<1>::new(config, 0x783B).expect("valid sim");
    let body = sim
        .spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .expect("body");
    human_order.source = SimOrderSource::lowered_agent(77);
    human_order.target = body;
    human_order.execute_tick = 1;
    let mut server = AuthoritativeSimServer::<1, 4>::new(
        AuthorityServerConfig::default(),
        sim,
        vec![AuthorityClientBinding {
            client_id: 1,
            body_id: body,
            hit_points: 125,
            inventory_items: vec!["server_owned_item_client_1".to_string()],
        }],
    )
    .expect("valid server");
    match server.submit_input(ClientInput {
        client_id: 1,
        sequence: 1,
        order: human_order,
    }) {
        Err(error @ EngineError::SimInvalidConfig { .. }) => error.code(),
        other => panic!("expected SimInvalidConfig, got {other:?}"),
    }
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
