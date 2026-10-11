use litd_engine::fsv_artifacts::env_path;
use std::fs;

use litd_engine::{
    EngineError,
    render::reference::{
        GPU_REFERENCE_SCENE_WGSL, ReferenceSceneSettings, render_reference_scene_gpu,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    pollster::block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let stats_path = env_path(
        "LITD_REFERENCE_GPU_SCENE_FSV_STATS",
        "tmp/issue1238-reference-gpu-scene-fsv.txt",
    );
    let image_path = env_path(
        "LITD_REFERENCE_GPU_SCENE_FSV_IMAGE",
        "tmp/issue1238-reference-gpu-scene.bmp",
    );
    let frame = render_reference_scene_gpu(ReferenceSceneSettings::default()).await?;
    if let Some(parent) = image_path.parent() {
        fs::create_dir_all(parent)?;
    }
    frame.image.write_bmp(&image_path)?;
    let image_bytes = fs::read(&image_path)?;
    let bmp_width = u32::from_le_bytes(image_bytes[18..22].try_into()?);
    let bmp_height = u32::from_le_bytes(image_bytes[22..26].try_into()?);
    let invalid_config_code = match invalid_probe().await {
        Err(err @ EngineError::ReferenceSceneInvalidConfig { .. }) => err.code(),
        other => panic!("expected ReferenceSceneInvalidConfig, got {other:?}"),
    };

    if let Some(parent) = stats_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &stats_path,
        format!(
            "gpu_rendered=true\n\
             image_path={}\n\
             image_bytes={}\n\
             image_checksum={}\n\
             bmp_signature={}\n\
             bmp_width={}\n\
             bmp_height={}\n\
             adapter_name={}\n\
             backend={}\n\
             texture_format={}\n\
             mesh_count={}\n\
             instance_count={}\n\
             draw_calls={}\n\
             readback_bytes={}\n\
             row_pitch={}\n\
             visual_comparison_pass={}\n\
             visual_band_summary={}\n\
             average_luma={:.6}\n\
             warm_highlight_ratio={:.6}\n\
             cool_shadow_ratio={:.6}\n\
             emissive_ratio={:.6}\n\
             edge_contrast={:.6}\n\
             hud_gold_ratio={:.6}\n\
             shader_has_vertex={}\n\
             shader_has_fragment={}\n\
             shader_has_tonemap={}\n\
             shader_has_point_light={}\n\
             invalid_config_code={}\n",
            image_path.display(),
            image_bytes.len(),
            frame.comparison.checksum,
            String::from_utf8_lossy(&image_bytes[0..2]),
            bmp_width,
            bmp_height,
            frame.adapter_name,
            frame.backend,
            frame.texture_format,
            frame.mesh_count,
            frame.instance_count,
            frame.draw_calls,
            frame.readback_bytes,
            frame.row_pitch,
            frame.comparison.pass,
            frame.comparison.band_summary(),
            frame.comparison.metrics.average_luma,
            frame.comparison.metrics.warm_highlight_ratio,
            frame.comparison.metrics.cool_shadow_ratio,
            frame.comparison.metrics.emissive_ratio,
            frame.comparison.metrics.edge_contrast,
            frame.comparison.metrics.hud_gold_ratio,
            GPU_REFERENCE_SCENE_WGSL.contains("litd_reference_scene_vs"),
            GPU_REFERENCE_SCENE_WGSL.contains("litd_reference_scene_fs"),
            GPU_REFERENCE_SCENE_WGSL.contains("litd_reference_tonemap"),
            GPU_REFERENCE_SCENE_WGSL.contains("point_color_intensity"),
            invalid_config_code,
        ),
    )?;
    if !frame.comparison.pass {
        panic!(
            "GPU reference scene visual bands failed: {}",
            frame.comparison.band_summary()
        );
    }
    println!(
        "wrote GPU reference scene FSV stats to {} and image to {}",
        stats_path.display(),
        image_path.display()
    );
    Ok(())
}

async fn invalid_probe() -> Result<(), EngineError> {
    render_reference_scene_gpu(ReferenceSceneSettings {
        width: 0,
        height: 540,
    })
    .await
    .map(|_| ())
}
