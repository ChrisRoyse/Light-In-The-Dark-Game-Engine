//! FSV #810: destructibles placement + data -> persist -> readback -> lowered
//! artifact, then deterministic transitions in a scripted sim run.
//!
//! Authors a destructible field (trees/gates/walls with HP/armor/model), persists
//! it through the REAL Calyx content path with NO ORT present (row lands as
//! `pending_measurement`), lowers the verified readback into a frozen sim package,
//! reopens the vault with a fresh handle to read the source of truth (#800), then
//! reconstructs the field FROM the persisted bytes and drives a scripted damage
//! script through the deterministic transition machine. The sim is run twice and
//! must produce the identical transition log + fingerprint. Three edge cases prove
//! the boundaries (overkill clamp, unknown target fail-closed, armor soak).

use std::error::Error;

use litd_engine::{
    calyx_content::{
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING, EngineCalyxContentStore,
        EngineContentConstellationReadback, EngineContentKind, EngineContentMeasurementConfig,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    world_authoring::persist_and_lower_world_content,
    world_destructibles::{
        DestructibleField, DestructibleFieldPayload, DestructibleKind, DestructibleSpec,
    },
    world_destructibles_sim::{DamageEvent, DestructibleState, run_destructible_sim},
};
use serde_json::{Value, json};

#[path = "fixtures/world_scenery_panel_fixture.rs"]
mod panel;

const PANEL_LENSES: usize = 12;
const CONTENT_KIND: EngineContentKind = EngineContentKind::Terrain;
const FIELD_ID: &str = "issue810_gate_yard";

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue810_destructibles")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent:issue810",
            panel::unavailable_panel_state(PANEL_LENSES),
        ),
    );

    // Author + persist + lower.
    let field = gate_yard();
    let record = field.to_content_record("issue810-fsv")?;
    let lowering = persist_and_lower_world_content(&store, &record)?;
    let readback = store.get_content_for_editor(CONTENT_KIND, FIELD_ID)?;

    // Reopen the vault with a fresh handle and rebuild the field FROM the durable
    // bytes -- the frozen source the sim consumes (never a live Calyx query).
    let on_disk = fresh_readback(&vault_dir, FIELD_ID)?;
    let persisted_payload: DestructibleFieldPayload =
        serde_json::from_value(on_disk.payload.clone())?;
    let sim_field = DestructibleField::from_payload(&persisted_payload)?;

    // Deterministic scripted sim run, executed twice from the persisted field.
    let lowered_agent_id = lowering.lowered_package.lowered_agent_id;
    let script = damage_script();
    let run_a = run_destructible_sim(&sim_field, &script, lowered_agent_id)?;
    let run_b = run_destructible_sim(&sim_field, &script, lowered_agent_id)?;

    let oak = run_a
        .transitions
        .iter()
        .filter(|t| t.target_id == "oak_1")
        .collect::<Vec<_>>();
    let oak_intact_to_damaged = oak
        .first()
        .map(|t| {
            t.state_before == DestructibleState::Intact
                && t.state_after == DestructibleState::Damaged
        })
        .unwrap_or(false);
    let oak_reaches_destroyed = oak
        .iter()
        .any(|t| t.state_after == DestructibleState::Destroyed);

    let author = json!({
        "field_id": field.field_id,
        "destructible_count": persisted_payload.destructible_count,
        "total_max_hp": persisted_payload.total_max_hp,
        "measurement_state": readback.measurement.measurement_state,
        "is_pending_without_ort": readback.measurement.measurement_state
            == ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        "cx_id": readback.cx_id,
        "row_seq": readback.database.seq,
        "row_sha256": readback.database.sha256,
    });
    let lowered = json!({
        "lowered_field_count": lowering.lowered_readback.field_count,
        "package_fingerprint": lowering.lowered_readback.package_fingerprint,
        "lowered_agent_id": lowered_agent_id,
        "calyx_live_query_allowed_in_hot_path":
            lowering.lowered_package.calyx_live_query_allowed_in_hot_path,
        "lowered_fields_nonempty": lowering.lowered_readback.field_count > 0,
    });
    let durable = json!({
        "on_disk_cx_matches_writer": on_disk.cx_id == readback.cx_id,
        "on_disk_sha_matches_writer": on_disk.database.sha256 == readback.database.sha256,
        "field_rebuilt_from_disk": sim_field.destructibles.len() as u32
            == persisted_payload.destructible_count,
    });
    let sim = json!({
        "event_count": run_a.event_count,
        "run_a_fingerprint": run_a.fingerprint,
        "run_b_fingerprint": run_b.fingerprint,
        "deterministic": run_a == run_b,
        "destroyed_count": run_a.destroyed_count,
        "damaged_count": run_a.damaged_count,
        "intact_count": run_a.intact_count,
        "oak_intact_to_damaged": oak_intact_to_damaged,
        "oak_reaches_destroyed": oak_reaches_destroyed,
    });

    let edges = json!({
        "overkill_clamps_and_stays_destroyed": overkill_edge(&sim_field, lowered_agent_id)?,
        "unknown_target_fails_closed": unknown_target_edge(&sim_field, lowered_agent_id)?,
        "armor_soak_reduces_damage": armor_soak_edge(lowered_agent_id)?,
    });

    let acceptance = json!({
        "persisted_pending_without_ort": author["is_pending_without_ort"],
        "lowered_artifact_produced": lowered["lowered_fields_nonempty"],
        "lowered_no_live_calyx": lowered["calyx_live_query_allowed_in_hot_path"] == false,
        "durable_readback_cx_matches": durable["on_disk_cx_matches_writer"],
        "durable_readback_sha_matches": durable["on_disk_sha_matches_writer"],
        "sim_field_rebuilt_from_persisted_bytes": durable["field_rebuilt_from_disk"],
        "sim_run_deterministic": sim["deterministic"],
        "sim_intact_damaged_destroyed_observed":
            sim["oak_intact_to_damaged"].as_bool().unwrap_or(false)
                && sim["oak_reaches_destroyed"].as_bool().unwrap_or(false),
        "edge_overkill_clamps": edges["overkill_clamps_and_stays_destroyed"]["passed"],
        "edge_unknown_target_fails_closed": edges["unknown_target_fails_closed"]["passed"],
        "edge_armor_soak": edges["armor_soak_reduces_damage"]["passed"],
    });

    let report = json!({
        "schema": "litd.engine.issue810.destructibles_fsv.v1",
        "issue": 810,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "environment": {
                "ORT_DYLIB_PATH_present": std::env::var_os("ORT_DYLIB_PATH").is_some(),
            },
            "manual_gate": "Manual FSV must read this report and the persisted Aster rows under engine_vault_dir; command success alone is not proof."
        },
        "sufficiency_argument": "A tree (no armor), a gate (armored), and a wall (heavily armored) is the smallest field that exercises every destructible axis: a full intact->damaged->destroyed lifecycle, an armor soak that zeroes a hit, and an overkill clamp. The field persists addon-absent, lowers into a frozen sim package, is rebuilt from the persisted bytes, and drives a scripted damage run that is reproduced bit-for-bit -- proving deterministic, sim-consumable destructible state end to end.",
        "author": author,
        "lowered": lowered,
        "durable_readback": durable,
        "scripted_sim": sim,
        "edges": edges,
        "acceptance": acceptance
    });

    let written = run.write_report(&report)?;
    for (field, value) in written.json["acceptance"]
        .as_object()
        .expect("acceptance object")
    {
        if value != true {
            return Err(format!("issue810 acceptance {field} was not true").into());
        }
    }
    println!(
        "ISSUE810_DESTRUCTIBLES_FSV report={} sha256={} destroyed={} deterministic={} state={}",
        written.path.display(),
        written.sha256,
        sim["destroyed_count"],
        sim["deterministic"],
        author["measurement_state"],
    );
    Ok(())
}

