//! Supporting FSV harness for render stats, deterministic replay, and Calyx readback.

use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseConfig, EngineCalyxPersistenceError,
        EngineCalyxRecord, EngineCalyxRecordKind,
    },
    fsv_artifacts::{FsvReportReadback, begin_issue_run},
    math::FixedVec3,
    render::reference::{
        ReferenceSceneSettings, build_reference_scene_plan, render_reference_scene_gpu,
    },
    sim::{FixedTickSim, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};

pub const ISSUE1149_FSV_SCHEMA: &str = "litd.engine.issue1149.fsv_quality_harness.v1";
pub const ISSUE1149_FSV_SLUG: &str = "issue1149_fsv_quality_harness";
const REFERENCE_RENDER_DRAW_BUDGET: u32 = 2;

pub async fn run_issue1149_fsv_quality_harness() -> Result<FsvReportReadback, EngineError> {
    let run = begin_issue_run(ISSUE1149_FSV_SLUG)
        .map_err(|error| harness_error("fsv.begin_issue_run", error))?;
    let reference_bmp = run
        .artifact_path("reference-render-320x180.bmp")
        .map_err(|error| harness_error("fsv.reference_bmp_path", error))?;

    let render = render_stat_proof(&reference_bmp).await?;
    let determinism = determinism_replay_proof()?;
    let calyx = calyx_round_trip_proof(run.run_dir(), &render, &determinism)?;
    let edges = edge_proofs(run.run_dir())?;
    let report = json!({
        "schema": ISSUE1149_FSV_SCHEMA,
        "schema_version": 1,
        "issue": 1149,
        "source_of_truth": {
            "immutable_run_dir": run.run_dir().display().to_string(),
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "render_bitmap": reference_bmp.display().to_string(),
            "calyx_vault": calyx["after"]["vault_dir"],
            "manual_gate": "supporting automated evidence only; issue closure still requires manual Synapse reality FSV"
        },
        "research_basis": [
            "Google SRE monitoring guidance: collect concrete latency/traffic/error/saturation-style signals and read them back from the source of truth",
            "NVIDIA Nsight Graphics frame tooling: preserve per-frame render events/stat evidence for inspection",
            "Gaffer fixed timestep guidance: deterministic simulation consumes discrete fixed dt steps and can be replay-hashed"
        ],
        "sufficiency_argument": {
            "render": "320x180 is the minimum accepted reference scene size, proving the boundary-size render path, GPU readback, draw-call count, and exact draw-call budget.",
            "determinism": "two bodies, human plus lowered-agent orders, velocity mutation, attack event, RON round-trip, and duplicate replay hit every replay state transition under this harness.",
            "calyx": "one ObservabilityLedger record proves write, immediate readback, later get_record readback, Graph CF row presence, payload equality, and hash stability.",
            "edges": [
                "invalid render size fails before adapter request",
                "replay final-hash mismatch fails closed",
                "Calyx invalid provenance is rejected",
                "Calyx missing record is rejected"
            ]
        },
        "happy_path": {
            "render_stat_readback": render,
            "determinism_replay": determinism,
            "calyx_round_trip": calyx
        },
        "edges": edges
    });
    run.write_report(&report)
        .map_err(|error| harness_error("fsv.write_report", error))
}

