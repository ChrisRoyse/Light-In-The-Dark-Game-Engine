use litd_hash::sha256_hex;
use std::fs;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    play::PlayCameraRig,
    play_responsiveness::{
        PlayResponsivenessInputEvent, PlayResponsivenessRunInput, build_play_responsiveness_ledger,
        parse_play_responsiveness_ledger, read_play_responsiveness_ledger,
        write_play_responsiveness_ledger,
    },
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1183_FSV_ROOT",
        "issue1183_play_responsiveness",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("play-responsiveness-ledger.json");
    let artifact_path = root.join("issue1183-play-responsiveness-readback.json");
    let document = fixture_document()?;

    let ledger = build_play_responsiveness_ledger(
        &document,
        "issue1183-manual-ledger",
        "litd.project.issue1183.manual",
        "human.qa.ember",
        PlayCameraRig::TopDown,
        vec![
            happy_run("overlay-off", false),
            happy_run("overlay-on", true),
        ],
    )?;
    println!(
        "happy_path before_write runs={} inputs={} max_latency={:.3} overlay_ok={}",
        ledger.readback.run_count,
        ledger.readback.input_count,
        ledger.readback.max_input_to_reaction_ms,
        ledger.readback.overlay_non_perturbing
    );
    let writeback = write_play_responsiveness_ledger(&ledger_path, &aster_dir, &ledger)?;
    let aster = read_play_responsiveness_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing play-responsiveness ledger in Aster")?;
    let parsed = parse_play_responsiveness_ledger(&aster.bytes)?;
    let state_bytes = fs::read(&ledger_path)?;
    if state_bytes != aster.bytes {
        return Err(format!("ledger state/Aster mismatch at {}", ledger_path.display()).into());
    }
    println!(
        "happy_path after_aster seq={} graph_rows={} ledger_sha={} state_matches_aster={} max_latency={:.3} overlay_ok={}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        state_bytes == aster.bytes,
        parsed.readback.max_input_to_reaction_ms,
        parsed.readback.overlay_non_perturbing
    );

    let edge_lag = edge_laggy(&document)?;
    let edge_spike = edge_spiky(&document)?;
    let edge_overlay = edge_overlay_drift(&document)?;
    let artifact = json!({
        "issue": 1183,
        "source_of_truth": "Aster Graph CF play-responsiveness ledger row plus JSON state-file readback",
        "artifact_path": artifact_path,
        "happy_path": {
            "ledger": {
                "path": ledger_path,
                "state_file_matches_aster": state_bytes == aster.bytes,
                "state_sha256": sha256_hex(&state_bytes),
                "aster_sha256": aster.sha256,
                "aster_seq": aster.seq,
                "aster_graph_rows": aster.graph_rows,
                "cell_key": writeback.cell_key,
                "index_key": writeback.index_key,
                "bytes": state_bytes.len()
            },
            "metrics": {
                "run_count": parsed.readback.run_count,
                "input_count": parsed.readback.input_count,
                "max_input_to_reaction_ms": parsed.readback.max_input_to_reaction_ms,
                "over_100ms_count": parsed.readback.over_100ms_count,
                "frame_spike_count": parsed.readback.frame_spike_count,
                "overlay_non_perturbing": parsed.readback.overlay_non_perturbing,
                "overlay_comparison": parsed.overlay_comparison
            }
        },
        "edge_cases": [edge_lag, edge_spike, edge_overlay]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1183 responsiveness FSV artifact={} bytes={} artifact_sha={} ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        aster.sha256
    );
    Ok(())
}

fn edge_laggy(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let ledger = build_play_responsiveness_ledger(
        document,
        "issue1183-manual-laggy",
        "litd.project.issue1183.manual",
        "human.qa.ember",
        PlayCameraRig::TopDown,
        vec![laggy_run()],
    )?;
    println!(
        "edge_lag before_threshold=100 after_max_latency={:.3} over_100={}",
        ledger.readback.max_input_to_reaction_ms, ledger.readback.over_100ms_count
    );
    Ok(json!({
        "name": "laggy_build_over_100ms_flagged",
        "before_threshold_ms": 100.0,
        "after_max_latency_ms": ledger.readback.max_input_to_reaction_ms,
        "over_100ms_count": ledger.readback.over_100ms_count
    }))
}

