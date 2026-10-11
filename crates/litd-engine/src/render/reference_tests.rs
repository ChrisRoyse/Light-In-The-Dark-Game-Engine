use super::*;

#[test]
fn reference_scene_composes_renderer_foundations() {
    let plan = build_reference_scene_plan(ReferenceSceneSettings {
        width: 640,
        height: 360,
    })
    .unwrap();
    assert_eq!(plan.pbr_material_count(), 4);
    assert_eq!(plan.lights.len(), 3);
    assert!(plan.light_assignment.non_empty_cluster_count() > 0);
    assert_eq!(plan.shadow_plan.directional_cascade_count(), 4);
    assert!(plan.post_plan.passes.len() >= 2);
    assert!(plan.screen_space_plan.passes.len() >= 4);
    assert_eq!(plan.stylized_library.preset_count(), 6);
    assert!(plan.batch_plan.visible_instance_ids.len() >= 3);
    assert_eq!(plan.ui_plan.passes.len(), 5);
    assert!(plan.reference_image_list().contains("GP03-combat.png"));
}

#[test]
fn reference_scene_image_passes_visual_target_bands() {
    let plan = build_reference_scene_plan(ReferenceSceneSettings {
        width: 640,
        height: 360,
    })
    .unwrap();
    let image = render_reference_scene_image(&plan).unwrap();
    let comparison = compare_reference_scene_image(&image);
    assert!(comparison.pass, "{}", comparison.band_summary());
    assert!(image.bmp_bytes().starts_with(b"BM"));
    assert_eq!(image.pixels().len(), 640 * 360);
}

#[test]
fn reference_scene_edge_metric_is_resolution_stable() {
    let canonical = reference_image_comparison(960, 540);
    let full_hd = reference_image_comparison(1920, 1080);
    let delta = (canonical.metrics.edge_contrast - full_hd.metrics.edge_contrast).abs();

    assert!(canonical.pass, "{}", canonical.band_summary());
    assert!(full_hd.pass, "{}", full_hd.band_summary());
    assert!(delta <= 0.008, "edge metric drifted by {delta}");
}

#[test]
fn flat_reference_image_fails_edge_contrast_band() {
    let image = ReferenceSceneImage::from_rgba_pixels(
        960,
        540,
        vec![[52, 52, 52, 255]; 960_usize * 540_usize],
    )
    .unwrap();
    let comparison = compare_reference_scene_image(&image);
    let edge = comparison
        .bands
        .iter()
        .find(|band| band.name == "edge_contrast")
        .unwrap();

    assert!(!comparison.pass);
    assert_eq!(edge.actual, 0.0);
    assert!(!edge.pass);
}

#[test]
fn invalid_reference_scene_size_fails_loud() {
    match build_reference_scene_plan(ReferenceSceneSettings {
        width: 0,
        height: 540,
    }) {
        Err(EngineError::ReferenceSceneInvalidConfig { field, .. }) => {
            assert_eq!(field, "visual_target");
        }
        other => panic!("expected ReferenceSceneInvalidConfig, got {other:?}"),
    }
}

fn reference_image_comparison(width: u32, height: u32) -> ReferenceSceneComparison {
    let plan = build_reference_scene_plan(ReferenceSceneSettings { width, height }).unwrap();
    let image = render_reference_scene_image(&plan).unwrap();
    compare_reference_scene_image(&image)
}

#[test]
fn reference_gpu_shader_declares_pipeline_contract() {
    assert!(GPU_REFERENCE_SCENE_WGSL.contains("@vertex"));
    assert!(GPU_REFERENCE_SCENE_WGSL.contains("litd_reference_scene_vs"));
    assert!(GPU_REFERENCE_SCENE_WGSL.contains("@fragment"));
    assert!(GPU_REFERENCE_SCENE_WGSL.contains("litd_reference_scene_fs"));
    assert!(GPU_REFERENCE_SCENE_WGSL.contains("litd_reference_tonemap"));
    assert!(GPU_REFERENCE_SCENE_WGSL.contains("point_color_intensity"));
}

#[test]
fn invalid_gpu_reference_scene_size_fails_before_adapter_request() {
    match pollster::block_on(render_reference_scene_gpu(ReferenceSceneSettings {
        width: 0,
        height: 540,
    })) {
        Err(EngineError::ReferenceSceneInvalidConfig { field, .. }) => {
            assert_eq!(field, "visual_target");
        }
        other => panic!("expected ReferenceSceneInvalidConfig, got {other:?}"),
    }
}