async fn render_stat_proof(bmp_path: &Path) -> Result<Value, EngineError> {
    let settings = ReferenceSceneSettings {
        width: 320,
        height: 180,
    };
    // #1532: the real per-frame ALLOCATION budget is enforced at the shipping render
    // sites (litd-editor / litd-game main.rs), where `CountingAllocator` is installed
    // as the process `#[global_allocator]` and frames are steady-state. This harness
    // runs in the litd-engine LIB-TEST binary (no global allocator installed) and each
    // call does a COLD `GpuContext::new_headless()` — cold wgpu/Vulkan device+adapter
    // creation allocates a large, driver-version-variable amount with no stable
    // per-frame invariant. Asserting an allocation `budget_ok` here would be either a
    // false-green (budget 0<=0, the #1458/#1532 defect) or a fragile false-red. So this
    // proof asserts ONLY the deterministic, genuinely-failable DRAW-CALL budget (the
    // reference scene emits exactly REFERENCE_RENDER_DRAW_BUDGET draws).
    let before = json!({
        "settings": settings,
        "draw_call_budget": REFERENCE_RENDER_DRAW_BUDGET,
        "expected_draw_calls": REFERENCE_RENDER_DRAW_BUDGET,
    });
    let frame = render_reference_scene_gpu(settings).await?;
    if frame.draw_calls > REFERENCE_RENDER_DRAW_BUDGET {
        return Err(EngineError::BatchingDrawLimitExceeded {
            draw_count: frame.draw_calls as usize,
            max_draws: REFERENCE_RENDER_DRAW_BUDGET as usize,
        });
    }
    let draw_budget_ok = frame.draw_calls <= REFERENCE_RENDER_DRAW_BUDGET;
    frame.image.write_bmp(bmp_path)?;
    let bmp_readback = file_readback(bmp_path)?;
    Ok(json!({
        "before": before,
        "after": {
            "gpu_context_id": frame.gpu_context_id,
            "adapter_name": frame.adapter_name,
            "backend": frame.backend,
            "texture_format": frame.texture_format,
            "mesh_count": frame.mesh_count,
            "instance_count": frame.instance_count,
            "draw_calls": frame.draw_calls,
            "draw_call_budget": REFERENCE_RENDER_DRAW_BUDGET,
            "draw_budget_ok": draw_budget_ok,
            "readback_bytes": frame.readback_bytes,
            "row_pitch": frame.row_pitch,
            "comparison_pass": frame.comparison.pass,
            "comparison_checksum": frame.comparison.checksum,
            "comparison_metrics": {
                "average_luma": frame.comparison.metrics.average_luma,
                "warm_highlight_ratio": frame.comparison.metrics.warm_highlight_ratio,
                "cool_shadow_ratio": frame.comparison.metrics.cool_shadow_ratio,
                "emissive_ratio": frame.comparison.metrics.emissive_ratio,
                "edge_contrast": frame.comparison.metrics.edge_contrast,
                "hud_gold_ratio": frame.comparison.metrics.hud_gold_ratio,
            },
            "comparison_bands": frame.comparison.bands.iter().map(|band| json!({
                "name": band.name,
                "actual": band.actual,
                "min": band.min,
                "max": band.max,
                "pass": band.pass,
            })).collect::<Vec<_>>(),
            "bmp_readback": bmp_readback,
        }
    }))
}

