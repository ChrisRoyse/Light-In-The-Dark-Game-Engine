//! Corpus-wide terrain+acoustics coverage suite (#1667). Assembles EVERY authored
//! AreaSpec (the same single-source corpus the #1704 registry suite uses), tags
//! each with terrain + acoustics, and proves the tagged set is EXACTLY the
//! registry set -- so no authored area is left untagged and no tag is an orphan.
//! Also proves Calyx persist+readback of the tagged specs and that a bogus tag /
//! missing coverage fails loud by name.

use std::collections::BTreeSet;

use crate::{
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AREA_TERRAIN_ACOUSTICS_TABLE_ID, AreaTerrainAcoustics, RPG_AREA_TERRAIN_ACOUSTICS_SCHEMA,
        SurfaceMaterialClass, TerrainTag, area_terrain_acoustics_content_records,
        area_terrain_acoustics_readback, build_area_terrain_acoustics_table,
        derive_area_terrain_acoustics, persist_and_lower_rpg_authored_content,
        validate_area_terrain_acoustics, validate_terrain_acoustics_coverage,
    },
};

use crate::rpg_area_registry_corpus_tests::full_authored_corpus;

fn registry_ids() -> BTreeSet<String> {
    full_authored_corpus()
        .iter()
        .map(|spec| spec.area_id.clone())
        .collect()
}

#[test]
fn issue1667_dump_untagged_natural_areas() {
    // Diagnostic (printed under --nocapture): every corpus area, its kind, and its
    // derive outcome. Any UNTAGGED line names a natural area still needing a
    // CF-factual terrain entry. Non-asserting: it is the authoring worklist.
    let mut untagged = 0usize;
    for spec in full_authored_corpus() {
        match derive_area_terrain_acoustics(&spec) {
            Ok(entry) => eprintln!(
                "TAGGED {} [{:?}] -> {}",
                spec.area_id,
                spec.area_kind,
                entry.primary_terrain.label()
            ),
            Err(error) => {
                untagged += 1;
                eprintln!("UNTAGGED {} [{:?}]: {error}", spec.area_id, spec.area_kind);
            }
        }
    }
    eprintln!("issue1667 untagged natural areas: {untagged}");
}

#[test]
fn issue1667_every_authored_area_has_terrain_and_acoustics() {
    let corpus = full_authored_corpus();
    let table = build_area_terrain_acoustics_table(&corpus)
        .unwrap_or_else(|e| panic!("terrain tagging failed: {e}"));
    let coverage = validate_terrain_acoustics_coverage(&registry_ids(), &table)
        .unwrap_or_else(|e| panic!("coverage lint failed: {e}"));

    assert_eq!(
        coverage.tagged_area_count, coverage.registry_area_count,
        "every authored area must be tagged"
    );
    assert!(
        coverage.registry_area_count >= 100,
        "expected the full Thera corpus (>=100 areas), got {}",
        coverage.registry_area_count
    );
    // All seven ranger-homeland natural terrains must exist so every ranger build
    // has a homeland (#1667 / the CF race-terrain matrix).
    assert_eq!(
        coverage.ranger_homeland_terrain_count, 7,
        "all seven ranger-homeland terrains must be represented; histogram={:?}",
        coverage.terrain_histogram
    );

    let readback = area_terrain_acoustics_readback(&table).unwrap();
    eprintln!(
        "issue1667 coverage: areas={} terrain_histogram={:?} light={:?} surface={:?} \
         reverb={:?} hide_legal={} exposed={} no_sneak_interior={} cf_documented={} \
         derived_from_kind={}",
        readback.area_count,
        readback.terrain_histogram,
        readback.light_histogram,
        readback.surface_histogram,
        readback.reverb_histogram,
        readback.hide_legal_count,
        readback.exposed_count,
        readback.no_sneak_interior_count,
        readback.cf_documented_count,
        readback.derived_from_kind_count,
    );
    // Every terrain family carries at least one area (no blind spot). This is the
    // stronger form of the ranger-homeland check across all nine families.
    for terrain in TerrainTag::all() {
        assert!(
            readback
                .terrain_histogram
                .get(terrain.label())
                .copied()
                .unwrap_or(0)
                > 0,
            "terrain family {} has zero tagged areas (blind spot)",
            terrain.label()
        );
    }
    assert_eq!(readback.table_sha256.len(), 64);
}

