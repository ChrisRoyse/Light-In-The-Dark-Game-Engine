// Aggregator for the #1649-#1660 Thera wilderness/region batch (~57 places
// across the Underdark, western forests, plains/hills, mountains, desert,
// swamps/wetlands, seas, rivers/lochs, and The Past). Registered as a single
// #[cfg(test)] module in lib.rs so the crate root stays under the <500-line
// ratchet (#1466/#800 sec.6); the per-cluster fixtures (each issue split into
// a/b halves to also stay under the ratchet after rustfmt expansion), the
// shared authoring support, the world-graph registry, and the tests live in
// sibling files here. #[path] is resolved relative to this file's directory
// (crates/litd-engine/src/rpg/). Mirrors the shape of
// `race_specs_batch_1001_1018.rs`.
#[path = "regions_thera_fixture_desert.rs"]
mod regions_thera_fixture_desert;
#[path = "regions_thera_fixture_desert_b.rs"]
mod regions_thera_fixture_desert_b;
#[path = "regions_thera_fixture_forests_a.rs"]
mod regions_thera_fixture_forests_a;
#[path = "regions_thera_fixture_forests_b.rs"]
mod regions_thera_fixture_forests_b;
#[path = "regions_thera_fixture_mountains.rs"]
mod regions_thera_fixture_mountains;
#[path = "regions_thera_fixture_mountains_b.rs"]
mod regions_thera_fixture_mountains_b;
#[path = "regions_thera_fixture_plains.rs"]
mod regions_thera_fixture_plains;
#[path = "regions_thera_fixture_plains_b.rs"]
mod regions_thera_fixture_plains_b;
#[path = "regions_thera_fixture_seas.rs"]
mod regions_thera_fixture_seas;
#[path = "regions_thera_fixture_seas_b.rs"]
mod regions_thera_fixture_seas_b;
#[path = "regions_thera_fixture_the_past.rs"]
mod regions_thera_fixture_the_past;
#[path = "regions_thera_fixture_the_past_b.rs"]
mod regions_thera_fixture_the_past_b;
#[path = "regions_thera_fixture_underdark.rs"]
mod regions_thera_fixture_underdark;
#[path = "regions_thera_fixture_underdark_b.rs"]
mod regions_thera_fixture_underdark_b;
#[path = "regions_thera_fixture_wetlands.rs"]
mod regions_thera_fixture_wetlands;
#[path = "regions_thera_fixture_wetlands_b.rs"]
mod regions_thera_fixture_wetlands_b;
#[path = "regions_thera_registry.rs"]
mod regions_thera_registry;
#[path = "regions_thera_support.rs"]
mod regions_thera_support;
#[path = "regions_thera_tests_persist.rs"]
mod regions_thera_tests_persist;
#[path = "regions_thera_tests_readback.rs"]
mod regions_thera_tests_readback;
#[path = "regions_thera_tests_registry.rs"]
mod regions_thera_tests_registry;

/// Every AreaSpec authored by the #1649-1660 wilderness/region batch (~57
/// places), for the full-corpus registry (#1704).
pub(crate) fn all_region_area_specs() -> Vec<litd_engine::rpg::AreaSpec> {
    regions_thera_registry::all_region_specs()
}
