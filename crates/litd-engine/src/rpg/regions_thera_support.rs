// Shared authoring + test support for the #1649-#1660 Thera wilderness/region
// batch. Mirrors the shape of `area_specs_test_fixture.rs`'s `room`/`internal`/
// `external` helpers plus `race_specs_tests_support.rs`'s `assert_code`/
// `content_store`, but generalized so every fixture file in this batch can
// build an `AreaSpec` in ~15-25 lines instead of the ~190 lines Galadon's
// one-off fixture takes. This file is NOT the framework (that stays in
// `area_specs*.rs`, off limits per the #987 concurrency rule); it is purely a
// batch-local convenience layer over the public `crate::rpg` types.

use crate::rpg::{
    AreaAccessRule, AreaConnection, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile,
    AreaResetProfile, AreaRoom, AreaSpec, AreaVisualContract, ConnectionDirection,
    ConnectionDiscovery, RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind, TravelKind,
};

/// Everything needed to build one wilderness/region `AreaSpec`. Recall defaults
/// to "not a hometown, no recall point" for every area built this way (correct
/// for wilderness per the task brief); the two CF hometown exceptions in this
/// batch (Arin-gird duergar, the Svirfneblin enclave) patch `.recall` on the
/// returned spec directly rather than complicating this blueprint.
pub(crate) struct AreaBlueprint<'a> {
    pub area_id: &'a str,
    pub display_name: &'a str,
    pub area_kind: AreaKind,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub wiki_slug: &'a str,
    pub source_history_label: &'a str,
    pub summary: &'a str,
    pub level_band: (u8, u8),
    pub rooms: Vec<AreaRoom>,
    pub connections: Vec<AreaConnection>,
    pub reset_id: &'a str,
    pub reset_minutes: u16,
    pub clears_corpses: bool,
    pub resets_doors: bool,
    pub repop_id: &'a str,
    pub repop_budget: u16,
    pub repop_minutes: u16,
    pub pressure_tier: u8,
    pub tileset_id: &'a str,
    pub model_id: &'a str,
    pub props: Vec<&'a str>,
    pub silhouette: &'a str,
    pub material_notes: &'a str,
    pub notes: Vec<&'a str>,
}

pub(crate) fn build_wild_area(bp: AreaBlueprint) -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: bp.area_id.to_string(),
        display_name: bp.display_name.to_string(),
        player_facing_canon_name: bp.display_name.to_string(),
        area_kind: bp.area_kind,
        source_issue: bp.source_issue,
        canon_issues: bp.canon_issues,
        mechanics_source_url: format!("https://wiki.carrionfields.net/area/{}", bp.wiki_slug),
        source_history_label: bp.source_history_label.to_string(),
        summary: bp.summary.to_string(),
        level_band: AreaLevelBand {
            min_level: bp.level_band.0,
            max_level: bp.level_band.1,
        },
        rooms: bp.rooms,
        connections: bp.connections,
        resets: AreaResetProfile {
            reset_id: bp.reset_id.to_string(),
            interval_minutes: bp.reset_minutes,
            clears_unlooted_corpses: bp.clears_corpses,
            resets_doors_and_locks: bp.resets_doors,
        },
        repop: AreaRepopProfile {
            repop_id: bp.repop_id.to_string(),
            baseline_population_budget: bp.repop_budget,
            repop_interval_minutes: bp.repop_minutes,
            max_pressure_tier: bp.pressure_tier,
        },
        recall: AreaRecallProfile {
            is_hometown_option: false,
            recall_room_id: None,
            has_academy: false,
            hometown_access: AreaAccessRule::Any,
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: bp.tileset_id.to_string(),
            model_id: bp.model_id.to_string(),
            required_props: bp.props.into_iter().map(|s| s.to_string()).collect(),
            silhouette: bp.silhouette.to_string(),
            material_notes: bp.material_notes.to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: bp.notes.into_iter().map(|s| s.to_string()).collect(),
    }
}

pub(crate) fn room(
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

/// Generic connection builder. `to_room` is `Some` for an internal move within
/// this area, `None` for a world-graph edge to another area (#1632). Never sets
/// `locked_by_ref`; none of this batch's topology needs a lock reference.
#[allow(clippy::too_many_arguments)]
pub(crate) fn conn(
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

pub(crate) fn assert_code(result: Result<(), crate::EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

pub(crate) fn content_store(
    label: &str,
) -> (
    crate::calyx_content::EngineCalyxContentStore,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1649-1660-{label}"));
    let database = crate::calyx_persistence::EngineCalyxDatabase::open(
        crate::calyx_persistence::EngineCalyxDatabaseConfig::for_vault(&vault_dir),
    )
    .unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
