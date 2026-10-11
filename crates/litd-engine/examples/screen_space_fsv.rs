use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::screen_space::{
        AntiAliasingSettings, SCREEN_SPACE_WGSL, ScreenSpacePassKind, ScreenSpaceSettings,
        SsrSettings, build_screen_space_plan, frame_jitter, motion_vector_uv, ssao_reference,
        taa_resolve,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_SCREEN_SPACE_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1091-screen-space-fsv.txt"));

    let settings = ScreenSpaceSettings {
        anti_aliasing: AntiAliasingSettings {
            msaa_samples: 4,
            ..AntiAliasingSettings::default()
        },
        ssr: SsrSettings {
            enabled: true,
            ..SsrSettings::default()
        },
        ..ScreenSpaceSettings::default()
    };
    let plan = build_screen_space_plan(settings, 1920, 1080)?;
    let jitter = frame_jitter(settings.anti_aliasing, 5, 1920, 1080)?;
    let motion = motion_vector_uv(Vec2::new(0.12, -0.08), Vec2::new(0.10, -0.05))?;
    let ao = ssao_reference(settings.ssao, 0.6, Vec3::Z, &[0.55, 0.58, 0.62, 0.50, 0.70])?;
    let taa = taa_resolve(
        settings.anti_aliasing,
        Vec3::new(0.8, 0.72, 0.6),
        Vec3::new(0.7, 0.75, 0.65),
        motion,
    )?;
    let invalid_code = match invalid_probe() {
        Err(err @ EngineError::ScreenSpaceInvalidConfig { .. }) => err.code(),
        other => panic!("expected ScreenSpaceInvalidConfig, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "pass_count={}\n\
             ssao_passes={}\n\
             msaa_samples={}\n\
             fxaa_enabled={}\n\
             taa_enabled={}\n\
             ssr_enabled={}\n\
             jitter_sequence={}\n\
             jitter_sample_index={}\n\
             jitter_pixel={:.6},{:.6}\n\
             jitter_ndc={:.8},{:.8}\n\
             motion_vector_uv={:.6},{:.6}\n\
             ssao_visibility={:.6}\n\
             taa_rgb={:.6},{:.6},{:.6}\n\
             wgsl_has_ssao={}\n\
             wgsl_has_taa={}\n\
             wgsl_has_fxaa={}\n\
             invalid_config_code={}\n",
            plan.passes.len(),
            plan.pass_count(ScreenSpacePassKind::SsaoGather)
                + plan.pass_count(ScreenSpacePassKind::SsaoBlur),
            settings.anti_aliasing.msaa_samples,
            settings.anti_aliasing.fxaa_enabled,
            settings.anti_aliasing.taa_enabled,
            settings.ssr.enabled,
            settings.anti_aliasing.jitter_sequence_len,
            jitter.sample_index,
            jitter.pixel_offset.x,
            jitter.pixel_offset.y,
            jitter.ndc_offset.x,
            jitter.ndc_offset.y,
            motion.x,
            motion.y,
            ao,
            taa.x,
            taa.y,
            taa.z,
            SCREEN_SPACE_WGSL.contains("litd_ssao_occlusion"),
            SCREEN_SPACE_WGSL.contains("litd_taa_resolve"),
            SCREEN_SPACE_WGSL.contains("litd_fxaa_luma"),
            invalid_code,
        ),
    )?;
    println!("wrote screen-space FSV stats to {}", path.display());
    Ok(())
}

fn invalid_probe() -> Result<(), EngineError> {
    let settings = ScreenSpaceSettings {
        anti_aliasing: AntiAliasingSettings {
            msaa_samples: 3,
            ..AntiAliasingSettings::default()
        },
        ..ScreenSpaceSettings::default()
    };
    settings.validate()
}