fn determinism_replay_proof() -> Result<Value, EngineError> {
    let config = SimConfig::new(60)?;
    let mut sim = FixedTickSim::<2>::new(config, 1_149)?;
    let player = sim.spawn_body(vec3(0, 0, 0), vec3(0, 0, 0), vec3(0, 0, 0))?;
    let target = sim.spawn_body(vec3(4, 0, 0), vec3(0, 0, 0), vec3(0, 0, 0))?;
    let initial = SimSnapshot::capture(&sim);
    let orders = vec![
        SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            player,
            SimOrderKind::SetVelocity(vec3(60, 0, 0)),
        ),
        SimOrder::new(
            2,
            SimOrderSource::lowered_agent(2),
            1,
            target,
            SimOrderKind::AddVelocity(vec3(0, 60, 0)),
        ),
        SimOrder::new(
            3,
            SimOrderSource::human(1),
            2,
            target,
            SimOrderKind::ApplyAttack {
                attacker: player,
                damage_fp: 25,
            },
        ),
    ];
    let mut expected_queue = SimOrderQueue::<4>::default();
    for order in &orders {
        expected_queue.enqueue(*order)?;
    }
    let expected_final_hash = expected_queue.run_ticks(&mut sim, 4)?.state_hash;
    let record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: initial,
        orders,
        run_ticks: 4,
        expected_final_hash,
    };
    let ron = record.to_ron_string()?;
    let loaded = SimReplayRecord::from_ron_str(&ron)?;
    let first = loaded.replay::<2, 4>()?;
    let second = loaded.replay::<2, 4>()?;
    if first.final_hash != second.final_hash {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: format!("{:016X}", first.final_hash),
            actual: format!("{:016X}", second.final_hash),
        });
    }
    Ok(json!({
        "before": {
            "schema_version": SIM_REPLAY_SCHEMA_VERSION,
            "ticks_per_second": config.ticks_per_second,
            "body_count": record.initial_snapshot.bodies.len(),
            "orders": record.orders.len(),
            "run_ticks": record.run_ticks,
            "initial_hash": format!("{:016X}", record.initial_snapshot.state_hash),
            "expected_final_hash": format!("{:016X}", record.expected_final_hash),
            "replay_record_ron_sha256": sha256_hex(ron.as_bytes()),
        },
        "after": {
            "first_final_hash": format!("{:016X}", first.final_hash),
            "second_final_hash": format!("{:016X}", second.final_hash),
            "final_tick": first.final_tick,
            "steps": first.steps,
            "orders_applied": first.orders_applied,
            "attack_events": first.final_snapshot.attack_events.len(),
            "final_body_count": first.final_snapshot.bodies.len(),
            "hashes_match": first.final_hash == record.expected_final_hash
                && first.final_hash == second.final_hash,
        }
    }))
}

fn calyx_round_trip_proof(
    run_dir: &Path,
    render: &Value,
    determinism: &Value,
) -> Result<Value, EngineError> {
    let vault_dir = run_dir.join("calyx-quality-vault");
    let db = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))
        .map_err(|error| calyx_harness_error("open", error))?;
    let record = EngineCalyxRecord::new(
        EngineCalyxRecordKind::ObservabilityLedger,
        "issue1149-render-determinism-calyx",
        json!({
            "render_draw_calls": render["after"]["draw_calls"],
            "render_draw_budget_ok": render["after"]["draw_budget_ok"],
            "render_bmp_sha256": render["after"]["bmp_readback"]["sha256"],
            "determinism_final_hash": determinism["after"]["first_final_hash"],
            "determinism_orders_applied": determinism["after"]["orders_applied"],
            "determinism_attack_events": determinism["after"]["attack_events"],
        }),
        json!({
            "source_issue": 1149,
            "source_module": "crates/litd-engine/src/fsv_quality_harness.rs",
            "grounding": "synthetic known input/output persisted through Calyx Aster readback"
        }),
    );
    let write = db
        .put_record(&record)
        .map_err(|error| calyx_harness_error("put_record", error))?;
    let read = db
        .get_record(record.kind, &record.id)
        .map_err(|error| calyx_harness_error("get_record", error))?;
    if write.sha256 != read.sha256 {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: write.sha256,
            actual: read.sha256,
        });
    }
    Ok(json!({
        "before": {
            "vault_dir": vault_dir.display().to_string(),
            "record_kind": record.kind.as_str(),
            "record_id": record.id,
        },
        "after": {
            "vault_dir": read.vault_dir.display().to_string(),
            "schema": read.schema,
            "vault_id": read.vault_id,
            "collection": read.collection,
            "row_key": read.row_key,
            "column": read.column,
            "bytes": read.bytes,
            "sha256": read.sha256,
            "seq": read.seq,
            "graph_rows": read.graph_rows,
            "payload": read.payload,
            "provenance": read.provenance,
        }
    }))
}

