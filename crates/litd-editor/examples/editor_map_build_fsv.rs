use litd_engine::fsv_artifacts::env_path;
use std::{fs, io, path::Path};

use litd_editor::{
    asset_products::{
        MODEL_PRODUCT_ID_PROPERTY, MODEL_SOURCE_PATH_PROPERTY, MODEL_SOURCE_SHA256_PROPERTY,
        product_from_vault_record,
    },
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, EditorRegion, EditorTerrainStamp, stable_fingerprint},
    map_build::{
        StaticShadowBakeSettings, calculate_shadows_and_save_map, export_script,
        load_saved_map_build, load_static_shadow_bake, parse_generated_map_script,
        validate_static_shadow_bake_for_document,
    },
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use litd_hash::sha256_hex;

const REAL_STATIC_MODEL: &str =
    "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = env_path("LITD_EDITOR_MAP_BUILD_FSV_ROOT", "tmp/issue1273-map-build");
    let script_path = root.join("map.lua");
    let shadow_path = root.join("static_shadows.ron");
    let map_path = root.join("saved_map.ron");
    let stats_path = root.join("stats.txt");

    let document = known_project()?;
    let source_before = source_truth_state(&document, &script_path, &shadow_path, &map_path)?;
    let script_export = export_script(&document, &script_path)?;
    let save_receipt = calculate_shadows_and_save_map(
        &document,
        &shadow_path,
        &map_path,
        StaticShadowBakeSettings::default(),
    )?;
    let script_readback = fs::read_to_string(&script_path)?;
    let script_summary = parse_generated_map_script(&script_readback)?;
    let shadow_readback = load_static_shadow_bake(&shadow_path)?;
    let map_readback = load_saved_map_build(&map_path)?;
    validate_static_shadow_bake_for_document(&shadow_readback, &document)?;
    let source_after_happy = source_truth_state(&document, &script_path, &shadow_path, &map_path)?;

    let empty_edge = edge_empty_project(&root)?;
    let stale_edge = edge_stale_shadow(&document, &shadow_readback, &root)?;
    let invalid_extension_edge = edge_invalid_script_extension(&document, &root)?;
    let source_final = source_truth_state(&document, &script_path, &shadow_path, &map_path)?;

    write_text(
        &stats_path,
        &format!(
            "issue=1273\n\
             source_of_truth=exported Lua file, static shadow RON blob, saved map RON, and reopened editor document fingerprints\n\
             source_before={}\n\
             source_after_happy={}\n\
             source_final={}\n\
             script_file={}\n\
             script_sha256={}\n\
             script_readback_sha256={}\n\
             script_readback_matches_export={}\n\
             script_document_fingerprint={}\n\
             script_object_count={}\n\
             script_region_count={}\n\
             script_terrain_stamp_count={}\n\
             script_contains_return_map={}\n\
             script_contains_real_model_path={}\n\
             shadow_file={}\n\
             shadow_artifact_sha256={}\n\
             shadow_cells_sha256={}\n\
             shadow_readback_cells_sha256={}\n\
             shadow_readback_matches_receipt={}\n\
             shadow_caster_count={}\n\
             shadow_nonzero_cell_count={}\n\
             saved_map_file={}\n\
             saved_map_sha256={}\n\
             saved_map_readback_matches_receipt={}\n\
             saved_map_document_fingerprint={}\n\
             saved_map_shadow_matches_shadow_file={}\n\
             edge_empty_before={}\n\
             edge_empty_action={}\n\
             edge_empty_after={}\n\
             edge_empty_object_count={}\n\
             edge_empty_caster_count={}\n\
             edge_empty_nonzero_cell_count={}\n\
             edge_stale_before={}\n\
             edge_stale_action={}\n\
             edge_stale_after={}\n\
             edge_stale_error={}\n\
             edge_stale_original_fingerprint={}\n\
             edge_stale_mutated_fingerprint={}\n\
             edge_invalid_extension_before={}\n\
             edge_invalid_extension_action={}\n\
             edge_invalid_extension_after={}\n\
             edge_invalid_extension_error={}\n",
            source_before,
            source_after_happy,
            source_final,
            script_path.display(),
            script_export.script_sha256,
            sha256_hex(script_readback.as_bytes()),
            script_export.script_sha256 == sha256_hex(script_readback.as_bytes()),
            script_summary.document_fingerprint,
            script_summary.object_count,
            script_summary.region_count,
            script_summary.terrain_stamp_count,
            script_readback.contains("return map"),
            script_readback.contains(REAL_STATIC_MODEL),
            shadow_path.display(),
            save_receipt.shadow_receipt.artifact_sha256,
            save_receipt.shadow_receipt.bake.cells_sha256,
            shadow_readback.cells_sha256,
            shadow_readback == save_receipt.shadow_receipt.bake,
            shadow_readback.casters.len(),
            shadow_readback.nonzero_cell_count(),
            map_path.display(),
            save_receipt.artifact_sha256,
            map_readback == save_receipt.saved_map,
            map_readback.document_fingerprint,
            map_readback.baked_static_shadows == shadow_readback,
            empty_edge.before_state,
            empty_edge.action,
            empty_edge.after_state,
            empty_edge.object_count,
            empty_edge.caster_count,
            empty_edge.nonzero_cell_count,
            stale_edge.before_state,
            stale_edge.action,
            stale_edge.after_state,
            stale_edge.error_code,
            stale_edge.original_fingerprint,
            stale_edge.mutated_fingerprint,
            invalid_extension_edge.before_state,
            invalid_extension_edge.action,
            invalid_extension_edge.after_state,
            invalid_extension_edge.error_code,
        ),
    )?;
    println!("wrote map build FSV stats to {}", stats_path.display());
    Ok(())
}

