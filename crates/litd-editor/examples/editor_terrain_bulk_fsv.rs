use litd_engine::fsv_artifacts::env_path;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::Path,
};

use litd_editor::{
    history::{EditorDocument, EditorObject, UndoRedoStack, stable_fingerprint, to_stable_ron},
    map_build::{
        StaticShadowBakeSettings, calculate_shadows_and_save_map, export_script,
        load_saved_map_build, parse_generated_map_script,
    },
    terrain_bulk_ops::{
        EditorTerrainCell, OBJECT_TYPE_ID_PROPERTY, TerrainBulkCatalog, TerrainBulkScope,
        adjust_cliff_levels, replace_cliff_type, replace_doodads, replace_tiles, replace_units,
        reset_height_field, terrain_bulk_summary,
    },
    terrain_variation::TerrainTileVariation,
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use litd_hash::sha256_hex;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = env_path(
        "LITD_EDITOR_TERRAIN_BULK_FSV_ROOT",
        "tmp/issue1269-terrain-bulk",
    );
    let before_path = root.join("document_before_ops.ron");
    let after_path = root.join("document_after_ops.ron");
    let script_path = root.join("map.lua");
    let shadow_path = root.join("static_shadows.ron");
    let map_path = root.join("saved_map.ron");
    let stats_path = root.join("stats.txt");

    let catalog = fixture_catalog();
    let mut document = fixture_document()?;
    let source_before = source_truth_state(
        &document,
        &before_path,
        &after_path,
        &script_path,
        &map_path,
    )?;
    write_text(&before_path, &to_stable_ron(&document)?)?;

    let replace_tiles_report = replace_tiles(
        &mut document,
        &catalog,
        "tileset.grass",
        "tileset.snow",
        &TerrainBulkScope::WholeMap,
    )?;
    let replace_cliffs_report = replace_cliff_type(
        &mut document,
        &catalog,
        "cliff.grass",
        "cliff.snow",
        &TerrainBulkScope::WholeMap,
    )?;
    let replace_doodads_report = replace_doodads(
        &mut document,
        &catalog,
        "doodad.tree",
        "doodad.rock",
        &TerrainBulkScope::WholeMap,
    )?;
    let replace_units_report = replace_units(
        &mut document,
        &catalog,
        "unit.footman",
        "unit.grunt",
        &TerrainBulkScope::WholeMap,
    )?;
    let adjust_report = adjust_cliff_levels(&mut document, 2, &TerrainBulkScope::WholeMap)?;
    let reset_report = reset_height_field(&mut document, 0.0, &TerrainBulkScope::WholeMap)?;
    write_text(&after_path, &to_stable_ron(&document)?)?;

    let script_export = export_script(&document, &script_path)?;
    let save_receipt = calculate_shadows_and_save_map(
        &document,
        &shadow_path,
        &map_path,
        StaticShadowBakeSettings::default(),
    )?;
    let script_readback = fs::read_to_string(&script_path)?;
    let script_summary = parse_generated_map_script(&script_readback)?;
    let map_readback = load_saved_map_build(&map_path)?;
    let after_readback = read_document(&after_path)?;
    if after_readback != document || map_readback.document != document {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bulk document readback mismatch",
        )
        .into());
    }
    let source_after_happy = source_truth_state(
        &document,
        &before_path,
        &after_path,
        &script_path,
        &map_path,
    )?;

    let selection_edge = edge_selection_scope(&catalog)?;
    let missing_target_edge = edge_missing_target(&catalog)?;
    let undo_edge = edge_undo_restores(&catalog)?;
    let source_final = source_truth_state(
        &document,
        &before_path,
        &after_path,
        &script_path,
        &map_path,
    )?;

    write_text(
        &stats_path,
        &format!(
            "issue=1269\n\
             source_of_truth=EditorDocument RON, generated Lua map script, saved map RON, and reopened readback structs\n\
             source_before={}\n\
             source_after_happy={}\n\
             source_final={}\n\
             before_document_file={}\n\
             before_document_sha256={}\n\
             after_document_file={}\n\
             after_document_sha256={}\n\
             script_file={}\n\
             script_sha256={}\n\
             script_readback_sha256={}\n\
             script_terrain_cell_count={}\n\
             saved_map_file={}\n\
             saved_map_sha256={}\n\
             saved_map_readback_matches_receipt={}\n\
             saved_map_terrain_cell_count={}\n\
             saved_map_tile_cell_a={}\n\
             saved_map_cliff_cell_a={}/{}\n\
             saved_map_height_cell_a={:.3}\n\
             saved_map_doodad_a_type={}\n\
             saved_map_unit_a_type={}\n\
             saved_map_unit_a_owner={}\n\
             saved_map_unit_a_facing={}\n\
             replace_tiles_before={}\n\
             replace_tiles_after={}\n\
             replace_tiles_changed={}\n\
             replace_cliffs_changed={}\n\
             replace_doodads_changed={}\n\
             replace_units_changed={}\n\
             adjust_cliffs_changed={}\n\
             reset_height_changed={}\n\
             edge_selection_before={}\n\
             edge_selection_after={}\n\
             edge_selection_cell_a={}\n\
             edge_selection_cell_b={}\n\
             edge_selection_unit_a={}\n\
             edge_selection_unit_b={}\n\
             edge_missing_target_before={}\n\
             edge_missing_target_after={}\n\
             edge_missing_target_unchanged={}\n\
             edge_missing_target_error={}\n\
             edge_undo_before={}\n\
             edge_undo_after_replace={}\n\
             edge_undo_after_undo={}\n\
             edge_undo_restored={}\n",
            source_before,
            source_after_happy,
            source_final,
            before_path.display(),
            sha256_file(&before_path)?,
            after_path.display(),
            sha256_file(&after_path)?,
            script_path.display(),
            script_export.script_sha256,
            sha256_hex(script_readback.as_bytes()),
            script_summary.terrain_cell_count,
            map_path.display(),
            save_receipt.artifact_sha256,
            map_readback == save_receipt.saved_map,
            map_readback.document.terrain_cells.len(),
            map_readback.document.terrain_cells["cell_a"].tile_id,
            map_readback.document.terrain_cells["cell_a"].cliff_type,
            map_readback.document.terrain_cells["cell_a"].cliff_level,
            map_readback.document.terrain_cells["cell_a"].height,
            map_readback.document.objects["doodad_a"].properties[OBJECT_TYPE_ID_PROPERTY],
            map_readback.document.objects["unit_a"].properties[OBJECT_TYPE_ID_PROPERTY],
            map_readback.document.objects["unit_a"].properties["owner"],
            map_readback.document.objects["unit_a"].properties["facing"],
            replace_tiles_report.before_fingerprint,
            replace_tiles_report.after_fingerprint,
            replace_tiles_report.changed_count,
            replace_cliffs_report.changed_count,
            replace_doodads_report.changed_count,
            replace_units_report.changed_count,
            adjust_report.changed_count,
            reset_report.changed_count,
            selection_edge.before,
            selection_edge.after,
            selection_edge.cell_a,
            selection_edge.cell_b,
            selection_edge.unit_a,
            selection_edge.unit_b,
            missing_target_edge.before,
            missing_target_edge.after,
            missing_target_edge.unchanged,
            missing_target_edge.error,
            undo_edge.before,
            undo_edge.after_replace,
            undo_edge.after_undo,
            undo_edge.restored,
        ),
    )?;
    println!("wrote terrain bulk FSV stats to {}", stats_path.display());
    Ok(())
}

