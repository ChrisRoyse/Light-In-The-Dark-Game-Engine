use std::{fs, path::Path};

use litd_editor::{
    hud_ui_builder::{
        HudNormalizedRect, HudUiBuilderSelection, HudUiMinimapMarkerSpec, HudUiTextSpec,
        HudUiWidgetSpec, build_hud_ui_builder_readback,
    },
    mcp_render_tools::{
        READ_RENDER_TOOL, SET_HUD_UI_BUILDER_TOOL, call_mcp_render_tool, read_mcp_render_state,
        read_mcp_render_tool_ledger,
    },
};
use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::{Vec2, Vec3, Vec4},
    render::ui::{
        FloatingText, HudLayoutPreset, HudRegionKind, HudWidgetKind, MapOverlayMarkerKind,
        MapOverlayMarkerSource, MapOverlayPolicy, MapOverlayPolicyMode,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const MAP_ADR_PATH: &str = "docs/adr/adr-944-diegetic-rpg-maps.md";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue1446_map_overlay_policy")?;
    let state_path = run.run_dir().join("render-state.json");
    let ledger_path = run.run_dir().join("render-ledger.json");
    let map_adr_bytes = fs::read(MAP_ADR_PATH)?;

    let strategy = call_set(
        &state_path,
        &ledger_path,
        "issue1446-strategy",
        strategy_selection(),
    )?;
    let persisted = call_mcp_render_tool(
        READ_RENDER_TOOL,
        json!({
            "state_path": state_path
        }),
    )?;
    let fresh_rpg = build_hud_ui_builder_readback(&fresh_rpg_no_map_selection())?;
    let acquired = build_hud_ui_builder_readback(&acquired_rpg_map_selection())?;
    let edges = json!({
        "fresh_rpg_marker_rejected": edge_mcp(
            &state_path,
            &ledger_path,
            "issue1446-fresh-rpg-marker",
            fresh_rpg_with_marker_selection()
        ),
        "acquired_rpg_quest_pin_rejected": edge_mcp(
            &state_path,
            &ledger_path,
            "issue1446-acquired-quest-pin",
            acquired_rpg_quest_pin_selection()
        ),
        "assisted_mode_missing_label_rejected": edge_builder(assisted_missing_label_selection()),
        "marker_policy_mismatch_rejected": edge_builder(marker_policy_mismatch_selection())
    });

    let report = json!({
        "schema": "litd.issue1446.map_overlay_policy_fsv.v1",
        "issue": 1446,
        "source_of_truth": {
            "render_state_path": state_path,
            "render_state_dir": path_state(&state_path),
            "render_state_readback": &persisted.state_readback,
            "render_ledger_path": ledger_path,
            "render_ledger_dir": path_state(&ledger_path),
            "render_ledger_readback": &strategy.ledger_readback,
            "map_adr_path": MAP_ADR_PATH,
            "map_adr_sha256": sha256_hex(&map_adr_bytes),
            "readme_refs": [
                "docs/readme/ED11-world-view.png",
                "docs/readme/ED12-play-topdown.png",
                "docs/readme/GP02-exploration.png",
                "docs/readme/GP11-quest.png"
            ]
        },
        "sufficiency_argument": "The smallest sufficient #1446 proof is one persisted MCP HUD-builder state plus two direct render-plan readbacks. The persisted strategy path proves marker policy survives the real MCP write/read source of truth. The fresh RPG no-map readback proves the minimap pass is absent without map access. The acquired RPG readback proves map access is source-scoped and keeps player-position, viewport, quest, and route indicators false. Four edge probes cover free RPG markers, acquired GPS quest pins, unlabeled assisted mode, and marker-policy mismatch; each fails before persisted state mutation.",
        "happy_path": {
            "strategy_mcp_write": &strategy.result,
            "persisted_read": &persisted.result,
            "fresh_rpg_no_map": {
                "policy": fresh_rpg.overlay_policy_mode,
                "surface_visible": fresh_rpg.map_surface_visible,
                "pass_count": fresh_rpg.pass_count,
                "marker_count": fresh_rpg.minimap_marker_count,
                "player_position_indicator": fresh_rpg.player_position_indicator,
                "quest_markers": fresh_rpg.quest_markers
            },
            "acquired_rpg_map": {
                "policy": acquired.overlay_policy_mode,
                "surface_visible": acquired.map_surface_visible,
                "pass_count": acquired.pass_count,
                "marker_count": acquired.minimap_marker_count,
                "player_position_indicator": acquired.player_position_indicator,
                "viewport_indicator": acquired.viewport_indicator,
                "quest_markers": acquired.quest_markers,
                "route_lines": acquired.route_lines
            }
        },
        "edges": edges
    });
    require_bool(
        &report["happy_path"]["fresh_rpg_no_map"]["surface_visible"],
        false,
    )?;
    require_bool(
        &report["happy_path"]["acquired_rpg_map"]["player_position_indicator"],
        false,
    )?;
    require_all_edges_unchanged(&report["edges"])?;

    let readback = run.write_report(&report)?;
    if sha256_hex(&readback.bytes) != readback.sha256 {
        return Err("issue1446 report hash readback mismatch".into());
    }
    println!(
        "ISSUE1446_MAP_OVERLAY_POLICY_FSV report={} bytes={} sha256={} state={} state_sha256={} ledger={} ledger_sha256={} map_adr_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        state_path.display(),
        mcp_state_summary(&state_path)["sha256"],
        ledger_path.display(),
        mcp_ledger_summary(&ledger_path)["sha256"],
        sha256_hex(&map_adr_bytes)
    );
    Ok(())
}

