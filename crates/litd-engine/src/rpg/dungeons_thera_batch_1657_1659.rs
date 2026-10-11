// Aggregator for the #1657-#1659 Thera dungeon batch (parent #987 world build,
// world-graph #1632): 20 AreaSpecs across crypts/undead (#1657), keeps/citadels
// (#1658), and towers/arcane sites (#1659). Registered as a single #[cfg(test)]
// module in lib.rs so the crate root stays under the <500-line ratchet (#1466/#800
// sec.6); the per-group fixtures, shared support, test support, and per-group tests
// live in sibling files here. #[path] resolves relative to this file's directory
// (crates/litd-engine/src/rpg/).
#[path = "dungeons_thera_fixture_crypts.rs"]
mod rpg_dungeons_thera_fixture_crypts;
#[path = "dungeons_thera_fixture_keeps_a.rs"]
mod rpg_dungeons_thera_fixture_keeps_a;
#[path = "dungeons_thera_fixture_keeps_b.rs"]
mod rpg_dungeons_thera_fixture_keeps_b;
#[path = "dungeons_thera_fixture_towers_a.rs"]
mod rpg_dungeons_thera_fixture_towers_a;
#[path = "dungeons_thera_fixture_towers_b.rs"]
mod rpg_dungeons_thera_fixture_towers_b;
#[path = "dungeons_thera_support.rs"]
mod rpg_dungeons_thera_support;
#[path = "dungeons_thera_test_support.rs"]
mod rpg_dungeons_thera_test_support;
#[path = "dungeons_thera_tests_crypts.rs"]
mod rpg_dungeons_thera_tests_crypts;
#[path = "dungeons_thera_tests_keeps.rs"]
mod rpg_dungeons_thera_tests_keeps;
#[path = "dungeons_thera_tests_towers.rs"]
mod rpg_dungeons_thera_tests_towers;

/// Every AreaSpec authored by the #1657-1659 dungeon batch (20 places), for the
/// full-corpus registry (#1704).
pub(crate) fn all_dungeon_area_specs() -> Vec<litd_engine::rpg::AreaSpec> {
    vec![
        rpg_dungeons_thera_fixture_crypts::graveyard_area_spec(),
        rpg_dungeons_thera_fixture_crypts::mausoleum_area_spec(),
        rpg_dungeons_thera_fixture_crypts::forgotten_crypts_area_spec(),
        rpg_dungeons_thera_fixture_crypts::eil_shaeria_area_spec(),
        rpg_dungeons_thera_fixture_crypts::ruins_of_maethien_area_spec(),
        rpg_dungeons_thera_fixture_keeps_a::keep_of_fhalaugash_area_spec(),
        rpg_dungeons_thera_fixture_keeps_a::keep_of_the_righteous_area_spec(),
        rpg_dungeons_thera_fixture_keeps_a::ruined_keep_area_spec(),
        rpg_dungeons_thera_fixture_keeps_a::lord_tabershaw_keep_area_spec(),
        rpg_dungeons_thera_fixture_keeps_b::keep_of_ceawlin_area_spec(),
        rpg_dungeons_thera_fixture_keeps_b::moudrilar_monastery_area_spec(),
        rpg_dungeons_thera_fixture_keeps_b::high_lords_keep_area_spec(),
        rpg_dungeons_thera_fixture_keeps_b::imperial_lands_sewers_area_spec(),
        rpg_dungeons_thera_fixture_towers_a::tower_of_high_sorcery_area_spec(),
        rpg_dungeons_thera_fixture_towers_a::tower_of_dragons_area_spec(),
        rpg_dungeons_thera_fixture_towers_a::octagonal_tower_area_spec(),
        rpg_dungeons_thera_fixture_towers_a::tower_of_trothon_area_spec(),
        rpg_dungeons_thera_fixture_towers_b::elemental_temple_area_spec(),
        rpg_dungeons_thera_fixture_towers_b::mines_of_zakiim_area_spec(),
        rpg_dungeons_thera_fixture_towers_b::ruins_of_delar_tol_area_spec(),
    ]
}
