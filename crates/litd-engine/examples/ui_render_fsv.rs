use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Aabb, Vec2, Vec3, Vec4},
    render::{
        camera::{RtsCamera, RtsCameraConfig, RtsCameraState},
        font::{FontAtlasBuildRequest, reference_windows_font_path},
        ui::{
            FloatingText, FogOfWarGrid, FontAtlas, HudLayoutPreset, HudRegionKind, HudWidget,
            HudWidgetKind, MapOverlayMarkerKind, MapOverlayMarkerSource, MapOverlayPolicy,
            MapOverlayPolicyMode, MinimapConfig, MinimapMarker, TextRun, UI_RENDER_WGSL, UiRect,
            UiRenderInputs, UiRenderSettings, VisionSource, build_ui_render_plan,
            reference_hud_layout,
        },
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_UI_RENDER_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1096-ui-render-fsv.txt"));

    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let layout = reference_hud_layout(settings, HudLayoutPreset::PlayTopDown)?;
    let command_bar = layout
        .region_rect(HudRegionKind::CommandBar)
        .expect("play preset must include a command bar");
    let status_strip = layout
        .region_rect(HudRegionKind::StatusStrip)
        .expect("play preset must include a status strip");
    let world = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(64.0))?;
    let camera = RtsCamera::new(
        RtsCameraConfig::standard(1920, 1080, world),
        RtsCameraState::isometric(Vec3::ZERO),
    )?;
    let inputs = UiRenderInputs {
        text_runs: vec![TextRun {
            id: "resource".to_string(),
            text: "Gold +125".to_string(),
            origin_px: Vec2::new(24.0, 22.0),
            font_px: 22.0,
            color: Vec4::new(1.0, 0.86, 0.25, 1.0),
        }],
        shaped_text_runs: Vec::new(),
        widgets: vec![
            HudWidget {
                id: "health".to_string(),
                rect_px: UiRect::new(
                    command_bar.min + Vec2::new(24.0, 92.0),
                    command_bar.min + Vec2::new(300.0, 124.0),
                )?,
                kind: HudWidgetKind::HealthBar {
                    current: 740.0,
                    max: 1000.0,
                },
            },
            HudWidget {
                id: "attack".to_string(),
                rect_px: UiRect::new(
                    command_bar.min + Vec2::new(340.0, 52.0),
                    command_bar.min + Vec2::new(404.0, 116.0),
                )?,
                kind: HudWidgetKind::CommandButton {
                    enabled: true,
                    cooldown: 0.0,
                },
            },
            HudWidget {
                id: "wood".to_string(),
                rect_px: UiRect::new(
                    status_strip.min + Vec2::new(180.0, 6.0),
                    status_strip.min + Vec2::new(300.0, 36.0),
                )?,
                kind: HudWidgetKind::ResourceCounter { amount: 320 },
            },
        ],
        floating_texts: vec![FloatingText {
            text: "+42!".to_string(),
            world_position: Vec3::ZERO,
            age_seconds: 0.4,
            lifetime_seconds: 1.2,
            rise_px_per_second: 38.0,
            color: Vec4::new(1.0, 0.18, 0.08, 1.0),
        }],
        minimap: MinimapConfig {
            world_bounds: world,
            rect_px: layout.minimap_rect_px,
        },
        overlay_policy: MapOverlayPolicy::runtime_strategy_minimap(),
        markers: vec![
            marker(
                "hero",
                MapOverlayMarkerKind::PlayerPosition,
                Vec3::new(0.0, 0.0, 0.0),
                Vec4::new(0.2, 0.8, 1.0, 1.0),
            ),
            marker(
                "enemy",
                MapOverlayMarkerKind::Danger,
                Vec3::new(32.0, 0.0, -24.0),
                Vec4::new(1.0, 0.1, 0.1, 1.0),
            ),
            marker(
                "gold",
                MapOverlayMarkerKind::Resource,
                Vec3::new(-40.0, 0.0, 28.0),
                Vec4::new(1.0, 0.8, 0.2, 1.0),
            ),
        ],
        viewport_world_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(18.0))?,
        fog: FogOfWarGrid::new(8, 8, world)?,
        vision_sources: vec![
            VisionSource {
                world_position: Vec3::ZERO,
                radius: 28.0,
            },
            VisionSource {
                world_position: Vec3::new(30.0, 0.0, -20.0),
                radius: 18.0,
            },
        ],
        view_projection: camera.view_projection_matrix(),
    };
    let atlas = FontAtlas::from_font_file(FontAtlasBuildRequest::for_text(
        reference_windows_font_path(),
        "Gold +125+42!",
        40.0,
        64,
        8,
        8,
    ))?
    .atlas;
    let plan = build_ui_render_plan(settings, &atlas, inputs)?;
    let viewport_rect = plan
        .minimap
        .viewport_rect_px
        .ok_or("strategy minimap should include a viewport rectangle")?;
    let invalid_config_code = match invalid_probe() {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => err.code(),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "pass_count={}\n\
             layout_preset={:?}\n\
             minimap_shape={:?}\n\
             command_slot_count={}\n\
             reference_images={}\n\
             glyph_quads={}\n\
             floating_glyph_quads={}\n\
             widget_count={}\n\
             minimap_markers={}\n\
             command_bar_min={:.3},{:.3}\n\
             command_bar_max={:.3},{:.3}\n\
             minimap_viewport_min={:.3},{:.3}\n\
             minimap_viewport_max={:.3},{:.3}\n\
             fog_hidden={}\n\
             fog_explored={}\n\
             fog_visible={}\n\
             wgsl_has_text={}\n\
             wgsl_has_minimap={}\n\
             wgsl_has_fog={}\n\
             invalid_config_code={}\n",
            plan.passes.len(),
            layout.preset,
            layout.minimap_shape,
            layout.command_slot_count,
            layout.reference_image_list(),
            plan.glyph_quads.len(),
            plan.floating_glyph_quads.len(),
            plan.widget_count,
            plan.minimap.markers.len(),
            command_bar.min.x,
            command_bar.min.y,
            command_bar.max.x,
            command_bar.max.y,
            viewport_rect.min.x,
            viewport_rect.min.y,
            viewport_rect.max.x,
            viewport_rect.max.y,
            plan.fog.hidden,
            plan.fog.explored,
            plan.fog.visible,
            UI_RENDER_WGSL.contains("litd_ui_sdf_alpha"),
            UI_RENDER_WGSL.contains("litd_minimap_project"),
            UI_RENDER_WGSL.contains("litd_fog_visibility"),
            invalid_config_code,
        ),
    )?;
    println!("wrote UI render FSV stats to {}", path.display());
    Ok(())
}

fn marker(
    id: &str,
    kind: MapOverlayMarkerKind,
    world_position: Vec3,
    color: Vec4,
) -> MinimapMarker {
    MinimapMarker {
        id: id.to_string(),
        kind,
        source: MapOverlayMarkerSource::StrategyRuntime,
        allowed_policy: MapOverlayPolicyMode::RuntimeStrategyMinimap,
        world_position,
        color,
        radius_px: 4.0,
    }
}

fn invalid_probe() -> Result<(), EngineError> {
    UiRenderSettings {
        viewport_width: 0.0,
        viewport_height: 1080.0,
    }
    .validate()
}