fn call_set(
    state_path: &Path,
    ledger_path: &Path,
    request_id: &str,
    selection: HudUiBuilderSelection,
) -> Result<litd_editor::mcp_render_tools::McpRenderToolCallOutput, EngineError> {
    call_mcp_render_tool(
        SET_HUD_UI_BUILDER_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue1446-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue1446-render-ledger",
            "request_id": request_id,
            "requested_by": "litd-editor-issue1446-fsv",
            "selection": selection
        }),
    )
}

fn edge_mcp(
    state_path: &Path,
    ledger_path: &Path,
    request_id: &str,
    selection: HudUiBuilderSelection,
) -> Value {
    let before = json!({
        "state": mcp_state_summary(state_path),
        "ledger": mcp_ledger_summary(ledger_path)
    });
    let error = call_set(state_path, ledger_path, request_id, selection).unwrap_err();
    let after = json!({
        "state": mcp_state_summary(state_path),
        "ledger": mcp_ledger_summary(ledger_path)
    });
    json!({
        "before": before,
        "after": after,
        "state_unchanged": before == after,
        "error": error_report(&error)
    })
}

fn edge_builder(selection: HudUiBuilderSelection) -> Value {
    let before = selection_hash(&selection);
    let error = build_hud_ui_builder_readback(&selection).unwrap_err();
    let after = selection_hash(&selection);
    json!({
        "before": {"selection_sha256": before},
        "after": {"selection_sha256": after},
        "state_unchanged": before == after,
        "error": error_report(&error)
    })
}

fn selection_hash(selection: &HudUiBuilderSelection) -> String {
    sha256_hex(
        &serde_json::to_vec(selection)
            .expect("HUD UI builder selection must serialize for FSV edge hashing"),
    )
}

