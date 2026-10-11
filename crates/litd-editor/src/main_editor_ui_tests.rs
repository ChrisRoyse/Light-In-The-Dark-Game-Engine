use super::*;
use std::collections::BTreeSet;

#[test]
fn runtime_evidence_reports_operational_egui_shell() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    println!(
        "issue1282 happy before: catalog_records={} usable_records={} shell_status=uninitialized selected_asset=none",
        catalog.record_count(),
        catalog.usable_record_count()
    );
    let runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let evidence = runtime.evidence();
    println!(
        "issue1282 happy after: selected_index={} selected_path={} selected_sha={} thumbnail={} status_health={} status_errors={} status_message={}",
        evidence.selected_asset_index,
        evidence.selected_asset_relative_path,
        evidence.selected_asset_sha256,
        evidence.selected_asset_thumbnail,
        evidence.status_health,
        evidence.status_error_count,
        evidence.status_message
    );

    assert_eq!(evidence.frontend_kind, "egui_wgpu_operational_editor");
    assert_eq!(evidence.active_mode, EditorMode::Terrain);
    assert_eq!(evidence.active_inspector_title, "Doodad Brush");
    assert!(evidence.reference_images.contains("ED11-world-view.png"));
    assert_eq!(evidence.viewport_capture_region, "410,74,1138x746");
    assert!(evidence.operational_control_count >= 12);
    assert!(evidence.world_scene_primitives >= 40);
    let foliage = evidence
        .foliage
        .as_ref()
        .expect("default runtime should carry foliage config evidence");
    assert_eq!(foliage.patch_count, 18);
    assert_eq!(foliage.target_clump_count, 214);
    assert_eq!(foliage.placed_clump_count, 214);
    assert!(foliage.gpu_instancing_required);
    assert_eq!(
        evidence.world_layer_state,
        WorldLayerVisibility::default().summary()
    );
    assert!(evidence.world_terrain_visible);
    assert!(evidence.world_foliage_visible);
    assert!(evidence.world_doodads_visible);
    assert!(evidence.world_fog_effects_visible);
    assert!(!evidence.world_grid_visible);
    assert!(!evidence.world_letterbox_mode);
    assert_eq!(evidence.map_build_status, "idle");
    assert_eq!(evidence.map_build_operation_count, 0);
    assert_eq!(
        evidence.minimap_source_id,
        "litd.asset.composition.stylized_nature_ed11_v1"
    );
    assert_eq!(
        evidence.minimap_source_kind,
        "composition.stylized_nature_ed11_v1"
    );
    assert!(evidence.minimap_source_path.contains("Stylized_Nature"));
    assert_eq!(evidence.minimap_feature_count, 330);
    assert_eq!(evidence.minimap_area_count, 32);
    assert_eq!(evidence.minimap_marker_count, 232);
    assert_eq!(evidence.minimap_ed11_rift_beacon_marker_count, 6);
    assert_eq!(evidence.minimap_ed11_depth_fog_card_marker_count, 12);
    assert_eq!(evidence.minimap_ed11_selection_halo_marker_count, 2);
    assert_eq!(evidence.minimap_ed11_foliage_patch_marker_count, 18);
    assert_eq!(evidence.minimap_ed11_branch_fissure_area_count, 12);
    assert_eq!(evidence.minimap_ring_count, 2);
    assert_eq!(
        evidence.minimap_layer_state,
        "areas=true;markers=true;rings=true"
    );
    assert_eq!(
        evidence.minimap_visible_area_count,
        evidence.minimap_area_count
    );
    assert_eq!(
        evidence.minimap_visible_marker_count,
        evidence.minimap_marker_count
    );
    assert_eq!(
        evidence.minimap_visible_ring_count,
        evidence.minimap_ring_count
    );
    assert_eq!(evidence.minimap_navigation_count, 0);
    assert_eq!(evidence.last_minimap_navigation, "none");
    assert_eq!(
        evidence.minimap_viewport_world_rect,
        "-36.6,-48.0->36.6,0.0"
    );
    assert_eq!(evidence.asset_vault_record_count, catalog.record_count());
    assert_eq!(
        evidence.asset_vault_usable_record_count,
        catalog.usable_record_count()
    );
    assert_eq!(
        evidence.asset_vault_sources,
        catalog.download_source_summary_line()
    );
    let coverage = catalog.coverage_summary().unwrap();
    assert_eq!(evidence.asset_vault_record_types, coverage.record_types);
    assert_eq!(evidence.asset_vault_asset_types, coverage.asset_types);
    assert_eq!(evidence.asset_vault_licenses, coverage.licenses);
    assert_eq!(
        evidence.asset_vault_semantic_role_counts,
        coverage.semantic_role_counts
    );
    assert_eq!(
        evidence.asset_vault_semantic_profile_count,
        coverage.semantic_profile_count
    );
    assert_eq!(
        evidence.asset_vault_semantic_classified_profile_count,
        coverage.semantic_classified_profile_count
    );
    assert_eq!(
        evidence.asset_vault_semantic_unclassified_profile_count,
        coverage.semantic_unclassified_profile_count
    );
    assert_eq!(
        evidence.asset_vault_semantic_tag_count,
        coverage.semantic_tag_count
    );
    assert!(evidence.asset_vault_asset_types.contains("audio="));
    assert_eq!(
        evidence.asset_vault_download_bytes_on_disk,
        coverage.download_bytes_on_disk
    );
    assert_eq!(
        evidence.asset_vault_extraction_bytes_on_disk,
        coverage.extraction_bytes_on_disk
    );
    assert_eq!(
        evidence.asset_vault_issue1242_p0_status,
        litd_editor::asset_vault::ISSUE1242_SEMANTIC_STATUS
    );
    assert!(evidence.asset_vault_asset_types.contains("model="));
    assert!(evidence.asset_vault_asset_types.contains("surface="));
    assert!(
        evidence
            .asset_vault_semantic_role_counts
            .contains("architecture=")
    );
    assert!(
        evidence
            .asset_vault_semantic_role_counts
            .contains("character=")
    );
    assert!(
        evidence
            .asset_vault_semantic_role_counts
            .contains("terrain_surface=")
    );
    assert!(
        evidence
            .asset_vault_semantic_role_counts
            .contains("vfx_light=")
    );
    assert!(evidence.asset_vault_sources.contains(&format!(
        "kaykit={}",
        catalog.download_source_count("kaykit")
    )));
    assert!(evidence.asset_vault_sources.contains(&format!(
        "quaternius={}",
        catalog.download_source_count("quaternius")
    )));
    assert_ne!(evidence.selected_asset_label, "Unit_Knight");
    assert!(
        evidence.selected_asset_relative_path.ends_with(".glb")
            || evidence.selected_asset_relative_path.ends_with(".gltf")
    );
    assert!(!evidence.selected_asset_label.trim().is_empty());
    assert!(!evidence.selected_asset_source.trim().is_empty());
    assert!(!evidence.selected_asset_id.trim().is_empty());
    assert!(!evidence.selected_asset_license.trim().is_empty());
    assert_eq!(evidence.selected_asset_sha256.len(), 64);
    assert!(evidence.selected_asset_bytes_on_disk > 0);
    assert_eq!(evidence.asset_thumbnail_count, 96);
    let unique_thumbnails = runtime
        .asset_entries
        .iter()
        .map(|entry| entry.thumbnail.path().to_path_buf())
        .collect::<BTreeSet<_>>();
    assert_eq!(unique_thumbnails.len(), evidence.asset_thumbnail_count);
    assert_eq!(evidence.asset_thumbnail_warm_target_count, 24);
    assert_eq!(evidence.asset_thumbnail_error_count, 0);
    assert!(!evidence.selected_asset_thumbnail.trim().is_empty());
    assert_eq!(evidence.status_health, "ok");
    assert_eq!(evidence.status_error_count, 0);
    assert_eq!(evidence.status_message, "No Errors");
}

