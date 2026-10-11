//! #1878 progressive composition packs: kenney core required; manual megakits optional.
//!
//! Missing optional packs are reported via greppable residual log and skipped — never
//! silently treated as full composition. Required kenney packs still fail loud.

use litd_engine::EngineError;

use crate::asset_vault::{AssetVaultCatalog, AssetVaultRecord};

use super::sources::{invalid_composition, source_archive_record};

/// Greppable residual status when optional composition packs are absent.
pub(super) const COMPOSITION_PROGRESSIVE_STATUS: &str =
    "issue1878_composition_progressive_residual";

pub(super) struct CompositionPacks<'a> {
    pub progressive: bool,
    pub residual: Vec<String>,
    pub stylized: Option<&'a AssetVaultRecord>,
    pub castle: &'a AssetVaultRecord,
    pub modular_dungeon: &'a AssetVaultRecord,
    pub village: Option<&'a AssetVaultRecord>,
    pub tower_defense: &'a AssetVaultRecord,
    pub include_gothic: bool,
}

pub(super) fn resolve_composition_packs(
    catalog: &AssetVaultCatalog,
) -> Result<CompositionPacks<'_>, EngineError> {
    let mut residual = Vec::new();

    let castle = source_archive_record(catalog, super::architecture::CASTLE_KIT_ASSET_ID)?;
    let modular_dungeon =
        source_archive_record(catalog, super::citadel::MODULAR_DUNGEON_KIT_ASSET_ID)?;
    let tower_defense = source_archive_record(catalog, super::kenney::TOWER_DEFENSE_KIT_ASSET_ID)?;

    let stylized = optional_download_archive(catalog, "stylizednaturemegakit", &mut residual);
    let village = optional_download_archive(catalog, "medievalvillagemegakit", &mut residual);

    let include_gothic = gothic_roots_present(catalog);
    if !include_gothic {
        residual.push(
            "gothic:sources/quaternius/game-kit/ultimatemodularruins+modularmedievalbuildings"
                .to_string(),
        );
    }

    let progressive = !residual.is_empty();
    if progressive {
        eprintln!(
            "ASSET_SCENE_COMPOSITION_RESIDUAL status={} residual_count={} residual={} \
             -- #1878 progressive composition; optional packs skipped (never silent full)",
            COMPOSITION_PROGRESSIVE_STATUS,
            residual.len(),
            residual.join(";")
        );
    }

    Ok(CompositionPacks {
        progressive,
        residual,
        stylized,
        castle,
        modular_dungeon,
        village,
        tower_defense,
        include_gothic,
    })
}

fn optional_download_archive<'a>(
    catalog: &'a AssetVaultCatalog,
    asset_id: &str,
    residual: &mut Vec<String>,
) -> Option<&'a AssetVaultRecord> {
    match catalog
        .records()
        .iter()
        .find(|record| record.record_type == "download" && record.asset_id == asset_id)
    {
        Some(record) => Some(record),
        None => {
            residual.push(format!("archive:{asset_id}"));
            None
        }
    }
}

fn gothic_roots_present(catalog: &AssetVaultCatalog) -> bool {
    const ROOTS: &[&str] = &[
        "sources/quaternius/game-kit/ultimatemodularruins/OBJ/",
        "sources/quaternius/game-kit/modularmedievalbuildings/OBJ/",
    ];
    ROOTS.iter().any(|root| {
        catalog.records().iter().any(|record| {
            matches!(record.record_type.as_str(), "download" | "extraction")
                && record.relative_path.starts_with(root)
                && record.absolute_path.is_file()
        })
    })
}

/// Optional extracted path — returns None when progressive skip is allowed.
pub(super) fn try_extracted_record<'a>(
    catalog: &'a AssetVaultCatalog,
    relative_path: &str,
) -> Option<&'a AssetVaultRecord> {
    catalog.records().iter().find(|record| {
        record.record_type == "extraction"
            && record.relative_path == relative_path
            && record.absolute_path.is_file()
    })
}

/// Optional download/extraction path for progressive placement loops.
pub(super) fn try_manifest_file_record<'a>(
    catalog: &'a AssetVaultCatalog,
    relative_path: &str,
) -> Option<&'a AssetVaultRecord> {
    catalog.records().iter().find(|record| {
        matches!(record.record_type.as_str(), "download" | "extraction")
            && record.relative_path == relative_path
            && record.absolute_path.is_file()
    })
}

/// Fail loud when progressive composition produced no imported doodad sources.
pub(super) fn ensure_progressive_floor(
    packs: &CompositionPacks<'_>,
    doodad_source_count: usize,
) -> Result<(), EngineError> {
    if doodad_source_count == 0 {
        return invalid_composition(
            "progressive_floor",
            format!(
                "progressive composition imported zero doodad sources; residual={}",
                packs.residual.join(";")
            ),
        );
    }
    Ok(())
}
