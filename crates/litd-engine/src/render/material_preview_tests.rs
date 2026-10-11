use super::*;
use crate::math::Vec4;
use crate::render::material::{TextureBinding, TextureSlot};

fn base_material() -> PbrMaterial {
    let mut material = PbrMaterial::new("preview probe");
    material.base_color_factor = Vec4::new(0.8, 0.2, 0.1, 1.0);
    material.metallic_factor = 0.4;
    material.roughness_factor = 0.35;
    material.occlusion_strength = 0.9;
    material.emissive_factor = Vec3::new(0.02, 0.01, 0.0);
    material
}

#[test]
fn preview_uniform_mirrors_authored_factors() {
    let material = base_material()
        .with_texture(TextureBinding::new(TextureSlot::BaseColor, "base"))
        .with_texture(TextureBinding::new(TextureSlot::Normal, "normal"));
    let gpu = material.to_preview_gpu();
    assert_eq!(gpu.base_color, [0.8, 0.2, 0.1, 1.0]);
    assert_eq!(gpu.emissive, [0.02, 0.01, 0.0, 0.0]);
    assert_eq!(gpu.scalars, [0.4, 0.35, 1.0, 0.9]);
    // texture_mask, has_base_color, has_normal, has_emissive
    assert_eq!(gpu.flags[0], material.texture_mask());
    assert_eq!(gpu.flags[1], 1);
    assert_eq!(gpu.flags[2], 1);
    assert_eq!(gpu.flags[3], 0);
}

#[test]
fn preview_uniform_is_64_bytes_std140_friendly() {
    // Four vec4-sized rows: base_color, emissive, scalars, flags.
    assert_eq!(std::mem::size_of::<PbrMaterialGpu>(), 64);
    assert_eq!(std::mem::align_of::<PbrMaterialGpu>(), 4);
}

#[test]
fn fingerprint_is_deterministic_and_edit_sensitive() {
    let material = base_material();
    let baseline = material.preview_fingerprint();
    assert_eq!(
        baseline,
        material.preview_fingerprint(),
        "stable across calls"
    );

    let mut edited = material.clone();
    edited.roughness_factor = 0.36;
    assert_ne!(
        baseline,
        edited.preview_fingerprint(),
        "a roughness edit must move the fingerprint"
    );

    let mut recolored = material.clone();
    recolored.base_color_factor = Vec4::new(0.1, 0.2, 0.8, 1.0);
    assert_ne!(
        baseline,
        recolored.preview_fingerprint(),
        "an albedo edit must move the fingerprint"
    );

    // Assigning a map flips a flag bit, which is inside the fingerprinted bytes.
    let mapped = material
        .clone()
        .with_texture(TextureBinding::new(TextureSlot::Emissive, "glow"));
    assert_ne!(
        baseline,
        mapped.preview_fingerprint(),
        "a map assignment must move the fingerprint"
    );
}

#[test]
fn settings_validate_accepts_default_and_rejects_edges() {
    assert!(MaterialPreviewSettings::default().validate().is_ok());

    let zero = MaterialPreviewSettings {
        width: 0,
        ..MaterialPreviewSettings::default()
    };
    assert!(matches!(
        zero.validate(),
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { .. }
        ))
    ));

    let dark = MaterialPreviewSettings {
        light_direction: Vec3::ZERO,
        ..MaterialPreviewSettings::default()
    };
    assert!(matches!(
        dark.validate(),
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { .. }
        ))
    ));

    let huge_ball = MaterialPreviewSettings {
        sphere_radius: 1.5,
        ..MaterialPreviewSettings::default()
    };
    assert!(matches!(
        huge_ball.validate(),
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { .. }
        ))
    ));
}
