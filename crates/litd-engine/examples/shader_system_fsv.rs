use std::{env, fs, path::PathBuf, thread, time::Duration};

use litd_engine::{
    EngineError,
    render::shader::{
        BindingDecl, BindingKind, SHADER_SYSTEM_WGSL, ShaderEntryPoints, ShaderReloadDecision,
        ShaderStage, ShaderSystemSettings, VariantAxis, build_shader_plan, load_wgsl_file,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env::var("LITD_SHADER_SYSTEM_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1093-shader-system-fsv.txt"));
    let shader_path = stats_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("tmp"))
        .join("issue1093-custom.wgsl");

    if let Some(parent) = stats_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&shader_path, custom_wgsl(1))?;
    let initial = load_wgsl_file(
        "litd.issue1093.custom",
        &shader_path,
        ShaderEntryPoints::graphics("litd_custom_vs", "litd_custom_fs"),
        fixture_bindings(),
        fixture_axes(),
    )?;
    let initial_plan = build_shader_plan(
        ShaderSystemSettings::default(),
        &initial,
        &fixture_bindings(),
    )?;

    thread::sleep(Duration::from_millis(5));
    fs::write(&shader_path, custom_wgsl(2))?;
    let reloaded = load_wgsl_file(
        "litd.issue1093.custom",
        &shader_path,
        ShaderEntryPoints::graphics("litd_custom_vs", "litd_custom_fs"),
        fixture_bindings(),
        fixture_axes(),
    )?;
    let reloaded_plan = build_shader_plan(
        ShaderSystemSettings::default(),
        &reloaded,
        &fixture_bindings(),
    )?;
    let reload = ShaderReloadDecision::compare(
        initial_plan.hot_reload.as_ref().unwrap(),
        reloaded_plan.hot_reload.as_ref().unwrap(),
    );
    let invalid_binding_code = match invalid_binding_probe(&initial) {
        Err(
            err @ EngineError::Rendering(
                litd_engine::error::RenderingError::ShaderBindingValidationFailed { .. },
            ),
        ) => err.code(),
        other => panic!("expected ShaderBindingValidationFailed, got {other:?}"),
    };

    fs::write(
        &stats_path,
        format!(
            "shader_id={}\n\
             source_hash={}\n\
             variant_count={}\n\
             variant_keys={}\n\
             binding_count={}\n\
             entry_points=vertex:{}|fragment:{}\n\
             hot_reload_path={}\n\
             hot_reload_changed={}\n\
             reload_previous_hash={}\n\
             reload_current_hash={}\n\
             wgsl_has_vertex_entry={}\n\
             wgsl_has_fragment_entry={}\n\
             source_binding_validation={}\n\
             builtin_contract_has_uniform={}\n\
             invalid_binding_code={}\n",
            initial_plan.id,
            initial_plan.source_hash,
            initial_plan.variants.len(),
            initial_plan
                .variants
                .iter()
                .map(|variant| variant.key.as_str())
                .collect::<Vec<_>>()
                .join("|"),
            initial_plan.bindings.len(),
            initial_plan.entry_points.vertex.as_deref().unwrap_or(""),
            initial_plan.entry_points.fragment.as_deref().unwrap_or(""),
            shader_path.display(),
            reload.changed,
            reload.previous_hash,
            reload.current_hash,
            initial.source.contains("@vertex"),
            initial.source.contains("@fragment"),
            true,
            SHADER_SYSTEM_WGSL.contains("var<uniform>"),
            invalid_binding_code,
        ),
    )?;
    println!("wrote shader-system FSV stats to {}", stats_path.display());
    Ok(())
}

fn invalid_binding_probe(
    shader: &litd_engine::render::shader::ShaderSource,
) -> Result<(), EngineError> {
    let mut layout = fixture_bindings();
    layout[2].kind = BindingKind::UniformBuffer;
    build_shader_plan(ShaderSystemSettings::default(), shader, &layout).map(|_| ())
}

fn fixture_bindings() -> Vec<BindingDecl> {
    vec![
        BindingDecl::new(0, 0, vec![ShaderStage::Vertex], BindingKind::UniformBuffer),
        BindingDecl::new(1, 0, vec![ShaderStage::Fragment], BindingKind::Texture),
        BindingDecl::new(1, 1, vec![ShaderStage::Fragment], BindingKind::Sampler),
    ]
}

fn fixture_axes() -> Vec<VariantAxis> {
    vec![
        VariantAxis::new("STYLE", vec!["TOON", "WATER"]),
        VariantAxis::new("FOG", vec!["OFF", "ON"]),
    ]
}

fn custom_wgsl(version: u32) -> String {
    format!(
        "struct Camera {{ view_proj: mat4x4<f32>, }};\n\
         @group(0) @binding(0) var<uniform> camera: Camera;\n\
         @group(1) @binding(0) var albedo_tex: texture_2d<f32>;\n\
         @group(1) @binding(1) var albedo_sampler: sampler;\n\
         struct VsIn {{ @location(0) position: vec3<f32>, @location(1) uv: vec2<f32>, }};\n\
         struct VsOut {{ @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, }};\n\
         @vertex fn litd_custom_vs(input: VsIn) -> VsOut {{\n\
         var out: VsOut;\n\
         out.position = camera.view_proj * vec4<f32>(input.position, 1.0);\n\
         out.uv = input.uv;\n\
         return out;\n\
         }}\n\
         @fragment fn litd_custom_fs(input: VsOut) -> @location(0) vec4<f32> {{\n\
         let tint = {version}.0 * 0.05;\n\
         return textureSample(albedo_tex, albedo_sampler, input.uv) + vec4<f32>(tint, 0.0, 0.0, 0.0);\n\
         }}\n"
    )
}
