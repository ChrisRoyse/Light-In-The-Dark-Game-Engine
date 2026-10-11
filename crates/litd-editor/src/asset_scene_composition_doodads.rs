//! Doodad pack placement loops for editor vault composition (#1878 progressive).

use litd_engine::EngineError;

use crate::asset_vault::AssetVaultCatalog;

use super::architecture::{CASTLE_KIT_GLB_ROOT, architecture_placements};
use super::build_helpers::stylized_asset_tint;
use super::citadel::{MODULAR_DUNGEON_GLB_ROOT, citadel_placements};
use super::depth_clusters::depth_cluster_placements;
use super::dressing::dressing_placements;
use super::geometry::{CompositionBuild, CompositionStats, append_asset_tinted};
use super::gothic::gothic_placements;
use super::kenney::{TOWER_DEFENSE_GLB_ROOT, kenney_skyline_placements};
use super::obj::append_obj_asset_tinted;
use super::placements::asset_placements;
use super::polyhaven::polyhaven_placements;
use super::progressive::{
    CompositionPacks, ensure_progressive_floor, try_extracted_record, try_manifest_file_record,
};
use super::sources::{extracted_record, manifest_file_record};
use super::texture_atlas::TextureAtlasBuilder;
use super::village::{MEDIEVAL_VILLAGE_GLTF_ROOT, village_placements};

const STYLIZED_GLTF_ROOT: &str = "extracted/quaternius/manual-game-kit/stylizednaturemegakit/Stylized_Nature_MegaKit_Standard/glTF";

