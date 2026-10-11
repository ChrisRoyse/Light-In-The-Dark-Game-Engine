use litd_engine::fsv_artifacts::env_path;
use std::fs;

use litd_engine::{
    EngineError,
    render::{
        batching::BATCHING_CULLING_WGSL,
        lighting::FORWARD_PLUS_WGSL,
        material::PBR_WGSL,
        post::POST_WGSL,
        reference::{
            ReferenceSceneSettings, build_reference_scene_plan, compare_reference_scene_image,
            render_reference_scene_image,
        },
        screen_space::SCREEN_SPACE_WGSL,
        shadow::SHADOW_WGSL,
        stylized::STYLIZED_EFFECTS_WGSL,
        ui::UI_RENDER_WGSL,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_REFERENCE_SCENE_FSV_STATS",
        "tmp/issue1097-reference-scene-fsv.txt",
    );
    let image_path = env_path(
        "LITD_REFERENCE_SCENE_FSV_IMAGE",
        "tmp/issue1097-reference-scene.bmp",
    );
    let plan = build_reference_scene_plan(ReferenceSceneSettings::default())?;
    let image = render_reference_scene_image(&plan)?;
    let comparison = compare_reference_scene_image(&image);
    if !comparison.pass {
        panic!(
            "reference scene visual bands failed: {}",
            comparison.band_summary()
        );
    }
    if let Some(parent) = image_path.parent() {
        fs::create_dir_all(parent)?;
    }
    image.write_bmp(&image_path)?;
    let image_bytes = fs::read(&image_path)?;
    let bmp_width = u32::from_le_bytes(image_bytes[18..22].try_into()?);
    let bmp_height = u32::from_le_bytes(image_bytes[22..26].try_into()?);
    let invalid_config_code = match invalid_probe() {
        Err(err @ EngineError::ReferenceSceneInvalidConfig { .. }) => err.code(),
        other => panic!("expected ReferenceSceneInvalidConfig, got {other:?}"),
    };

    if let Some(parent) = stats_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &stats_path,
        format!(
            "reference_images={}\n\
             image_path={}\n\
             image_bytes={}\n\
             image_checksum={}\n\
             bmp_signature={}\n\
             bmp_width={}\n\
             bmp_height={}\n\
             visual_comparison_pass={}\n\
             visual_band_summary={}\n\
             average_luma={:.6}\n\
             warm_highlight_ratio={:.6}\n\
             cool_shadow_ratio={:.6}\n\
             emissive_ratio={:.6}\n\
             edge_contrast={:.6}\n\
             hud_gold_ratio={:.6}\n\
             pbr_materials={}\n\
             light_count={}\n\
             clustered_non_empty={}\n\
             clustered_max_occupancy={}\n\
             shadow_entries={}\n\
             directional_cascades={}\n\
             point_shadow_faces={}\n\
             spot_shadow_maps={}\n\
             post_passes={}\n\
             screen_space_passes={}\n\
             stylized_presets={}\n\
             stylized_variants={}\n\
             visible_instances={}\n\
             draw_batches={}\n\
             ui_passes={}\n\
             ui_glyph_quads={}\n\
             ui_floating_glyph_quads={}\n\
             wgsl_has_pbr={}\n\
             wgsl_has_clustered={}\n\
             wgsl_has_shadow={}\n\
             wgsl_has_post={}\n\
             wgsl_has_screen_space={}\n\
             wgsl_has_stylized={}\n\
             wgsl_has_batching={}\n\
             wgsl_has_ui={}\n\
             invalid_config_code={}\n",
            plan.reference_image_list(),
            image_path.display(),
            image_bytes.len(),
            comparison.checksum,
            String::from_utf8_lossy(&image_bytes[0..2]),
            bmp_width,
            bmp_height,
            comparison.pass,
            comparison.band_summary(),
            comparison.metrics.average_luma,
            comparison.metrics.warm_highlight_ratio,
            comparison.metrics.cool_shadow_ratio,
            comparison.metrics.emissive_ratio,
            comparison.metrics.edge_contrast,
            comparison.metrics.hud_gold_ratio,
            plan.pbr_material_count(),
            plan.lights.len(),
            plan.light_assignment.non_empty_cluster_count(),
            plan.light_assignment.max_cluster_occupancy(),
            plan.shadow_plan.entries.len(),
            plan.shadow_plan.directional_cascade_count(),
            plan.shadow_plan.point_face_count(),
            plan.shadow_plan.spot_map_count(),
            plan.post_plan.passes.len(),
            plan.screen_space_plan.passes.len(),
            plan.stylized_library.preset_count(),
            plan.stylized_library.total_variant_count(),
            plan.batch_plan.visible_instance_ids.len(),
            plan.batch_plan.draw_batches.len(),
            plan.ui_plan.passes.len(),
            plan.ui_plan.glyph_quads.len(),
            plan.ui_plan.floating_glyph_quads.len(),
            PBR_WGSL.contains("distribution_ggx"),
            FORWARD_PLUS_WGSL.contains("litd_cluster_lights"),
            SHADOW_WGSL.contains("litd_shadow_pcf"),
            POST_WGSL.contains("litd_tonemap_aces"),
            SCREEN_SPACE_WGSL.contains("litd_ssao"),
            STYLIZED_EFFECTS_WGSL.contains("litd_stylized_fire_fs"),
            BATCHING_CULLING_WGSL.contains("litd_cull_instances"),
            UI_RENDER_WGSL.contains("litd_minimap_project"),
            invalid_config_code,
        ),
    )?;
    println!(
        "wrote reference scene FSV stats to {} and image to {}",
        stats_path.display(),
        image_path.display()
    );
    Ok(())
}

fn invalid_probe() -> Result<(), EngineError> {
    build_reference_scene_plan(ReferenceSceneSettings {
        width: 0,
        height: 540,
    })
    .map(|_| ())
}
