use std::path::{Path, PathBuf};

use litd_editor::{
    hud_ui_builder::{
        HudNormalizedRect, HudUiBuilderSelection, HudUiMinimapMarkerSpec, HudUiTextSpec,
        HudUiWidgetSpec, build_hud_ui_builder_readback,
    },
    mcp_render_tools::{
        READ_RENDER_TOOL, SET_HUD_UI_BUILDER_TOOL, call_mcp_render_tool, read_mcp_render_state,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue872_hud_ui_builder")?;
    let report_path = run.report_path();
    let root = report_path.parent().ok_or("report path has no parent")?;
    let state_path = root.join("mcp-render-state.json");
    let ledger_path = root.join("mcp-render-ledger.json");

    let happy = mcp_happy_path(&state_path, &ledger_path)?;
    let edges = vec![
        edge_duplicate_widget_id()?,
        edge_invalid_rect()?,
        edge_empty_widgets()?,
        edge_mcp_missing_region(root, &state_path)?,
    ];
    let report = json!({
        "schema": "litd.issue872.hud_ui_builder_fsv.v1",
        "issue": 872,
        "source_of_truth": {
            "builder": "No-code HUD selection lowered into litd_engine::render::ui::UiRenderPlan",
            "mcp_render_state": "Calyx Aster MCP render state row read back through litd.engine.render.read_render",
            "manual_fsv": "Open this report through Synapse and inspect happy_path/read_render/edges against expected_actuals."
        },
        "visual_references_reviewed": [
            "docs/readme/ED01-editor-shell.png",
            "docs/readme/ED12-play-topdown.png",
            "docs/readme/GP09-inventory.png",
            "docs/readme/GP19-spellbook.png"
        ],
        "happy_path": happy,
        "edges": edges,
        "expected_actuals": {
            "builder_id": "issue872.play_hud",
            "widget_count": 3,
            "text_run_count": 2,
            "minimap_marker_count": 3,
            "pass_count": 5,
            "command_slot_count": 10,
            "minimap_shape": "Circular",
            "edge_count": 4
        }
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "issue872_hud_ui_builder_fsv report={} bytes={} sha256={} builder={} widgets={} passes={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["happy_path"]["builder_readback"]["builder_id"],
        readback.json["happy_path"]["builder_readback"]["widget_count"],
        readback.json["happy_path"]["builder_readback"]["pass_count"],
        readback.json["edges"]
            .as_array()
            .map(|edges| edges.len())
            .unwrap_or(0),
    );
    Ok(())
}

fn mcp_happy_path(
    state_path: &Path,
    ledger_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let selection = sample_selection();
    let direct = build_hud_ui_builder_readback(&selection)?;
    let output = call_mcp_render_tool(
        SET_HUD_UI_BUILDER_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue872-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue872-hud-ui-builder-ledger",
            "request_id": "issue872-set-hud-ui-builder",
            "requested_by": "litd-editor-issue872-fsv",
            "selection": selection
        }),
    )?;
    let state = read_mcp_render_state(state_path)?;
    let read = call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": state_path }))?;
    Ok(json!({
        "direct_builder_readback": direct,
        "mcp_result": output.result,
        "state_sha256": state.sha256,
        "state_update_count": state.state.update_count,
        "state_widget_count": state.hud_ui_builder_widget_count,
        "state_text_run_count": state.hud_ui_builder_text_run_count,
        "state_pass_count": state.hud_ui_builder_pass_count,
        "builder_readback": state.hud_ui_builder,
        "read_render": read.result,
        "ledger_sha256": output.ledger_readback.map(|readback| readback.sha256)
    }))
}

fn edge_duplicate_widget_id() -> Result<Value, Box<dyn std::error::Error>> {
    let mut selection = sample_selection();
    selection.widgets[1].widget_id = selection.widgets[0].widget_id.clone();
    edge_direct("duplicate_widget_id", selection)
}

fn edge_invalid_rect() -> Result<Value, Box<dyn std::error::Error>> {
    let mut selection = sample_selection();
    selection.widgets[0].rect.max = Vec2::new(1.25, 0.8);
    edge_direct("invalid_rect", selection)
}

fn edge_empty_widgets() -> Result<Value, Box<dyn std::error::Error>> {
    let mut selection = sample_selection();
    selection.widgets.clear();
    edge_direct("empty_widgets", selection)
}

fn edge_direct(
    case: &str,
    selection: HudUiBuilderSelection,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = selection_hash(&selection)?;
    let error = build_hud_ui_builder_readback(&selection).unwrap_err();
    let after = selection_hash(&selection)?;
    Ok(json!({
        "case": case,
        "error_code": error.code(),
        "error": error.to_string(),
        "before_sha256": before,
        "after_sha256": after,
        "state_unchanged": before == after
    }))
}

fn selection_hash(selection: &HudUiBuilderSelection) -> Result<String, Box<dyn std::error::Error>> {
    Ok(sha256_hex(&serde_json::to_vec(selection)?))
}

fn edge_mcp_missing_region(
    root: &Path,
    state_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger_path = root.join("edge-missing-region-ledger.json");
    let before = file_state(state_path, &ledger_path);
    let mut selection = sample_selection();
    selection.widgets[0].region = HudRegionKind::LeftHierarchy;
    let error = call_mcp_render_tool(
        SET_HUD_UI_BUILDER_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue872-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue872-edge-missing-region",
            "request_id": "issue872-edge-missing-region",
            "requested_by": "litd-editor-issue872-fsv",
            "selection": selection
        }),
    )
    .unwrap_err();
    let after = file_state(state_path, &ledger_path);
    Ok(json!({
        "case": "mcp_missing_region",
        "before": before,
        "after": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string()
    }))
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

fn file_state(state_path: &Path, ledger_path: &PathBuf) -> Value {
    json!({
        "state_exists": state_path.exists(),
        "state_len": std::fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_len": std::fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn assert_report(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    if report["happy_path"]["builder_readback"]["builder_id"] != "issue872.play_hud" {
        return Err("builder id mismatch".into());
    }
    if report["happy_path"]["builder_readback"]["widget_count"] != 3 {
        return Err("widget count mismatch".into());
    }
    if report["happy_path"]["builder_readback"]["pass_count"] != 5 {
        return Err("pass count mismatch".into());
    }
    if report["edges"].as_array().map(|edges| edges.len()) != Some(4) {
        return Err("edge count mismatch".into());
    }
    let invalid_code = EngineError::UiRenderInvalidConfig {
        field: String::new(),
        reason: String::new(),
    }
    .code();
    for edge in report["edges"].as_array().ok_or("edges missing")? {
        if edge["error_code"] != invalid_code {
            return Err("edge did not fail with UI invalid config".into());
        }
        if edge["state_unchanged"] != true {
            return Err("edge mutated state".into());
        }
    }
    Ok(())
}