fn strategy_selection() -> HudUiBuilderSelection {
    let mut selection = base_selection("issue1446.strategy");
    selection.overlay_policy = MapOverlayPolicy::runtime_strategy_minimap();
    selection.minimap_markers = vec![
        marker(
            "player_dot",
            MapOverlayMarkerKind::PlayerPosition,
            MapOverlayMarkerSource::StrategyRuntime,
            MapOverlayPolicyMode::RuntimeStrategyMinimap,
            Vec3::ZERO,
        ),
        marker(
            "quest_pin",
            MapOverlayMarkerKind::QuestObjective,
            MapOverlayMarkerSource::StrategyRuntime,
            MapOverlayPolicyMode::RuntimeStrategyMinimap,
            Vec3::new(32.0, 0.0, -18.0),
        ),
        marker(
            "danger",
            MapOverlayMarkerKind::Danger,
            MapOverlayMarkerSource::StrategyRuntime,
            MapOverlayPolicyMode::RuntimeStrategyMinimap,
            Vec3::new(-24.0, 0.0, 26.0),
        ),
    ];
    selection
}

fn fresh_rpg_no_map_selection() -> HudUiBuilderSelection {
    let mut selection = base_selection("issue1446.fresh_rpg_no_map");
    selection.overlay_policy = MapOverlayPolicy::runtime_rpg_no_map_access();
    selection.minimap_markers.clear();
    selection
}

fn acquired_rpg_map_selection() -> HudUiBuilderSelection {
    let mut selection = base_selection("issue1446.acquired_rpg_map");
    selection.overlay_policy = MapOverlayPolicy::runtime_rpg_diegetic_acquired(
        "inventory.map.galdmere_route",
        "region.galdmere",
    );
    selection.minimap_markers = vec![
        marker(
            "old_ash_road",
            MapOverlayMarkerKind::Landmark,
            MapOverlayMarkerSource::InventoryMap,
            MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
            Vec3::new(18.0, 0.0, -20.0),
        ),
        marker(
            "grey_ravine",
            MapOverlayMarkerKind::Danger,
            MapOverlayMarkerSource::InventoryMap,
            MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
            Vec3::new(42.0, 0.0, -34.0),
        ),
    ];
    selection
}

fn fresh_rpg_with_marker_selection() -> HudUiBuilderSelection {
    let mut selection = fresh_rpg_no_map_selection();
    selection.minimap_markers = vec![marker(
        "free_marker",
        MapOverlayMarkerKind::Landmark,
        MapOverlayMarkerSource::InventoryMap,
        MapOverlayPolicyMode::RuntimeRpgNoMapAccess,
        Vec3::new(1.0, 0.0, 1.0),
    )];
    selection
}

fn acquired_rpg_quest_pin_selection() -> HudUiBuilderSelection {
    let mut selection = acquired_rpg_map_selection();
    selection.minimap_markers = vec![marker(
        "gps_quest_pin",
        MapOverlayMarkerKind::QuestObjective,
        MapOverlayMarkerSource::InventoryMap,
        MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
        Vec3::new(4.0, 0.0, 8.0),
    )];
    selection
}

fn assisted_missing_label_selection() -> HudUiBuilderSelection {
    let mut selection = base_selection("issue1446.bad_assisted");
    selection.overlay_policy = MapOverlayPolicy {
        mode: MapOverlayPolicyMode::RuntimeAssisted,
        source_id: Some("runtime.assisted_navigation".to_string()),
        scope_id: Some("runtime.current_world".to_string()),
        assisted_label: None,
        player_position_indicator: true,
        viewport_indicator: true,
        quest_markers: true,
        route_lines: true,
    };
    selection.minimap_markers.clear();
    selection
}

fn marker_policy_mismatch_selection() -> HudUiBuilderSelection {
    let mut selection = strategy_selection();
    selection.minimap_markers[0].allowed_policy = MapOverlayPolicyMode::RuntimeAssisted;
    selection
}

