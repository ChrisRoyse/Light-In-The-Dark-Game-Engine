use super::*;
use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, stable_fingerprint},
};
use litd_engine::{
    math::{FixedVec3, Transform, Vec3},
    sim::SimBodyId,
};

#[test]
fn play_session_builds_sim_snapshot_and_presentation() {
    let document = fixture_document();
    let session = PlaySession::start(&document, PlayCameraRig::TopDown).unwrap();

    assert_eq!(session.state, PlaySessionState::Running);
    assert_eq!(session.build.spawned_bodies.len(), 1);
    assert_eq!(session.build.snapshot.bodies.len(), 1);
    assert_eq!(session.build.presentation.instances.len(), 1);
    assert_eq!(session.build.camera_config.rig, PlayCameraRig::TopDown);
    assert_eq!(session.build.camera_config.player_id, "player_1");
    assert_eq!(
        session.source_document.play_camera_config,
        session.build.camera_config
    );
    assert_eq!(
        session.build.source_fingerprint,
        stable_fingerprint(&session.source_document).unwrap()
    );
    assert_eq!(session.build.camera_pose.player_id, "player_1");
    assert_eq!(
        session.build.camera_pose.target_world,
        Vec3::new(3.0, 0.0, -6.0)
    );
    assert_eq!(session.build.camera_pose.distance, 104.0);
    assert_eq!(session.build.camera_pose.pitch_degrees, -82.0);
    assert_eq!(session.build.camera_pose.fov_degrees, 42.0);
    assert!(
        session.build.presentation.instances[0]
            .render_instance
            .mesh_id
            .contains("litd.asset.quaternius")
    );
    assert_eq!(session.build.visible_instance_ids, vec![1]);
}

#[test]
fn hot_reload_rebuilds_when_document_fingerprint_changes() {
    let mut document = fixture_document();
    let mut session = PlaySession::start(&document, PlayCameraRig::TopDown).unwrap();
    document.revision_id = "calyx-rev-2".to_string();
    document
        .objects
        .get_mut("unit_knight")
        .unwrap()
        .transform
        .translation
        .x = 8.0;

    let plan = session.hot_reload(&document).unwrap();

    assert!(plan.needs_reload);
    assert_eq!(plan.changed_object_ids, vec!["unit_knight"]);
    assert_eq!(session.generation, 1);
    assert_eq!(session.source_document.revision_id, "calyx-rev-2");
    assert_eq!(session.build.source_revision_id, "calyx-rev-2");
}

#[test]
fn hot_reload_preserves_live_runtime_snapshot_for_data_edits() {
    let mut document = fixture_document();
    let mut session = PlaySession::start(&document, PlayCameraRig::TopDown).unwrap();
    let body_id = session.build.spawned_bodies[0].body_id;
    let initial_position = session.build.snapshot.bodies[0].position;
    let advanced = session
        .run_runtime_ticks_with_body_velocity(
            SimBodyId::from_raw(body_id),
            FixedVec3::from_i32s(64, 0, 0),
            64,
        )
        .unwrap();
    document.revision_id = "calyx-rev-material".to_string();
    document
        .objects
        .get_mut("unit_knight")
        .unwrap()
        .properties
        .insert("faction".to_string(), "Dawnward_Reloaded".to_string());

    let plan = session.hot_reload(&document).unwrap();

    assert!(plan.needs_reload);
    assert_eq!(session.generation, 1);
    assert_eq!(session.build.snapshot.tick, advanced.tick);
    assert_eq!(session.build.snapshot.state_hash, advanced.state_hash);
    assert_ne!(session.build.snapshot.bodies[0].position, initial_position);
    assert_eq!(
        session.build.presentation.instances[0]
            .render_instance
            .material_id,
        "Dawnward_Reloaded"
    );
    assert_eq!(
        session.build.presentation.source_state_hash,
        advanced.state_hash
    );
}

#[test]
fn save_during_active_play_tick_fails_loud() {
    let document = fixture_document();
    let session = PlaySession::start(&document, PlayCameraRig::TopDown).unwrap();

    let error = session
        .require_save_allowed_during_tick(true)
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.play.save.mid_tick"));
    assert!(session.require_save_allowed_during_tick(false).is_ok());
}

#[test]
fn camera_rig_switch_rebuilds_artifact_without_mocking_document() {
    let document = fixture_document();
    let mut session = PlaySession::start(&document, PlayCameraRig::TopDown).unwrap();
    println!(
        "edge=rig_switch before_generation={} before_rig={:?} before_camera=target={:.1},{:.1},{:.1};distance={:.1};fov={:.1}",
        session.generation,
        session.build.camera_rig,
        session.build.camera_pose.target_world.x,
        session.build.camera_pose.target_world.y,
        session.build.camera_pose.target_world.z,
        session.build.camera_pose.distance,
        session.build.camera_pose.fov_degrees
    );

    session.set_camera_rig(PlayCameraRig::OverShoulder).unwrap();

    assert_eq!(session.generation, 1);
    assert_eq!(session.build.camera_rig, PlayCameraRig::OverShoulder);
    assert_eq!(session.build.camera_config.rig, PlayCameraRig::OverShoulder);
    assert_eq!(
        session.build.camera_pose.target_world,
        Vec3::new(3.0, 12.0, 34.0)
    );
    assert_eq!(session.build.camera_pose.distance, 28.0);
    assert_eq!(session.build.camera_pose.fov_degrees, 62.0);
    assert_eq!(session.build.spawned_bodies.len(), 1);
    assert_eq!(session.build.visible_instance_ids, vec![1]);
    println!(
        "edge=rig_switch after_generation={} after_rig={:?} after_camera=target={:.1},{:.1},{:.1};distance={:.1};fov={:.1}",
        session.generation,
        session.build.camera_rig,
        session.build.camera_pose.target_world.x,
        session.build.camera_pose.target_world.y,
        session.build.camera_pose.target_world.z,
        session.build.camera_pose.distance,
        session.build.camera_pose.fov_degrees
    );
}

