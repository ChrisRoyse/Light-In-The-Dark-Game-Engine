use super::*;

#[test]
fn no_code_hud_builder_lowers_to_real_ui_render_plan() {
    let readback = build_hud_ui_builder_readback(&sample_selection()).unwrap();

    assert_eq!(readback.schema, HUD_UI_BUILDER_SCHEMA);
    assert_eq!(readback.builder_id, "issue872.play_hud");
    assert_eq!(readback.preset, HudLayoutPreset::PlayTopDown);
    assert_eq!(readback.widget_count, 3);
    assert_eq!(readback.text_run_count, 2);
    assert_eq!(readback.minimap_marker_count, 3);
    assert_eq!(readback.pass_count, 5);
    assert_eq!(readback.command_slot_count, 10);
    assert_eq!(readback.minimap_shape, MinimapShape::Circular);
    assert_eq!(
        readback.overlay_policy_mode,
        MapOverlayPolicyMode::RuntimeStrategyMinimap
    );
    assert!(readback.map_surface_visible);
    assert!(readback.player_position_indicator);
    assert!(readback.glyph_quad_count >= "Quest".len());
    assert!(readback.floating_glyph_quad_count > 0);
    assert!(readback.fog_visible > 0);
    assert!(
        readback
            .reference_images
            .iter()
            .any(|path| path.ends_with("ED12-play-topdown.png"))
    );
    assert_eq!(readback.selection_sha256.len(), 64);
    assert_eq!(readback.plan_sha256.len(), 64);
}

#[test]
fn duplicate_widget_ids_fail_closed() {
    let mut selection = sample_selection();
    selection.widgets[1].widget_id = selection.widgets[0].widget_id.clone();
    let error = build_hud_ui_builder_readback(&selection).unwrap_err();
    assert!(error.to_string().contains("widget_id"));
    assert!(error.to_string().contains("duplicate"));
}

#[test]
fn missing_region_for_preset_fails_closed() {
    let mut selection = sample_selection();
    selection.widgets[0].region = HudRegionKind::LeftHierarchy;
    let error = build_hud_ui_builder_readback(&selection).unwrap_err();
    assert!(error.to_string().contains("region"));
    assert!(error.to_string().contains("LeftHierarchy"));
}

#[test]
fn invalid_normalized_rect_fails_closed() {
    let mut selection = sample_selection();
    selection.widgets[0].rect.max = Vec2::new(1.2, 0.4);
    let error = build_hud_ui_builder_readback(&selection).unwrap_err();
    assert!(error.to_string().contains("rect"));
    assert!(error.to_string().contains("0..=1"));
}

#[test]
fn empty_widget_set_fails_closed() {
    let mut selection = sample_selection();
    selection.widgets.clear();
    let error = build_hud_ui_builder_readback(&selection).unwrap_err();
    assert!(error.to_string().contains("widgets"));
}

#[test]
fn runtime_rpg_no_map_access_rejects_minimap_markers() {
    let mut selection = sample_selection();
    selection.overlay_policy = MapOverlayPolicy::runtime_rpg_no_map_access();
    let error = build_hud_ui_builder_readback(&selection).unwrap_err();
    assert!(error.to_string().contains("fresh RPG runtime"));
}

#[test]
fn acquired_rpg_map_allows_scoped_landmarks_without_gps() {
    let mut selection = sample_selection();
    selection.overlay_policy = MapOverlayPolicy::runtime_rpg_diegetic_acquired(
        "inventory.map.galdmere_route",
        "region.galdmere",
    );
    selection.minimap_markers = vec![marker(
        "broken_lantern_shrine",
        MapOverlayMarkerKind::Landmark,
        MapOverlayMarkerSource::InventoryMap,
        MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
        Vec3::new(24.0, 0.0, -18.0),
        Vec4::new(0.82, 0.68, 0.42, 1.0),
    )];

    let readback = build_hud_ui_builder_readback(&selection).unwrap();

    assert_eq!(
        readback.overlay_policy_mode,
        MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired
    );
    assert!(readback.map_surface_visible);
    assert!(!readback.player_position_indicator);
    assert!(!readback.viewport_indicator);
    assert!(!readback.quest_markers);
    assert!(!readback.route_lines);
    assert_eq!(readback.minimap_marker_count, 1);
}

#[test]
fn acquired_rpg_map_rejects_quest_marker_gps() {
    let mut selection = sample_selection();
    selection.overlay_policy = MapOverlayPolicy::runtime_rpg_diegetic_acquired(
        "inventory.map.galdmere_route",
        "region.galdmere",
    );
    selection.minimap_markers = vec![marker(
        "quest_pin",
        MapOverlayMarkerKind::QuestObjective,
        MapOverlayMarkerSource::InventoryMap,
        MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
        Vec3::new(10.0, 0.0, 0.0),
        Vec4::new(1.0, 0.74, 0.18, 1.0),
    )];

    let error = build_hud_ui_builder_readback(&selection).unwrap_err();

    assert!(error.to_string().contains("marker kind/source"));
}

fn sample_selection() -> HudUiBuilderSelection {
    HudUiBuilderSelection {
        builder_id: "issue872.play_hud".to_string(),
        preset: HudLayoutPreset::PlayTopDown,
        viewport_width_px: 1920,
        viewport_height_px: 1080,
        widgets: vec![
            HudUiWidgetSpec {
                widget_id: "health_bar".to_string(),
                region: HudRegionKind::CommandBar,
                rect: rect([0.02, 0.58], [0.36, 0.82]),
                kind: HudWidgetKind::HealthBar {
                    current: 740.0,
                    max: 1000.0,
                },
            },
            HudUiWidgetSpec {
                widget_id: "attack_button".to_string(),
                region: HudRegionKind::CommandBar,
                rect: rect([0.44, 0.20], [0.52, 0.72]),
                kind: HudWidgetKind::CommandButton {
                    enabled: true,
                    cooldown: 0.0,
                },
            },
            HudUiWidgetSpec {
                widget_id: "gold_counter".to_string(),
                region: HudRegionKind::StatusStrip,
                rect: rect([0.10, 0.12], [0.20, 0.86]),
                kind: HudWidgetKind::ResourceCounter { amount: 320 },
            },
        ],
        text_runs: vec![
            HudUiTextSpec {
                text_id: "quest_title".to_string(),
                region: HudRegionKind::QuestTracker,
                offset_px: Vec2::new(18.0, 28.0),
                font_px: 22.0,
                text: "Quest: Light the gate".to_string(),
                color: Vec4::new(1.0, 0.86, 0.32, 1.0),
            },
            HudUiTextSpec {
                text_id: "resource_text".to_string(),
                region: HudRegionKind::StatusStrip,
                offset_px: Vec2::new(212.0, 10.0),
                font_px: 18.0,
                text: "Gold 320".to_string(),
                color: Vec4::new(0.95, 0.91, 0.82, 1.0),
            },
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

fn rect(min: [f32; 2], max: [f32; 2]) -> HudNormalizedRect {
    HudNormalizedRect {
        min: Vec2::new(min[0], min[1]),
        max: Vec2::new(max[0], max[1]),
    }
}
