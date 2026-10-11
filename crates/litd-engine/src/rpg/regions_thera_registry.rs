// World-graph registry (#1632) for the #1649-#1660 Thera wilderness/region
// batch: the union of every area id this batch either authors or references,
// plus every authored AreaSpec, for referential-integrity and parameterized
// tests. Mirrors `galadon_known_area_ids()`'s role in
// `area_specs_test_fixture.rs`, generalized across ~57 areas split across 17
// fixture files (a/b halves per issue cluster to respect the <500-line
// ratchet, #1466/#800 sec.6).

use std::collections::BTreeSet;

use crate::rpg::{AreaSpec, GALADON_AREA_ID};

use super::regions_thera_fixture_desert::{all_desert_a_specs, desert_a_area_ids};
use super::regions_thera_fixture_desert_b::{all_desert_b_specs, desert_b_area_ids};
use super::regions_thera_fixture_forests_a::{all_forests_a_specs, forests_a_area_ids};
use super::regions_thera_fixture_forests_b::{all_forests_b_specs, forests_b_area_ids};
use super::regions_thera_fixture_mountains::{all_mountains_a_specs, mountains_a_area_ids};
use super::regions_thera_fixture_mountains_b::{all_mountains_b_specs, mountains_b_area_ids};
use super::regions_thera_fixture_plains::{all_plains_a_specs, plains_a_area_ids};
use super::regions_thera_fixture_plains_b::{all_plains_b_specs, plains_b_area_ids};
use super::regions_thera_fixture_seas::{all_seas_a_specs, seas_a_area_ids};
use super::regions_thera_fixture_seas_b::{all_seas_b_specs, seas_b_area_ids};
use super::regions_thera_fixture_the_past::{all_the_past_a_specs, the_past_a_area_ids};
use super::regions_thera_fixture_the_past_b::{all_the_past_b_specs, the_past_b_area_ids};
use super::regions_thera_fixture_underdark::{all_underdark_a_specs, underdark_a_area_ids};
use super::regions_thera_fixture_underdark_b::{all_underdark_b_specs, underdark_b_area_ids};
use super::regions_thera_fixture_wetlands::{all_wetlands_a_specs, wetlands_a_area_ids};
use super::regions_thera_fixture_wetlands_b::{all_wetlands_b_specs, wetlands_b_area_ids};

/// Every area id this batch authors or references as a connection target,
/// including the not-yet-authored satellite anchors (Cragstone, Hamsah
/// Mu'tazz) and the always-known framework hub (Galadon).
pub(crate) fn known_area_ids() -> BTreeSet<&'static str> {
    let mut ids = BTreeSet::new();
    ids.insert(GALADON_AREA_ID);
    ids.extend(underdark_a_area_ids());
    ids.extend(underdark_b_area_ids());
    ids.extend(forests_a_area_ids());
    ids.extend(forests_b_area_ids());
    ids.extend(plains_a_area_ids());
    ids.extend(plains_b_area_ids());
    ids.extend(mountains_a_area_ids());
    ids.extend(mountains_b_area_ids());
    ids.extend(desert_a_area_ids());
    ids.extend(desert_b_area_ids());
    ids.extend(wetlands_a_area_ids());
    ids.extend(wetlands_b_area_ids());
    ids.extend(seas_a_area_ids());
    ids.extend(seas_b_area_ids());
    ids.extend(the_past_a_area_ids());
    ids.extend(the_past_b_area_ids());
    ids
}

/// Every AreaSpec authored by this batch (~57 places across #1649-#1660,
/// excluding #1648/#1659 which are owned by sibling batches).
pub(crate) fn all_region_specs() -> Vec<AreaSpec> {
    let mut specs = Vec::new();
    specs.extend(all_underdark_a_specs());
    specs.extend(all_underdark_b_specs());
    specs.extend(all_forests_a_specs());
    specs.extend(all_forests_b_specs());
    specs.extend(all_plains_a_specs());
    specs.extend(all_plains_b_specs());
    specs.extend(all_mountains_a_specs());
    specs.extend(all_mountains_b_specs());
    specs.extend(all_desert_a_specs());
    specs.extend(all_desert_b_specs());
    specs.extend(all_wetlands_a_specs());
    specs.extend(all_wetlands_b_specs());
    specs.extend(all_seas_a_specs());
    specs.extend(all_seas_b_specs());
    specs.extend(all_the_past_a_specs());
    specs.extend(all_the_past_b_specs());
    specs
}

/// The set of source issues this batch is responsible for (used by the
/// readback test to assert every spec's `source_issue` is one of these).
pub(crate) fn batch_source_issues() -> BTreeSet<u32> {
    [1649, 1650, 1651, 1652, 1653, 1654, 1655, 1656, 1660]
        .into_iter()
        .collect()
}
