use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::Vec3,
    render::post::{
        BloomSettings, ColorGrade, POST_WGSL, PostProcessSettings, apply_post_process_reference,
        build_post_process_plan,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_POST_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1090-post-fsv.txt"));

    let defaults = PostProcessSettings::default();
    let settings = PostProcessSettings {
        exposure_ev: 0.35,
        bloom: BloomSettings {
            threshold: 0.9,
            intensity: 0.28,
            ..defaults.bloom
        },
        color_grade: ColorGrade {
            saturation: 1.18,
            contrast: 1.12,
            ..defaults.color_grade
        },
        ..defaults
    };

    let plan = build_post_process_plan(settings, 1920, 1080)?;
    let result = apply_post_process_reference(settings, Vec3::new(4.2, 1.3, 0.35))?;
    let invalid_code = match invalid_probe(settings) {
        Err(err @ EngineError::PostProcessInvalidConfig { .. }) => err.code(),
        other => panic!("expected PostProcessInvalidConfig, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "pass_count={}\n\
             bloom_mips={}\n\
             downsample_passes={}\n\
             upsample_passes={}\n\
             tonemap_operator=aces\n\
             exposed_hdr_rgb={:.6},{:.6},{:.6}\n\
             bloom_rgb={:.6},{:.6},{:.6}\n\
             tonemapped_rgb={:.6},{:.6},{:.6}\n\
             output_rgb={:.6},{:.6},{:.6}\n\
             lut_size={}\n\
             lut_strength={:.3}\n\
             wgsl_has_aces={}\n\
             wgsl_has_bloom={}\n\
             wgsl_has_lut={}\n\
             invalid_config_code={}\n",
            plan.passes.len(),
            settings.bloom.mip_count,
            plan.downsample_pass_count(),
            plan.upsample_pass_count(),
            result.exposed_hdr.x,
            result.exposed_hdr.y,
            result.exposed_hdr.z,
            result.bloom.x,
            result.bloom.y,
            result.bloom.z,
            result.tonemapped.x,
            result.tonemapped.y,
            result.tonemapped.z,
            result.output.x,
            result.output.y,
            result.output.z,
            settings.color_grade.lut_size,
            settings.color_grade.lut_strength,
            POST_WGSL.contains("litd_tonemap_aces"),
            POST_WGSL.contains("litd_bloom_prefilter"),
            POST_WGSL.contains("litd_apply_lut_grade"),
            invalid_code,
        ),
    )?;
    println!("wrote post-processing FSV stats to {}", path.display());
    Ok(())
}

fn invalid_probe(mut settings: PostProcessSettings) -> Result<(), EngineError> {
    settings.color_grade.lut_size = 1;
    settings.validate()
}