#[test]
fn issue1667_missing_coverage_fails_loud_by_name() {
    // Edge: drop one area's tag and prove the coverage lint fails loud naming the
    // untagged area (not a silent pass).
    let corpus = full_authored_corpus();
    let mut table = build_area_terrain_acoustics_table(&corpus).unwrap();
    let dropped = table.entries.remove(0).area_id;
    let error = validate_terrain_acoustics_coverage(&registry_ids(), &table)
        .expect_err("dropping a tag must fail coverage");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    let message = error.to_string();
    assert!(
        message.contains("have no terrain+acoustics tag"),
        "must fail as an untagged-areas error: {message}"
    );
    assert!(
        message.contains(&dropped),
        "diagnostic must name the untagged area {dropped}: {message}"
    );
}

#[test]
fn issue1667_bogus_terrain_tag_fails_loud_by_name() {
    // Edge: a forest paved in sand is an inconsistent acoustic tag and must fail
    // loud, naming the area and the illegal surface.
    let bogus = AreaTerrainAcoustics {
        area_id: "area.thera.__bogus__".to_string(),
        primary_terrain: TerrainTag::Forest,
        secondary_terrain: Vec::new(),
        light_level: crate::rpg::LightLevel::Dim,
        surface_material: SurfaceMaterialClass::Sand,
        reverb: crate::rpg::ReverbClass::Open,
        occlusion: crate::rpg::OcclusionClass::Partial,
        hide_legal: true,
        exposed: false,
        no_sneak_interior: false,
        derivation: crate::rpg::TerrainDerivation::CfDocumented,
        note: "deliberately bogus".to_string(),
    };
    let error =
        validate_area_terrain_acoustics(&bogus).expect_err("a forest of sand must fail validation");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    let message = error.to_string();
    assert!(
        message.contains("area.thera.__bogus__") && message.contains("sand"),
        "diagnostic must name the area and the illegal surface: {message}"
    );
}

#[test]
fn issue1667_persists_and_reads_back_from_calyx() {
    let corpus = full_authored_corpus();
    let table = build_area_terrain_acoustics_table(&corpus).unwrap();
    let records = area_terrain_acoustics_content_records(&table).unwrap();
    // One summary row plus one row per tagged area.
    assert_eq!(records.len(), table.entries.len() + 1);
    assert!(records.iter().all(|r| {
        r.source
            .path
            .starts_with("editor://rpg/thera/area_terrain_acoustics/")
    }));

    let (store, _vault) = content_store("issue1667");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, records.len());

    let summary = store
        .get_content_for_editor(
            EngineContentKind::Terrain,
            &format!("{AREA_TERRAIN_ACOUSTICS_TABLE_ID}.summary"),
        )
        .unwrap();
    assert_eq!(
        summary.payload["surface_kind"],
        "area_terrain_acoustics_table"
    );
    assert_eq!(summary.payload["schema"], RPG_AREA_TERRAIN_ACOUSTICS_SCHEMA);
    assert_eq!(summary.payload["area_count"], table.entries.len());

    // Spot-check one CF-documented natural area round-trips with its terrain.
    let forest = &table
        .entries
        .iter()
        .find(|e| e.primary_terrain == TerrainTag::Forest)
        .expect("corpus has at least one forest");
    let row = store
        .get_content_for_editor(
            EngineContentKind::Terrain,
            &format!("{AREA_TERRAIN_ACOUSTICS_TABLE_ID}.{}", forest.area_id),
        )
        .unwrap();
    assert_eq!(row.payload["surface_kind"], "area_terrain_acoustics");
    assert_eq!(row.payload["primary_terrain"], "forest");
    assert_eq!(row.payload["hide_legal"], true);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("{label}-terrain"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
