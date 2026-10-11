use litd_engine::fsv_artifacts::env_path;
use std::{fs, io, path::Path};

use litd_editor::{
    history::{EditorDocument, EditorTerrainStamp, stable_fingerprint, to_stable_ron},
    map_build::{
        StaticShadowBakeSettings, calculate_shadows_and_save_map, export_script,
        load_saved_map_build, parse_generated_map_script,
    },
    terrain_variation::{
        TerrainRandomGroup, TerrainRandomGroupMember, TerrainRandomGroupMemberKind,
        TerrainRandomGroupPreview, TerrainTileVariation, auto_cycle_random_group_preview,
        reset_texture_variations, tile_variation_summary, validate_tile_variations,
    },
};
use litd_engine::{EngineError, math::Transform};
use litd_hash::sha256_hex;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = env_path(
        "LITD_EDITOR_TERRAIN_VARIATION_FSV_ROOT",
        "tmp/issue1270-terrain-variation",
    );
    let before_document_path = root.join("document_before_reset.ron");
    let after_document_path = root.join("document_after_reset.ron");
    let script_path = root.join("map.lua");
    let shadow_path = root.join("static_shadows.ron");
    let map_path = root.join("saved_map.ron");
    let stats_path = root.join("stats.txt");

    let mut document = known_terrain_document()?;
    let source_before = source_truth_state(
        &document,
        &before_document_path,
        &after_document_path,
        &script_path,
        &map_path,
    )?;
    write_text(&before_document_path, &to_stable_ron(&document)?)?;
    let reset_before_summary = tile_variation_summary(&document.terrain_tile_variations);
    let reset_report = reset_texture_variations(&mut document.terrain_tile_variations, 77_001)?;
    let reset_after_summary = tile_variation_summary(&document.terrain_tile_variations);
    write_text(&after_document_path, &to_stable_ron(&document)?)?;

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
    let after_document_readback = read_document(&after_document_path)?;
    if after_document_readback != document {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "after-reset document readback differs from generated document",
        )
        .into());
    }
    if map_readback.document != document {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "saved map document readback differs from generated document",
        )
        .into());
    }
    let source_after_happy = source_truth_state(
        &document,
        &before_document_path,
        &after_document_path,
        &script_path,
        &map_path,
    )?;

    let deterministic_edge = edge_deterministic_reset(&document)?;
    let fixed_group_edge = edge_one_member_group_fixed()?;
    let auto_cycle_edge = edge_auto_cycle_does_not_mutate_saved_data(&document)?;
    let invalid_edge = edge_invalid_duplicate_tile_fails_without_mutating(&document)?;
    let source_final = source_truth_state(
        &document,
        &before_document_path,
        &after_document_path,
        &script_path,
        &map_path,
    )?;

    let tile_spawn = map_readback
        .document
        .terrain_tile_variations
        .get("tile_spawn_grass")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing tile_spawn_grass"))?;
    let group = map_readback
        .document
        .random_groups
        .get("forest_floor_mix")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing forest_floor_mix"))?;

    write_text(
        &stats_path,
        &format!(
            "issue=1270\n\
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
             script_readback_matches_export={}\n\
             script_terrain_variation_count={}\n\
             script_random_group_count={}\n\
             script_contains_tile_spawn_grass={}\n\
             script_contains_forest_floor_mix={}\n\
             saved_map_file={}\n\
             saved_map_sha256={}\n\
             saved_map_readback_matches_receipt={}\n\
             saved_map_document_fingerprint={}\n\
             saved_map_terrain_variation_count={}\n\
             saved_map_random_group_count={}\n\
             saved_map_tile_spawn_grass_index={}\n\
             saved_map_tile_spawn_grass_seed={}\n\
             saved_map_group_member_count={}\n\
             reset_before_summary={}\n\
             reset_report_before_summary={}\n\
             reset_after_summary={}\n\
             reset_report_after_summary={}\n\
             edge_deterministic_before={}\n\
             edge_deterministic_action={}\n\
             edge_deterministic_after={}\n\
             edge_deterministic_matches_happy={}\n\
             edge_fixed_group_before={}\n\
             edge_fixed_group_action={}\n\
             edge_fixed_group_after={}\n\
             edge_fixed_group_member={}\n\
             edge_auto_cycle_before={}\n\
             edge_auto_cycle_action={}\n\
             edge_auto_cycle_after={}\n\
             edge_auto_cycle_document_unchanged={}\n\
             edge_invalid_duplicate_before={}\n\
             edge_invalid_duplicate_action={}\n\
             edge_invalid_duplicate_after={}\n\
             edge_invalid_duplicate_error={}\n",
            source_before,
            source_after_happy,
            source_final,
            before_document_path.display(),
            sha256_file(&before_document_path)?,
            after_document_path.display(),
            sha256_file(&after_document_path)?,
            script_path.display(),
            script_export.script_sha256,
            sha256_hex(script_readback.as_bytes()),
            script_export.script_sha256 == sha256_hex(script_readback.as_bytes()),
            script_summary.terrain_variation_count,
            script_summary.random_group_count,
            script_readback.contains("tile_spawn_grass"),
            script_readback.contains("forest_floor_mix"),
            map_path.display(),
            save_receipt.artifact_sha256,
            map_readback == save_receipt.saved_map,
            map_readback.document_fingerprint,
            map_readback.document.terrain_tile_variations.len(),
            map_readback.document.random_groups.len(),
            tile_spawn.variation_index,
            tile_spawn.seed,
            group.members.len(),
            reset_before_summary,
            reset_report.before_summary,
            reset_after_summary,
            reset_report.after_summary,
            deterministic_edge.before_state,
            deterministic_edge.action,
            deterministic_edge.after_state,
            deterministic_edge.matches_happy_path,
            fixed_group_edge.before_state,
            fixed_group_edge.action,
            fixed_group_edge.after_state,
            fixed_group_edge.member_asset_ref,
            auto_cycle_edge.before_state,
            auto_cycle_edge.action,
            auto_cycle_edge.after_state,
            auto_cycle_edge.document_unchanged,
            invalid_edge.before_state,
            invalid_edge.action,
            invalid_edge.after_state,
            invalid_edge.error_code,
        ),
    )?;
    println!(
        "wrote terrain variation FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

#[derive(Debug)]
struct BoolEdge {
    before_state: String,
    action: String,
    after_state: String,
    matches_happy_path: bool,
}

#[derive(Debug)]
struct FixedGroupEdge {
    before_state: String,
    action: String,
    after_state: String,
    member_asset_ref: String,
}

#[derive(Debug)]
struct AutoCycleEdge {
    before_state: String,
    action: String,
    after_state: String,
    document_unchanged: bool,
}

#[derive(Debug)]
struct ErrorEdge {
    before_state: String,
    action: String,
    after_state: String,
    error_code: String,
}

fn edge_deterministic_reset(document: &EditorDocument) -> Result<BoolEdge, EngineError> {
    let expected_summary = tile_variation_summary(&document.terrain_tile_variations);
    let mut different_start = known_terrain_document()?;
    different_start
        .terrain_tile_variations
        .get_mut("tile_spawn_grass")
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.terrain_variation.fsv.tile_spawn_grass".to_string(),
            reason: "missing known tile".to_string(),
        })?
        .variation_index = 3;
    let before_state = tile_variation_summary(&different_start.terrain_tile_variations);
    reset_texture_variations(&mut different_start.terrain_tile_variations, 77_001)?;
    let after_state = tile_variation_summary(&different_start.terrain_tile_variations);
    Ok(BoolEdge {
        before_state,
        action: "reset_variations_seed_77001_from_different_start".to_string(),
        after_state: after_state.clone(),
        matches_happy_path: after_state == expected_summary,
    })
}

