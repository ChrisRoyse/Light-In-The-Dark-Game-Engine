use super::*;
use crate::render::shader::ShaderEntryPoints;

#[test]
fn library_contains_all_curated_presets() {
    let library = build_stylized_shader_library(ShaderSystemSettings::default()).unwrap();
    assert_eq!(library.preset_count(), 6);
    assert!(library.find(StylizedPresetKind::Water).is_some());
    assert!(library.find(StylizedPresetKind::Fire).is_some());
    assert!(library.find(StylizedPresetKind::Glow).is_some());
    assert!(library.find(StylizedPresetKind::Toon).is_some());
    assert!(library.find(StylizedPresetKind::Dissolve).is_some());
    assert!(library.find(StylizedPresetKind::ForceField).is_some());
}

#[test]
fn default_materials_validate_for_every_preset() {
    let library = build_stylized_shader_library(ShaderSystemSettings::default()).unwrap();
    for plan in &library.presets {
        let material = default_material(&plan.preset);
        validate_material(&plan.preset, &material).unwrap();
        assert_eq!(plan.shader_plan.bindings.len(), 4);
    }
}

#[test]
fn scalar_out_of_range_fails_loud() {
    let library = build_stylized_shader_library(ShaderSystemSettings::default()).unwrap();
    let water = &library.find(StylizedPresetKind::Water).unwrap().preset;
    let mut material = default_material(water);
    material
        .values
        .insert("wave_strength".to_string(), StylizedValue::Scalar(9.0));

    match validate_material(water, &material) {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::StylizedShaderInvalidConfig {
                preset, field, ..
            },
        )) => {
            assert_eq!(preset, "water");
            assert_eq!(field, "wave_strength");
        }
        other => panic!("expected StylizedShaderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn wgsl_contains_all_effect_entry_points() {
    assert!(STYLIZED_EFFECTS_WGSL.contains("litd_stylized_water_fs"));
    assert!(STYLIZED_EFFECTS_WGSL.contains("litd_stylized_fire_fs"));
    assert!(STYLIZED_EFFECTS_WGSL.contains("litd_stylized_glow_fs"));
    assert!(STYLIZED_EFFECTS_WGSL.contains("litd_stylized_toon_fs"));
    assert!(STYLIZED_EFFECTS_WGSL.contains("litd_stylized_dissolve_fs"));
    assert!(STYLIZED_EFFECTS_WGSL.contains("litd_stylized_force_field_fs"));
}

#[test]
fn gallery_readback_accepts_curated_defaults_and_wgsl_escape() {
    let library = build_stylized_shader_library(ShaderSystemSettings::default()).unwrap();
    let mut selection = StylizedGallerySelection::curated_defaults(&library).unwrap();
    selection.selected_preset = StylizedPresetKind::Glow;
    selection.custom_wgsl = Some(WgslShaderEscape {
        shader_id: "litd.test.wgsl_escape".to_string(),
        source: custom_wgsl_escape(),
        entry_points: ShaderEntryPoints::graphics("test_vs", "test_fs"),
        bindings: Vec::new(),
        variant_axes: Vec::new(),
    });

    let readback =
        build_stylized_gallery_readback(ShaderSystemSettings::default(), &library, &selection)
            .unwrap();
    assert_eq!(readback.selected_preset, "glow");
    assert_eq!(readback.library_preset_count, 6);
    assert_eq!(readback.material_count, 6);
    assert_eq!(
        readback.custom_wgsl.unwrap().shader_id,
        "litd.test.wgsl_escape"
    );
}

#[test]
fn gallery_rejects_glsl_escape() {
    let library = build_stylized_shader_library(ShaderSystemSettings::default()).unwrap();
    let mut selection = StylizedGallerySelection::curated_defaults(&library).unwrap();
    selection.custom_wgsl = Some(WgslShaderEscape {
        shader_id: "litd.test.glsl_escape".to_string(),
        source: "#version 450\nvoid main() { gl_FragColor = vec4(1.0); }".to_string(),
        entry_points: ShaderEntryPoints::graphics("test_vs", "test_fs"),
        bindings: Vec::new(),
        variant_axes: Vec::new(),
    });

    match build_stylized_gallery_readback(ShaderSystemSettings::default(), &library, &selection) {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::StylizedShaderInvalidConfig { field, .. },
        )) => {
            assert_eq!(field, "custom_wgsl.source");
        }
        other => panic!("expected StylizedShaderInvalidConfig, got {other:?}"),
    }
}

fn custom_wgsl_escape() -> String {
    r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn test_vs(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VertexOut;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    return out;
}

@fragment
fn test_fs() -> @location(0) vec4<f32> {
    return vec4<f32>(0.2, 0.7, 1.0, 1.0);
}
"#
    .to_string()
}
