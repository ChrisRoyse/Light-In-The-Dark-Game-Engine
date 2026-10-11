use crate::{
    render::ui::MapOverlayPolicyMode,
    rpg::{
        RPG_MAP_ACCESS_SCHEMA, RpgMapAccessState, RpgMapFeature, RpgMapFeatureKind, RpgMapFidelity,
        RpgMapOpenRequest, RpgMapRuntimeState, RpgMapSource, RpgMapSourceKind, RpgMapSourceStatus,
        read_rpg_map_runtime_state, remove_rpg_map_source, resolve_rpg_map_access,
        write_rpg_map_runtime_state,
    },
};

#[test]
fn issue1445_fresh_rpg_session_has_no_map_surface() {
    let state = RpgMapRuntimeState::fresh("player.ashen.new");
    let plan = resolve_rpg_map_access(&state, &RpgMapOpenRequest::no_map_screen()).unwrap();

    assert_eq!(plan.readback.access, RpgMapAccessState::NoMapAccess);
    assert_eq!(
        plan.readback.policy.mode,
        MapOverlayPolicyMode::RuntimeRpgNoMapAccess
    );
    assert!(!plan.readback.map_surface_visible);
    assert!(plan.readback.gps_indicators_disabled);
    assert!(plan.markers.is_empty());
}

#[test]
fn issue944_owned_maps_do_not_create_always_on_hud_map() {
    let state = state_with_sources(vec![cartographer_map(), found_map()]);
    let plan = resolve_rpg_map_access(&state, &RpgMapOpenRequest::no_map_screen()).unwrap();

    assert_eq!(plan.readback.access, RpgMapAccessState::NoMapAccess);
    assert_eq!(
        plan.readback.policy.mode,
        MapOverlayPolicyMode::RuntimeRpgNoMapAccess
    );
    assert_eq!(plan.readback.source_count, 2);
    assert_eq!(plan.readback.available_source_count, 2);
    assert!(!plan.readback.map_surface_visible);
    assert!(plan.readback.active_source_id.is_none());
    assert!(plan.readback.scope_id.is_none());
    assert!(plan.readback.source_kind.is_none());
    assert!(plan.readback.policy.source_id.is_none());
    assert!(plan.readback.policy.scope_id.is_none());
    assert!(plan.readback.gps_indicators_disabled);
    assert!(plan.markers.is_empty());
}

#[test]
fn issue1445_cartographer_purchase_grants_scoped_map_without_gps() {
    let state = state_with_sources(vec![cartographer_map()]);
    let plan = resolve_rpg_map_access(
        &state,
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )
    .unwrap();

    assert_eq!(plan.readback.access, RpgMapAccessState::DiegeticMapVisible);
    assert_eq!(
        plan.readback.policy.mode,
        MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired
    );
    assert_eq!(
        plan.readback.scope_id.as_deref(),
        Some("scope.galdmere.old_ash_road")
    );
    assert_eq!(
        plan.readback.source_kind,
        Some(RpgMapSourceKind::CartographerPurchase)
    );
    assert!(plan.readback.map_surface_visible);
    assert!(plan.readback.gps_indicators_disabled);
    assert!(!plan.readback.policy.player_position_indicator);
    assert!(!plan.readback.policy.quest_markers);
    assert_eq!(plan.markers.len(), 2);
}

#[test]
fn issue1445_found_map_can_be_incomplete_and_source_scoped() {
    let state = state_with_sources(vec![found_map()]);
    let plan = resolve_rpg_map_access(
        &state,
        &RpgMapOpenRequest::open_source("map.found.ruined_causeway"),
    )
    .unwrap();

    assert_eq!(
        plan.readback.source_kind,
        Some(RpgMapSourceKind::FoundOrLooted)
    );
    assert_eq!(plan.readback.fidelity, Some(RpgMapFidelity::Incomplete));
    assert_eq!(
        plan.readback.scope_id.as_deref(),
        Some("scope.ember_wastes.ruined_causeway")
    );
    assert_eq!(plan.markers.len(), 1);
    assert!(plan.readback.gps_indicators_disabled);
}

#[test]
fn issue1445_lost_source_degrades_to_no_map_access() {
    let mut state = state_with_sources(vec![cartographer_map()]);
    remove_rpg_map_source(
        &mut state,
        "map.cartographer.old_ash_road",
        RpgMapSourceStatus::Lost,
    )
    .unwrap();
    let plan = resolve_rpg_map_access(
        &state,
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )
    .unwrap();

    assert_eq!(plan.readback.access, RpgMapAccessState::SourceUnavailable);
    assert!(!plan.readback.map_surface_visible);
    assert_eq!(
        plan.readback.degraded_reason.as_deref(),
        Some("map_source_lost")
    );
    assert!(plan.markers.is_empty());
}