#[test]
fn status_strip_starts_clean_and_fbx_selection_still_fails_loud() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence();
    println!(
        "issue1282 fbx_edge before: selected_index={} selected_path={} status_health={} status_errors={} status_message={}",
        before.selected_asset_index,
        before.selected_asset_relative_path,
        before.status_health,
        before.status_error_count,
        before.status_message
    );

    assert!(
        before.selected_asset_relative_path.ends_with(".glb")
            || before.selected_asset_relative_path.ends_with(".gltf")
    );
    assert_eq!(before.status_health, "ok");
    assert_eq!(before.status_error_count, 0);
    assert_eq!(before.status_message, "No Errors");

    let fbx_index = runtime
        .asset_entries
        .iter()
        .position(|entry| entry.relative_path.to_ascii_lowercase().ends_with(".fbx"))
        .expect("real editor palette should include at least one FBX edge asset");
    runtime.select_asset(fbx_index);
    let after = runtime.evidence();
    println!(
        "issue1282 fbx_edge after: selected_index={} selected_path={} status_health={} status_errors={} status_message={}",
        after.selected_asset_index,
        after.selected_asset_relative_path,
        after.status_health,
        after.status_error_count,
        after.status_message
    );

    assert!(after.selected_asset_relative_path.ends_with(".fbx"));
    assert_eq!(after.status_health, "error");
    assert_eq!(after.status_error_count, 1);
    assert!(after.status_message.contains("Previewer"));
    assert!(
        after
            .status_message
            .contains("editor.asset_previewer.source.extension")
    );
}

