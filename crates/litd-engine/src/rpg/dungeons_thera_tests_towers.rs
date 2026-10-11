// #1659 towers & arcane sites: per-spec fact checks + the authoritative store
// round-trip FSV, a batch-wide registry-integrity check across all 20 dungeons, and
// the consolidated fail-closed edges over a representative spec.

use std::collections::BTreeSet;

use crate::rpg::{AreaKind, area_spec_readback, validate_area_connections_against_registry};

use super::rpg_dungeons_thera_fixture_crypts::{
    eil_shaeria_area_spec, forgotten_crypts_area_spec, graveyard_area_spec, mausoleum_area_spec,
    ruins_of_maethien_area_spec,
};
use super::rpg_dungeons_thera_fixture_keeps_a::{
    keep_of_fhalaugash_area_spec, keep_of_the_righteous_area_spec, lord_tabershaw_keep_area_spec,
    ruined_keep_area_spec,
};
use super::rpg_dungeons_thera_fixture_keeps_b::{
    high_lords_keep_area_spec, imperial_lands_sewers_area_spec, keep_of_ceawlin_area_spec,
    moudrilar_monastery_area_spec,
};
use super::rpg_dungeons_thera_fixture_towers_a::{
    octagonal_tower_area_spec, tower_of_dragons_area_spec, tower_of_high_sorcery_area_spec,
    tower_of_trothon_area_spec,
};
use super::rpg_dungeons_thera_fixture_towers_b::{
    elemental_temple_area_spec, mines_of_zakiim_area_spec, ruins_of_delar_tol_area_spec,
};
use super::rpg_dungeons_thera_support::{
    ELEMENTAL_TEMPLE_AREA_ID, MINES_OF_ZAKIIM_AREA_ID, OCTAGONAL_TOWER_AREA_ID,
    RUINS_OF_DELAR_TOL_AREA_ID, TOWER_OF_DRAGONS_AREA_ID, TOWER_OF_HIGH_SORCERY_AREA_ID,
    TOWER_OF_TROTHON_AREA_ID, dungeon_known_area_ids,
};
use super::rpg_dungeons_thera_test_support::{assert_dungeon_roundtrip, assert_fail_closed_edges};

fn check(spec: &crate::rpg::AreaSpec, area_id: &str, min: u8, max: u8) {
    let readback = area_spec_readback(spec).unwrap();
    assert_eq!(readback.area_id, area_id);
    assert_eq!(readback.source_issue, 1659);
    assert_eq!(readback.area_kind, AreaKind::Dungeon);
    assert_eq!(readback.level_band_min, min);
    assert_eq!(readback.level_band_max, max);
}

#[test]
fn issue1659_tower_of_high_sorcery_matches_facts_and_roundtrips() {
    let spec = tower_of_high_sorcery_area_spec();
    check(&spec, TOWER_OF_HIGH_SORCERY_AREA_ID, 10, 30);
    assert_dungeon_roundtrip(&spec, "tower-of-high-sorcery");
}

#[test]
fn issue1659_tower_of_dragons_matches_facts_and_roundtrips() {
    let spec = tower_of_dragons_area_spec();
    check(&spec, TOWER_OF_DRAGONS_AREA_ID, 10, 51);
    assert_dungeon_roundtrip(&spec, "tower-of-dragons");
}

#[test]
fn issue1659_octagonal_tower_matches_facts_and_roundtrips() {
    let spec = octagonal_tower_area_spec();
    check(&spec, OCTAGONAL_TOWER_AREA_ID, 40, 51);
    assert_dungeon_roundtrip(&spec, "octagonal-tower");
}

#[test]
fn issue1659_tower_of_trothon_matches_facts_and_roundtrips() {
    let spec = tower_of_trothon_area_spec();
    check(&spec, TOWER_OF_TROTHON_AREA_ID, 40, 51);
    assert_dungeon_roundtrip(&spec, "tower-of-trothon");
}

#[test]
fn issue1659_elemental_temple_matches_facts_and_roundtrips() {
    let spec = elemental_temple_area_spec();
    check(&spec, ELEMENTAL_TEMPLE_AREA_ID, 30, 50);
    assert_dungeon_roundtrip(&spec, "elemental-temple");
}

#[test]
fn issue1659_mines_of_zakiim_matches_facts_and_roundtrips() {
    let spec = mines_of_zakiim_area_spec();
    check(&spec, MINES_OF_ZAKIIM_AREA_ID, 30, 51);
    assert_dungeon_roundtrip(&spec, "mines-of-zakiim");
}

#[test]
fn issue1659_ruins_of_delar_tol_matches_facts_and_roundtrips() {
    let spec = ruins_of_delar_tol_area_spec();
    check(&spec, RUINS_OF_DELAR_TOL_AREA_ID, 20, 40);
    assert_dungeon_roundtrip(&spec, "ruins-of-delar-tol");
}

#[test]
fn issue1659_towers_fail_closed_edges() {
    assert_fail_closed_edges(&tower_of_high_sorcery_area_spec());
}

/// Batch-wide world-graph integrity (#1632): every one of the 20 authored dungeons
/// passes cross-area referential integrity against the shared known-area registry,
/// and every area id is unique.
#[test]
fn issue1657_1659_all_dungeons_pass_registry_integrity() {
    let specs = vec![
        graveyard_area_spec(),
        mausoleum_area_spec(),
        forgotten_crypts_area_spec(),
        eil_shaeria_area_spec(),
        ruins_of_maethien_area_spec(),
        keep_of_fhalaugash_area_spec(),
        keep_of_the_righteous_area_spec(),
        ruined_keep_area_spec(),
        lord_tabershaw_keep_area_spec(),
        keep_of_ceawlin_area_spec(),
        moudrilar_monastery_area_spec(),
        imperial_lands_sewers_area_spec(),
        high_lords_keep_area_spec(),
        tower_of_high_sorcery_area_spec(),
        tower_of_dragons_area_spec(),
        octagonal_tower_area_spec(),
        tower_of_trothon_area_spec(),
        elemental_temple_area_spec(),
        mines_of_zakiim_area_spec(),
        ruins_of_delar_tol_area_spec(),
    ];
    assert_eq!(specs.len(), 20);
    let known: BTreeSet<&str> = dungeon_known_area_ids().into_iter().collect();
    let mut seen = BTreeSet::new();
    for spec in &specs {
        validate_area_connections_against_registry(spec, &known).unwrap();
        assert!(known.contains(spec.area_id.as_str()));
        assert!(
            seen.insert(spec.area_id.clone()),
            "duplicate area id {}",
            spec.area_id
        );
    }
}
