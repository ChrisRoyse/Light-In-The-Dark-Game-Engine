use super::*;
use crate::editor_state_store::EditorStateStore;
use crate::{
    asset_products::{
        AssetProduct, AssetProductKind, MODEL_PRODUCT_ID_PROPERTY, MODEL_SOURCE_PATH_PROPERTY,
        MODEL_SOURCE_SHA256_PROPERTY,
    },
    clipboard::{ClipboardPasteOptions, ClipboardPlacement, ClipboardTransform},
    play::{PlayCameraConfig, PlayCameraRig},
};
use litd_engine::math::{Transform, Vec3};
use std::fs;

#[test]
fn selection_copy_and_paste_are_stable() {
    let mut document = fixture_document();
    let mut selection = SelectionSet::default();
    selection.select_many(["unit_knight", "vfx_fire"]);

    let clipboard = copy_selection(&document, &selection).unwrap();
    let pasted = paste_clipboard(&mut document, &clipboard, "_copy").unwrap();
    let first = to_stable_ron(&document).unwrap();
    let second = to_stable_ron(&document).unwrap();

    assert_eq!(clipboard.objects.len(), 2);
    assert_eq!(pasted, vec!["unit_knight_copy", "vfx_fire_copy"]);
    assert_eq!(document.objects.len(), 5);
    assert_eq!(first, second);
}

#[test]
fn cross_project_paste_imports_dependencies_and_reconciles_ids() {
    // Keep `_root` bound for the whole test so the on-disk fixture model survives validate/paste.
    let (_root, product) = fixture_asset_product("cross-project");
    let source = fixture_document_with_clipboard_data(product.clone());
    let mut destination = EditorDocument::new("calyx://world/destination", "calyx-rev-b").unwrap();
    destination
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    destination
        .upsert_object(EditorObject::new(
            "unit_knight_copy",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(-4.0, 0.0, -4.0)),
        ))
        .unwrap();

    let mut selection = SelectionSet::default();
    selection.select_many(["unit_knight", "region_spawn", "terrain_ramp"]);
    let clipboard = copy_selection(&source, &selection).unwrap();
    let before = stable_fingerprint(&destination).unwrap();
    let report = paste_clipboard_with_options(
        &mut destination,
        &clipboard,
        ClipboardPasteOptions {
            id_suffix: "_copy".to_string(),
            placement: ClipboardPlacement::AtCursor {
                cursor: Vec3::new(20.0, 0.0, 30.0),
            },
            transform: ClipboardTransform::Rotate90Clockwise,
        },
    )
    .unwrap();

    assert_eq!(report.before_fingerprint, before);
    assert_ne!(report.after_fingerprint, before);
    assert_eq!(
        report.imported_asset_product_ids,
        vec![product.product_id.clone()]
    );
    assert_eq!(report.dependency_count, 2);
    assert_eq!(report.id_map["unit_knight"], "unit_knight_copy_2");
    assert_eq!(report.id_map["region_spawn"], "region_spawn_copy");
    assert_eq!(report.id_map["terrain_ramp"], "terrain_ramp_copy");
    assert!(destination.asset_products.contains_key(&product.product_id));
    assert!(destination.objects.contains_key("unit_knight_copy_2"));
    assert!(destination.regions.contains_key("region_spawn_copy"));
    assert!(destination.terrain_stamps.contains_key("terrain_ramp_copy"));
    assert_eq!(
        destination.objects["unit_knight_copy_2"]
            .properties
            .get(MODEL_PRODUCT_ID_PROPERTY),
        Some(&product.product_id)
    );
    source.validate().unwrap();
    destination.validate().unwrap();
}

#[test]
fn paste_is_atomic_when_clipboard_references_missing_product() {
    let (_root, product) = fixture_asset_product("missing-product");
    let source = fixture_document_with_clipboard_data(product);
    let mut selection = SelectionSet::default();
    selection.select_many(["unit_knight"]);
    let mut clipboard = copy_selection(&source, &selection).unwrap();
    clipboard.asset_products.clear();
    clipboard.fingerprint = stable_fingerprint(&(
        &clipboard.source_calyx_document_id,
        &clipboard.objects,
        &clipboard.regions,
        &clipboard.terrain_stamps,
        &clipboard.asset_products,
    ))
    .unwrap();

    let mut destination = fixture_document();
    let before = stable_fingerprint(&destination).unwrap();
    let error = paste_clipboard(&mut destination, &clipboard, "_copy").unwrap_err();
    let after = stable_fingerprint(&destination).unwrap();

    assert_eq!(before, after);
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(!destination.objects.contains_key("unit_knight_copy"));
}

