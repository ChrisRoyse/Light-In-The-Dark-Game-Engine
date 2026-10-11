//! Harness plumbing for the #1552 Ward-corpus FSV (split out to keep each file under the 500-line
//! ratchet, #1466). Content ingest, the sim runner, and the read-back/report helpers live here;
//! `main()` and the scenario cases live in the parent example file.

use std::io;

use litd_engine::{
    agent_response::EngineAgentVerbTag,
    agent_response_gate::{
        EngineAgentBinding, EngineAgentOrderEnvelope, EngineAgentResponseReadback,
        EngineAgentWireResponse, read_agent_response_admission, read_lowered_agent_order,
    },
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxPersistenceError, EngineCalyxRecord, EngineCalyxRecordKind,
    },
    fsv_artifacts::FsvRun,
    math::FixedVec3,
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderQueue},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

pub(crate) const TARGET_BODY: u32 = 0;
pub(crate) const EXECUTE_TICK: u64 = 1;
pub(crate) const TICKS: u64 = 2;
pub(crate) const CONTENT_ROW_ID: &str = "world.thera.agent.gate_guard";

/// Ingest a world-agent content row, then READ IT BACK so the corpus source is derived from a
/// persisted record and never from example literals.
pub(crate) fn ingest_and_read_agent_content(
    database: &EngineCalyxDatabase,
) -> Result<EngineContentRecord, Box<dyn std::error::Error>> {
    let record = EngineContentRecord {
        kind: EngineContentKind::Agent,
        id: CONTENT_ROW_ID.to_string(),
        display_name: "Gate Guard".to_string(),
        payload: json!({
            "agent_id": "guard",
            "tags": ["fire", "ward", "gate", "protect", "enemy", "intruder", "line"],
            "trigger_terms": ["enemy", "gate"],
            "prompt_template": "Agent {agent}: protect the gate from {signal}",
        }),
        source: EngineContentSource {
            path: "authored/world/thera/agents/gate_guard.json".to_string(),
            author: "editor".to_string(),
            source_hash: sha256_hex(b"gate_guard"),
        },
        anchors: Vec::new(),
    };
    let stored = EngineCalyxRecord::new(
        EngineCalyxRecordKind::World,
        record.id.clone(),
        serde_json::to_value(&record)?,
        json!({"source_issue": 1552, "content_kind": record.kind.as_str()}),
    );
    database.put_record(&stored)?;
    let back = database.get_record(EngineCalyxRecordKind::World, CONTENT_ROW_ID)?;
    Ok(serde_json::from_value::<EngineContentRecord>(back.payload)?)
}

pub(crate) fn envelope() -> EngineAgentOrderEnvelope {
    EngineAgentOrderEnvelope {
        allowed_verbs: [EngineAgentVerbTag::Hold, EngineAgentVerbTag::SetVelocity]
            .into_iter()
            .collect(),
        max_move_units_per_tick: 2,
        max_damage_milli: 1_200,
        max_rationale_chars: 320,
        allow_self_attack: false,
    }
}

pub(crate) fn binding() -> EngineAgentBinding {
    EngineAgentBinding {
        agent_id: "guard".to_string(),
        target_body: SimBodyId::from_raw(TARGET_BODY),
        source_id: 7,
        execute_tick: EXECUTE_TICK,
        sequence: 1,
        grounding_refs: vec![
            "grounding://signal/source".to_string(),
            "signal://issue1552/enemy-at-gate".to_string(),
        ],
    }
}

pub(crate) fn wire(response_id: &str, content: &str) -> EngineAgentWireResponse {
    EngineAgentWireResponse {
        response_id: response_id.to_string(),
        model: "deterministic-fsv/issue1552".to_string(),
        finish_reason: "stop".to_string(),
        content: content.to_string(),
        prompt_sha256: sha256_hex(content.as_bytes()),
    }
}

