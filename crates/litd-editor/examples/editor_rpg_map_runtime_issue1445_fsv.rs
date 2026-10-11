use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::{Vec3, Vec4},
    render::ui::{
        MapOverlayMarkerKind, MapOverlayMarkerSource, MapOverlayPolicy, MapOverlayPolicyMode,
        MinimapMarker, validate_map_overlay_policy,
    },
    rpg::{
        RPG_MAP_ACCESS_SCHEMA, RpgMapFeature, RpgMapFeatureKind, RpgMapFidelity, RpgMapOpenRequest,
        RpgMapRuntimeState, RpgMapSource, RpgMapSourceKind, RpgMapSourceStatus,
        remove_rpg_map_source, resolve_rpg_map_access, state_hash, write_rpg_map_runtime_state,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const MAP_ADR_PATH: &str = "docs/adr/adr-944-diegetic-rpg-maps.md";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1445_rpg_map_runtime")?;
    let map_adr_path = fs::canonicalize(workspace_path(MAP_ADR_PATH))?;
    let map_adr = fs::read(&map_adr_path)?;

    let fresh_state = RpgMapRuntimeState::fresh("player.ashen.fresh");
    let fresh_state_readback = write_rpg_map_runtime_state(
        &run.run_dir().join("fresh-player-map-state-vault"),
        &fresh_state,
    )?;
    let fresh_plan = resolve_rpg_map_access(&fresh_state, &RpgMapOpenRequest::no_map_screen())?;

    let acquired_state = state_with_sources(vec![cartographer_map(), found_map()]);
    let acquired_state_readback = write_rpg_map_runtime_state(
        &run.run_dir().join("acquired-player-map-state-vault"),
        &acquired_state,
    )?;
    let cartographer_plan = resolve_rpg_map_access(
        &acquired_state,
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )?;
    let found_plan = resolve_rpg_map_access(
        &acquired_state,
        &RpgMapOpenRequest::open_source("map.found.ruined_causeway"),
    )?;

    let mut lost_state = acquired_state.clone();
    remove_rpg_map_source(
        &mut lost_state,
        "map.cartographer.old_ash_road",
        RpgMapSourceStatus::Lost,
    )?;
    let lost_state_readback = write_rpg_map_runtime_state(
        &run.run_dir().join("lost-player-map-state-vault"),
        &lost_state,
    )?;
    let lost_plan = resolve_rpg_map_access(
        &lost_state,
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )?;
    let strategy = explicit_strategy_minimap()?;
    let assisted = assisted_opt_in_map()?;

    let edges = json!({
        "unknown_source_rejected": unknown_source_edge(&acquired_state),
        "duplicate_source_rejected": duplicate_source_edge(),
        "empty_authored_content_rejected": empty_content_edge(),
        "manual_gps_marker_injection_rejected": gps_injection_edge(&cartographer_plan),
        "missing_acquired_policy_source_rejected": missing_policy_edge(),
    });

    let report = json!({
        "schema": "litd.issue1445.rpg_map_runtime_fsv.v1",
        "issue": 1445,
        "source_of_truth": {
            "map_adr_path": map_adr_path,
            "map_adr_sha256": sha256_hex(&map_adr),
            "fresh_state": fresh_state_readback,
            "acquired_state": acquired_state_readback,
            "lost_state": lost_state_readback,
            "readme_refs": [
                "docs/readme/ED12-play-topdown.png",
                "docs/readme/GP02-exploration.png",
                "docs/readme/GP11-quest.png",
                "docs/readme/L03-ashen-reach.png"
            ]
        },
        "sufficiency_argument": "The minimal #1445 proof is one fresh RPG player state, one state with a cartographer purchase plus a found map, one lost-source state, and explicit strategy/assisted policy readbacks. That covers no free map access, cartographer source gating, found/looted incomplete map scope, source loss/degradation, non-RPG marker opt-in, assisted-mode labeling, and the render policy handoff that suppresses player dots, viewport rectangles, quest markers, and route lines on RPG diegetic maps. Five edge probes cover unknown source fallback, duplicate source IDs, empty authored content, a manual GPS marker injection attempt, and missing acquired-map policy source.",
        "happy_path": {
            "fresh_rpg_session": {
                "access": fresh_plan.readback.access,
                "policy": fresh_plan.readback.policy.mode,
                "map_surface_visible": fresh_plan.readback.map_surface_visible,
                "marker_count": fresh_plan.markers.len(),
                "gps_indicators_disabled": fresh_plan.readback.gps_indicators_disabled,
            },
            "cartographer_purchase": {
                "access": cartographer_plan.readback.access,
                "source_kind": cartographer_plan.readback.source_kind,
                "scope_id": cartographer_plan.readback.scope_id,
                "policy": cartographer_plan.readback.policy.mode,
                "map_surface_visible": cartographer_plan.readback.map_surface_visible,
                "marker_count": cartographer_plan.markers.len(),
                "player_position_indicator": cartographer_plan.readback.policy.player_position_indicator,
                "viewport_indicator": cartographer_plan.readback.policy.viewport_indicator,
                "quest_markers": cartographer_plan.readback.policy.quest_markers,
                "route_lines": cartographer_plan.readback.policy.route_lines,
            },
            "found_looted_map": {
                "access": found_plan.readback.access,
                "source_kind": found_plan.readback.source_kind,
                "scope_id": found_plan.readback.scope_id,
                "fidelity": found_plan.readback.fidelity,
                "marker_count": found_plan.markers.len(),
                "gps_indicators_disabled": found_plan.readback.gps_indicators_disabled,
            },
            "lost_source": {
                "access": lost_plan.readback.access,
                "map_surface_visible": lost_plan.readback.map_surface_visible,
                "degraded_reason": lost_plan.readback.degraded_reason,
                "marker_count": lost_plan.markers.len(),
            },
            "explicit_strategy_minimap": {
                "policy": strategy["mode"],
                "surface_visible": strategy["surface_visible"],
                "player_position_indicator": strategy["player_position_indicator"],
                "marker_count": strategy["marker_count"],
            },
            "assisted_opt_in": {
                "policy": assisted["mode"],
                "assisted_label": assisted["assisted_label"],
                "surface_visible": assisted["surface_visible"],
                "quest_markers": assisted["quest_markers"],
                "marker_count": assisted["marker_count"],
            }
        },
        "edges": edges
    });
    require_bool(
        &report["happy_path"]["fresh_rpg_session"]["map_surface_visible"],
        false,
    )?;
    require_bool(
        &report["happy_path"]["cartographer_purchase"]["player_position_indicator"],
        false,
    )?;
    require_bool(
        &report["happy_path"]["cartographer_purchase"]["quest_markers"],
        false,
    )?;
    require_access(
        &report["happy_path"]["lost_source"]["access"],
        "source_unavailable",
    )?;
    require_all_edges_unchanged(&report["edges"])?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1445_RPG_MAP_RUNTIME_FSV report={} bytes={} sha256={} fresh_state={} acquired_state={} lost_state={} map_adr_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        report["source_of_truth"]["fresh_state"]["sha256"],
        report["source_of_truth"]["acquired_state"]["sha256"],
        report["source_of_truth"]["lost_state"]["sha256"],
        sha256_hex(&map_adr)
    );
    Ok(())
}