#[derive(Debug)]
struct SelectionEdge {
    before: String,
    after: String,
    cell_a: String,
    cell_b: String,
    unit_a: String,
    unit_b: String,
}

#[derive(Debug)]
struct MissingTargetEdge {
    before: String,
    after: String,
    unchanged: bool,
    error: String,
}

#[derive(Debug)]
struct UndoEdge {
    before: String,
    after_replace: String,
    after_undo: String,
    restored: bool,
}

fn edge_selection_scope(catalog: &TerrainBulkCatalog) -> Result<SelectionEdge, EngineError> {
    let mut document = fixture_document()?;
    let before = terrain_bulk_summary(&document);
    let scope = TerrainBulkScope::Selection {
        terrain_cell_ids: BTreeSet::from(["cell_a".to_string()]),
        object_ids: BTreeSet::from(["unit_a".to_string()]),
    };
    replace_tiles(
        &mut document,
        catalog,
        "tileset.grass",
        "tileset.snow",
        &scope,
    )?;
    replace_units(&mut document, catalog, "unit.footman", "unit.grunt", &scope)?;
    Ok(SelectionEdge {
        before,
        after: terrain_bulk_summary(&document),
        cell_a: document.terrain_cells["cell_a"].tile_id.clone(),
        cell_b: document.terrain_cells["cell_b"].tile_id.clone(),
        unit_a: document.objects["unit_a"].properties[OBJECT_TYPE_ID_PROPERTY].clone(),
        unit_b: document.objects["unit_b"].properties[OBJECT_TYPE_ID_PROPERTY].clone(),
    })
}

fn edge_missing_target(catalog: &TerrainBulkCatalog) -> Result<MissingTargetEdge, EngineError> {
    let mut document = fixture_document()?;
    let before = stable_fingerprint(&document)?;
    let error = replace_tiles(
        &mut document,
        catalog,
        "tileset.grass",
        "tileset.missing",
        &TerrainBulkScope::WholeMap,
    )
    .unwrap_err();
    let after = stable_fingerprint(&document)?;
    Ok(MissingTargetEdge {
        before: before.clone(),
        after: after.clone(),
        unchanged: before == after,
        error: error.code().to_string(),
    })
}

