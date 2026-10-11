// Aggregator for the #1001-#1018 race-spec batch (18 races). Registered as a single
// #[cfg(test)] module in lib.rs so the crate root stays under the <500-line ratchet
// (#1466/#800 sec.6); the per-group fixtures, the shared test support, and the per-group
// tests live in sibling files here. #[path] is resolved relative to this file's directory
// (crates/litd-engine/src/rpg/).
#[path = "race_specs_fixture_g1.rs"]
mod rpg_race_specs_fixture_g1;
#[path = "race_specs_fixture_g2.rs"]
mod rpg_race_specs_fixture_g2;
#[path = "race_specs_fixture_g3.rs"]
mod rpg_race_specs_fixture_g3;
#[path = "race_specs_fixture_g4.rs"]
mod rpg_race_specs_fixture_g4;
#[path = "race_specs_fixture_g5.rs"]
mod rpg_race_specs_fixture_g5;
#[path = "race_specs_fixture_g6.rs"]
mod rpg_race_specs_fixture_g6;
// #1019 Centaur lands in its own group so the g6 test file stays under the <500-line ratchet.
#[path = "race_specs_fixture_g7.rs"]
mod rpg_race_specs_fixture_g7;
#[path = "race_specs_tests_g1.rs"]
mod rpg_race_specs_tests_g1;
#[path = "race_specs_tests_g2.rs"]
mod rpg_race_specs_tests_g2;
#[path = "race_specs_tests_g3.rs"]
mod rpg_race_specs_tests_g3;
#[path = "race_specs_tests_g4.rs"]
mod rpg_race_specs_tests_g4;
#[path = "race_specs_tests_g5.rs"]
mod rpg_race_specs_tests_g5;
#[path = "race_specs_tests_g6.rs"]
mod rpg_race_specs_tests_g6;
#[path = "race_specs_tests_g7.rs"]
mod rpg_race_specs_tests_g7;
#[path = "race_specs_tests_support.rs"]
mod rpg_race_specs_tests_support;

/// Every #1001-#1019 race spec (Half-Elf through Centaur), aggregated from the
/// per-group fixtures. Exposed `pub(crate)` so the #1631 class-access corpus
/// suite validates the SAME authored specs the per-race tests use, with no
/// parallel roster to drift.
pub(crate) fn all_batch_race_specs() -> Vec<crate::rpg::RaceSpec> {
    vec![
        rpg_race_specs_fixture_g1::half_elf_race_spec(),
        rpg_race_specs_fixture_g1::ashkin_mixed_race_spec(),
        rpg_race_specs_fixture_g1::ashkin_deep_race_spec(),
        rpg_race_specs_fixture_g2::wood_elf_race_spec(),
        rpg_race_specs_fixture_g2::dwarf_race_spec(),
        rpg_race_specs_fixture_g2::onyx_kin_race_spec(),
        rpg_race_specs_fixture_g3::gnome_race_spec(),
        rpg_race_specs_fixture_g3::deep_kin_race_spec(),
        rpg_race_specs_fixture_g3::orc_race_spec(),
        rpg_race_specs_fixture_g4::goblin_race_spec(),
        rpg_race_specs_fixture_g4::minotaur_race_spec(),
        rpg_race_specs_fixture_g4::storm_giant_race_spec(),
        rpg_race_specs_fixture_g5::fire_giant_race_spec(),
        rpg_race_specs_fixture_g5::frost_giant_race_spec(),
        rpg_race_specs_fixture_g5::cloud_giant_race_spec(),
        rpg_race_specs_fixture_g6::sylphkin_race_spec(),
        rpg_race_specs_fixture_g6::felisar_race_spec(),
        rpg_race_specs_fixture_g6::saur_race_spec(),
        rpg_race_specs_fixture_g7::centaur_race_spec(),
    ]
}
