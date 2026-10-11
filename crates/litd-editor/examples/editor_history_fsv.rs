use litd_engine::fsv_artifacts::env_path;
use std::{env, fs, path::PathBuf};

use litd_editor::{
    asset_products::{
        AssetProduct, AssetProductKind, MODEL_PRODUCT_ID_PROPERTY, MODEL_SOURCE_PATH_PROPERTY,
        MODEL_SOURCE_SHA256_PROPERTY,
    },
    clipboard::{ClipboardPasteOptions, ClipboardPlacement, ClipboardTransform},
    history::{
        EditorDocument, EditorObject, EditorRegion, EditorTerrainStamp, SelectionSet,
        UndoRedoStack, autosave_record, copy_selection, paste_clipboard,
        paste_clipboard_with_options, stable_fingerprint, to_stable_ron,
    },
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_STATS",
        "tmp/issue1222-cross-project-clipboard/stats.txt",
    );
    let source_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_SOURCE",
        "tmp/issue1222-cross-project-clipboard/source_project.ron",
    );
    let clipboard_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_CLIPBOARD",
        "tmp/issue1222-cross-project-clipboard/clipboard_payload.ron",
    );
    let before_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_BEFORE",
        "tmp/issue1222-cross-project-clipboard/destination_before.ron",
    );
    let after_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_AFTER",
        "tmp/issue1222-cross-project-clipboard/destination_after.ron",
    );
    let autosave_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_AUTOSAVE",
        "tmp/issue1222-cross-project-clipboard/destination_autosave.ron",
    );
    let mirror_horizontal_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_MIRROR_HORIZONTAL",
        "tmp/issue1222-cross-project-clipboard/destination_mirror_horizontal.ron",
    );
    let mirror_vertical_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_MIRROR_VERTICAL",
        "tmp/issue1222-cross-project-clipboard/destination_mirror_vertical.ron",
    );
    let rotate_ccw_path = env_path(
        "LITD_EDITOR_HISTORY_FSV_ROTATE_CCW",
        "tmp/issue1222-cross-project-clipboard/destination_rotate_ccw.ron",
    );
    let asset_root = env_path(
        "LITD_EDITOR_HISTORY_FSV_ASSET_ROOT",
        "tmp/issue1222-cross-project-clipboard/assets",
    );

    let product = fixture_asset_product(&asset_root)?;
    let source = source_project(product.clone())?;
    let mut destination = destination_project()?;
    let mut selection = SelectionSet::default();
    selection.select_many(["unit_knight", "vfx_fire", "region_spawn", "terrain_ramp"]);
    let clipboard = copy_selection(&source, &selection)?;
    let before_fingerprint = stable_fingerprint(&destination)?;
    write_text(&source_path, &to_stable_ron(&source)?)?;
    write_text(&clipboard_path, &to_stable_ron(&clipboard)?)?;
    write_text(&before_path, &to_stable_ron(&destination)?)?;

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
    )?;
    let after_fingerprint = stable_fingerprint(&destination)?;
    write_text(&after_path, &to_stable_ron(&destination)?)?;
    let after_readback = read_document(&after_path)?;
    let after_readback_fingerprint = stable_fingerprint(&after_readback)?;

    let autosave = autosave_record(&destination, true)?;
    write_text(&autosave_path, &to_stable_ron(&autosave)?)?;

    let mut history = UndoRedoStack::new(8)?;
    let before_doc = read_document(&before_path)?;
    history.record(
        "cross-project clipboard paste",
        before_doc,
        destination.clone(),
    )?;

    let empty_before = stable_fingerprint(&destination)?;
    let empty_error = copy_selection(&destination, &SelectionSet::default())
        .unwrap_err()
        .code()
        .to_string();
    let empty_after = stable_fingerprint(&destination)?;

    let mut missing_product_clipboard = clipboard.clone();
    missing_product_clipboard.asset_products.clear();
    missing_product_clipboard.fingerprint = stable_fingerprint(&(
        &missing_product_clipboard.source_calyx_document_id,
        &missing_product_clipboard.objects,
        &missing_product_clipboard.regions,
        &missing_product_clipboard.terrain_stamps,
        &missing_product_clipboard.asset_products,
    ))?;
    let mut missing_product_destination = destination_project()?;
    let missing_before = stable_fingerprint(&missing_product_destination)?;
    let missing_product_error = paste_clipboard(
        &mut missing_product_destination,
        &missing_product_clipboard,
        "_copy",
    )
    .unwrap_err()
    .code()
    .to_string();
    let missing_after = stable_fingerprint(&missing_product_destination)?;

    let mut invalid_suffix_destination = destination_project()?;
    let invalid_suffix_before = stable_fingerprint(&invalid_suffix_destination)?;
    let invalid_suffix_error = paste_clipboard(&mut invalid_suffix_destination, &clipboard, "")
        .unwrap_err()
        .code()
        .to_string();
    let invalid_suffix_after = stable_fingerprint(&invalid_suffix_destination)?;

    let mut mirror_horizontal = destination_project()?;
    paste_clipboard_with_options(
        &mut mirror_horizontal,
        &clipboard,
        ClipboardPasteOptions {
            id_suffix: "_mh".to_string(),
            placement: ClipboardPlacement::InPlace,
            transform: ClipboardTransform::MirrorHorizontal,
        },
    )?;
    write_text(&mirror_horizontal_path, &to_stable_ron(&mirror_horizontal)?)?;

    let mut mirror_vertical = destination_project()?;
    paste_clipboard_with_options(
        &mut mirror_vertical,
        &clipboard,
        ClipboardPasteOptions {
            id_suffix: "_mv".to_string(),
            placement: ClipboardPlacement::InPlace,
            transform: ClipboardTransform::MirrorVertical,
        },
    )?;
    write_text(&mirror_vertical_path, &to_stable_ron(&mirror_vertical)?)?;

    let mut rotate_ccw = destination_project()?;
    paste_clipboard_with_options(
        &mut rotate_ccw,
        &clipboard,
        ClipboardPasteOptions {
            id_suffix: "_ccw".to_string(),
            placement: ClipboardPlacement::AtCursor {
                cursor: Vec3::new(-2.0, 0.0, 4.0),
            },
            transform: ClipboardTransform::Rotate90CounterClockwise,
        },
    )?;
    write_text(&rotate_ccw_path, &to_stable_ron(&rotate_ccw)?)?;

    write_text(
        &stats_path,
        &format!(
            "issue=1222\n\
             source_project={}\n\
             clipboard_file={}\n\
             destination_before={}\n\
             destination_after={}\n\
             autosave_file={}\n\
             source_document_id={}\n\
             destination_document_id={}\n\
             selection_ids={}\n\
             clipboard_object_count={}\n\
             clipboard_region_count={}\n\
             clipboard_terrain_stamp_count={}\n\
             clipboard_asset_product_count={}\n\
             clipboard_fingerprint={}\n\
             before_fingerprint={}\n\
             report_before_fingerprint={}\n\
             after_fingerprint={}\n\
             after_readback_fingerprint={}\n\
             after_readback_matches={}\n\
             imported_asset_products={}\n\
             dependency_count={}\n\
             dependency_files_exist={}\n\
             pasted_object_ids={}\n\
             pasted_region_ids={}\n\
             pasted_terrain_stamp_ids={}\n\
             duplicate_unit_reconciled_to={}\n\
             paste_transform=rotate90_clockwise\n\
             paste_cursor=20.0,0.0,30.0\n\
             object_count_before=2\n\
             object_count_after={}\n\
             region_count_after={}\n\
             terrain_stamp_count_after={}\n\
             undo_depth={}\n\
             autosave_fingerprint={}\n\
             edge_empty_before={}\n\
             edge_empty_after={}\n\
             edge_empty_unchanged={}\n\
             edge_empty_error={}\n\
             edge_missing_product_before={}\n\
             edge_missing_product_after={}\n\
             edge_missing_product_unchanged={}\n\
             edge_missing_product_error={}\n\
             edge_invalid_suffix_before={}\n\
             edge_invalid_suffix_after={}\n\
             edge_invalid_suffix_unchanged={}\n\
             edge_invalid_suffix_error={}\n\
             edge_mirror_horizontal_file={}\n\
             edge_mirror_horizontal_scale={:?}\n\
             edge_mirror_vertical_file={}\n\
             edge_mirror_vertical_scale={:?}\n\
             edge_rotate_ccw_file={}\n\
             edge_rotate_ccw_exists={}\n\
             window_opened=false\n\
             renderer_touched=false\n",
            source_path.display(),
            clipboard_path.display(),
            before_path.display(),
            after_path.display(),
            autosave_path.display(),
            source.calyx_document_id,
            destination.calyx_document_id,
            selection.summary(),
            clipboard.objects.len(),
            clipboard.regions.len(),
            clipboard.terrain_stamps.len(),
            clipboard.asset_products.len(),
            clipboard.fingerprint,
            before_fingerprint,
            report.before_fingerprint,
            after_fingerprint,
            after_readback_fingerprint,
            after_fingerprint == after_readback_fingerprint,
            report.imported_asset_product_ids.join("|"),
            report.dependency_count,
            product_dependencies_exist(&product),
            report.pasted_object_ids.join("|"),
            report.pasted_region_ids.join("|"),
            report.pasted_terrain_stamp_ids.join("|"),
            report.id_map["unit_knight"],
            destination.objects.len(),
            destination.regions.len(),
            destination.terrain_stamps.len(),
            history.undo.len(),
            autosave.stable_fingerprint,
            empty_before,
            empty_after,
            empty_before == empty_after,
            empty_error,
            missing_before,
            missing_after,
            missing_before == missing_after,
            missing_product_error,
            invalid_suffix_before,
            invalid_suffix_after,
            invalid_suffix_before == invalid_suffix_after,
            invalid_suffix_error,
            mirror_horizontal_path.display(),
            mirror_horizontal.objects["unit_knight_mh"].transform.scale,
            mirror_vertical_path.display(),
            mirror_vertical.objects["unit_knight_mv"].transform.scale,
            rotate_ccw_path.display(),
            rotate_ccw.objects.contains_key("unit_knight_ccw"),
        ),
    )?;
    println!(
        "wrote editor clipboard FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

fn source_project(product: AssetProduct) -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new("calyx://world/source-cross-project", "calyx-rev-a")?;
    document
        .asset_products
        .insert(product.product_id.clone(), product.clone());
    document.upsert_object(
        EditorObject::new("world", "World", None, Transform::IDENTITY)
            .with_property("name", "Source Citadel"),
    )?;
    document.upsert_object(
        EditorObject::new(
            "unit_knight",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
        )
        .with_property("faction", "Dawnward")
        .with_property(MODEL_PRODUCT_ID_PROPERTY, product.product_id)
        .with_property(MODEL_SOURCE_SHA256_PROPERTY, product.source_sha256)
        .with_property(MODEL_SOURCE_PATH_PROPERTY, product.source_relative_path),
    )?;
    document.upsert_object(EditorObject::new(
        "vfx_fire",
        "Vfx",
        Some("unit_knight".to_string()),
        Transform::from_translation(Vec3::new(3.0, 2.0, -6.0)),
    ))?;
    document.upsert_region(EditorRegion::new(
        "region_spawn",
        "Spawn Region",
        Vec3::new(12.0, 0.0, -2.0),
        Vec3::new(2.0, 1.0, 3.0),
    ))?;
    document.upsert_terrain_stamp(EditorTerrainStamp::new(
        "terrain_ramp",
        "Ramp",
        Transform::from_translation(Vec3::new(6.0, 0.0, -8.0)),
        4,
        3,
        1.5,
    ))?;
    document.validate()?;
    Ok(document)
}

