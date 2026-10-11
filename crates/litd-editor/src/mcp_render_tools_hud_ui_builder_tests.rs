use std::{fs, path::Path};

use litd_engine::{
    math::{Vec2, Vec3, Vec4},
    render::ui::{
        FloatingText, HudLayoutPreset, HudRegionKind, HudWidgetKind, MapOverlayMarkerKind,
        MapOverlayMarkerSource, MapOverlayPolicy, MapOverlayPolicyMode,
    },
};
use serde_json::json;

use crate::hud_ui_builder::{
    HudNormalizedRect, HudUiBuilderSelection, HudUiMinimapMarkerSpec, HudUiTextSpec,
    HudUiWidgetSpec,
};

use super::*;

#[test]
fn issue872_hud_ui_builder_mcp_writes_and_reads_real_state() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue872_hud_ui_builder", "mcp").unwrap();
    reset_root(&root);
    let state_path = root.join("render-state.json");
    let ledger_path = root.join("hud-builder-ledger.json");

    let output = call_mcp_render_tool(
        SET_HUD_UI_BUILDER_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue872-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue872-hud-builder-ledger",
            "request_id": "issue872-set-hud-builder",
            "requested_by": "litd-editor-issue872-test",
            "selection": sample_selection()
        }),
    )
    .unwrap();
    assert_eq!(output.tool_name, SET_HUD_UI_BUILDER_TOOL);
    assert_eq!(output.result["hud_ui_builder_widget_count"], 3);
    assert_eq!(output.result["hud_ui_builder_text_run_count"], 2);
    assert_eq!(output.result["hud_ui_builder_pass_count"], 5);
    assert_eq!(
        output.result["hud_ui_builder_overlay_policy_mode"],
        "runtime_strategy_minimap"
    );
    assert_eq!(
        output.result["hud_ui_builder_player_position_indicator"],
        true
    );

    let state = read_mcp_render_state(&state_path).unwrap();
    assert_eq!(state.hud_ui_builder_widget_count, 3);
    assert_eq!(state.hud_ui_builder_text_run_count, 2);
    assert_eq!(state.hud_ui_builder_pass_count, 5);
    assert!(
        state
            .hud_ui_builder
            .as_ref()
            .unwrap()
            .reference_images
            .iter()
            .any(|path| path.ends_with("ED12-play-topdown.png"))
    );

    let read = call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read.result["hud_ui_builder_id"], "issue872.play_hud");
    assert_eq!(read.result["hud_ui_builder_widget_count"], 3);
    assert_eq!(read.result["hud_ui_builder_pass_count"], 5);
    assert_eq!(
        read.result["hud_ui_builder_overlay_policy_mode"],
        "runtime_strategy_minimap"
    );
    assert!(ledger_path.exists());
}

#[test]
fn issue872_hud_ui_builder_edges_fail_closed_without_writing() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue872_hud_ui_builder", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("render-state.json");
    let ledger_path = root.join("edge-ledger.json");

    let before = state_summary(&state_path, &ledger_path);
    let mut bad_region = sample_selection();
    bad_region.widgets[0].region = HudRegionKind::LeftHierarchy;
    let error = call_mcp_render_tool(
        SET_HUD_UI_BUILDER_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue872-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue872-bad-region",
            "request_id": "issue872-bad-region",
            "requested_by": "litd-editor-issue872-test",
            "selection": bad_region
        }),
    )
    .unwrap_err();
    let after = state_summary(&state_path, &ledger_path);
    assert_eq!(before, after);
    assert!(error.to_string().contains("region"));

    let mut rpg_gps = sample_selection();
    rpg_gps.overlay_policy = MapOverlayPolicy::runtime_rpg_diegetic_acquired(
        "inventory.map.galdmere_route",
        "region.galdmere",
    );
    rpg_gps.minimap_markers = vec![marker(
        "gps_pin",
        MapOverlayMarkerKind::QuestObjective,
        MapOverlayMarkerSource::InventoryMap,
        MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
        Vec3::new(12.0, 0.0, 4.0),
        Vec4::new(1.0, 0.74, 0.18, 1.0),
    )];
    let before = state_summary(&state_path, &ledger_path);
    let error = call_mcp_render_tool(
        SET_HUD_UI_BUILDER_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue872-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue872-rpg-gps",
            "request_id": "issue872-rpg-gps",
            "requested_by": "litd-editor-issue1446-test",
            "selection": rpg_gps
        }),
    )
    .unwrap_err();
    let after = state_summary(&state_path, &ledger_path);
    assert_eq!(before, after);
    assert!(error.to_string().contains("marker kind/source"));
}

