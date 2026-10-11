use std::{fs, path::PathBuf};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    play::PlayCameraRig,
};
use litd_engine::{
    math::{FixedVec3, Transform, Vec3},
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};

use super::*;

#[test]
fn issue1186_session_capture_source_of_truth_fsv() {
    let variants = fixture_variants();
    let request = request(&variants[0].document, "issue1186-session-ledger", "a");
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let ledger_path = root.join("session-capture-ledger.json");
    let aster_dir = root.join("aster");

    let ledger = capture_play_session(variants, request).unwrap();
    let before_catalog = ledger.readback.variant_catalog_fingerprint.clone();
    println!(
        "issue1186 happy before_write active={} final_hash={:016X} restart_hash={:016X} variants={} checkpoint={}",
        ledger.active_variant_id,
        ledger.readback.full_replay_final_hash,
        ledger.readback.restart_final_hash,
        ledger.readback.variant_count,
        ledger.readback.checkpoint_tick
    );
    let toggled = toggle_ab_variant(&ledger, "b").unwrap();
    assert_eq!(toggled.active_variant_id, "b");
    assert_eq!(toggled.readback.variant_catalog_fingerprint, before_catalog);
    assert!(toggled.readback.a_b_isolated);
    assert_eq!(
        toggled.readback.full_replay_final_hash,
        ledger.readback.full_replay_final_hash
    );

    let writeback = write_play_session_capture_ledger(&ledger_path, &aster_dir, &toggled).unwrap();
    let aster = read_play_session_capture_ledger(&aster_dir, &toggled.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_play_session_capture_ledger(&aster.bytes).unwrap();
    assert!(!ledger_path.exists());
    assert_eq!(writeback.bytes, aster.bytes);
    assert_eq!(parsed.active_variant_id, "b");
    assert!(parsed.readback.replay_final_hash_matches);
    assert!(parsed.readback.restart_matches_full_replay);
    assert!(parsed.readback.a_b_isolated);
    assert_eq!(parsed.readback.lowered_order_count, 1);
    assert_eq!(parsed.readback.human_order_count, 1);
    println!(
        "issue1186 happy after_aster session={} active={} seq={} graph_rows={} ledger_sha={} state_matches_aster={} cell={} index={}",
        parsed.session_id,
        parsed.active_variant_id,
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        writeback.bytes == aster.bytes,
        writeback.cell_key,
        writeback.index_key
    );
}

#[test]
fn issue1186_edges_fail_closed_and_preserve_state() {
    let variants = fixture_variants();
    let request = request(&variants[0].document, "issue1186-edge-ledger", "a");
    let ledger = capture_play_session(variants, request).unwrap();

    let restarted = restart_session_again(&ledger).unwrap();
    assert_eq!(
        restarted.restart.restart_final_hash,
        ledger.replay.expected_final_hash
    );
    assert!(restarted.restart.matches_full_replay);
    assert_eq!(restarted.events.len(), ledger.events.len() + 1);
    println!(
        "issue1186 edge_restart before_events={} after_events={} checkpoint={} final_hash={:016X}",
        ledger.events.len(),
        restarted.events.len(),
        restarted.checkpoint.checkpoint_tick,
        restarted.restart.restart_final_hash
    );

    let bytes = serde_json::to_vec_pretty(&ledger).unwrap();
    let mut corrupted = bytes.clone();
    corrupted.truncate(bytes.len() / 3);
    let corrupt_error = parse_play_session_capture_ledger(&corrupted)
        .unwrap_err()
        .to_string();
    assert!(corrupt_error.contains("parse play-session capture ledger"));
    println!(
        "issue1186 edge_corrupt before_bytes={} after_bytes={} error={}",
        bytes.len(),
        corrupted.len(),
        corrupt_error
    );

    let before_a = ledger
        .variants
        .iter()
        .find(|variant| variant.variant_id == "a")
        .unwrap()
        .document_fingerprint
        .clone();
    let before_catalog = ledger.readback.variant_catalog_fingerprint.clone();
    let toggled = toggle_ab_variant(&ledger, "b").unwrap();
    let after_a = toggled
        .variants
        .iter()
        .find(|variant| variant.variant_id == "a")
        .unwrap()
        .document_fingerprint
        .clone();
    assert_eq!(before_a, after_a);
    assert_eq!(before_catalog, toggled.readback.variant_catalog_fingerprint);
    assert!(toggled.readback.a_b_isolated);
    println!(
        "issue1186 edge_ab_isolation before_active={} after_active={} a_unchanged={} catalog_unchanged={}",
        ledger.active_variant_id,
        toggled.active_variant_id,
        before_a == after_a,
        before_catalog == toggled.readback.variant_catalog_fingerprint
    );
}

fn request(
    document: &EditorDocument,
    ledger_id: &str,
    active_variant_id: &str,
) -> PlaySessionCaptureRequest {
    PlaySessionCaptureRequest::new(PlaySessionCaptureRequestArgs {
        ledger_id,
        session_id: "issue1186-session",
        actor_id: "human.qa.ember",
        active_variant_id,
        camera_rig: PlayCameraRig::TopDown,
        run_ticks: 4,
        checkpoint_tick: 2,
        orders: orders(document),
    })
}

fn orders(document: &EditorDocument) -> Vec<SimOrder> {
    let session = crate::play::PlaySession::start(document, PlayCameraRig::TopDown).unwrap();
    let body = SimBodyId::from_raw(session.build.spawned_bodies[0].body_id);
    vec![
        SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(FixedVec3::from_i32s(64, 0, 0)),
        ),
        SimOrder::new(
            3,
            SimOrderSource::lowered_agent(7),
            1,
            body,
            SimOrderKind::AddVelocity(FixedVec3::from_i32s(0, 64, 0)),
        ),
    ]
}

fn fixture_variants() -> Vec<PlaySessionVariantInput> {
    vec![
        PlaySessionVariantInput {
            variant_id: "a".to_string(),
            label: "baseline combat route".to_string(),
            document: fixture_document("issue1186-rev-a", 3.0, "Dawnward"),
        },
        PlaySessionVariantInput {
            variant_id: "b".to_string(),
            label: "variant ember route".to_string(),
            document: fixture_document("issue1186-rev-b", 8.0, "Vesper"),
        },
    ]
}

fn fixture_document(revision_id: &str, x: f32, faction: &str) -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1186-session-capture", revision_id).unwrap();
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
                Transform::from_translation(Vec3::new(x, 0.0, -6.0)),
            )
            .with_property("faction", faction),
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
    crate::test_scratch::issue_scratch_dir("issue1186", label)
}
