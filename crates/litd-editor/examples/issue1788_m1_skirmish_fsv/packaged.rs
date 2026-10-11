//! #1788 phase-1 packaged-catalog readback: read the SHIPPED `.litdgame` bytes back off disk and
//! count exactly what per-faction records they carry. With the #1849 packaging cap lifted, the
//! shipped catalog IS the full authored unit/structure census -- `census_matches` proves the
//! packaged counts equal the authored census counts, read back from the package bytes.

use std::path::Path;

use litd_engine::game_runtime::RuntimeDatasetsManifest;

/// Per-faction record counts READ BACK from the SHIPPED package bytes (not the builder's in-memory
/// intent): the economy catalog unit/structure TYPE records (keyed by faction-prefixed id) and the
/// placed starting units (by dense owner slot).
#[derive(Debug, serde::Serialize)]
pub struct FactionPackaged {
    pub economy_unit_records: usize,
    pub economy_structure_records: usize,
    pub placed_starting_units: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct PackagedCatalog {
    pub emberhold: FactionPackaged,
    pub veilspun: FactionPackaged,
    pub ai_profiles: usize,
}

/// The FULL per-faction catalog the package must ship (#1849 cap lifted): all 8 mobile unit
/// TYPES + all 6 structure TYPES = 14 economy unit records (of which 6 are structures) + 3
/// placed starting units; the packaged config keeps exactly one authored AI profile.
const PKG_ECONOMY_UNIT_RECORDS: usize = 14;
const PKG_ECONOMY_STRUCTURE_RECORDS: usize = 6;
const PKG_PLACED_UNITS: usize = 3;
const PKG_AI_PROFILES: usize = 1;

impl PackagedCatalog {
    pub fn matches_expected(&self) -> bool {
        let faction_ok = |f: &FactionPackaged| {
            f.economy_unit_records == PKG_ECONOMY_UNIT_RECORDS
                && f.economy_structure_records == PKG_ECONOMY_STRUCTURE_RECORDS
                && f.placed_starting_units == PKG_PLACED_UNITS
        };
        faction_ok(&self.emberhold)
            && faction_ok(&self.veilspun)
            && self.ai_profiles == PKG_AI_PROFILES
    }

    /// census == packaged (#1849 DONE-WHEN): the unit/structure TYPE records read back from
    /// the shipped bytes equal the authored per-faction census counts exactly.
    pub fn census_matches(&self, census_units: usize, census_structures: usize) -> bool {
        let faction_ok = |f: &FactionPackaged| {
            f.economy_unit_records == census_units + census_structures
                && f.economy_structure_records == census_structures
        };
        faction_ok(&self.emberhold) && faction_ok(&self.veilspun)
    }
}

/// Read the shipped `.litdgame` bytes back off disk and count exactly what per-faction records
/// they carry. Economy catalog counts come straight from the parsed package JSON (faction-prefixed
/// ids); placed units come from re-verifying the shipped lowered-match canonical bytes (fail-loud
/// on drift), grouped by dense owner slot (0 = Emberhold/player_one, 1 = Veilspun/player_two).
pub fn read_packaged_catalog(
    package_path: &Path,
    manifest: &RuntimeDatasetsManifest,
) -> Result<PackagedCatalog, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(package_path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let runtime = value
        .get("runtime")
        .ok_or("packaged bytes carry no runtime section")?;
    let economy = runtime
        .get("economy")
        .ok_or("packaged bytes carry no runtime.economy")?;
    let count_prefixed = |array_key: &str, id_key: &str, prefix: &str| -> usize {
        economy
            .get(array_key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|e| {
                        e.get(id_key)
                            .and_then(|s| s.as_str())
                            .is_some_and(|s| s.starts_with(prefix))
                    })
                    .count()
            })
            .unwrap_or(0)
    };
    // Placed units live inside the lowered-match canonical RON; verify() re-parses the SHIPPED
    // bytes (fail-loud on any drift), so the placed roster is genuinely the packaged one.
    let blueprint = manifest.lowered_match.verify()?;
    let placed = |slot: u8| blueprint.units.iter().filter(|u| u.owner == slot).count();
    let ai_profiles = runtime
        .get("ai_profiles")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok(PackagedCatalog {
        emberhold: FactionPackaged {
            economy_unit_records: count_prefixed("units", "unit_id", "emberhold"),
            economy_structure_records: count_prefixed("structures", "struct_id", "emberhold"),
            placed_starting_units: placed(0),
        },
        veilspun: FactionPackaged {
            economy_unit_records: count_prefixed("units", "unit_id", "veilspun"),
            economy_structure_records: count_prefixed("structures", "struct_id", "veilspun"),
            placed_starting_units: placed(1),
        },
        ai_profiles,
    })
}