pub(crate) fn ward_slots(readback: &EngineAgentResponseReadback) -> String {
    let Some(gate) = readback.gate.as_ref() else {
        return "<ward never ran>".to_string();
    };
    gate.verdicts
        .iter()
        .flat_map(|verdict| verdict.ward_slots.iter())
        .map(|slot| {
            format!(
                "slot{}: cos={:.4} tau={:.4} {}",
                slot.slot,
                slot.cos,
                slot.tau,
                if slot.pass { "PASS" } else { "FAIL" }
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Print the admit/refuse outcome by READING THE LEDGER BACK, plus the tau per slot from the store.
pub(crate) fn report_case(
    label: &str,
    database: &EngineCalyxDatabase,
    admission_id: &str,
    sim_hash: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ledger = read_agent_response_admission(database, admission_id)?;
    let order_present = read_lowered_agent_order(database, admission_id).is_ok();
    println!(
        "CASE {label} admitted={} stage={:?} code={} order_row={} sim_state_hash={}",
        ledger.admitted,
        ledger.stage,
        ledger.code,
        if order_present { "PRESENT" } else { "ABSENT" },
        sim_hash
            .map(|hash| format!("{hash:#018x}"))
            .unwrap_or_else(|| "baseline".to_string()),
    );
    println!("       ward(readback): {}", ward_slots(&ledger));
    Ok(())
}

pub(crate) fn assert_no_order(
    database: &EngineCalyxDatabase,
    admission_id: &str,
    baseline_hash: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    match database.get_record(EngineCalyxRecordKind::LoweredAgentOrder, admission_id) {
        Err(EngineCalyxPersistenceError::MissingRecord { .. }) => {}
        Ok(row) => {
            return Err(
                io::Error::other(format!("a REFUSED response lowered an order: {row:?}")).into(),
            );
        }
        Err(other) => return Err(other.into()),
    }
    let refused_hash = run_sim(None)?;
    ensure(
        refused_hash == baseline_hash,
        "a refused response perturbed the sim — it must be bit-identical to baseline",
    )?;
    Ok(())
}

pub(crate) fn run_sim(order: Option<SimOrder>) -> Result<u64, Box<dyn std::error::Error>> {
    let mut sim = FixedTickSim::<2>::new(SimConfig::new(64)?, 0x1552)?;
    let body = sim.spawn_body(vec3(0, 0, 0), vec3(0, 0, 0), vec3(0, 0, 0))?;
    ensure(
        body.index() == TARGET_BODY,
        "the FSV binding assumes body index 0",
    )?;
    let mut queue = SimOrderQueue::<8>::default();
    if let Some(order) = order {
        queue.enqueue(order)?;
    }
    queue.run_ticks(&mut sim, TICKS)?;
    Ok(sim.state_hash())
}

pub(crate) fn case_row(readback: &EngineAgentResponseReadback, sim_hash: Option<u64>) -> Value {
    json!({
        "admission_id": readback.admission_id,
        "stage": readback.stage,
        "code": readback.code,
        "admitted": readback.admitted,
        "lowering_allowed": readback.lowering_allowed,
        "reasons": readback.reasons,
        "ward_overall_pass": readback
            .gate
            .as_ref()
            .and_then(|gate| gate.verdicts.first())
            .map(|verdict| verdict.ward_overall_pass),
        "ward_slots": ward_slots(readback),
        "sim_state_hash": sim_hash.map(|hash| format!("{hash:#018x}")),
    })
}

pub(crate) fn write_report(
    run: &FsvRun,
    database: &EngineCalyxDatabase,
    cases: Vec<Value>,
    baseline_hash: u64,
    corpus_sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Count admit/refuse from the rows on disk, not from the case variables.
    let ids = [
        "issue1552-legit",
        "issue1552-garbage",
        "issue1552-injection",
    ];
    let mut admitted = 0;
    let mut refused = 0;
    let mut order_present = 0;
    for id in ids {
        let ledger = read_agent_response_admission(database, id)?;
        if ledger.admitted {
            admitted += 1;
        } else {
            refused += 1;
        }
        if read_lowered_agent_order(database, id).is_ok() {
            order_present += 1;
        }
        ensure(
            ledger.admitted == ledger.lowering_allowed,
            "ACCEPTANCE: a stored admission violates admitted == lowering_allowed",
        )?;
    }
    ensure(
        admitted == 1 && refused == 2 && order_present == 1,
        &format!(
            "ACCEPTANCE: expected 1 admitted / 2 refused / 1 order row; got admitted={admitted} \
             refused={refused} order_present={order_present}"
        ),
    )?;
    let report = json!({
        "schema": "litd.engine.issue1552.ward_corpus_fsv.v1",
        "issue": 1552,
        "corpus_sha256": corpus_sha256,
        "baseline_state_hash": format!("{baseline_hash:#018x}"),
        "acceptance": {
            "admitted": admitted,
            "refused": refused,
            "order_rows_present": order_present,
            "degenerate_calibration_failed_loud": true,
        },
        "cases": cases,
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1552_WARD_CORPUS_FSV report={} sha256={} run_id={} admitted={admitted} refused={refused} \
         order_rows_present={order_present} corpus_sha256={corpus_sha256} degenerate_failed_loud=true",
        readback.path.display(),
        readback.sha256,
        run.run_id(),
    );
    Ok(())
}

pub(crate) fn vec3(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

pub(crate) fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message.to_string()))
    }
}