#[test]
fn default_preview_asset_selection_rejects_real_palette_without_previewable_models() {
    let runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let fbx_entries = runtime
        .asset_entries
        .iter()
        .filter(|entry| entry.relative_path.to_ascii_lowercase().ends_with(".fbx"))
        .cloned()
        .collect::<Vec<_>>();
    println!(
        "issue1282 no_previewable_edge before: real_fbx_entries={} first_fbx={} previewable_count=0",
        fbx_entries.len(),
        fbx_entries
            .first()
            .map(|entry| entry.relative_path.as_str())
            .unwrap_or("none")
    );
    assert!(!fbx_entries.is_empty());

    let result = main_editor_asset_entries::default_model_preview_asset_index(&fbx_entries);
    println!(
        "issue1282 no_previewable_edge after: result={}",
        match &result {
            Ok(index) => format!("unexpected_ok index={index}"),
            Err(error) => error.to_string(),
        }
    );

    let error = result.expect_err("real FBX-only palette must not pick a default preview asset");
    let message = error.to_string();
    assert!(message.contains("editor.shell.runtime.asset_catalog.model_preview_default"));
    assert!(message.contains("glTF/GLB"));
}

#[test]
fn status_strip_reports_real_missing_previewer_asset_error() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let mut missing_record = catalog
        .records()
        .iter()
        .find(|record| record.relative_path.ends_with(".glb"))
        .expect("real vault should contain at least one GLB record")
        .clone();
    missing_record.absolute_path = std::path::PathBuf::from("tmp/issue1281-status/missing.glb");
    let error = litd_editor::asset_previewer::load_model_previewer_from_record(
        &missing_record,
        litd_editor::asset_previewer::ModelPreviewRequest::default(),
    )
    .expect_err("missing GLB source must fail loud")
    .to_string();

    runtime.model_previewer = None;
    runtime.model_previewer_error = error;
    let evidence = runtime.evidence();

    assert_eq!(evidence.status_health, "error");
    assert_eq!(evidence.status_error_count, 1);
    assert!(evidence.status_message.contains("Previewer"));
    assert!(
        evidence
            .status_message
            .contains("editor.asset_previewer.source.read")
    );
    assert!(evidence.status_message.contains("missing.glb"));
}

