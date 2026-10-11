use super::*;
use crate::render::font::{FontAtlasBuildRequest, reference_windows_font_path};

#[test]
fn text_layout_emits_one_quad_per_visible_character() {
    let atlas = FontAtlas::from_font_file(FontAtlasBuildRequest::for_text(
        reference_windows_font_path(),
        "Gold +125",
        40.0,
        64,
        8,
        8,
    ))
    .unwrap()
    .atlas;
    let run = TextRun {
        id: "gold".to_string(),
        text: "Gold +125".to_string(),
        origin_px: Vec2::new(12.0, 20.0),
        font_px: 20.0,
        color: Vec4::ONE,
    };
    let quads = layout_text_run(&atlas, &run).unwrap();
    assert_eq!(quads.len(), 9);
    assert_eq!(quads[0].codepoint, 'G' as u32);
}

#[test]
fn minimap_projects_world_center_to_rect_center() {
    let config = MinimapConfig {
        world_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(50.0)).unwrap(),
        rect_px: UiRect::new(Vec2::new(100.0, 100.0), Vec2::new(300.0, 300.0)).unwrap(),
    };
    let plan = build_minimap_plan(
        &MapOverlayPolicy::runtime_strategy_minimap(),
        &config,
        &[MinimapMarker {
            id: "hero".to_string(),
            kind: MapOverlayMarkerKind::PlayerPosition,
            source: MapOverlayMarkerSource::StrategyRuntime,
            allowed_policy: MapOverlayPolicyMode::RuntimeStrategyMinimap,
            world_position: Vec3::ZERO,
            color: Vec4::ONE,
            radius_px: 4.0,
        }],
        Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(10.0)).unwrap(),
    )
    .unwrap();
    assert_eq!(plan.markers[0].position_px, Vec2::new(200.0, 200.0));
    assert!(plan.viewport_rect_px.is_some());
}

#[test]
fn runtime_rpg_no_map_access_suppresses_minimap_pass() {
    let world = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(16.0)).unwrap();
    let settings = UiRenderSettings {
        viewport_width: 640.0,
        viewport_height: 360.0,
    };
    let atlas = FontAtlas::from_font_file(FontAtlasBuildRequest::for_text(
        reference_windows_font_path(),
        "HUD",
        40.0,
        64,
        8,
        8,
    ))
    .unwrap()
    .atlas;
    let plan = build_ui_render_plan(
        settings,
        &atlas,
        UiRenderInputs {
            text_runs: vec![TextRun {
                id: "hud".to_string(),
                text: "HUD".to_string(),
                origin_px: Vec2::new(8.0, 8.0),
                font_px: 16.0,
                color: Vec4::ONE,
            }],
            shaped_text_runs: Vec::new(),
            widgets: Vec::new(),
            floating_texts: Vec::new(),
            minimap: MinimapConfig {
                world_bounds: world,
                rect_px: UiRect::new(Vec2::new(480.0, 20.0), Vec2::new(620.0, 160.0)).unwrap(),
            },
            overlay_policy: MapOverlayPolicy::runtime_rpg_no_map_access(),
            markers: Vec::new(),
            viewport_world_bounds: world,
            fog: FogOfWarGrid::new(2, 2, world).unwrap(),
            vision_sources: Vec::new(),
            view_projection: Mat4::IDENTITY,
        },
    )
    .unwrap();

    assert!(!plan.passes.contains(&UiPassKind::Minimap));
    assert!(!plan.minimap.policy.surface_visible);
    assert!(plan.minimap.viewport_rect_px.is_none());
}

#[test]
fn acquired_rpg_map_rejects_player_position_marker() {
    let world = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(16.0)).unwrap();
    let config = MinimapConfig {
        world_bounds: world,
        rect_px: UiRect::new(Vec2::new(0.0, 0.0), Vec2::new(128.0, 128.0)).unwrap(),
    };
    let error = build_minimap_plan(
        &MapOverlayPolicy::runtime_rpg_diegetic_acquired(
            "inventory.map.galdmere_purchased",
            "region.galdmere",
        ),
        &config,
        &[MinimapMarker {
            id: "player_dot".to_string(),
            kind: MapOverlayMarkerKind::PlayerPosition,
            source: MapOverlayMarkerSource::InventoryMap,
            allowed_policy: MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
            world_position: Vec3::ZERO,
            color: Vec4::ONE,
            radius_px: 4.0,
        }],
        world,
    )
    .unwrap_err();

    assert!(error.to_string().contains("marker kind/source"));
}