fn sample_selection() -> HudUiBuilderSelection {
    HudUiBuilderSelection {
        builder_id: "issue872.play_hud".to_string(),
        preset: HudLayoutPreset::PlayTopDown,
        viewport_width_px: 1920,
        viewport_height_px: 1080,
        widgets: vec![
            widget(
                "health_bar",
                HudRegionKind::CommandBar,
                [0.02, 0.58],
                [0.36, 0.82],
                HudWidgetKind::HealthBar {
                    current: 740.0,
                    max: 1000.0,
                },
            ),
            widget(
                "attack_button",
                HudRegionKind::CommandBar,
                [0.44, 0.20],
                [0.52, 0.72],
                HudWidgetKind::CommandButton {
                    enabled: true,
                    cooldown: 0.0,
                },
            ),
            widget(
                "gold_counter",
                HudRegionKind::StatusStrip,
                [0.10, 0.12],
                [0.20, 0.86],
                HudWidgetKind::ResourceCounter { amount: 320 },
            ),
        ],
        text_runs: vec![
            text(
                "quest_title",
                HudRegionKind::QuestTracker,
                "Quest: Light the gate",
            ),
            text("resource_text", HudRegionKind::StatusStrip, "Gold 320"),
        ],
        overlay_policy: MapOverlayPolicy::runtime_strategy_minimap(),
        minimap_markers: vec![
            marker(
                "hero",
                MapOverlayMarkerKind::PlayerPosition,
                MapOverlayMarkerSource::StrategyRuntime,
                MapOverlayPolicyMode::RuntimeStrategyMinimap,
                Vec3::ZERO,
                Vec4::new(0.25, 0.85, 1.0, 1.0),
            ),
            marker(
                "objective",
                MapOverlayMarkerKind::QuestObjective,
                MapOverlayMarkerSource::StrategyRuntime,
                MapOverlayPolicyMode::RuntimeStrategyMinimap,
                Vec3::new(42.0, 0.0, -18.0),
                Vec4::new(1.0, 0.74, 0.18, 1.0),
            ),
            marker(
                "threat",
                MapOverlayMarkerKind::Danger,
                MapOverlayMarkerSource::StrategyRuntime,
                MapOverlayPolicyMode::RuntimeStrategyMinimap,
                Vec3::new(-32.0, 0.0, 26.0),
                Vec4::new(1.0, 0.14, 0.12, 1.0),
            ),
        ],
        floating_texts: vec![FloatingText {
            text: "+42".to_string(),
            world_position: Vec3::new(0.15, 0.10, 0.0),
            age_seconds: 0.35,
            lifetime_seconds: 1.2,
            rise_px_per_second: 36.0,
            color: Vec4::new(1.0, 0.28, 0.10, 1.0),
        }],
    }
}

fn widget(
    id: &str,
    region: HudRegionKind,
    min: [f32; 2],
    max: [f32; 2],
    kind: HudWidgetKind,
) -> HudUiWidgetSpec {
    HudUiWidgetSpec {
        widget_id: id.to_string(),
        region,
        rect: HudNormalizedRect {
            min: Vec2::new(min[0], min[1]),
            max: Vec2::new(max[0], max[1]),
        },
        kind,
    }
}

fn text(id: &str, region: HudRegionKind, text: &str) -> HudUiTextSpec {
    HudUiTextSpec {
        text_id: id.to_string(),
        region,
        offset_px: Vec2::new(18.0, 18.0),
        font_px: 20.0,
        text: text.to_string(),
        color: Vec4::new(0.95, 0.91, 0.82, 1.0),
    }
}

fn marker(
    id: &str,
    kind: MapOverlayMarkerKind,
    source: MapOverlayMarkerSource,
    allowed_policy: MapOverlayPolicyMode,
    world_position: Vec3,
    color: Vec4,
) -> HudUiMinimapMarkerSpec {
    HudUiMinimapMarkerSpec {
        marker_id: id.to_string(),
        kind,
        source,
        allowed_policy,
        world_position,
        color,
        radius_px: 4.0,
    }
}

fn state_summary(state_path: &Path, ledger_path: &Path) -> serde_json::Value {
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
