use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    render::{
        shader::ShaderSystemSettings,
        stylized::{
            STYLIZED_EFFECTS_WGSL, StylizedPresetKind, StylizedValue,
            build_stylized_shader_library, default_material, validate_material,
        },
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_STYLIZED_SHADER_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1094-stylized-shader-fsv.txt"));

    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    for plan in &library.presets {
        validate_material(&plan.preset, &default_material(&plan.preset))?;
    }
    let water = &library.find(StylizedPresetKind::Water).unwrap().preset;
    let fire = library.find(StylizedPresetKind::Fire).unwrap();
    let invalid_config_code = match invalid_material_probe(water) {
        Err(
            err @ EngineError::Rendering(
                litd_engine::error::RenderingError::StylizedShaderInvalidConfig { .. },
            ),
        ) => err.code(),
        other => panic!("expected StylizedShaderInvalidConfig, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "preset_count={}\n\
             preset_ids={}\n\
             total_parameters={}\n\
             total_variants={}\n\
             binding_count={}\n\
             water_parameters={}\n\
             water_variant_count={}\n\
             fire_shader_id={}\n\
             fire_variant_count={}\n\
             default_materials_valid=true\n\
             wgsl_has_water={}\n\
             wgsl_has_fire={}\n\
             wgsl_has_glow={}\n\
             wgsl_has_toon={}\n\
             wgsl_has_dissolve={}\n\
             wgsl_has_force_field={}\n\
             invalid_config_code={}\n",
            library.preset_count(),
            library.preset_ids(),
            library.total_parameter_count(),
            library.total_variant_count(),
            fire.shader_plan.bindings.len(),
            water.parameters.len(),
            library
                .find(StylizedPresetKind::Water)
                .unwrap()
                .shader_plan
                .variants
                .len(),
            fire.shader_plan.id,
            fire.shader_plan.variants.len(),
            STYLIZED_EFFECTS_WGSL.contains("litd_stylized_water_fs"),
            STYLIZED_EFFECTS_WGSL.contains("litd_stylized_fire_fs"),
            STYLIZED_EFFECTS_WGSL.contains("litd_stylized_glow_fs"),
            STYLIZED_EFFECTS_WGSL.contains("litd_stylized_toon_fs"),
            STYLIZED_EFFECTS_WGSL.contains("litd_stylized_dissolve_fs"),
            STYLIZED_EFFECTS_WGSL.contains("litd_stylized_force_field_fs"),
            invalid_config_code,
        ),
    )?;
    println!("wrote stylized shader FSV stats to {}", path.display());
    Ok(())
}

fn invalid_material_probe(
    preset: &litd_engine::render::stylized::StylizedShaderPreset,
) -> Result<(), EngineError> {
    let mut material = default_material(preset);
    material
        .values
        .insert("wave_strength".to_string(), StylizedValue::Scalar(9.0));
    validate_material(preset, &material)
}
