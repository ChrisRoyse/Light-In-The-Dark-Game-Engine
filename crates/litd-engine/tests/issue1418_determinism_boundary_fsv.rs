use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use litd_engine::{
    EngineError,
    app::{AppModule, AppRuntime},
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
    determinism_boundary::{
        HotPathPhase, enter_hot_path, reset_for_fsv, snapshot as boundary_snapshot,
    },
    fsv_artifacts::begin_issue_run,
    game_package::{
        LITD_LOCK_IN_POLICY, LITD_LOWERED_DOCUMENT_SCHEMA, LITD_QUICK_SHARE_SCHEMA,
        LITD_RUNTIME_ID, RuntimePackageAssetRef, RuntimePackageEndpoint, RuntimePackageHostMode,
        compute_runtime_manifest_fingerprint, load_runtime_locked_package,
    },
    math::FixedVec3,
    sim::{FixedTickSim, SimConfig},
};
use serde_json::{Value, json};

const FORBIDDEN_HOT_PATH_TOKENS: &[&str] = &[
    "calyx_aster",
    "AsterVault",
    "PlainColumn",
    "write_calyx_json_bytes",
    "read_calyx_json_bytes",
];

#[test]
fn issue1418_hot_tick_and_render_boundary_consumes_only_lowered_data() {
    litd_engine::init_logging();
    reset_for_fsv();
    let run = begin_issue_run("issue1418_determinism_boundary").unwrap();
    let package_path = run
        .artifact_path("issue1418-runtime.litdgame.json")
        .unwrap();
    let package = package_value(LITD_LOWERED_DOCUMENT_SCHEMA).unwrap();
    let package_bytes = serde_json::to_vec_pretty(&package).unwrap();
    fs::write(&package_path, &package_bytes).unwrap();
    let package_disk = fs::read(&package_path).unwrap();
    assert_eq!(package_disk, package_bytes);

    let cold_spec = calyx_spec(run.artifact_path("cold-calyx-vault").unwrap());
    let cold_payload = br#"{"artifact":"cold-authoring-readback","value":42}"#;
    let cold_write = write_calyx_json_bytes(&cold_spec, cold_payload).unwrap();
    let cold_read = read_calyx_json_bytes(&cold_spec).unwrap().unwrap();
    assert_eq!(cold_read.bytes, cold_payload);
    assert_eq!(cold_read.sha256, litd_hash::sha256_hex(cold_payload));

    let readback = load_runtime_locked_package(&package_path, "issue1418-fsv").unwrap();
    assert_eq!(
        readback.document_summary.document_schema.as_deref(),
        Some(LITD_LOWERED_DOCUMENT_SCHEMA)
    );
    assert_eq!(readback.document_summary.lowered_artifact_count, 2);
    assert_eq!(readback.package_bytes, package_bytes.len());
    assert_eq!(fs::read(&package_path).unwrap(), package_bytes);

    let before_hot = boundary_snapshot();
    run_app_frame_hot_path();
    run_fixed_tick_hot_path();
    let after_hot = boundary_snapshot();
    assert!(after_hot.sim_tick_entries >= before_hot.sim_tick_entries + 5);
    assert_eq!(
        after_hot.render_frame_entries,
        before_hot.render_frame_entries
    );
    assert_eq!(after_hot.calyx_access_total, before_hot.calyx_access_total);
    assert_eq!(
        after_hot.hot_path_calyx_access_total,
        before_hot.hot_path_calyx_access_total
    );

    let sim_edge = blocked_calyx_write_in_phase(
        HotPathPhase::SimTick,
        run.artifact_path("blocked-sim-calyx-vault").unwrap(),
    );
    let render_edge = blocked_calyx_write_in_phase(
        HotPathPhase::RenderFrame,
        run.artifact_path("blocked-render-calyx-vault").unwrap(),
    );
    let non_lowered_edge =
        non_lowered_package_edge(&run.run_dir().join("non-lowered.litdgame.json"));
    let missing_document_edge =
        missing_document_edge(&run.run_dir().join("missing-document.litdgame.json"));
    let static_scan = scan_hot_path_sources().unwrap();
    assert!(static_scan["violations"].as_array().unwrap().is_empty());

    let final_snapshot = boundary_snapshot();
    assert_eq!(final_snapshot.hot_path_calyx_access_total, 2);

    let report = json!({
        "schema_version": 1,
        "issue": 1418,
        "source_of_truth": {
            "lowered_package_path": package_path,
            "lowered_package_sha256": litd_hash::sha256_hex(&package_bytes),
            "cold_aster_vault": cold_write.vault_dir,
            "cold_aster_seq": cold_write.seq,
            "cold_aster_cell_key": cold_write.cell_key,
            "hot_path_source_files": static_scan["files"],
        },
        "happy_path": {
            "runtime_package": {
                "schema": readback.schema,
                "document_schema": readback.document_summary.document_schema,
                "lowered_artifact_count": readback.document_summary.lowered_artifact_count,
                "package_sha256": readback.package_sha256,
                "package_bytes": readback.package_bytes,
                "manifest_integrity_verified": readback.manifest_integrity_verified,
                "runtime_lock_verified": readback.runtime_lock_verified,
                "playable_on_litd_runtime": readback.playable_on_litd_runtime,
            },
            "cold_calyx_access_outside_hot_path": {
                "seq": cold_read.seq,
                "sha256": cold_read.sha256,
                "graph_rows": cold_read.graph_rows,
                "bytes_match": cold_read.bytes == cold_payload,
            },
            "boundary_before_hot_path": before_hot,
            "boundary_after_hot_path": after_hot,
            "calyx_access_unchanged_during_hot_path":
                after_hot.calyx_access_total == before_hot.calyx_access_total,
            "hot_path_calyx_violations_unchanged_during_hot_path":
                after_hot.hot_path_calyx_access_total == before_hot.hot_path_calyx_access_total,
        },
        "edge_cases": [
            sim_edge,
            render_edge,
            non_lowered_edge,
            missing_document_edge,
        ],
        "static_scan": static_scan,
        "boundary_final": final_snapshot,
        "research": {
            "fixed_step_reference": "https://gafferongames.com/post/fix_your_timestep/",
            "deterministic_lockstep_reference": "https://gafferongames.com/post/deterministic_lockstep/",
            "reproducible_artifact_reference": "https://reproducible-builds.org/docs/source-date-epoch/"
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(report_readback.json["issue"], json!(1418));
    assert!(!report_readback.bytes.is_empty());
    println!(
        "ISSUE1418_DETERMINISM_BOUNDARY_FSV report={} sha256={} package={} package_sha256={} cold_seq={} hot_violations={}",
        report_readback.path.display(),
        report_readback.sha256,
        package_path.display(),
        litd_hash::sha256_hex(&package_bytes),
        cold_read.seq,
        final_snapshot.hot_path_calyx_access_total,
    );
}

struct BoundaryNoopModule;

impl AppModule for BoundaryNoopModule {
    fn name(&self) -> &'static str {
        "issue1418.boundary_noop"
    }
}

fn run_app_frame_hot_path() {
    let mut runtime = AppRuntime::default();
    runtime.modules_mut().register(BoundaryNoopModule).unwrap();
    runtime.start().unwrap();
    let start = Instant::now();
    runtime.frame(start).unwrap();
    runtime.frame(start + Duration::from_millis(34)).unwrap();
    runtime.shutdown().unwrap();
}

fn run_fixed_tick_hot_path() {
    let config = SimConfig::new(64).unwrap();
    let mut sim = FixedTickSim::<2>::new(config, 0x1418).unwrap();
    sim.spawn_body(
        FixedVec3::ZERO,
        FixedVec3::from_i32s(64, 0, 0),
        FixedVec3::ZERO,
    )
    .unwrap();
    let report = sim.run_ticks(3).unwrap();
    assert_eq!(report.steps, 3);
    assert_eq!(report.final_tick, 3);
}

fn blocked_calyx_write_in_phase(phase: HotPathPhase, vault_dir: PathBuf) -> Value {
    let spec = calyx_spec(vault_dir.clone());
    let before_exists = vault_dir.exists();
    let error = {
        let _guard = enter_hot_path(phase);
        write_calyx_json_bytes(&spec, br#"{"must_not_write":true}"#).unwrap_err()
    };
    let after_exists = vault_dir.exists();
    assert!(!before_exists);
    assert!(!after_exists);
    assert!(error.contains("deterministic hot path"));
    json!({
        "name": format!("blocked_calyx_write_in_{}", phase.as_str()),
        "phase": phase,
        "before_vault_exists": before_exists,
        "after_vault_exists": after_exists,
        "state_unchanged": before_exists == after_exists,
        "error": error,
    })
}

fn non_lowered_package_edge(path: &Path) -> Value {
    let package = package_value("litd.editor_document.v1").unwrap();
    let bytes = serde_json::to_vec_pretty(&package).unwrap();
    fs::write(path, &bytes).unwrap();
    let before = fs::read(path).unwrap();
    let error = load_runtime_locked_package(path, "issue1418-fsv")
        .unwrap_err()
        .to_string();
    let after = fs::read(path).unwrap();
    assert_eq!(before, after);
    assert!(error.contains("document.schema"));
    json!({
        "name": "non_lowered_document_schema",
        "before_bytes": before.len(),
        "after_bytes": after.len(),
        "state_unchanged": before == after,
        "error": error,
    })
}

fn missing_document_edge(path: &Path) -> Value {
    let mut package = package_value(LITD_LOWERED_DOCUMENT_SCHEMA).unwrap();
    package.as_object_mut().unwrap().remove("document");
    let bytes = serde_json::to_vec_pretty(&package).unwrap();
    fs::write(path, &bytes).unwrap();
    let before = fs::read(path).unwrap();
    let error = load_runtime_locked_package(path, "issue1418-fsv")
        .unwrap_err()
        .to_string();
    let after = fs::read(path).unwrap();
    assert_eq!(before, after);
    assert!(error.contains("document"));
    json!({
        "name": "missing_runtime_document",
        "before_bytes": before.len(),
        "after_bytes": after.len(),
        "state_unchanged": before == after,
        "error": error,
    })
}

fn scan_hot_path_sources() -> std::io::Result<Value> {
    let root = workspace_root();
    let files = [
        "crates/litd-engine/src/app.rs",
        "crates/litd-engine/src/sim.rs",
        "crates/litd-game/src/runtime_renderer.rs",
    ];
    let mut violations = Vec::new();
    let mut file_reports = Vec::new();
    for file in files {
        let path = root.join(file);
        let text = fs::read_to_string(&path)?;
        let mut hits = Vec::new();
        for token in FORBIDDEN_HOT_PATH_TOKENS {
            if text.contains(token) {
                hits.push(*token);
                violations.push(json!({"file": file, "token": token}));
            }
        }
        file_reports.push(json!({
            "path": path,
            "bytes": text.len(),
            "sha256": litd_hash::sha256_hex(text.as_bytes()),
            "forbidden_hits": hits,
        }));
    }
    Ok(json!({
        "forbidden_tokens": FORBIDDEN_HOT_PATH_TOKENS,
        "files": file_reports,
        "violations": violations,
    }))
}

fn package_value(document_schema: &str) -> Result<Value, EngineError> {
    let share_id = "issue1418determinism";
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 14180,
        path: format!("/share/{share_id}"),
    };
    let assets = vec![RuntimePackageAssetRef {
        product_id: "litd.asset.issue1418.lowered.mesh".to_string(),
        source_relative_path: "assets/issue1418/lowered.mesh".to_string(),
        source_sha256: "1418141814181418141814181418141814181418141814181418141814181418"
            .to_string(),
        source_bytes: 4096,
    }];
    let document_fingerprint = "1418ad0c1418ad0c";
    let play_build_fingerprint = "1418beef1418beef";
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue1418",
        LITD_RUNTIME_ID,
        &endpoint,
        document_fingerprint,
        play_build_fingerprint,
        &assets,
    )?;
    Ok(json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": format!("litd.quick_share.{share_id}"),
        "created_by": "agent.issue1418",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": endpoint.uri(),
        "document": {
            "schema": document_schema,
            "calyx_document_id": "calyx://world/issue1418-boundary",
            "revision_id": "rev-issue1418",
            "lowered_artifacts": {
                "kernel:spawn-risk": {
                    "artifact_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "kind": "kernel",
                    "source": "Aster Graph CF structured lowered artifact"
                },
                "prediction:economy-pulse": {
                    "artifact_id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "kind": "prediction",
                    "source": "Aster Graph CF structured lowered artifact"
                }
            }
        },
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": assets.len(),
        "assets": assets,
        "manifest_fingerprint": manifest_fingerprint
    }))
}

fn calyx_spec(vault_dir: PathBuf) -> CalyxJsonStoreSpec<'static> {
    CalyxJsonStoreSpec {
        vault_dir: Box::leak(vault_dir.into_boxed_path()),
        vault_id: "01ARZ3NDEKTSV4RRFFQ69G5FCD",
        purpose: b"litd-engine-issue1418-determinism-boundary",
        collection: "issue1418_boundary",
        row_key: b"boundary/source_of_truth",
        column: b"state_json",
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}