fn gate_yard() -> DestructibleField {
    DestructibleField {
        field_id: FIELD_ID.to_string(),
        display_name: "Gate Yard".to_string(),
        destructibles: vec![
            DestructibleSpec {
                id: "oak_1".to_string(),
                kind: DestructibleKind::Tree,
                tile_x: 2,
                tile_z: 3,
                yaw_deg: 90,
                max_hp: 50,
                armor: 0,
                model_ref: "models/oak.glb".to_string(),
            },
            DestructibleSpec {
                id: "north_gate".to_string(),
                kind: DestructibleKind::Gate,
                tile_x: 4,
                tile_z: 0,
                yaw_deg: 0,
                max_hp: 400,
                armor: 10,
                model_ref: "models/gate.glb".to_string(),
            },
            DestructibleSpec {
                id: "wall_a".to_string(),
                kind: DestructibleKind::Wall,
                tile_x: 5,
                tile_z: 0,
                yaw_deg: 0,
                max_hp: 800,
                armor: 25,
                model_ref: "models/wall.glb".to_string(),
            },
        ],
    }
}

fn damage_script() -> Vec<DamageEvent> {
    vec![
        DamageEvent {
            tick: 0,
            sequence: 0,
            target_id: "oak_1".to_string(),
            amount: 30,
        },
        DamageEvent {
            tick: 1,
            sequence: 0,
            target_id: "oak_1".to_string(),
            amount: 30,
        },
        DamageEvent {
            tick: 2,
            sequence: 0,
            target_id: "north_gate".to_string(),
            amount: 60,
        },
        DamageEvent {
            tick: 2,
            sequence: 0,
            target_id: "wall_a".to_string(),
            amount: 5,
        },
    ]
}