fn edge_undo_restores(catalog: &TerrainBulkCatalog) -> Result<UndoEdge, EngineError> {
    let before_doc = fixture_document()?;
    let before = stable_fingerprint(&before_doc)?;
    let mut after_doc = before_doc.clone();
    replace_doodads(
        &mut after_doc,
        catalog,
        "doodad.tree",
        "doodad.rock",
        &TerrainBulkScope::WholeMap,
    )?;
    let after_replace = stable_fingerprint(&after_doc)?;
    let mut history = UndoRedoStack::new(4)?;
    history.record("replace doodads", before_doc.clone(), after_doc.clone())?;
    let mut document = after_doc;
    history.undo(&mut document)?;
    let after_undo = stable_fingerprint(&document)?;
    Ok(UndoEdge {
        before: before.clone(),
        after_replace,
        after_undo: after_undo.clone(),
        restored: before == after_undo,
    })
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new("calyx://world/terrain-bulk-fsv", "rev-1269")?;
    for cell in [
        EditorTerrainCell::new("cell_a", 0, 0, "tileset.grass", "cliff.grass", 1, 8.0),
        EditorTerrainCell::new("cell_b", 1, 0, "tileset.grass", "cliff.grass", 0, 7.0),
        EditorTerrainCell::new("cell_c", 0, 1, "tileset.dirt", "cliff.dirt", -1, 6.5),
    ] {
        document.upsert_terrain_cell(cell)?;
    }
    for variation in [
        TerrainTileVariation::new("var_cell_a", 0, 0, "tileset.grass", 4, 1, 12_690),
        TerrainTileVariation::new("var_cell_b", 1, 0, "tileset.grass", 4, 2, 12_690),
        TerrainTileVariation::new("var_cell_c", 0, 1, "tileset.dirt", 3, 0, 12_690),
    ] {
        document.upsert_terrain_tile_variation(variation)?;
    }
    for object in fixture_objects() {
        document.upsert_object(object)?;
    }
    document.validate()?;
    Ok(document)
}

fn fixture_objects() -> Vec<EditorObject> {
    vec![
        typed_object(
            "doodad_a",
            "Doodad",
            "doodad.tree",
            Vec3::new(1.0, 0.0, 1.0),
        ),
        typed_object(
            "doodad_b",
            "Doodad",
            "doodad.tree",
            Vec3::new(3.0, 0.0, 1.0),
        ),
        typed_object("unit_a", "Unit", "unit.footman", Vec3::new(4.0, 0.0, 2.0))
            .with_property("owner", "player_1")
            .with_property("facing", "90"),
        typed_object("unit_b", "Unit", "unit.footman", Vec3::new(5.0, 0.0, 2.0))
            .with_property("owner", "player_2")
            .with_property("facing", "180"),
    ]
}

fn typed_object(id: &str, kind: &str, type_id: &str, translation: Vec3) -> EditorObject {
    EditorObject::new(id, kind, None, Transform::from_translation(translation))
        .with_property(OBJECT_TYPE_ID_PROPERTY, type_id)
}

fn fixture_catalog() -> TerrainBulkCatalog {
    TerrainBulkCatalog::new(
        BTreeMap::from([
            ("tileset.grass".to_string(), 4),
            ("tileset.dirt".to_string(), 3),
            ("tileset.snow".to_string(), 2),
        ]),
        BTreeSet::from([
            "cliff.grass".to_string(),
            "cliff.dirt".to_string(),
            "cliff.snow".to_string(),
        ]),
        BTreeSet::from(["doodad.tree".to_string(), "doodad.rock".to_string()]),
        BTreeSet::from(["unit.footman".to_string(), "unit.grunt".to_string()]),
    )
}

fn source_truth_state(
    document: &EditorDocument,
    before_path: &Path,
    after_path: &Path,
    script_path: &Path,
    map_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        "document={};before_doc={};after_doc={};script={};map={}",
        stable_fingerprint(document)?,
        artifact_state(before_path)?,
        artifact_state(after_path)?,
        artifact_state(script_path)?,
        artifact_state(map_path)?,
    ))
}

fn read_document(path: &Path) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let document: EditorDocument = ron::from_str(&text)?;
    document.validate()?;
    Ok(document)
}

fn artifact_state(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok("missing".to_string());
    }
    let bytes = fs::read(path)?;
    Ok(format!("exists:{}:{}", bytes.len(), sha256_hex(&bytes)))
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(sha256_hex(&fs::read(path)?))
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
