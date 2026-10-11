use super::*;

#[test]
fn expands_variants_and_validates_bindings() {
    let shader = fixture_shader();
    let plan = build_shader_plan(
        ShaderSystemSettings::default(),
        &shader,
        &fixture_bindings(),
    )
    .unwrap();
    assert_eq!(plan.variants.len(), 4);
    assert_eq!(plan.bindings.len(), 3);
    assert!(
        plan.variants
            .iter()
            .any(|variant| variant.key == "FOG=ON;STYLE=TOON")
    );
}

#[test]
fn binding_mismatch_fails_loud() {
    let shader = fixture_shader();
    let mut layout = fixture_bindings();
    layout[2].kind = BindingKind::UniformBuffer;
    match build_shader_plan(ShaderSystemSettings::default(), &shader, &layout) {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::ShaderBindingValidationFailed {
                shader_id,
                binding,
                ..
            },
        )) => {
            assert_eq!(shader_id, "litd.test.shader");
            assert_eq!(binding, "group=1,binding=1");
        }
        other => panic!("expected ShaderBindingValidationFailed, got {other:?}"),
    }
}

#[test]
fn missing_entry_point_fails_loud() {
    let shader = ShaderSource::new(
        "litd.bad.shader",
        "fn other() {}",
        ShaderEntryPoints::compute("missing_main"),
    );
    match build_shader_plan(ShaderSystemSettings::default(), &shader, &[]) {
        Err(EngineError::Rendering(litd_engine::error::RenderingError::ShaderInvalidConfig {
            field,
            ..
        })) => {
            assert_eq!(field, "shader.entry_points.compute");
        }
        other => panic!("expected ShaderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn reload_decision_detects_hash_change() {
    let previous = HotReloadStamp {
        path: PathBuf::from("shader.wgsl"),
        modified_ms: 10,
        source_hash: shader_fingerprint("fn a() {}"),
    };
    let current = HotReloadStamp {
        source_hash: shader_fingerprint("fn b() {}"),
        ..previous.clone()
    };
    let decision = ShaderReloadDecision::compare(&previous, &current);
    assert!(decision.changed);
    assert_ne!(decision.previous_hash, decision.current_hash);
}

fn fixture_shader() -> ShaderSource {
    ShaderSource::new(
        "litd.test.shader",
        SHADER_SYSTEM_WGSL,
        ShaderEntryPoints::graphics("litd_shader_system_vs", "litd_shader_system_fs"),
    )
    .with_bindings(fixture_bindings())
    .with_variant_axes(vec![
        VariantAxis::new("STYLE", vec!["TOON", "WATER"]),
        VariantAxis::new("FOG", vec!["OFF", "ON"]),
    ])
}

fn fixture_bindings() -> Vec<BindingDecl> {
    vec![
        BindingDecl::new(0, 0, vec![ShaderStage::Vertex], BindingKind::UniformBuffer),
        BindingDecl::new(1, 0, vec![ShaderStage::Fragment], BindingKind::Texture),
        BindingDecl::new(1, 1, vec![ShaderStage::Fragment], BindingKind::Sampler),
    ]
}