fn edge_spiky(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let ledger = build_play_responsiveness_ledger(
        document,
        "issue1183-manual-spiky",
        "litd.project.issue1183.manual",
        "human.qa.ember",
        PlayCameraRig::TopDown,
        vec![spiky_run()],
    )?;
    println!(
        "edge_spike before_spikes=0 after_spikes={} spike_ms={:.3}",
        ledger.readback.frame_spike_count, ledger.reports[0].frame_spikes[0].frame_time_ms
    );
    Ok(json!({
        "name": "frame_time_spike_captured",
        "before_spike_count": 0,
        "after_spike_count": ledger.readback.frame_spike_count,
        "spike_ms": ledger.reports[0].frame_spikes[0].frame_time_ms
    }))
}

fn edge_overlay_drift(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let ledger = build_play_responsiveness_ledger(
        document,
        "issue1183-manual-overlay-drift",
        "litd.project.issue1183.manual",
        "human.qa.ember",
        PlayCameraRig::TopDown,
        vec![happy_run("overlay-off", false), overlay_drift_run()],
    )?;
    let comparison = ledger.overlay_comparison.as_ref().expect("comparison");
    println!(
        "edge_overlay before_ok=true after_ok={} delta={:.3}",
        ledger.readback.overlay_non_perturbing, comparison.max_latency_delta_ms
    );
    Ok(json!({
        "name": "overlay_drift_detected",
        "before_overlay_non_perturbing": true,
        "after_overlay_non_perturbing": ledger.readback.overlay_non_perturbing,
        "max_latency_delta_ms": comparison.max_latency_delta_ms,
        "overlay_render_cost_ms": comparison.max_overlay_render_cost_ms
    }))
}

fn happy_run(run_id: &str, overlay_enabled: bool) -> PlayResponsivenessRunInput {
    PlayResponsivenessRunInput {
        run_id: run_id.to_string(),
        overlay_enabled,
        juice_audit_enabled: true,
        overlay_render_cost_ms: if overlay_enabled { 0.35 } else { 0.0 },
        frame_times_ms: vec![16.0, 16.0, 16.0, 16.0, 16.0, 16.0],
        input_events: vec![
            event("input-attack", "attack", 0.0, 4.0, 7.0, 2, 48.0),
            event("input-dodge", "dodge", 48.0, 52.0, 55.0, 5, 96.0),
        ],
    }
}

fn laggy_run() -> PlayResponsivenessRunInput {
    PlayResponsivenessRunInput {
        run_id: "laggy-build".to_string(),
        overlay_enabled: false,
        juice_audit_enabled: true,
        overlay_render_cost_ms: 0.0,
        frame_times_ms: vec![16.0, 16.0, 16.0, 16.0, 16.0, 16.0, 16.0, 16.0],
        input_events: vec![event("input-lag", "attack", 0.0, 60.0, 70.0, 7, 128.0)],
    }
}

fn spiky_run() -> PlayResponsivenessRunInput {
    PlayResponsivenessRunInput {
        run_id: "spiky-build".to_string(),
        overlay_enabled: false,
        juice_audit_enabled: false,
        overlay_render_cost_ms: 0.0,
        frame_times_ms: vec![16.0, 55.0, 16.0, 16.0],
        input_events: vec![event("input-spike", "confirm", 0.0, 4.0, 7.0, 2, 87.0)],
    }
}

fn overlay_drift_run() -> PlayResponsivenessRunInput {
    PlayResponsivenessRunInput {
        run_id: "overlay-on".to_string(),
        overlay_enabled: true,
        juice_audit_enabled: true,
        overlay_render_cost_ms: 1.5,
        frame_times_ms: vec![16.0, 16.0, 16.8, 16.0, 16.0, 16.8],
        input_events: vec![
            event("input-attack", "attack", 0.0, 4.0, 7.0, 2, 48.8),
            event("input-dodge", "dodge", 48.0, 52.0, 55.0, 5, 97.6),
        ],
    }
}

fn event(
    input_id: &str,
    action: &str,
    input_timestamp_ms: f64,
    processing_start_ms: f64,
    processing_end_ms: f64,
    reaction_frame_index: usize,
    expected_reaction_timestamp_ms: f64,
) -> PlayResponsivenessInputEvent {
    PlayResponsivenessInputEvent {
        input_id: input_id.to_string(),
        action: action.to_string(),
        input_timestamp_ms,
        processing_start_ms,
        processing_end_ms,
        reaction_frame_index,
        expected_reaction_timestamp_ms,
        tolerance_ms: 0.001,
    }
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new(
        "calyx://world/issue1183-play-responsiveness",
        "issue1183-rev-1",
    )?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_playtest_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.play_responsiveness.fsv.asset".to_string(),
            reason: "missing real Quaternius Rock1 asset record".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}
