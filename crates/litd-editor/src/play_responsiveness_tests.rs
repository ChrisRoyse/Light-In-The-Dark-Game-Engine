use std::{fs, path::PathBuf};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
};
use litd_engine::math::{Transform, Vec3};

use super::*;

#[test]
fn issue1183_responsiveness_source_of_truth_fsv() {
    let document = fixture_document();
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let ledger_path = root.join("play-responsiveness-ledger.json");
    let aster_dir = root.join("aster");
    let ledger = build_play_responsiveness_ledger(
        &document,
        "issue1183-responsiveness-ledger",
        "litd.project.issue1183.fsv",
        "human.qa.ember",
        PlayCameraRig::TopDown,
        vec![
            happy_run("overlay-off", false),
            happy_run("overlay-on", true),
        ],
    )
    .unwrap();
    println!(
        "issue1183 happy before_write runs={} inputs={} max_latency={:.3} overlay_ok={}",
        ledger.readback.run_count,
        ledger.readback.input_count,
        ledger.readback.max_input_to_reaction_ms,
        ledger.readback.overlay_non_perturbing
    );
    let writeback = write_play_responsiveness_ledger(&ledger_path, &aster_dir, &ledger).unwrap();
    let aster = read_play_responsiveness_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_play_responsiveness_ledger(&aster.bytes).unwrap();
    assert!(!ledger_path.exists());
    assert_eq!(writeback.bytes, aster.bytes);
    assert_eq!(parsed.readback.run_count, 2);
    assert_eq!(parsed.readback.input_count, 4);
    assert_eq!(parsed.readback.over_100ms_count, 0);
    assert_eq!(parsed.readback.frame_spike_count, 0);
    assert!(parsed.readback.overlay_non_perturbing);
    assert!(parsed.readback.max_input_to_reaction_ms <= INSTANT_FEEL_THRESHOLD_MS);
    println!(
        "issue1183 happy after_aster seq={} graph_rows={} ledger_sha={} state_matches_aster={} inputs={} max_latency={:.3} overlay_ok={}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        writeback.bytes == aster.bytes,
        aster.input_count,
        parsed.readback.max_input_to_reaction_ms,
        aster.overlay_non_perturbing
    );
}

#[test]
fn issue1183_edges_flag_lag_spikes_and_overlay_drift() {
    let document = fixture_document();
    let laggy = build_play_responsiveness_ledger(
        &document,
        "issue1183-laggy-ledger",
        "litd.project.issue1183.fsv",
        "human.qa.ember",
        PlayCameraRig::TopDown,
        vec![laggy_run()],
    )
    .unwrap();
    println!(
        "issue1183 edge_lag before_threshold={} after_max_latency={:.3} over_100={}",
        RESPONSE_THRESHOLD_MS,
        laggy.readback.max_input_to_reaction_ms,
        laggy.readback.over_100ms_count
    );
    assert_eq!(laggy.readback.over_100ms_count, 1);
    assert!(laggy.readback.max_input_to_reaction_ms > RESPONSE_THRESHOLD_MS);

    let spiky = build_play_responsiveness_ledger(
        &document,
        "issue1183-spike-ledger",
        "litd.project.issue1183.fsv",
        "human.qa.ember",
        PlayCameraRig::TopDown,
        vec![spiky_run()],
    )
    .unwrap();
    println!(
        "issue1183 edge_spike before_spikes=0 after_spikes={} spike_ms={:.3}",
        spiky.readback.frame_spike_count, spiky.reports[0].frame_spikes[0].frame_time_ms
    );
    assert_eq!(spiky.readback.frame_spike_count, 1);

    let drift = build_play_responsiveness_ledger(
        &document,
        "issue1183-overlay-drift-ledger",
        "litd.project.issue1183.fsv",
        "human.qa.ember",
        PlayCameraRig::TopDown,
        vec![happy_run("overlay-off", false), overlay_drift_run()],
    )
    .unwrap();
    println!(
        "issue1183 edge_overlay before_ok=true after_ok={} delta={:.3}",
        drift.readback.overlay_non_perturbing,
        drift
            .overlay_comparison
            .as_ref()
            .unwrap()
            .max_latency_delta_ms
    );
    assert!(!drift.readback.overlay_non_perturbing);
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

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new(
        "calyx://world/issue1183-play-responsiveness",
        "issue1183-rev-1",
    )
    .unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_playtest_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    let record = AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .unwrap();
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record).unwrap();
    document
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1183", label)
}