#[test]
fn paste_transforms_cover_mirror_and_counter_rotation_edges() {
    let (_root, product) = fixture_asset_product("paste-transforms");
    let source = fixture_document_with_clipboard_data(product);
    let mut selection = SelectionSet::default();
    selection.select_many(["unit_knight"]);
    let clipboard = copy_selection(&source, &selection).unwrap();

    let mut mirror_horizontal = fixture_document();
    let horizontal_report = paste_clipboard_with_options(
        &mut mirror_horizontal,
        &clipboard,
        ClipboardPasteOptions {
            id_suffix: "_mh".to_string(),
            placement: ClipboardPlacement::InPlace,
            transform: ClipboardTransform::MirrorHorizontal,
        },
    )
    .unwrap();
    assert_eq!(horizontal_report.pasted_object_ids, vec!["unit_knight_mh"]);
    assert_eq!(
        mirror_horizontal.objects["unit_knight_mh"].transform.scale,
        Vec3::new(-1.0, 1.0, 1.0)
    );

    let mut mirror_vertical = fixture_document();
    paste_clipboard_with_options(
        &mut mirror_vertical,
        &clipboard,
        ClipboardPasteOptions {
            id_suffix: "_mv".to_string(),
            placement: ClipboardPlacement::InPlace,
            transform: ClipboardTransform::MirrorVertical,
        },
    )
    .unwrap();
    assert_eq!(
        mirror_vertical.objects["unit_knight_mv"].transform.scale,
        Vec3::new(1.0, 1.0, -1.0)
    );

    let mut counter = fixture_document();
    paste_clipboard_with_options(
        &mut counter,
        &clipboard,
        ClipboardPasteOptions {
            id_suffix: "_ccw".to_string(),
            placement: ClipboardPlacement::AtCursor {
                cursor: Vec3::new(-2.0, 0.0, 4.0),
            },
            transform: ClipboardTransform::Rotate90CounterClockwise,
        },
    )
    .unwrap();
    assert!(counter.objects.contains_key("unit_knight_ccw"));
    counter.validate().unwrap();
}

#[test]
fn empty_clipboard_selection_fails_loud() {
    let document = fixture_document();
    let selection = SelectionSet::default();
    let error = copy_selection(&document, &selection).unwrap_err();

    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
}

#[test]
fn undo_and_redo_restore_document_mementos() {
    let before = fixture_document();
    let mut after = before.clone();
    after.revision_id = "calyx-rev-2".to_string();
    after
        .objects
        .get_mut("unit_knight")
        .unwrap()
        .transform
        .translation
        .x = 8.0;
    let mut document = after.clone();
    let mut history = UndoRedoStack::new(8).unwrap();
    history
        .record("move knight", before.clone(), after)
        .unwrap();

    assert_eq!(
        history.undo(&mut document).unwrap(),
        Some("move knight".to_string())
    );
    assert_eq!(document, before);
    assert_eq!(
        history.redo(&mut document).unwrap(),
        Some("move knight".to_string())
    );
    assert_eq!(document.revision_id, "calyx-rev-2");
}

