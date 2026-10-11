// Aggregator for the #1637-1643 Thera city batch (7 cities: Voralian City,
// Hamsah Mu-tazz, Udgaard, Seantryn Modan, Arkham, Tir-Talath, Darsylon).
// Registered as a single #[cfg(test)] module in lib.rs so the crate root stays
// under the <500-line ratchet (#1466/#800 sec.6), mirroring the precedent set
// by race_specs_batch_1001_1018.rs. The per-city fixtures, shared fixture
// helpers, and tests live in sibling files here. #[path] is resolved relative
// to this file's directory (crates/litd-engine/src/rpg/).
#[path = "cities_thera_fixture_arkham.rs"]
mod rpg_cities_thera_fixture_arkham;
#[path = "cities_thera_fixture_darsylon.rs"]
mod rpg_cities_thera_fixture_darsylon;
#[path = "cities_thera_fixture_hamsah.rs"]
mod rpg_cities_thera_fixture_hamsah;
#[path = "cities_thera_fixture_seantryn.rs"]
mod rpg_cities_thera_fixture_seantryn;
#[path = "cities_thera_fixture_support.rs"]
mod rpg_cities_thera_fixture_support;
#[path = "cities_thera_fixture_tirtalath.rs"]
mod rpg_cities_thera_fixture_tirtalath;
#[path = "cities_thera_fixture_udgaard.rs"]
mod rpg_cities_thera_fixture_udgaard;
#[path = "cities_thera_fixture_voralian.rs"]
mod rpg_cities_thera_fixture_voralian;
#[path = "cities_thera_tests_g1.rs"]
mod rpg_cities_thera_tests_g1;
#[path = "cities_thera_tests_g2.rs"]
mod rpg_cities_thera_tests_g2;

/// Every AreaSpec authored by the #1637-1643 city batch (7 cities), for the
/// full-corpus registry (#1704).
pub(crate) fn all_city_area_specs() -> Vec<litd_engine::rpg::AreaSpec> {
    vec![
        rpg_cities_thera_fixture_voralian::voralian_area_spec(),
        rpg_cities_thera_fixture_hamsah::hamsah_area_spec(),
        rpg_cities_thera_fixture_udgaard::udgaard_area_spec(),
        rpg_cities_thera_fixture_seantryn::seantryn_area_spec(),
        rpg_cities_thera_fixture_arkham::arkham_area_spec(),
        rpg_cities_thera_fixture_tirtalath::tir_talath_area_spec(),
        rpg_cities_thera_fixture_darsylon::darsylon_area_spec(),
    ]
}