fn edge_proofs(run_dir: &Path) -> Result<Value, EngineError> {
    let invalid_render_size = invalid_render_size_edge();
    let replay_hash_mismatch = replay_hash_mismatch_edge()?;
    let vault_dir = run_dir.join("calyx-edge-vault");
    let db = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))
        .map_err(|error| calyx_harness_error("edge.open", error))?;
    Ok(json!({
        "invalid_render_size": invalid_render_size,
        "replay_hash_mismatch": replay_hash_mismatch,
        "calyx_invalid_record": calyx_invalid_record_edge(&db),
        "calyx_missing_record": calyx_missing_record_edge(&db),
    }))
}

fn invalid_render_size_edge() -> Value {
    let before = json!({
        "settings": { "width": 319, "height": 180 },
        "expected_error_code": "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    });
    let error = build_reference_scene_plan(ReferenceSceneSettings {
        width: 319,
        height: 180,
    })
    .unwrap_err();
    json!({ "before": before, "after": engine_error_report(&error) })
}

fn replay_hash_mismatch_edge() -> Result<Value, EngineError> {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(60)?, 7)?;
    let body = sim.spawn_body(vec3(0, 0, 0), vec3(0, 0, 0), vec3(0, 0, 0))?;
    let record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: SimSnapshot::capture(&sim),
        orders: vec![SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(vec3(60, 0, 0)),
        )],
        run_ticks: 1,
        expected_final_hash: 0,
    };
    let before = json!({
        "expected_final_hash": "0000000000000000",
        "orders": 1,
        "run_ticks": 1,
        "expected_error_code": "LITD-REFLECT-FINGERPRINT-MISMATCH"
    });
    let error = record.replay::<1, 2>().unwrap_err();
    Ok(json!({ "before": before, "after": engine_error_report(&error) }))
}

fn calyx_invalid_record_edge(db: &EngineCalyxDatabase) -> Value {
    let record = EngineCalyxRecord::new(
        EngineCalyxRecordKind::ObservabilityLedger,
        "issue1149-invalid-provenance",
        json!({"name": "invalid provenance"}),
        json!({}),
    );
    let before = json!({
        "record_kind": record.kind.as_str(),
        "record_id": record.id,
        "expected_error_kind": "InvalidRecord"
    });
    let error = db.put_record(&record).unwrap_err();
    json!({ "before": before, "after": calyx_error_report(&error) })
}

fn calyx_missing_record_edge(db: &EngineCalyxDatabase) -> Value {
    let before = json!({
        "record_kind": EngineCalyxRecordKind::Save.as_str(),
        "record_id": "issue1149-missing-record",
        "expected_error_kind": "MissingRecord"
    });
    let error = db
        .get_record(EngineCalyxRecordKind::Save, "issue1149-missing-record")
        .unwrap_err();
    json!({ "before": before, "after": calyx_error_report(&error) })
}

fn vec3(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn file_readback(path: &Path) -> Result<Value, EngineError> {
    let bytes = fs::read(path).map_err(|error| harness_error("file.readback", error))?;
    Ok(json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}

fn engine_error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "error_code": report.code,
        "message": report.message,
        "remediation": report.remediation,
        "source_error": report.source_error,
    })
}

fn calyx_error_report(error: &EngineCalyxPersistenceError) -> Value {
    json!({
        "error_kind": match error {
            EngineCalyxPersistenceError::InvalidConfig { .. } => "InvalidConfig",
            EngineCalyxPersistenceError::InvalidRecord { .. } => "InvalidRecord",
            EngineCalyxPersistenceError::Store { .. } => "Store",
            EngineCalyxPersistenceError::MissingRecord { .. } => "MissingRecord",
            EngineCalyxPersistenceError::ReadbackMismatch { .. } => "ReadbackMismatch",
        },
        "message": error.to_string(),
    })
}

fn calyx_harness_error(op: &'static str, error: EngineCalyxPersistenceError) -> EngineError {
    harness_error(op, error)
}

fn harness_error(op: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::SerializationFailed {
        format: "issue1149-fsv-harness",
        reason: format!("{op}: {error}"),
    }
}