fn overkill_edge(field: &DestructibleField, agent: u32) -> Result<Value, Box<dyn Error>> {
    let events = vec![
        DamageEvent {
            tick: 0,
            sequence: 0,
            target_id: "oak_1".to_string(),
            amount: 9999,
        },
        DamageEvent {
            tick: 1,
            sequence: 0,
            target_id: "oak_1".to_string(),
            amount: 9999,
        },
    ];
    let report = run_destructible_sim(field, &events, agent)?;
    let first = &report.transitions[0];
    let second = &report.transitions[1];
    Ok(json!({
        "before_hp": first.hp_before,
        "after_first_hit_hp": first.hp_after,
        "first_effective": first.effective_damage,
        "second_effective_on_destroyed": second.effective_damage,
        "passed": first.hp_after == 0
            && first.effective_damage == 50
            && second.effective_damage == 0
            && second.state_after == DestructibleState::Destroyed,
    }))
}

fn unknown_target_edge(field: &DestructibleField, agent: u32) -> Result<Value, Box<dyn Error>> {
    let events = vec![DamageEvent {
        tick: 0,
        sequence: 0,
        target_id: "ghost".to_string(),
        amount: 10,
    }];
    let result = run_destructible_sim(field, &events, agent);
    let code = result.as_ref().err().map(|error| error.code());
    Ok(json!({
        "before": "field intact",
        "after": "sim rejected, no transition applied",
        "error_code": code,
        "passed": code == Some("WORLD_DESTRUCTIBLE_SIM_TARGET"),
    }))
}

fn armor_soak_edge(agent: u32) -> Result<Value, Box<dyn Error>> {
    let field = DestructibleField {
        field_id: "issue810_one_wall".to_string(),
        display_name: "One Wall".to_string(),
        destructibles: vec![DestructibleSpec {
            id: "wall".to_string(),
            kind: DestructibleKind::Wall,
            tile_x: 0,
            tile_z: 0,
            yaw_deg: 0,
            max_hp: 100,
            armor: 25,
            model_ref: "models/wall.glb".to_string(),
        }],
    };
    let events = vec![DamageEvent {
        tick: 0,
        sequence: 0,
        target_id: "wall".to_string(),
        amount: 60,
    }];
    let report = run_destructible_sim(&field, &events, agent)?;
    let t = &report.transitions[0];
    Ok(json!({
        "before_hp": t.hp_before,
        "raw_amount": t.raw_amount,
        "effective_after_armor": t.effective_damage,
        "after_hp": t.hp_after,
        "passed": t.effective_damage == 35 && t.hp_after == 65
            && t.state_after == DestructibleState::Damaged,
    }))
}

fn fresh_readback(
    vault_dir: &std::path::Path,
    id: &str,
) -> Result<EngineContentConstellationReadback, Box<dyn Error>> {
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(
            "separate-readback:issue810",
            panel::unavailable_panel_state(PANEL_LENSES),
        ),
    );
    Ok(store.get_content_for_editor(CONTENT_KIND, id)?)
}