fn edge_one_member_group_fixed() -> Result<FixedGroupEdge, EngineError> {
    let group = TerrainRandomGroup::new(
        "single_tile",
        "Single Tile",
        9,
        vec![TerrainRandomGroupMember::new(
            "only",
            TerrainRandomGroupMemberKind::Tile,
            "tileset.grass.only",
            1,
        )],
    );
    let before_state = format!("members={};seed={}", group.members.len(), group.seed);
    let first = group.select_member("tile:0,0", 1)?;
    let second = group.select_member("tile:99,99", 999)?;
    if first != second {
        return Err(EngineError::UiRenderInvalidConfig {
            field: "editor.terrain_variation.fsv.fixed_group".to_string(),
            reason: "one-member group selected inconsistent members".to_string(),
        });
    }
    Ok(FixedGroupEdge {
        before_state,
        action: "select_single_member_group_from_two_keys".to_string(),
        after_state: format!("first={};second={}", first.asset_ref, second.asset_ref),
        member_asset_ref: first.asset_ref.clone(),
    })
}

fn edge_auto_cycle_does_not_mutate_saved_data(
    document: &EditorDocument,
) -> Result<AutoCycleEdge, EngineError> {
    let before_state = stable_fingerprint(document)?;
    let group = document
        .random_groups
        .get("forest_floor_mix")
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.terrain_variation.fsv.group".to_string(),
            reason: "missing forest_floor_mix".to_string(),
        })?;
    let preview = TerrainRandomGroupPreview::new(group)?;
    let auto_cycled = auto_cycle_random_group_preview(group, &preview)?;
    let after_state = stable_fingerprint(document)?;
    Ok(AutoCycleEdge {
        before_state: before_state.clone(),
        action: format!(
            "auto_cycle_preview_index_{}_to_{}",
            preview.current_index, auto_cycled.current_index
        ),
        after_state: after_state.clone(),
        document_unchanged: before_state == after_state,
    })
}