fn workspace_path(relative_path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative_path)
}

fn unknown_source_edge(state: &RpgMapRuntimeState) -> Value {
    let before = state_hash(state).unwrap();
    let error = resolve_rpg_map_access(
        state,
        &RpgMapOpenRequest::open_source("map.cartographer.free_fallback"),
    )
    .unwrap_err();
    json!({
        "error": error.to_string(),
        "state_unchanged": before == state_hash(state).unwrap()
    })
}

fn duplicate_source_edge() -> Value {
    let state = state_with_sources(vec![cartographer_map(), cartographer_map()]);
    let before = state_hash(&state).unwrap();
    let error = resolve_rpg_map_access(
        &state,
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )
    .unwrap_err();
    json!({
        "error": error.to_string(),
        "state_unchanged": before == state_hash(&state).unwrap()
    })
}

fn empty_content_edge() -> Value {
    let mut source = cartographer_map();
    source.features.clear();
    let state = state_with_sources(vec![source]);
    let before = state_hash(&state).unwrap();
    let error = resolve_rpg_map_access(
        &state,
        &RpgMapOpenRequest::open_source("map.cartographer.old_ash_road"),
    )
    .unwrap_err();
    json!({
        "error": error.to_string(),
        "state_unchanged": before == state_hash(&state).unwrap()
    })
}