pub(super) fn append_composition_doodad_packs(
    catalog: &AssetVaultCatalog,
    packs: &CompositionPacks<'_>,
    build: &mut CompositionBuild,
    stats: &mut CompositionStats,
    texture_atlas: &mut TextureAtlasBuilder,
) -> Result<(), EngineError> {
    let doodad_sources_before = stats.source_paths.len();
    let mut skipped_optional_placements = 0usize;

    if packs.stylized.is_some() {
        for placement in asset_placements() {
            let relative_path = format!("{STYLIZED_GLTF_ROOT}/{}", placement.file_name);
            let Some(record) = try_extracted_record(catalog, &relative_path) else {
                skipped_optional_placements += 1;
                continue;
            };
            let import = append_asset_tinted(
                record,
                *placement,
                stylized_asset_tint(placement.file_name),
                build,
                texture_atlas,
            )?;
            stats.source_paths.insert(record.relative_path.clone());
            stats.source_paths.extend(import.source_paths);
            stats.material_count += import.material_count;
            stats.texture_count += import.texture_count;
            stats.image_count += import.image_count;
            stats.degenerate_triangle_count += import.degenerate_triangle_count;
        }
    }

    for placement in architecture_placements() {
        let relative_path = format!("{CASTLE_KIT_GLB_ROOT}/{}", placement.file_name);
        let record = extracted_record(catalog, &relative_path)?;
        let import = append_asset_tinted(
            record,
            placement.asset_placement(),
            [0.34, 0.38, 0.46, 0.0],
            build,
            texture_atlas,
        )?;
        stats.source_paths.insert(record.relative_path.clone());
        stats.source_paths.extend(import.source_paths);
        stats.material_count += import.material_count;
        stats.texture_count += import.texture_count;
        stats.image_count += import.image_count;
        stats.degenerate_triangle_count += import.degenerate_triangle_count;
    }

    for placement in citadel_placements() {
        let relative_path = format!("{MODULAR_DUNGEON_GLB_ROOT}/{}", placement.file_name);
        let record = extracted_record(catalog, &relative_path)?;
        let import = append_asset_tinted(
            record,
            placement.asset_placement(),
            [0.30, 0.33, 0.40, 0.0],
            build,
            texture_atlas,
        )?;
        stats.source_paths.insert(record.relative_path.clone());
        stats.source_paths.extend(import.source_paths);
        stats.material_count += import.material_count;
        stats.texture_count += import.texture_count;
        stats.image_count += import.image_count;
        stats.degenerate_triangle_count += import.degenerate_triangle_count;
    }

    if packs.village.is_some() {
        for placement in village_placements() {
            let relative_path = format!("{MEDIEVAL_VILLAGE_GLTF_ROOT}/{}", placement.file_name);
            let Some(record) = try_extracted_record(catalog, &relative_path) else {
                skipped_optional_placements += 1;
                continue;
            };
            let import = append_asset_tinted(
                record,
                placement.asset_placement(),
                placement.color_tint,
                build,
                texture_atlas,
            )?;
            stats.source_paths.insert(record.relative_path.clone());
            stats.source_paths.extend(import.source_paths);
            stats.material_count += import.material_count;
            stats.texture_count += import.texture_count;
            stats.image_count += import.image_count;
            stats.degenerate_triangle_count += import.degenerate_triangle_count;
        }
    }

    for placement in kenney_skyline_placements() {
        let relative_path = format!("{TOWER_DEFENSE_GLB_ROOT}/{}", placement.file_name);
        let record = manifest_file_record(catalog, &relative_path)?;
        let import = append_asset_tinted(
            record,
            placement.asset_placement(),
            placement.color_tint,
            build,
            texture_atlas,
        )?;
        stats.source_paths.insert(record.relative_path.clone());
        stats.source_paths.extend(import.source_paths);
        stats.material_count += import.material_count;
        stats.texture_count += import.texture_count;
        stats.image_count += import.image_count;
        stats.degenerate_triangle_count += import.degenerate_triangle_count;
    }

    if packs.include_gothic {
        for placement in gothic_placements() {
            let mtl_relative_path = placement.mtl_relative_path();
            let Some(record) = try_manifest_file_record(catalog, placement.relative_path) else {
                skipped_optional_placements += 1;
                continue;
            };
            let Some(mtl_record) = try_manifest_file_record(catalog, &mtl_relative_path) else {
                skipped_optional_placements += 1;
                continue;
            };
            let import = append_obj_asset_tinted(
                record,
                mtl_record,
                placement.asset_placement(),
                placement.color_tint,
                build,
            )?;
            stats.source_paths.insert(record.relative_path.clone());
            stats.source_paths.insert(mtl_record.relative_path.clone());
            stats.source_paths.extend(import.source_paths);
            stats.material_count += import.material_count;
            stats.texture_count += import.texture_count;
            stats.image_count += import.image_count;
            stats.degenerate_triangle_count += import.degenerate_triangle_count;
        }
    }

    // Progressive residual (#1878/#1790): skip multi-GB polyhaven/depth/dressing packs so
    // live editor boot stays operator/Synapse-visible. Full composition still loads them when
    // all optional archives are present (packs.progressive == false).
    if packs.progressive {
        skipped_optional_placements += polyhaven_placements().len()
            + depth_cluster_placements().len()
            + dressing_placements().len();
        eprintln!(
            "ASSET_SCENE_COMPOSITION_RESIDUAL skipped_heavy_optional_packs=polyhaven+depth+dressing \
             progressive=true -- kenney-core-only progressive doodads"
        );
    } else {
        for placement in polyhaven_placements() {
            let record = manifest_file_record(catalog, placement.relative_path)?;
            let import = append_asset_tinted(
                record,
                placement.asset_placement(),
                placement.color_tint,
                build,
                texture_atlas,
            )?;
            stats.source_paths.insert(record.relative_path.clone());
            stats.source_paths.extend(import.source_paths);
            stats.material_count += import.material_count;
            stats.texture_count += import.texture_count;
            stats.image_count += import.image_count;
            stats.degenerate_triangle_count += import.degenerate_triangle_count;
        }

        for placement in depth_cluster_placements() {
            let Some(record) = try_manifest_file_record(catalog, placement.relative_path) else {
                skipped_optional_placements += 1;
                continue;
            };
            let import = append_asset_tinted(
                record,
                placement.asset_placement(),
                placement.color_tint,
                build,
                texture_atlas,
            )?;
            stats.source_paths.insert(record.relative_path.clone());
            stats.source_paths.extend(import.source_paths);
            stats.material_count += import.material_count;
            stats.texture_count += import.texture_count;
            stats.image_count += import.image_count;
            stats.degenerate_triangle_count += import.degenerate_triangle_count;
        }

        for placement in dressing_placements() {
            let Some(record) = try_manifest_file_record(catalog, placement.relative_path) else {
                skipped_optional_placements += 1;
                continue;
            };
            let import = append_asset_tinted(
                record,
                placement.asset_placement(),
                placement.color_tint,
                build,
                texture_atlas,
            )?;
            stats.source_paths.insert(record.relative_path.clone());
            stats.source_paths.extend(import.source_paths);
            stats.material_count += import.material_count;
            stats.texture_count += import.texture_count;
            stats.image_count += import.image_count;
            stats.degenerate_triangle_count += import.degenerate_triangle_count;
        }
    }

    let doodad_imported = stats
        .source_paths
        .len()
        .saturating_sub(doodad_sources_before);
    ensure_progressive_floor(packs, doodad_imported)?;
    if skipped_optional_placements > 0 {
        eprintln!(
            "ASSET_SCENE_COMPOSITION_RESIDUAL skipped_optional_placements={} progressive={}",
            skipped_optional_placements, packs.progressive
        );
    }
    Ok(())
}
