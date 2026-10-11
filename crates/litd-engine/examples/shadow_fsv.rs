use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::Vec3,
    render::{
        lighting::{RenderLight, SpotCone},
        shadow::{SHADOW_WGSL, ShadowCamera, ShadowSettings, build_shadow_plan},
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_SHADOW_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1089-shadow-fsv.txt"));

    let camera = ShadowCamera {
        position: Vec3::new(0.0, 8.0, -12.0),
        forward: Vec3::new(0.0, -0.35, 1.0),
        up: Vec3::Y,
        fov_y_rad: 60.0_f32.to_radians(),
        aspect: 16.0 / 9.0,
        near: 0.5,
        far: 80.0,
    };
    let settings = ShadowSettings::default();
    let lights = vec![
        RenderLight::directional("sun", Vec3::new(0.2, 0.6, 1.0), Vec3::ONE, 3.0),
        RenderLight::point("torch", Vec3::new(2.0, 2.0, 3.0), 10.0, Vec3::ONE, 6.0),
        RenderLight::spot(
            "gate",
            Vec3::new(-3.0, 4.0, 1.0),
            Vec3::new(0.3, -1.0, 0.6),
            18.0,
            SpotCone::new(0.35, 0.8),
            Vec3::ONE,
            5.0,
        ),
    ];
    let plan = build_shadow_plan(settings, camera, &lights)?;
    let overflow_code = match overflow_probe(camera, &lights) {
        Err(err @ EngineError::ShadowAtlasOverflow { .. }) => err.code(),
        other => panic!("expected ShadowAtlasOverflow, got {other:?}"),
    };
    let first = &plan.entries[0];
    let last = plan
        .entries
        .last()
        .expect("shadow plan should have entries");

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "entry_count={}\n\
             directional_cascades={}\n\
             point_faces={}\n\
             spot_maps={}\n\
             atlas_tile_capacity={}\n\
             first_tile={},{},{}\n\
             last_tile={},{},{}\n\
             first_cascade_near_far={:.6},{:.6}\n\
             last_vp_determinant={:.6}\n\
             pcf_radius={}\n\
             wgsl_has_pcf={}\n\
             wgsl_has_compare_sampler={}\n\
             overflow_error_code={}\n",
            plan.entries.len(),
            plan.directional_cascade_count(),
            plan.point_face_count(),
            plan.spot_map_count(),
            plan.settings.tile_capacity()?,
            first.tile.x,
            first.tile.y,
            first.tile.size,
            last.tile.x,
            last.tile.y,
            last.tile.size,
            match first.kind {
                litd_engine::render::shadow::ShadowMapKind::DirectionalCascade {
                    split_near,
                    split_far: _,
                    ..
                } => split_near,
                _ => 0.0,
            },
            match first.kind {
                litd_engine::render::shadow::ShadowMapKind::DirectionalCascade {
                    split_near: _,
                    split_far,
                    ..
                } => split_far,
                _ => 0.0,
            },
            last.view_projection.determinant(),
            plan.settings.pcf_radius,
            SHADOW_WGSL.contains("litd_shadow_pcf"),
            SHADOW_WGSL.contains("textureSampleCompare"),
            overflow_code,
        ),
    )?;
    println!("wrote shadow FSV stats to {}", path.display());
    Ok(())
}

fn overflow_probe(camera: ShadowCamera, lights: &[RenderLight]) -> Result<(), EngineError> {
    let settings = ShadowSettings {
        atlas_size: 1024,
        tile_size: 1024,
        ..ShadowSettings::default()
    };
    build_shadow_plan(settings, camera, lights).map(|_| ())
}