fn gps_injection_edge(plan: &litd_engine::rpg::RpgMapRenderPlan) -> Value {
    let mut markers = plan.markers.clone();
    let source_before = hash_json(&json!({
        "policy": &plan.overlay_policy,
        "markers": &plan.markers,
    }));
    markers.push(MinimapMarker {
        id: "gps.player_dot".to_string(),
        kind: MapOverlayMarkerKind::PlayerPosition,
        source: MapOverlayMarkerSource::InventoryMap,
        allowed_policy: MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
        world_position: Vec3::ZERO,
        color: Vec4::new(1.0, 0.0, 0.0, 1.0),
        radius_px: 5.0,
    });
    let candidate_before = hash_json(&json!({
        "policy": &plan.overlay_policy,
        "markers": &markers,
    }));
    let error = validate_map_overlay_policy(&plan.overlay_policy, &markers).unwrap_err();
    let candidate_after = hash_json(&json!({
        "policy": &plan.overlay_policy,
        "markers": &markers,
    }));
    let source_after = hash_json(&json!({
        "policy": &plan.overlay_policy,
        "markers": &plan.markers,
    }));
    json!({
        "error": error.to_string(),
        "before": {
            "source_policy_marker_sha256": source_before,
            "injected_candidate_sha256": candidate_before,
        },
        "after": {
            "source_policy_marker_sha256": source_after,
            "injected_candidate_sha256": candidate_after,
        },
        "state_unchanged": source_before == source_after && candidate_before == candidate_after
    })
}

fn missing_policy_edge() -> Value {
    let policy = MapOverlayPolicy {
        mode: MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
        source_id: None,
        scope_id: Some("scope.galdmere.old_ash_road".to_string()),
        assisted_label: None,
        player_position_indicator: false,
        viewport_indicator: false,
        quest_markers: false,
        route_lines: false,
    };
    let before = hash_json(&json!({"policy": &policy, "markers": []}));
    let error = validate_map_overlay_policy(&policy, &[]).unwrap_err();
    let after = hash_json(&json!({"policy": &policy, "markers": []}));
    json!({
        "error": error.to_string(),
        "before": {"policy_marker_sha256": before},
        "after": {"policy_marker_sha256": after},
        "state_unchanged": before == after
    })
}

fn hash_json(value: &Value) -> String {
    sha256_hex(
        &serde_json::to_vec(value)
            .expect("map runtime edge value must serialize for hash readback"),
    )
}

fn explicit_strategy_minimap() -> Result<Value, EngineError> {
    let policy = MapOverlayPolicy::runtime_strategy_minimap();
    let marker = MinimapMarker {
        id: "strategy.player_dot".to_string(),
        kind: MapOverlayMarkerKind::PlayerPosition,
        source: MapOverlayMarkerSource::StrategyRuntime,
        allowed_policy: MapOverlayPolicyMode::RuntimeStrategyMinimap,
        world_position: Vec3::ZERO,
        color: Vec4::new(0.2, 0.8, 1.0, 1.0),
        radius_px: 4.0,
    };
    let readback = validate_map_overlay_policy(&policy, &[marker])?;
    serde_json::to_value(readback).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize strategy readback: {error}"),
    })
}

fn assisted_opt_in_map() -> Result<Value, EngineError> {
    let policy = MapOverlayPolicy::runtime_assisted("Assistive navigation enabled by player");
    let marker = MinimapMarker {
        id: "assist.quest_hint".to_string(),
        kind: MapOverlayMarkerKind::QuestObjective,
        source: MapOverlayMarkerSource::AssistedOptIn,
        allowed_policy: MapOverlayPolicyMode::RuntimeAssisted,
        world_position: Vec3::new(4.0, 0.0, -4.0),
        color: Vec4::new(1.0, 0.9, 0.1, 1.0),
        radius_px: 4.0,
    };
    let readback = validate_map_overlay_policy(&policy, &[marker])?;
    serde_json::to_value(readback).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize assisted readback: {error}"),
    })
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
                [12.0, 0.0, -8.0],
            ),
            feature(
                "danger.mire_toll",
                RpgMapFeatureKind::Danger,
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
            [-16.0, 0.0, 22.0],
        )],
    }
}

fn feature(feature_id: &str, kind: RpgMapFeatureKind, world_position: [f32; 3]) -> RpgMapFeature {
    RpgMapFeature {
        feature_id: feature_id.to_string(),
        kind,
        label: feature_id.replace('.', " "),
        world_position,
    }
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1445.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_access(value: &Value, expected: &str) -> Result<(), EngineError> {
    if value.as_str() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1445.report.access".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1445.edge.{name}"),
                reason: "edge mutated source state".to_string(),
            });
        }
    }
    Ok(())
}
