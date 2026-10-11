// #1658 keeps & citadels: per-spec fact checks + the authoritative store round-trip
// FSV, plus the consolidated fail-closed edges over a representative spec.

use crate::rpg::{AreaKind, area_spec_readback};

use super::rpg_dungeons_thera_fixture_keeps_a::{
    keep_of_fhalaugash_area_spec, keep_of_the_righteous_area_spec, lord_tabershaw_keep_area_spec,
    ruined_keep_area_spec,
};
use super::rpg_dungeons_thera_fixture_keeps_b::{
    high_lords_keep_area_spec, imperial_lands_sewers_area_spec, keep_of_ceawlin_area_spec,
    moudrilar_monastery_area_spec,
};
use super::rpg_dungeons_thera_support::{
    HIGH_LORDS_KEEP_AREA_ID, IMPERIAL_LANDS_SEWERS_AREA_ID, KEEP_OF_CEAWLIN_AREA_ID,
    KEEP_OF_FHALAUGASH_AREA_ID, KEEP_OF_THE_RIGHTEOUS_AREA_ID, LORD_TABERSHAW_KEEP_AREA_ID,
    MOUDRILAR_MONASTERY_AREA_ID, RUINED_KEEP_AREA_ID,
};
use super::rpg_dungeons_thera_test_support::{assert_dungeon_roundtrip, assert_fail_closed_edges};

fn check(spec: &crate::rpg::AreaSpec, area_id: &str, min: u8, max: u8) {
    let readback = area_spec_readback(spec).unwrap();
    assert_eq!(readback.area_id, area_id);
    assert_eq!(readback.source_issue, 1658);
    assert_eq!(readback.area_kind, AreaKind::Dungeon);
    assert_eq!(readback.level_band_min, min);
    assert_eq!(readback.level_band_max, max);
}

#[test]
fn issue1658_keep_of_fhalaugash_matches_facts_and_roundtrips() {
    let spec = keep_of_fhalaugash_area_spec();
    check(&spec, KEEP_OF_FHALAUGASH_AREA_ID, 11, 25);
    assert_dungeon_roundtrip(&spec, "keep-of-fhalaugash");
}

#[test]
fn issue1658_keep_of_the_righteous_matches_facts_and_roundtrips() {
    let spec = keep_of_the_righteous_area_spec();
    check(&spec, KEEP_OF_THE_RIGHTEOUS_AREA_ID, 10, 25);
    assert_dungeon_roundtrip(&spec, "keep-of-the-righteous");
}

#[test]
fn issue1658_ruined_keep_matches_facts_and_roundtrips() {
    let spec = ruined_keep_area_spec();
    check(&spec, RUINED_KEEP_AREA_ID, 10, 25);
    assert_dungeon_roundtrip(&spec, "ruined-keep");
}

#[test]
fn issue1658_lord_tabershaw_keep_matches_facts_and_roundtrips() {
    let spec = lord_tabershaw_keep_area_spec();
    check(&spec, LORD_TABERSHAW_KEEP_AREA_ID, 5, 25);
    assert_dungeon_roundtrip(&spec, "lord-tabershaw-keep");
}

#[test]
fn issue1658_keep_of_ceawlin_matches_facts_and_roundtrips() {
    let spec = keep_of_ceawlin_area_spec();
    check(&spec, KEEP_OF_CEAWLIN_AREA_ID, 15, 30);
    assert_dungeon_roundtrip(&spec, "keep-of-ceawlin");
}

#[test]
fn issue1658_moudrilar_monastery_matches_facts_and_roundtrips() {
    let spec = moudrilar_monastery_area_spec();
    check(&spec, MOUDRILAR_MONASTERY_AREA_ID, 20, 35);
    assert_dungeon_roundtrip(&spec, "moudrilar-monastery");
}

#[test]
fn issue1658_imperial_lands_sewers_matches_facts_and_roundtrips() {
    let spec = imperial_lands_sewers_area_spec();
    check(&spec, IMPERIAL_LANDS_SEWERS_AREA_ID, 20, 25);
    // The Imperial Sewers are modeled as Sewer rooms beneath the demesne.
    assert!(
        spec.rooms
            .iter()
            .any(|r| r.room_kind == crate::rpg::RoomKind::Sewer)
    );
    assert_dungeon_roundtrip(&spec, "imperial-lands-sewers");
}

#[test]
fn issue1658_high_lords_keep_matches_facts_and_roundtrips() {
    let spec = high_lords_keep_area_spec();
    check(&spec, HIGH_LORDS_KEEP_AREA_ID, 35, 51);
    assert_dungeon_roundtrip(&spec, "high-lords-keep");
}

#[test]
fn issue1658_keeps_fail_closed_edges() {
    assert_fail_closed_edges(&high_lords_keep_area_spec());
}