#[test]
fn reference_layout_uses_readme_play_hud_shape() {
    let layout = reference_hud_layout(
        UiRenderSettings {
            viewport_width: 1920.0,
            viewport_height: 1080.0,
        },
        HudLayoutPreset::PlayTopDown,
    )
    .unwrap();
    assert_eq!(layout.minimap_shape, MinimapShape::Circular);
    assert_eq!(layout.command_slot_count, 10);
    assert!(layout.region_rect(HudRegionKind::CommandBar).is_some());
    assert!(
        layout
            .reference_image_list()
            .contains("ED12-play-topdown.png")
    );
    assert!(layout.reference_image_list().contains("GP03-combat.png"));
}

#[test]
fn fog_reveal_marks_cells_visible_and_preserves_explored() {
    let bounds = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(16.0)).unwrap();
    let grid = FogOfWarGrid::new(4, 4, bounds).unwrap();
    let revealed = reveal_fog(
        &grid,
        &[VisionSource {
            world_position: Vec3::ZERO,
            radius: 12.0,
        }],
    )
    .unwrap();
    let summary = summarize_fog(&revealed);
    assert!(summary.visible > 0);
    assert!(summary.hidden < 16);
}

#[test]
fn invalid_viewport_fails_loud() {
    match (UiRenderSettings {
        viewport_width: 0.0,
        viewport_height: 720.0,
    }
    .validate())
    {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => assert_eq!(field, "viewport"),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn issue1462_layout_uses_real_advances_not_fixed_width() {
    let atlas = FontAtlas::from_font_file(FontAtlasBuildRequest::for_text(
        reference_windows_font_path(),
        "iW",
        40.0,
        64,
        8,
        8,
    ))
    .unwrap()
    .atlas;
    let adv_i = atlas.glyph('i' as u32).unwrap().advance_width_px;
    let adv_w = atlas.glyph('W' as u32).unwrap().advance_width_px;
    // Proportional font: 'W' must be wider than 'i'. A fixed 0.6em advance could
    // never produce this inequality.
    assert!(
        adv_w > adv_i,
        "expected proportional advances, got i={adv_i} W={adv_w}"
    );

    let run_i = TextRun {
        id: "i".to_string(),
        text: "iiii".to_string(),
        origin_px: Vec2::new(0.0, 0.0),
        font_px: 40.0,
        color: Vec4::ONE,
    };
    let run_w = TextRun {
        text: "WWWW".to_string(),
        ..run_i.clone()
    };
    let quads_i = layout_text_run(&atlas, &run_i).unwrap();
    let quads_w = layout_text_run(&atlas, &run_w).unwrap();
    for quad in &quads_i {
        assert!((quad.rect_px.size().x - adv_i).abs() <= 0.001);
    }
    for quad in &quads_w {
        assert!((quad.rect_px.size().x - adv_w).abs() <= 0.001);
    }
    let width_i = quads_i.last().unwrap().rect_px.max.x - quads_i[0].rect_px.min.x;
    let width_w = quads_w.last().unwrap().rect_px.max.x - quads_w[0].rect_px.min.x;
    // The old code produced 4 * 40 * 0.6 = 96.0 for BOTH strings.
    assert!(
        (width_i - 96.0).abs() > 1.0,
        "iiii must not reproduce old fixed width"
    );
    assert!(
        width_w > width_i * 2.0,
        "WWWW must render far wider than iiii"
    );
}

#[test]
fn issue1462_glyph_uv_is_off_hot_path_but_layout_fails_loud() {
    let atlas = FontAtlas::from_font_file(FontAtlasBuildRequest::for_text(
        reference_windows_font_path(),
        "iW",
        40.0,
        64,
        8,
        8,
    ))
    .unwrap()
    .atlas;
    let mut tampered = atlas.clone();
    // Corrupt a glyph advance WITHOUT recomputing the atlas fnv1a64.
    tampered.glyphs[0].advance_width_px += 137.0;
    // glyph_uv must not re-validate (it would catch the fnv mismatch if it did).
    assert!(
        tampered.glyph_uv('i').is_ok(),
        "glyph_uv must not re-validate per lookup"
    );
    // The integrity boundary still catches the corruption.
    assert!(
        tampered.validate().is_err(),
        "validate() must catch a tampered atlas"
    );
    // layout validates once up front and fails loud.
    let run = TextRun {
        id: "t".to_string(),
        text: "iiii".to_string(),
        origin_px: Vec2::new(0.0, 0.0),
        font_px: 40.0,
        color: Vec4::ONE,
    };
    assert!(
        layout_text_run(&tampered, &run).is_err(),
        "layout must fail loud on a corrupted atlas"
    );
}
