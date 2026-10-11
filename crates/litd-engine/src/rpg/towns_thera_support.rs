// Shared helpers + area-id registry for the #1644-#1647 Thera town/settlement batch.
// Room/connection builders mirror the #1636 Galadon fixture but take an explicit
// `to_area_id` so one set of helpers serves every place. All area-id constants live
// here so the fixtures and the tests share one source of truth (and the registry
// referential-integrity check can be run against the full known set, #1632).
use std::collections::BTreeSet;

use crate::{
    EngineError,
    calyx_content::EngineCalyxContentStore,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
};
use litd_engine::rpg::{
    AreaAccessRule, AreaConnection, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile,
    AreaResetProfile, AreaRoom, AreaSpec, AreaVisualContract, ConnectionDirection,
    ConnectionDiscovery, GALADON_AREA_ID, RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind,
    TravelKind,
};

/// Every batch spec transcribes CF-factual topology only; name/lore is originalized
/// per #984.
pub const SOURCE_LABEL: &str = "CF factual topology, ranking rosters, and reset cadence only";

// --- #1644 northwest guild towns -------------------------------------------
pub const HILLCREST_AREA_ID: &str = "area.thera.hillcrest";
pub const DAGDAN_AREA_ID: &str = "area.thera.dagdan";
pub const BLACKCLAW_AREA_ID: &str = "area.thera.blackclaw";
pub const AYR_TRINIL_AREA_ID: &str = "area.thera.ayr_trinil";
pub const CRAGSTONE_AREA_ID: &str = "area.thera.cragstone";
pub const AKAN_AREA_ID: &str = "area.thera.akan";
pub const CASTLE_OF_AKAN_AREA_ID: &str = "area.thera.castle_of_akan";

// --- #1645 central & eastern villages --------------------------------------
pub const EVERMOON_HOLLOW_AREA_ID: &str = "area.thera.evermoon_hollow";
pub const ALDEVARI_AREA_ID: &str = "area.thera.aldevari";
// Reused id: resolves Galadon's forward-stub edge to Copeham Inn.
pub const COPEHAM_INN_AREA_ID: &str = "area.thera.galadon.copeham_inn";
pub const INN_OF_THE_ETERNAL_STAR_AREA_ID: &str = "area.thera.inn_of_the_eternal_star";
pub const BALATOR_AREA_ID: &str = "area.thera.balator";
// Reused id: resolves Galadon's forward-stub edge to the Vale.
pub const VALE_OF_ARENDYL_AREA_ID: &str = "area.thera.vale_of_arendyl";

// --- #1646 far-east & south settlements ------------------------------------
pub const BAROVIA_AREA_ID: &str = "area.thera.barovia";
pub const TETH_AZELETH_AREA_ID: &str = "area.thera.teth_azeleth";
pub const BASILICA_AREA_ID: &str = "area.thera.basilica";
pub const DRA_MELKHUR_AREA_ID: &str = "area.thera.dra_melkhur";
pub const ISLE_OF_CORTE_AREA_ID: &str = "area.thera.isle_of_corte";
pub const MANSION_OF_VDRAMIR_AREA_ID: &str = "area.thera.mansion_of_vdramir";

// --- #1647 racial homelands ------------------------------------------------
pub const GRINNING_SKULL_VILLAGE_AREA_ID: &str = "area.thera.grinning_skull_village";
// Reused id: resolves Galadon's forward-stub edge to the Goblin Village.
pub const GOBLIN_VILLAGE_AREA_ID: &str = "area.thera.galadon.goblin_village";
pub const GOBLIN_OUTPOST_AREA_ID: &str = "area.thera.goblin_outpost";
pub const YSIGRATH_AREA_ID: &str = "area.thera.ysigrath";
pub const MAL_TRAKIS_AREA_ID: &str = "area.thera.mal_trakis";
pub const HALFLING_LANDS_AREA_ID: &str = "area.thera.halfling_lands";
pub const MORTORN_AREA_ID: &str = "area.thera.mortorn";
pub const ARIAL_SKY_SANCTUM_AREA_ID: &str = "area.thera.arial_sky_sanctum";

// Forward-stub neighbours named in #1632 but authored by other/earlier issues.
// Listed in the known set so world-graph edges to them pass referential integrity.
// #1704 registry fix: no top-level "area.thera.underdark" is authored; the
// Underdark is authored as discrete sub-areas by the #1660 batch. The Cragstone
// descent lands in the Underdark Tunnels (the authored entry sub-area).
pub const UNDERDARK_AREA_ID: &str = "area.thera.underdark.tunnels";
pub const ARKHAM_AREA_ID: &str = "area.thera.arkham";
pub const VORALIAN_AREA_ID: &str = "area.thera.voralian";
pub const RUINS_OF_MAETHIEN_AREA_ID: &str = "area.thera.ruins_of_maethien";
pub const FIELDS_OF_BALATORN_AREA_ID: &str = "area.thera.fields_of_balatorn";
pub const MINES_OF_ZAKIIM_AREA_ID: &str = "area.thera.mines_of_zakiim";
pub const THE_WASTES_AREA_ID: &str = "area.thera.the_wastes";