fn edge_invalid_duplicate_tile_fails_without_mutating(
    document: &EditorDocument,
) -> Result<ErrorEdge, EngineError> {
    let before_state = stable_fingerprint(document)?;
    let mut invalid_tiles = document.terrain_tile_variations.clone();
    invalid_tiles.insert(
        "tile_duplicate_coord".to_string(),
        TerrainTileVariation::new("tile_duplicate_coord", 0, 0, "tileset.grass", 4, 1, 77_001),
    );
    let error_code = match validate_tile_variations(&invalid_tiles) {
        Ok(_) => {
            return Err(EngineError::UiRenderInvalidConfig {
                field: "editor.terrain_variation.fsv.duplicate_tile".to_string(),
                reason: "duplicate tile coordinate unexpectedly validated".to_string(),
            });
        }
        Err(error) => error.code().to_string(),
    };
    let after_state = stable_fingerprint(document)?;
    Ok(ErrorEdge {
        before_state,
        action: "validate_duplicate_tile_coord_clone".to_string(),
        after_state,
        error_code,
    })
}

fn known_terrain_document() -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new("calyx://world/terrain-variation-fsv", "rev-1270")?;
    document.upsert_terrain_stamp(EditorTerrainStamp::new(
        "terrain_ramp",
        "Ramp",
        Transform::IDENTITY,
        4,
        3,
        1.25,
    ))?;
    document.upsert_terrain_tile_variation(TerrainTileVariation::new(
        "tile_spawn_grass",
        0,
        0,
        "tileset.grass",
        4,
        0,
        11,
    ))?;
    document.upsert_terrain_tile_variation(TerrainTileVariation::new(
        "tile_spawn_dirt",
        1,
        0,
        "tileset.dirt",
        3,
        1,
        11,
    ))?;
    document.upsert_terrain_tile_variation(TerrainTileVariation::new(
        "tile_spawn_rock",
        0,
        1,
        "tileset.rock",
        2,
        1,
        11,
    ))?;
    document.upsert_random_group(TerrainRandomGroup::new(
        "forest_floor_mix",
        "Forest Floor Mix",
        88_123,
        vec![
            TerrainRandomGroupMember::new(
                "grass_a",
                TerrainRandomGroupMemberKind::Tile,
                "tileset.grass.variant_a",
                5,
            ),
            TerrainRandomGroupMember::new(
                "grass_b",
                TerrainRandomGroupMemberKind::Tile,
                "tileset.grass.variant_b",
                3,
            ),
            TerrainRandomGroupMember::new(
                "fern",
                TerrainRandomGroupMemberKind::Doodad,
                "doodad.fern.low",
                2,
            ),
        ],
    ))?;
    document.validate()?;
    Ok(document)
}

fn source_truth_state(
    document: &EditorDocument,
    before_document_path: &Path,
    after_document_path: &Path,
    script_path: &Path,
    map_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        "document={};before_doc={};after_doc={};script={};map={}",
        stable_fingerprint(document)?,
        artifact_state(before_document_path)?,
        artifact_state(after_document_path)?,
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
