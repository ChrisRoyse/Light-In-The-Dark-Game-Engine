// #1657 crypts & undead sites: per-spec fact checks + the authoritative store
// round-trip FSV (author -> persist_and_lower -> separate readback, sha256 identical),
// plus the consolidated fail-closed edges over a representative spec.

use crate::rpg::{AreaKind, area_spec_readback};

use super::rpg_dungeons_thera_fixture_crypts::{
    eil_shaeria_area_spec, forgotten_crypts_area_spec, graveyard_area_spec, mausoleum_area_spec,
    ruins_of_maethien_area_spec,
};
use super::rpg_dungeons_thera_support::{
    EIL_SHAERIA_AREA_ID, FORGOTTEN_CRYPTS_AREA_ID, GRAVEYARD_AREA_ID, MAUSOLEUM_AREA_ID,
    RUINS_OF_MAETHIEN_AREA_ID,
};
use super::rpg_dungeons_thera_test_support::{assert_dungeon_roundtrip, assert_fail_closed_edges};

#[test]
fn issue1657_graveyard_reuses_galadon_edge_id_and_roundtrips() {
    let spec = graveyard_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, GRAVEYARD_AREA_ID);
    // MUST equal the area id Galadon's `conn.galadon.to_graveyard` external edge targets.
    assert_eq!(
        spec.area_id,
        crate::rpg_area_specs_test_fixture::GALADON_GRAVEYARD_AREA_ID
    );
    assert_eq!(readback.source_issue, 1657);
    assert_eq!(readback.area_kind, AreaKind::Dungeon);
    assert_eq!(readback.level_band_min, 1);
    assert_eq!(readback.level_band_max, 10);
    // Graveyard's external edge returns to the Galadon hub (#1632).
    assert!(
        spec.connections
            .iter()
            .any(|c| c.to_area_id == "area.thera.galadon")
    );
    assert_dungeon_roundtrip(&spec, "graveyard");
}

#[test]
fn issue1657_mausoleum_matches_facts_and_roundtrips() {
    let spec = mausoleum_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, MAUSOLEUM_AREA_ID);
    assert_eq!(readback.source_issue, 1657);
    assert_eq!(readback.area_kind, AreaKind::Dungeon);
    assert_eq!(readback.level_band_min, 25);
    assert_eq!(readback.level_band_max, 50);
    assert_dungeon_roundtrip(&spec, "mausoleum");
}

#[test]
fn issue1657_forgotten_crypts_matches_facts_and_roundtrips() {
    let spec = forgotten_crypts_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, FORGOTTEN_CRYPTS_AREA_ID);
    assert_eq!(readback.source_issue, 1657);
    assert_eq!(readback.area_kind, AreaKind::Dungeon);
    assert_eq!(readback.level_band_min, 6);
    assert_eq!(readback.level_band_max, 30);
    assert_dungeon_roundtrip(&spec, "forgotten-crypts");
}

#[test]
fn issue1657_eil_shaeria_matches_facts_and_roundtrips() {
    let spec = eil_shaeria_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, EIL_SHAERIA_AREA_ID);
    assert_eq!(readback.source_issue, 1657);
    assert_eq!(readback.area_kind, AreaKind::Dungeon);
    assert_eq!(readback.level_band_min, 35);
    assert_eq!(readback.level_band_max, 50);
    assert_dungeon_roundtrip(&spec, "eil-shaeria");
}

#[test]
fn issue1657_ruins_of_maethien_matches_facts_and_roundtrips() {
    let spec = ruins_of_maethien_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, RUINS_OF_MAETHIEN_AREA_ID);
    assert_eq!(readback.source_issue, 1657);
    assert_eq!(readback.area_kind, AreaKind::Dungeon);
    assert_eq!(readback.level_band_min, 40);
    assert_eq!(readback.level_band_max, 51);
    assert_dungeon_roundtrip(&spec, "ruins-of-maethien");
}

#[test]
fn issue1657_crypts_fail_closed_edges() {
    assert_fail_closed_edges(&graveyard_area_spec());
}