/// Every area id this batch authors (27 places).
pub const AUTHORED_TOWN_AREA_IDS: [&str; 27] = [
    HILLCREST_AREA_ID,
    DAGDAN_AREA_ID,
    BLACKCLAW_AREA_ID,
    AYR_TRINIL_AREA_ID,
    CRAGSTONE_AREA_ID,
    AKAN_AREA_ID,
    CASTLE_OF_AKAN_AREA_ID,
    EVERMOON_HOLLOW_AREA_ID,
    ALDEVARI_AREA_ID,
    COPEHAM_INN_AREA_ID,
    INN_OF_THE_ETERNAL_STAR_AREA_ID,
    BALATOR_AREA_ID,
    VALE_OF_ARENDYL_AREA_ID,
    BAROVIA_AREA_ID,
    TETH_AZELETH_AREA_ID,
    BASILICA_AREA_ID,
    DRA_MELKHUR_AREA_ID,
    ISLE_OF_CORTE_AREA_ID,
    MANSION_OF_VDRAMIR_AREA_ID,
    GRINNING_SKULL_VILLAGE_AREA_ID,
    GOBLIN_VILLAGE_AREA_ID,
    GOBLIN_OUTPOST_AREA_ID,
    YSIGRATH_AREA_ID,
    MAL_TRAKIS_AREA_ID,
    HALFLING_LANDS_AREA_ID,
    MORTORN_AREA_ID,
    ARIAL_SKY_SANCTUM_AREA_ID,
];

const FORWARD_STUB_AREA_IDS: [&str; 8] = [
    GALADON_AREA_ID,
    UNDERDARK_AREA_ID,
    ARKHAM_AREA_ID,
    VORALIAN_AREA_ID,
    RUINS_OF_MAETHIEN_AREA_ID,
    FIELDS_OF_BALATORN_AREA_ID,
    MINES_OF_ZAKIIM_AREA_ID,
    THE_WASTES_AREA_ID,
];

/// The full set of area ids any batch connection may target (#1632 world graph):
/// every authored place plus the forward-stub neighbours.
pub fn known_area_ids() -> BTreeSet<&'static str> {
    AUTHORED_TOWN_AREA_IDS
        .into_iter()
        .chain(FORWARD_STUB_AREA_IDS)
        .collect()
}

/// Compact per-place expectation table used by the data-driven readback tests.
pub struct TownExpectation {
    pub area_id: &'static str,
    pub source_issue: u32,
    pub area_kind: AreaKind,
    pub is_hometown_option: bool,
    pub has_recall_point: bool,
    pub min_rooms: usize,
}

/// Every authored AreaSpec paired with its expected facts, in issue order.
pub fn all_town_specs() -> Vec<(AreaSpec, TownExpectation)> {
    let mut specs = Vec::new();
    super::towns_thera_fixture_nw_a::push_specs(&mut specs);
    super::towns_thera_fixture_nw_b::push_specs(&mut specs);
    super::towns_thera_fixture_central_a::push_specs(&mut specs);
    super::towns_thera_fixture_central_b::push_specs(&mut specs);
    super::towns_thera_fixture_east_a::push_specs(&mut specs);
    super::towns_thera_fixture_east_b::push_specs(&mut specs);
    super::towns_thera_fixture_homelands_a::push_specs(&mut specs);
    super::towns_thera_fixture_homelands_b::push_specs(&mut specs);
    specs
}

/// canon_issues = the mandatory {987, 1632, source} plus any extra ties, in a
/// stable order. `validate_area_spec` requires the source issue be present.
pub fn canon(source_issue: u32, extra: &[u32]) -> Vec<u32> {
    let mut issues = vec![987, 1598, 1632, source_issue];
    issues.extend_from_slice(extra);
    issues
}

pub fn wiki(slug: &str) -> String {
    format!("https://wiki.carrionfields.net/area/{slug}")
}

pub fn band(min_level: u8, max_level: u8) -> AreaLevelBand {
    AreaLevelBand {
        min_level,
        max_level,
    }
}

pub fn resets(reset_id: &str, interval_minutes: u16) -> AreaResetProfile {
    AreaResetProfile {
        reset_id: reset_id.to_string(),
        interval_minutes,
        clears_unlooted_corpses: true,
        resets_doors_and_locks: true,
    }
}

pub fn repop(
    repop_id: &str,
    baseline_population_budget: u16,
    repop_interval_minutes: u16,
    max_pressure_tier: u8,
) -> AreaRepopProfile {
    AreaRepopProfile {
        repop_id: repop_id.to_string(),
        baseline_population_budget,
        repop_interval_minutes,
        max_pressure_tier,
    }
}