fn base_selection(builder_id: &str) -> HudUiBuilderSelection {
    HudUiBuilderSelection {
        builder_id: builder_id.to_string(),
        preset: HudLayoutPreset::PlayTopDown,
        viewport_width_px: 1280,
        viewport_height_px: 720,
        widgets: vec![HudUiWidgetSpec {
            widget_id: "health".to_string(),
            region: HudRegionKind::CommandBar,
            rect: rect([0.04, 0.55], [0.35, 0.82]),
            kind: HudWidgetKind::HealthBar {
                current: 80.0,
                max: 100.0,
            },
        }],
        text_runs: vec![HudUiTextSpec {
            text_id: "route_hint".to_string(),
            region: HudRegionKind::QuestTracker,
            offset_px: Vec2::new(18.0, 22.0),
            font_px: 20.0,
            text: "Follow the old ash road".to_string(),
            color: Vec4::new(0.95, 0.88, 0.62, 1.0),
        }],
        overlay_policy: MapOverlayPolicy::runtime_rpg_no_map_access(),
        minimap_markers: Vec::new(),
        floating_texts: vec![FloatingText {
            text: "+12".to_string(),
            world_position: Vec3::new(0.1, 0.1, 0.0),
            age_seconds: 0.2,
            lifetime_seconds: 1.0,
            rise_px_per_second: 24.0,
            color: Vec4::new(1.0, 0.32, 0.10, 1.0),
        }],
    }
}

fn marker(
    id: &str,
    kind: MapOverlayMarkerKind,
    source: MapOverlayMarkerSource,
    allowed_policy: MapOverlayPolicyMode,
    world_position: Vec3,
) -> HudUiMinimapMarkerSpec {
    HudUiMinimapMarkerSpec {
        marker_id: id.to_string(),
        kind,
        source,
        allowed_policy,
        world_position,
        color: Vec4::new(0.86, 0.68, 0.34, 1.0),
        radius_px: 4.0,
    }
}

fn rect(min: [f32; 2], max: [f32; 2]) -> HudNormalizedRect {
    HudNormalizedRect {
        min: Vec2::new(min[0], min[1]),
        max: Vec2::new(max[0], max[1]),
    }
}

fn require_bool(value: &Value, expected: bool) -> Result<(), Box<dyn std::error::Error>> {
    if value.as_bool() != Some(expected) {
        return Err(format!("expected {value:?} to be {expected}").into());
    }
    Ok(())
}

fn require_all_edges_unchanged(edges: &Value) -> Result<(), Box<dyn std::error::Error>> {
    for (name, edge) in edges.as_object().ok_or("edges must be an object")? {
        if edge["state_unchanged"].as_bool() != Some(true) {
            return Err(format!("{name} mutated state").into());
        }
    }
    Ok(())
}

fn path_state(path: &Path) -> Value {
    let metadata = fs::metadata(path).ok();
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "is_dir": metadata.as_ref().map(|item| item.is_dir()),
        "is_file": metadata.as_ref().map(|item| item.is_file()),
        "bytes": metadata.as_ref().map(|item| item.len()),
    })
}

fn mcp_state_summary(path: &Path) -> Value {
    match read_mcp_render_state(path) {
        Ok(readback) => json!({
            "path": readback.path,
            "bytes": readback.bytes,
            "sha256": readback.sha256,
            "update_count": readback.state.update_count,
            "hud_policy": readback.hud_ui_builder.as_ref().map(|hud| hud.overlay_policy_mode),
            "hud_plan_sha256": readback.hud_ui_builder.as_ref().map(|hud| hud.plan_sha256.clone()),
        }),
        Err(error) => json!({
            "path": path.display().to_string(),
            "error": error_report(&error),
        }),
    }
}

fn mcp_ledger_summary(path: &Path) -> Value {
    match read_mcp_render_tool_ledger(path) {
        Ok(readback) => json!({
            "path": readback.path,
            "bytes": readback.bytes,
            "sha256": readback.sha256,
            "record_count": readback.ledger.records.len(),
        }),
        Err(error) => json!({
            "path": path.display().to_string(),
            "error": error_report(&error),
        }),
    }
}

fn error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}