#[test]
fn issue1445_map_access_fails_closed_on_invalid_edges() {
    let duplicate = state_with_sources(vec![cartographer_map(), cartographer_map()]);
    let error = resolve_rpg_map_access(
        &duplicate,
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("duplicate map.cartographer.old_ash_road"));

    let unknown = resolve_rpg_map_access(
        &state_with_sources(vec![cartographer_map()]),
        &RpgMapOpenRequest::open_source("map.cartographer.free_fallback"),
    )
    .unwrap_err()
    .to_string();
    assert!(unknown.contains("unknown map source"));

    let mut invalid_quality = cartographer_map();
    invalid_quality.quality_tier = 0;
    let quality_error = resolve_rpg_map_access(
        &state_with_sources(vec![invalid_quality]),
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )
    .unwrap_err()
    .to_string();
    assert!(quality_error.contains("quality_tier"));

    let mut no_features = cartographer_map();
    no_features.features.clear();
    let features_error = resolve_rpg_map_access(
        &state_with_sources(vec![no_features]),
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )
    .unwrap_err()
    .to_string();
    assert!(features_error.contains("requires authored content"));
}

#[test]
fn issue1445_persisted_player_map_state_round_trips_from_file() {
    let path = temp_state_path();
    let state = state_with_sources(vec![cartographer_map(), found_map()]);
    let write = write_rpg_map_runtime_state(&path, &state).unwrap();
    let read = read_rpg_map_runtime_state(&path).unwrap();

    assert_eq!(write.sha256, read.sha256);
    assert_eq!(read.state.sources.len(), 2);
    assert_eq!(
        read.state.sources[0].source_id,
        "map.cartographer.old_ash_road"
    );
    // #1535: `path` (TestTempDir) removes its dir on Drop; no manual cleanup needed.
}

fn state_with_sources(sources: Vec<RpgMapSource>) -> RpgMapRuntimeState {
    RpgMapRuntimeState {
        schema: RPG_MAP_ACCESS_SCHEMA.to_string(),
        player_id: "player.ashen.cartography".to_string(),
        sources,
    }
}

fn cartographer_map() -> RpgMapSource {
    RpgMapSource {
        source_id: "map.cartographer.old_ash_road".to_string(),
        kind: RpgMapSourceKind::CartographerPurchase,
        status: RpgMapSourceStatus::Available,
        scope_id: "scope.galdmere.old_ash_road".to_string(),
        display_name: "Old Ash Road Parchment".to_string(),
        acquired_from: "cartographer.nella_cinderink".to_string(),
        quality_tier: 3,
        fidelity: RpgMapFidelity::Complete,
        features: vec![
            feature(
                "landmark.gate_of_cinders",
                RpgMapFeatureKind::Landmark,
                "Gate of Cinders",
                [12.0, 0.0, -8.0],
            ),
            feature(
                "danger.mire_toll",
                RpgMapFeatureKind::Danger,
                "Old toll mire",
                [28.0, 0.0, -18.0],
            ),
        ],
    }
}

fn found_map() -> RpgMapSource {
    RpgMapSource {
        source_id: "map.found.ruined_causeway".to_string(),
        kind: RpgMapSourceKind::FoundOrLooted,
        status: RpgMapSourceStatus::Available,
        scope_id: "scope.ember_wastes.ruined_causeway".to_string(),
        display_name: "Charred Causeway Scrap".to_string(),
        acquired_from: "loot.ash_bandit_satchel".to_string(),
        quality_tier: 1,
        fidelity: RpgMapFidelity::Incomplete,
        features: vec![feature(
            "landmark.broken_milepost",
            RpgMapFeatureKind::Landmark,
            "Broken milepost",
            [-16.0, 0.0, 22.0],
        )],
    }
}

fn feature(
    feature_id: &str,
    kind: RpgMapFeatureKind,
    label: &str,
    world_position: [f32; 3],
) -> RpgMapFeature {
    RpgMapFeature {
        feature_id: feature_id.to_string(),
        kind,
        label: label.to_string(),
        world_position,
    }
}

// #1535: TestTempDir auto-cleans on Drop.
fn temp_state_path() -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new("issue1445-map-state-vault")
}