#[test]
fn empty_minimap_scene_keeps_world_outline_and_zero_visible_counts() {
    let default_report = default_scene_report_for_tests().unwrap();
    let default_runtime =
        EditorShellRuntime::new_with_world_scene_report(1920.0, 1080.0, &default_report).unwrap();
    let expected_viewport = default_runtime.evidence().minimap_viewport_world_rect;
    let report = empty_minimap_scene_report_for_tests().unwrap();
    let runtime = EditorShellRuntime::new_with_world_scene_report(1920.0, 1080.0, &report).unwrap();
    let evidence = runtime.evidence();

    assert_eq!(
        evidence.minimap_feature_count,
        report.world_map.outline.len()
    );
    assert_eq!(evidence.minimap_area_count, 0);
    assert_eq!(evidence.minimap_marker_count, 0);
    assert_eq!(evidence.minimap_ring_count, 0);
    assert_eq!(evidence.minimap_visible_area_count, 0);
    assert_eq!(evidence.minimap_visible_marker_count, 0);
    assert_eq!(evidence.minimap_visible_ring_count, 0);
    assert_eq!(
        evidence.minimap_layer_state,
        "areas=true;markers=true;rings=true"
    );
    assert_eq!(evidence.minimap_viewport_world_rect, expected_viewport);
}

#[test]
fn blank_world_scene_builds_shell_with_zero_authored_primitives() {
    let loaded = litd_editor::asset_scene::load_blank_editor_world_asset_scene().unwrap();
    let runtime =
        EditorShellRuntime::new_with_world_scene_report(1920.0, 1080.0, &loaded.report).unwrap();
    let evidence = runtime.evidence();

    assert_eq!(evidence.world_scene_primitives, 0);
    assert!(evidence.foliage.is_none());
    assert_eq!(
        evidence.minimap_source_id,
        "litd.editor.blank_world.authoring_v1"
    );
    assert_eq!(evidence.minimap_source_kind, "blank_authoring_bounds");
    assert_eq!(
        evidence.minimap_source_path,
        "generated://editor/blank-world/authoring-v1"
    );
    assert_eq!(evidence.minimap_feature_count, 4);
    assert_eq!(evidence.minimap_area_count, 0);
    assert_eq!(evidence.minimap_marker_count, 0);
    assert_eq!(evidence.minimap_ring_count, 0);
}

#[test]
fn minimap_viewport_world_rect_tracks_camera_distance() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence().minimap_viewport_world_rect;

    runtime.plan.viewport.camera.distance = 96.0;
    let after = runtime.evidence().minimap_viewport_world_rect;

    assert_eq!(before, "-36.6,-48.0->36.6,0.0");
    assert_eq!(after, "-24.4,-40.0->24.4,-8.0");
}

#[test]
fn play_camera_rig_selection_persists_to_shell_state_file() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let state_path = std::path::Path::new("tmp").join(format!(
        "litd-editor-play-rig-persistence-{}-{unique}-aster",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&state_path);
    println!("edge=rig_persistence before_exists={}", state_path.exists());

    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut writer = ShellStateStore::new(state_path.clone());
    writer
        .save_plan_with_runtime_state(
            &base,
            WorldLayerVisibility::default(),
            PlayCameraRig::FirstPerson,
        )
        .unwrap();
    let saved = String::from_utf8(
        litd_editor::editor_state_store::EditorStateStore::new(state_path.clone())
            .read_shell_state()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(saved.contains("\"schema_version\": 12"));
    assert!(saved.contains("\"command_keybindings\""));
    assert!(saved.contains("\"play_camera_rig\": \"FirstPerson\""));

    let mut reader = ShellStateStore::new(state_path.clone());
    let _ = reader.load_or_initialize(settings, base).unwrap();

    assert_eq!(reader.play_camera_rig(), PlayCameraRig::FirstPerson);
    assert!(reader.loaded_from_disk());
    println!(
        "edge=rig_persistence after_exists={} state_path={} rig={:?}",
        state_path.exists(),
        state_path.display(),
        reader.play_camera_rig()
    );
    let _ = std::fs::remove_dir_all(&state_path);
}