#[derive(Debug)]
struct EmptyEdge {
    before_state: String,
    action: String,
    after_state: String,
    object_count: usize,
    caster_count: usize,
    nonzero_cell_count: usize,
}

#[derive(Debug)]
struct StaleEdge {
    before_state: String,
    action: String,
    after_state: String,
    error_code: String,
    original_fingerprint: String,
    mutated_fingerprint: String,
}

#[derive(Debug)]
struct ErrorEdge {
    before_state: String,
    action: String,
    after_state: String,
    error_code: String,
}

fn edge_empty_project(root: &Path) -> Result<EmptyEdge, Box<dyn std::error::Error>> {
    let document = EditorDocument::new("calyx://world/empty-map-build-fsv", "rev-empty")?;
    let script_path = root.join("edge_empty.lua");
    let shadow_path = root.join("edge_empty_shadows.ron");
    let map_path = root.join("edge_empty_map.ron");
    let before_state = source_truth_state(&document, &script_path, &shadow_path, &map_path)?;
    let export = export_script(&document, &script_path)?;
    calculate_shadows_and_save_map(
        &document,
        &shadow_path,
        &map_path,
        StaticShadowBakeSettings::default(),
    )?;
    let shadow_readback = load_static_shadow_bake(&shadow_path)?;
    let after_state = source_truth_state(&document, &script_path, &shadow_path, &map_path)?;
    Ok(EmptyEdge {
        before_state,
        action: format!("export_and_bake_empty_project={}", script_path.display()),
        after_state,
        object_count: export.summary.object_count,
        caster_count: shadow_readback.casters.len(),
        nonzero_cell_count: shadow_readback.nonzero_cell_count(),
    })
}

fn edge_stale_shadow(
    document: &EditorDocument,
    shadow: &litd_editor::map_build::StaticShadowBake,
    root: &Path,
) -> Result<StaleEdge, Box<dyn std::error::Error>> {
    let stale_path = root.join("static_shadows.ron");
    let before_state = artifact_state(&stale_path)?;
    let original_fingerprint = stable_fingerprint(document)?;
    let mut mutated = document.clone();
    mutated
        .terrain_stamps
        .get_mut("terrain_ramp")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing terrain_ramp"))?
        .height_delta = 4.0;
    let mutated_fingerprint = stable_fingerprint(&mutated)?;
    let error_code = validation_error_code(
        validate_static_shadow_bake_for_document(shadow, &mutated),
        "stale shadow unexpectedly validated",
    )?;
    let after_state = artifact_state(&stale_path)?;
    Ok(StaleEdge {
        before_state,
        action: "validate_existing_shadow_after_terrain_height_delta_edit".to_string(),
        after_state,
        error_code,
        original_fingerprint,
        mutated_fingerprint,
    })
}

fn edge_invalid_script_extension(
    document: &EditorDocument,
    root: &Path,
) -> Result<ErrorEdge, Box<dyn std::error::Error>> {
    let bad_path = root.join("edge_bad_script.txt");
    let before_state = artifact_state(&bad_path)?;
    let error_code = validation_error_code(
        export_script(document, &bad_path).map(|_| ()),
        "invalid script extension unexpectedly exported",
    )?;
    let after_state = artifact_state(&bad_path)?;
    Ok(ErrorEdge {
        before_state,
        action: format!("export_script_invalid_extension={}", bad_path.display()),
        after_state,
        error_code,
    })
}

fn known_project() -> Result<EditorDocument, EngineError> {
    let catalog = AssetVaultCatalog::load_default()?;
    let record = catalog
        .records()
        .iter()
        .find(|record| record.relative_path == REAL_STATIC_MODEL)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.map_build.fsv.asset".to_string(),
            reason: format!("missing real asset-vault record {REAL_STATIC_MODEL}"),
        })?;
    let product = product_from_vault_record(record)?;
    let mut document = EditorDocument::new("calyx://world/map-build-fsv", "rev-known")?;
    document
        .asset_products
        .insert(product.product_id.clone(), product.clone());
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "doodad_rock",
            "Doodad",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(12.0, 0.0, -18.0)),
        )
        .with_property("static_shadow", "true")
        .with_property("shadow_footprint_tiles", "3x2")
        .with_property("shadow_height_tiles", "4")
        .with_property(MODEL_PRODUCT_ID_PROPERTY, product.product_id)
        .with_property(MODEL_SOURCE_SHA256_PROPERTY, product.source_sha256)
        .with_property(MODEL_SOURCE_PATH_PROPERTY, product.source_relative_path),
    )?;
    document.upsert_region(EditorRegion::new(
        "region_spawn",
        "Spawn Plateau",
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(8.0, 2.0, 8.0),
    ))?;
    document.upsert_terrain_stamp(EditorTerrainStamp::new(
        "terrain_ramp",
        "Ramp",
        Transform::from_translation(Vec3::new(-8.0, 0.0, 6.0)),
        5,
        3,
        2.25,
    ))?;
    document.validate()?;
    Ok(document)
}

fn source_truth_state(
    document: &EditorDocument,
    script_path: &Path,
    shadow_path: &Path,
    map_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        "document={};script={};shadow={};map={}",
        stable_fingerprint(document)?,
        artifact_state(script_path)?,
        artifact_state(shadow_path)?,
        artifact_state(map_path)?
    ))
}

fn artifact_state(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok("missing".to_string());
    }
    let bytes = fs::read(path)?;
    Ok(format!("exists:{}:{}", bytes.len(), sha256_hex(&bytes)))
}

fn validation_error_code<T>(
    result: Result<T, EngineError>,
    success_message: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    match result {
        Ok(_) => Err(io::Error::new(io::ErrorKind::InvalidData, success_message).into()),
        Err(error) => Ok(error.code().to_string()),
    }
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