#[test]
fn undo_and_redo_restore_document_from_mvcc_snapshots() {
    let root = test_root("mvcc-history");
    let store = EditorStateStore::new(&root);
    let before = fixture_document();
    let mut after = before.clone();
    after.revision_id = "calyx-rev-2".to_string();
    after
        .objects
        .get_mut("unit_knight")
        .unwrap()
        .transform
        .translation
        .x = 8.0;
    let mut document = after.clone();
    let mut history = UndoRedoStack::new(8).unwrap();

    history
        .record_with_mvcc(&store, "move knight", before.clone(), after.clone())
        .unwrap();
    let entry = history.undo.last().unwrap();
    let before_seq = entry.before_snapshot_seq.unwrap();
    let after_seq = entry.after_snapshot_seq.unwrap();
    let before_bytes = store
        .read_history_bytes_at(&before.calyx_document_id, before_seq)
        .unwrap()
        .expect("before MVCC history row");
    let after_bytes = store
        .read_history_bytes_at(&after.calyx_document_id, after_seq)
        .unwrap()
        .expect("after MVCC history row");

    assert_ne!(before_seq, after_seq);
    assert!(before_bytes.len() > 128);
    assert!(after_bytes.len() > 128);
    assert_ne!(before_bytes, after_bytes);
    assert_eq!(
        history.undo_from_mvcc(&store, &mut document).unwrap(),
        Some("move knight".to_string())
    );
    assert_eq!(document, before);
    assert_eq!(
        history.redo_from_mvcc(&store, &mut document).unwrap(),
        Some("move knight".to_string())
    );
    assert_eq!(document, after);
    let graph_rows = store.read_graph_rows().unwrap();
    assert_eq!(graph_rows.len(), 1);
    assert_eq!(store.latest_seq().unwrap(), after_seq);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn autosave_record_carries_calyx_ids_and_fingerprint() {
    let document = fixture_document();
    let record = autosave_record(&document, true).unwrap();

    assert_eq!(record.calyx_document_id, "calyx://world/ashen-citadel");
    assert_eq!(record.calyx_revision_id, "calyx-rev-1");
    assert!(record.pending_changes);
    assert_eq!(record.stable_fingerprint.len(), 16);
    assert!(record.stable_ron.contains("unit_knight"));
}

#[test]
fn document_play_camera_config_is_validated_and_fingerprinted() {
    let mut document = fixture_document();
    let before = stable_fingerprint(&document).unwrap();
    document
        .set_play_camera_config(
            PlayCameraConfig::preset(PlayCameraRig::OverShoulder).for_player("player_2"),
        )
        .unwrap();
    let after = stable_fingerprint(&document).unwrap();
    let stable_ron = to_stable_ron(&document).unwrap();

    assert_ne!(before, after);
    assert_eq!(document.play_camera_config.player_id, "player_2");
    assert!(stable_ron.contains("play_camera_config"));
    assert!(stable_ron.contains("OverShoulder"));

    let mut invalid = document.clone();
    invalid.play_camera_config.fov_degrees = 0.0;
    let error = invalid.validate().unwrap_err().to_string();
    assert!(error.contains("editor.play.camera.fov"));
}

#[test]
fn invalid_history_config_fails_loud() {
    match UndoRedoStack::new(0) {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
            assert!(field.contains("history.max_entries"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new("calyx://world/ashen-citadel", "calyx-rev-1").unwrap();
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
                "unit_knight",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    document
        .upsert_object(EditorObject::new(
            "vfx_fire",
            "Vfx",
            Some("unit_knight".to_string()),
            Transform::from_translation(Vec3::new(3.0, 2.0, -6.0)),
        ))
        .unwrap();
    document
}

fn fixture_document_with_clipboard_data(product: AssetProduct) -> EditorDocument {
    let mut document = fixture_document();
    document
        .asset_products
        .insert(product.product_id.clone(), product.clone());
    let unit = document.objects.get_mut("unit_knight").unwrap();
    unit.properties.insert(
        MODEL_PRODUCT_ID_PROPERTY.to_string(),
        product.product_id.clone(),
    );
    unit.properties.insert(
        MODEL_SOURCE_SHA256_PROPERTY.to_string(),
        product.source_sha256.clone(),
    );
    unit.properties.insert(
        MODEL_SOURCE_PATH_PROPERTY.to_string(),
        product.source_relative_path.clone(),
    );
    document
        .upsert_region(
            EditorRegion::new(
                "region_spawn",
                "Spawn Region",
                Vec3::new(12.0, 0.0, -2.0),
                Vec3::new(2.0, 1.0, 3.0),
            )
            .with_property("spawn_table", "dawnward"),
        )
        .unwrap();
    document
        .upsert_terrain_stamp(
            EditorTerrainStamp::new(
                "terrain_ramp",
                "Ramp",
                Transform::from_translation(Vec3::new(6.0, 0.0, -8.0)),
                4,
                3,
                1.5,
            )
            .with_property("material", "ash-stone"),
        )
        .unwrap();
    document.validate().unwrap();
    document
}

// Returns the TestTempDir guard ALONGSIDE the product: `TestTempDir` deletes its directory on
// Drop (#1495), so the caller MUST keep the returned guard alive for the whole test — otherwise
// the on-disk `models/knight.glb` this product points at is removed the instant this fn returns,
// and the later `document.validate()` (which inspects the model file) fails with os error 3 (#1534).
fn fixture_asset_product(name: &str) -> (litd_engine::test_temp::TestTempDir, AssetProduct) {
    let root = test_root(name);
    let model_dir = root.join("models");
    let texture_dir = root.join("textures");
    fs::create_dir_all(&model_dir).unwrap();
    fs::create_dir_all(&texture_dir).unwrap();
    let model_path = model_dir.join("knight.glb");
    let base_path = texture_dir.join("knight_base.png");
    let normal_path = texture_dir.join("knight_normal.png");
    fs::write(&model_path, b"real clipboard fixture model bytes").unwrap();
    fs::write(&base_path, b"real clipboard base texture bytes").unwrap();
    fs::write(&normal_path, b"real clipboard normal texture bytes").unwrap();
    let product = AssetProduct {
        product_id: format!("litd.asset.test.{name}.mesh.abcdef01"),
        product_kind: AssetProductKind::Mesh,
        source: "test-fixture".to_string(),
        asset_id: format!("clipboard-{name}"),
        title: "Clipboard Fixture Knight".to_string(),
        license: "CC0-1.0".to_string(),
        license_url: "https://creativecommons.org/publicdomain/zero/1.0/".to_string(),
        source_url: "file://local-fixture".to_string(),
        source_relative_path: "models/knight.glb".to_string(),
        source_absolute_path: model_path.display().to_string(),
        source_sha256: "a".repeat(64),
        source_bytes: fs::metadata(&model_path).unwrap().len(),
        builder_id: "litd.asset.vault_product_builder".to_string(),
        builder_fingerprint: "asset-products-v1".to_string(),
        deterministic_sub_id: "abcdef01".to_string(),
        dependencies: vec![
            "textures/knight_base.png".to_string(),
            "textures/knight_normal.png".to_string(),
        ],
    };
    product.validate().unwrap();
    (root, product)
}

fn test_root(name: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(&format!("history-{name}"))
}