pub fn recall(
    is_hometown_option: bool,
    recall_room_id: Option<&str>,
    has_academy: bool,
    hometown_access: AreaAccessRule,
) -> AreaRecallProfile {
    AreaRecallProfile {
        is_hometown_option,
        recall_room_id: recall_room_id.map(str::to_string),
        has_academy,
        hometown_access,
    }
}

pub fn access_any() -> AreaAccessRule {
    AreaAccessRule::Any
}

pub fn access_restricted(values: &[&str]) -> AreaAccessRule {
    AreaAccessRule::Restricted {
        values: values.iter().map(|value| value.to_string()).collect(),
    }
}

pub fn visual(
    tileset_id: &str,
    model_id: &str,
    required_props: &[&str],
    silhouette: &str,
    material_notes: &str,
) -> AreaVisualContract {
    AreaVisualContract {
        renderer: "Rust/wgpu".to_string(),
        tileset_id: tileset_id.to_string(),
        model_id: model_id.to_string(),
        required_props: required_props.iter().map(|p| p.to_string()).collect(),
        silhouette: silhouette.to_string(),
        material_notes: material_notes.to_string(),
    }
}

pub fn notes(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| line.to_string()).collect()
}

/// Assemble a full AreaSpec from the varying scalars plus prebuilt rooms,
/// connections, recall, and visual. Keeps each fixture free of boilerplate while
/// leaving every CF-factual value explicit at the call site.
#[allow(clippy::too_many_arguments)]
pub fn area_spec(
    area_id: &str,
    display_name: &str,
    area_kind: AreaKind,
    source_issue: u32,
    canon_extra: &[u32],
    mechanics_slug: &str,
    level: (u8, u8),
    summary: &str,
    rooms: Vec<AreaRoom>,
    connections: Vec<AreaConnection>,
    resets_profile: AreaResetProfile,
    repop_profile: AreaRepopProfile,
    recall_profile: AreaRecallProfile,
    visual_contract: AreaVisualContract,
    implementation_notes: &[&str],
) -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: area_id.to_string(),
        display_name: display_name.to_string(),
        player_facing_canon_name: display_name.to_string(),
        area_kind,
        source_issue,
        canon_issues: canon(source_issue, canon_extra),
        mechanics_source_url: wiki(mechanics_slug),
        source_history_label: SOURCE_LABEL.to_string(),
        summary: summary.to_string(),
        level_band: band(level.0, level.1),
        rooms,
        connections,
        resets: resets_profile,
        repop: repop_profile,
        recall: recall_profile,
        visual: visual_contract,
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: notes(implementation_notes),
    }
}

pub fn expect(
    area_id: &'static str,
    source_issue: u32,
    area_kind: AreaKind,
    is_hometown_option: bool,
    has_recall_point: bool,
    min_rooms: usize,
) -> TownExpectation {
    TownExpectation {
        area_id,
        source_issue,
        area_kind,
        is_hometown_option,
        has_recall_point,
        min_rooms,
    }
}

/// Fresh Calyx content store backed by an auto-cleaning temp vault (#1535).
pub fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1644-1647-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}

pub fn assert_code(result: Result<(), EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

pub fn room(
    room_id: &str,
    display_name: &str,
    room_kind: RoomKind,
    summary: &str,
    concealment: RoomConcealment,
    npcs: &[&str],
) -> AreaRoom {
    AreaRoom {
        room_id: room_id.to_string(),
        display_name: display_name.to_string(),
        room_kind,
        summary: summary.to_string(),
        concealment,
        npc_population_ids: npcs.iter().map(|id| id.to_string()).collect(),
    }
}

/// A visible internal move to another room within the same area.
pub fn internal(
    connection_id: &str,
    from_room_id: &str,
    area_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
) -> AreaConnection {
    connection(
        connection_id,
        from_room_id,
        area_id,
        Some(to_room_id),
        direction,
        travel_kind,
        ConnectionDiscovery::Visible,
    )
}

/// A concealed/hidden internal move within the same area (#944).
pub fn hidden_internal(
    connection_id: &str,
    from_room_id: &str,
    area_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    connection(
        connection_id,
        from_room_id,
        area_id,
        Some(to_room_id),
        direction,
        travel_kind,
        discovery,
    )
}

/// A world-graph edge out to another area (#1632); `to_room_id` is None.
pub fn external(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    connection(
        connection_id,
        from_room_id,
        to_area_id,
        None,
        direction,
        travel_kind,
        discovery,
    )
}

#[allow(clippy::too_many_arguments)]
fn connection(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    to_room_id: Option<&str>,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: to_area_id.to_string(),
        to_room_id: to_room_id.map(str::to_string),
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}
