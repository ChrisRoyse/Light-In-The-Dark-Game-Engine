// Aggregator for the #1644-#1647 Thera town/settlement batch (27 AreaSpecs across the
// NW guild towns, central & eastern villages, far-east & south settlements, and racial
// homelands). Registered as a single #[cfg(test)] module in lib.rs so the crate root
// stays under the <500-line ratchet (#1466/#800 sec.6); the shared support, the
// per-group fixtures, and the per-concern tests live in sibling files here. #[path] is
// resolved relative to this file's directory (crates/litd-engine/src/rpg/).
#[path = "towns_thera_support.rs"]
mod towns_thera_support;

#[path = "towns_thera_fixture_central_a.rs"]
mod towns_thera_fixture_central_a;
#[path = "towns_thera_fixture_central_b.rs"]
mod towns_thera_fixture_central_b;
#[path = "towns_thera_fixture_east_a.rs"]
mod towns_thera_fixture_east_a;
#[path = "towns_thera_fixture_east_b.rs"]
mod towns_thera_fixture_east_b;
#[path = "towns_thera_fixture_homelands_a.rs"]
mod towns_thera_fixture_homelands_a;
#[path = "towns_thera_fixture_homelands_b.rs"]
mod towns_thera_fixture_homelands_b;
#[path = "towns_thera_fixture_nw_a.rs"]
mod towns_thera_fixture_nw_a;
#[path = "towns_thera_fixture_nw_b.rs"]
mod towns_thera_fixture_nw_b;

#[path = "towns_thera_tests_persist.rs"]
mod towns_thera_tests_persist;
#[path = "towns_thera_tests_readback.rs"]
mod towns_thera_tests_readback;
#[path = "towns_thera_tests_registry.rs"]
mod towns_thera_tests_registry;

/// Every AreaSpec authored by the #1644-1647 town batch (27 places), for the
/// full-corpus registry (#1704).
pub(crate) fn all_town_area_specs() -> Vec<litd_engine::rpg::AreaSpec> {
    towns_thera_support::all_town_specs()
        .into_iter()
        .map(|(spec, _)| spec)
        .collect()
}