#[test]
fn per_player_camera_config_persists_into_build_artifact() {
    let document = fixture_document();
    let config = PlayCameraConfig::preset(PlayCameraRig::Isometric).for_player("player_2");
    println!(
        "edge=per_player before_player={} before_rig={:?}",
        config.player_id, config.rig
    );

    let session = PlaySession::start_with_camera_config(&document, config).unwrap();

    assert_eq!(session.build.camera_rig, PlayCameraRig::Isometric);
    assert_eq!(
        session.source_document.play_camera_config.rig,
        PlayCameraRig::Isometric
    );
    assert_eq!(
        session.source_document.play_camera_config.player_id,
        "player_2"
    );
    assert_eq!(session.build.camera_config.player_id, "player_2");
    assert_eq!(session.build.camera_pose.player_id, "player_2");
    assert_eq!(
        session.build.camera_pose.target_world,
        Vec3::new(3.0, 0.0, -6.0)
    );
    assert_eq!(session.build.camera_pose.yaw_degrees, 45.0);
    assert_eq!(session.build.camera_pose.pitch_degrees, -55.0);
    println!(
        "edge=per_player after_config_player={} after_pose_player={} after_rig={:?} after_yaw={:.1} after_pitch={:.1}",
        session.build.camera_config.player_id,
        session.build.camera_pose.player_id,
        session.build.camera_rig,
        session.build.camera_pose.yaw_degrees,
        session.build.camera_pose.pitch_degrees
    );
}

#[test]
fn document_owned_camera_config_drives_play_artifact() {
    let mut document = fixture_document();
    let default_fingerprint = stable_fingerprint(&document).unwrap();
    println!(
        "edge=document_camera before_fingerprint={} before_rig={:?}",
        default_fingerprint, document.play_camera_config.rig
    );
    document
        .set_play_camera_config(
            PlayCameraConfig::preset(PlayCameraRig::FirstPerson).for_player("player_3"),
        )
        .unwrap();
    let configured_fingerprint = stable_fingerprint(&document).unwrap();

    let session = PlaySession::start_from_document(&document).unwrap();

    assert_ne!(configured_fingerprint, default_fingerprint);
    assert_eq!(
        session.source_document.play_camera_config.player_id,
        "player_3"
    );
    assert_eq!(session.build.camera_rig, PlayCameraRig::FirstPerson);
    assert_eq!(session.build.camera_config, document.play_camera_config);
    assert_eq!(session.build.source_fingerprint, configured_fingerprint);
    assert_eq!(
        session.build.camera_pose.target_world,
        Vec3::new(3.0, 7.0, 64.0)
    );
    println!(
        "edge=document_camera after_fingerprint={} after_player={} after_rig={:?} after_camera=target={:.1},{:.1},{:.1};distance={:.1};fov={:.1}",
        configured_fingerprint,
        session.build.camera_config.player_id,
        session.build.camera_rig,
        session.build.camera_pose.target_world.x,
        session.build.camera_pose.target_world.y,
        session.build.camera_pose.target_world.z,
        session.build.camera_pose.distance,
        session.build.camera_pose.fov_degrees
    );
}

#[test]
fn invalid_camera_config_fails_loud_before_session_build() {
    let document = fixture_document();
    let mut config = PlayCameraConfig::preset(PlayCameraRig::TopDown);
    config.distance = 0.0;
    println!(
        "edge=invalid_camera before_distance={} before_player={}",
        config.distance, config.player_id
    );

    let error = PlaySession::start_with_camera_config(&document, config)
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.play.camera.distance"));
    println!("edge=invalid_camera after_error={error}");
}

#[test]
fn play_build_requires_a_unit() {
    let document = EditorDocument::new("calyx://world/empty", "rev-1").unwrap();
    match PlaySession::start(&document, PlayCameraRig::TopDown) {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
            assert!(field.contains("document.units"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn issue1849_play_build_carries_full_catalog_past_previous_sixteen_cap() {
    let document = many_unit_document(34);
    let session = PlaySession::start(&document, PlayCameraRig::TopDown).unwrap();

    assert_eq!(session.build.spawned_bodies.len(), 34);
    assert_eq!(session.build.snapshot.bodies.len(), 34);
    assert_eq!(session.build.presentation.instances.len(), 34);
}

#[test]
fn issue1849_document_beyond_play_capacity_fails_loud_not_truncated() {
    let document = many_unit_document(PLAY_SIM_MAX_BODIES + 1);
    match PlaySession::start(&document, PlayCameraRig::TopDown) {
        Err(EngineError::SimCapacityExceeded { capacity }) => {
            assert_eq!(capacity, PLAY_SIM_MAX_BODIES);
        }
        other => panic!("expected SimCapacityExceeded, got {other:?}"),
    }
}

/// A valid document with exactly `unit_count` Unit objects, all bound to the same
/// real vault model product as the fixture knight.
fn many_unit_document(unit_count: usize) -> EditorDocument {
    let mut document = fixture_document();
    let template = document.objects.get("unit_knight").unwrap().clone();
    for index in 1..unit_count {
        let mut object = template.clone();
        object.id = format!("unit_clone_{index:05}");
        object.transform.translation.x = index as f32;
        document.upsert_object(object).unwrap();
    }
    document
}

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new("calyx://world/ashen-citadel", "calyx-rev-1").unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_knight",
                "Unit",
                None,
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
    assign_vault_model_to_unit(&mut document, "unit_knight", &record).unwrap();
    document
}