fn destination_project() -> Result<EditorDocument, EngineError> {
    let mut document =
        EditorDocument::new("calyx://world/destination-cross-project", "calyx-rev-b")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(EditorObject::new(
        "unit_knight_copy",
        "Unit",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(-4.0, 0.0, -4.0)),
    ))?;
    document.validate()?;
    Ok(document)
}

fn fixture_asset_product(root: &PathBuf) -> Result<AssetProduct, EngineError> {
    let root = if root.is_absolute() {
        root.clone()
    } else {
        env::current_dir().map_err(io_error)?.join(root)
    };
    let model_dir = root.join("models");
    let texture_dir = root.join("textures");
    fs::create_dir_all(&model_dir).map_err(io_error)?;
    fs::create_dir_all(&texture_dir).map_err(io_error)?;
    let model_path = model_dir.join("knight.glb");
    let base_path = texture_dir.join("knight_base.png");
    let normal_path = texture_dir.join("knight_normal.png");
    fs::write(&model_path, b"real clipboard fixture model bytes").map_err(io_error)?;
    fs::write(&base_path, b"real clipboard base texture bytes").map_err(io_error)?;
    fs::write(&normal_path, b"real clipboard normal texture bytes").map_err(io_error)?;
    let product = AssetProduct {
        product_id: "litd.asset.test.clipboard.mesh.abcdef01".to_string(),
        product_kind: AssetProductKind::Mesh,
        source: "test-fixture".to_string(),
        asset_id: "clipboard-knight".to_string(),
        title: "Clipboard Fixture Knight".to_string(),
        license: "CC0-1.0".to_string(),
        license_url: "https://creativecommons.org/publicdomain/zero/1.0/".to_string(),
        source_url: "file://local-fixture".to_string(),
        source_relative_path: "models/knight.glb".to_string(),
        source_absolute_path: model_path.display().to_string(),
        source_sha256: "b".repeat(64),
        source_bytes: fs::metadata(&model_path).map_err(io_error)?.len(),
        builder_id: "litd.asset.vault_product_builder".to_string(),
        builder_fingerprint: "asset-products-v1".to_string(),
        deterministic_sub_id: "abcdef01".to_string(),
        dependencies: vec![
            "textures/knight_base.png".to_string(),
            "textures/knight_normal.png".to_string(),
        ],
    };
    product.validate()?;
    Ok(product)
}

fn product_dependencies_exist(product: &AssetProduct) -> bool {
    let root = PathBuf::from(&product.source_absolute_path)
        .parent()
        .and_then(|path| path.parent())
        .map(PathBuf::from);
    let Some(root) = root else {
        return false;
    };
    product
        .dependencies
        .iter()
        .all(|dependency| root.join(dependency).is_file())
}

fn read_document(path: &PathBuf) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let document: EditorDocument = ron::from_str(&text)?;
    document.validate()?;
    Ok(document)
}

fn write_text(path: &PathBuf, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn io_error(error: std::io::Error) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: "editor.clipboard.fsv.io".to_string(),
        reason: error.to_string(),
    }
}
