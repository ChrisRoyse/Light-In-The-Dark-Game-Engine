use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Vec3, Vec4},
    render::material::{
        IblEnvironment, PBR_WGSL, PbrLight, PbrMaterial, PbrSample, TextureBinding, TextureSlot,
        shade,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_PBR_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1087-pbr-fsv.txt"));

    let mut material = PbrMaterial::new("Ash Iron");
    material.base_color_factor = Vec4::new(0.8, 0.2, 0.1, 1.0);
    material.metallic_factor = 0.5;
    material.roughness_factor = 0.35;
    material.occlusion_strength = 0.9;
    material.emissive_factor = Vec3::new(0.02, 0.01, 0.0);
    material = material
        .with_texture(TextureBinding::new(TextureSlot::BaseColor, "ash-iron-base"))
        .with_texture(TextureBinding::new(
            TextureSlot::MetallicRoughness,
            "ash-iron-mr",
        ))
        .with_texture(TextureBinding::new(TextureSlot::Normal, "ash-iron-normal"))
        .with_texture(TextureBinding::new(
            TextureSlot::AmbientOcclusion,
            "ash-iron-ao",
        ))
        .with_texture(TextureBinding::new(
            TextureSlot::Emissive,
            "ash-iron-emissive",
        ))
        .with_texture(TextureBinding::new(
            TextureSlot::Irradiance,
            "studio-irradiance",
        ))
        .with_texture(TextureBinding::new(
            TextureSlot::PrefilteredEnvironment,
            "studio-prefilter",
        ))
        .with_texture(TextureBinding::new(TextureSlot::BrdfLut, "brdf-lut"));

    let result = shade(
        &material,
        PbrSample {
            normal: Vec3::Z,
            view: Vec3::Z,
            light: PbrLight {
                direction_to_light: Vec3::new(0.0, 0.25, 1.0),
                radiance: Vec3::splat(3.0),
            },
            environment: IblEnvironment::default(),
        },
    )?;
    let invalid_code = {
        let mut invalid = material.clone();
        invalid.roughness_factor = 0.0;
        match invalid.validate() {
            Err(
                err @ EngineError::Rendering(
                    litd_engine::error::RenderingError::PbrMaterialInvalidFactor { .. },
                ),
            ) => err.code(),
            other => panic!("expected PbrMaterialInvalidFactor, got {other:?}"),
        }
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "material_name={}\n\
             texture_count={}\n\
             texture_mask={}\n\
             has_base_color={}\n\
             has_normal={}\n\
             has_ibl={}\n\
             direct_rgb={:.6},{:.6},{:.6}\n\
             ibl_rgb={:.6},{:.6},{:.6}\n\
             emissive_rgb={:.6},{:.6},{:.6}\n\
             final_rgb={:.6},{:.6},{:.6}\n\
             wgsl_has_ggx={}\n\
             invalid_factor_code={}\n",
            material.name,
            material.textures.len(),
            material.texture_mask(),
            material.has_texture(TextureSlot::BaseColor),
            material.has_texture(TextureSlot::Normal),
            material.has_texture(TextureSlot::Irradiance)
                && material.has_texture(TextureSlot::PrefilteredEnvironment)
                && material.has_texture(TextureSlot::BrdfLut),
            result.direct.x,
            result.direct.y,
            result.direct.z,
            result.ibl.x,
            result.ibl.y,
            result.ibl.z,
            result.emissive.x,
            result.emissive.y,
            result.emissive.z,
            result.color.x,
            result.color.y,
            result.color.z,
            PBR_WGSL.contains("litd_distribution_ggx"),
            invalid_code,
        ),
    )?;
    println!("wrote PBR material FSV stats to {}", path.display());
    Ok(())
}
