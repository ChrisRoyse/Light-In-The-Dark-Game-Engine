use crate::{
    EngineError,
    math::{Vec3, Vec4},
    render::{
        catalog::{
            ShaderMaterialCatalogInput, ShaderMaterialCatalogSettings, ShaderMaterialRecordInput,
            ShaderMaterialReuseRelationship, build_shader_material_catalog,
        },
        material::{PbrMaterial, TextureBinding, TextureSlot},
        shader::{
            BindingDecl, BindingKind, SHADER_SYSTEM_WGSL, ShaderEntryPoints, ShaderPlan,
            ShaderSource, ShaderStage, ShaderSystemSettings, VariantAxis, build_shader_plan,
        },
    },
};

#[test]
fn catalog_flags_near_duplicate_materials_without_flagging_distinct_records() {
    let shader = fixture_shader_plan();
    let low = shader
        .variants
        .iter()
        .find(|variant| variant.key == "QUALITY=LOW")
        .unwrap();
    let records = vec![
        ShaderMaterialRecordInput::pbr_variant(
            "ash-a",
            "Ash Stone A",
            &shader,
            low,
            fixture_material("Ash Stone", 0.42),
        ),
        ShaderMaterialRecordInput::pbr_variant(
            "ash-b",
            "Ash Stone B",
            &shader,
            low,
            fixture_material("Ash Stone B", 0.44),
        ),
        ShaderMaterialRecordInput::pbr_variant(
            "obsidian",
            "Obsidian",
            &shader,
            low,
            distinct_material(),
        ),
    ];
    let catalog = build_shader_material_catalog(
        ShaderMaterialCatalogSettings::default(),
        ShaderMaterialCatalogInput { records },
    )
    .unwrap();

    assert_eq!(catalog.record_count, 3);
    let ash_group = group_for(&catalog, "ash-a");
    assert_eq!(ash_group.candidates.len(), 1);
    assert_eq!(ash_group.candidates[0].record_id, "ash-b");
    assert_eq!(
        ash_group.candidates[0].relationship,
        ShaderMaterialReuseRelationship::NearDuplicate
    );
    assert!(ash_group.candidates[0].can_collapse);
    assert!(catalog.reuse_groups.iter().all(|group| {
        !group
            .candidates
            .iter()
            .any(|candidate| candidate.record_id == "obsidian")
    }));
}

#[test]
fn catalog_distinguishes_exact_duplicates_from_parameterized_variants() {
    let shader = fixture_shader_plan();
    let low = shader
        .variants
        .iter()
        .find(|variant| variant.key == "QUALITY=LOW")
        .unwrap();
    let high = shader
        .variants
        .iter()
        .find(|variant| variant.key == "QUALITY=HIGH")
        .unwrap();
    let records = vec![
        ShaderMaterialRecordInput::pbr_variant(
            "base",
            "Base",
            &shader,
            low,
            fixture_material("Base", 0.42),
        ),
        ShaderMaterialRecordInput::pbr_variant(
            "exact",
            "Exact",
            &shader,
            low,
            fixture_material("Exact", 0.42),
        ),
        ShaderMaterialRecordInput::pbr_variant(
            "variant",
            "Variant",
            &shader,
            high,
            fixture_material("Variant", 0.42),
        ),
    ];
    let catalog = build_shader_material_catalog(
        ShaderMaterialCatalogSettings::default(),
        ShaderMaterialCatalogInput { records },
    )
    .unwrap();
    let group = group_for(&catalog, "base");

    assert_eq!(group.candidates.len(), 2);
    let exact = candidate(group, "exact");
    assert_eq!(
        exact.relationship,
        ShaderMaterialReuseRelationship::ExactDuplicate
    );
    assert!(exact.can_collapse);
    let variant = candidate(group, "variant");
    assert_eq!(
        variant.relationship,
        ShaderMaterialReuseRelationship::ParameterizedVariant
    );
    assert!(!variant.can_collapse);
}

#[test]
fn catalog_includes_threshold_boundary_and_excludes_above_threshold() {
    let shader = fixture_shader_plan();
    let variant = shader.variants.first().unwrap();
    let settings = ShaderMaterialCatalogSettings {
        near_duplicate_threshold: 0.01,
    };
    let records = vec![
        ShaderMaterialRecordInput::pbr_variant(
            "base",
            "Base",
            &shader,
            variant,
            fixture_material("Base", 0.42),
        ),
        ShaderMaterialRecordInput::pbr_variant(
            "boundary",
            "Boundary",
            &shader,
            variant,
            fixture_material("Boundary", 0.43),
        ),
        ShaderMaterialRecordInput::pbr_variant(
            "outside",
            "Outside",
            &shader,
            variant,
            fixture_material("Outside", 0.4311),
        ),
    ];
    let catalog =
        build_shader_material_catalog(settings, ShaderMaterialCatalogInput { records }).unwrap();
    let group = group_for(&catalog, "base");

    assert!(candidate(group, "boundary").can_collapse);
    assert!(
        !group
            .candidates
            .iter()
            .any(|candidate| candidate.record_id == "outside")
    );
    assert!(catalog.reuse_groups.iter().all(|group| {
        !group
            .candidates
            .iter()
            .any(|candidate| candidate.record_id == "outside")
    }));
}

#[test]
fn catalog_fails_loud_on_invalid_or_ambiguous_inputs() {
    let shader = fixture_shader_plan();
    let variant = shader.variants.first().unwrap();
    let duplicate_id = vec![
        ShaderMaterialRecordInput::pbr_variant(
            "dup",
            "One",
            &shader,
            variant,
            fixture_material("One", 0.42),
        ),
        ShaderMaterialRecordInput::pbr_variant(
            "dup",
            "Two",
            &shader,
            variant,
            fixture_material("Two", 0.42),
        ),
    ];
    match build_shader_material_catalog(
        ShaderMaterialCatalogSettings::default(),
        ShaderMaterialCatalogInput {
            records: duplicate_id,
        },
    ) {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { field, .. },
        )) => {
            assert_eq!(field, "records.record_id");
        }
        other => panic!("expected MaterialCatalogInvalidConfig, got {other:?}"),
    }

    let mut duplicate_texture = fixture_material("duplicate texture", 0.42);
    duplicate_texture
        .textures
        .push(TextureBinding::new(TextureSlot::BaseColor, "ash-base-copy"));
    let result = build_shader_material_catalog(
        ShaderMaterialCatalogSettings::default(),
        ShaderMaterialCatalogInput {
            records: vec![ShaderMaterialRecordInput::pbr_variant(
                "bad-texture",
                "Bad Texture",
                &shader,
                variant,
                duplicate_texture,
            )],
        },
    );
    match result {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { field, .. },
        )) => {
            assert_eq!(field, "material.textures.slot");
        }
        other => panic!("expected MaterialCatalogInvalidConfig, got {other:?}"),
    }
}

fn group_for<'a>(
    catalog: &'a crate::render::catalog::ShaderMaterialCatalog,
    id: &str,
) -> &'a crate::render::catalog::ShaderMaterialReuseGroup {
    catalog
        .reuse_groups
        .iter()
        .find(|group| group.canonical_record_id == id)
        .unwrap()
}

fn candidate<'a>(
    group: &'a crate::render::catalog::ShaderMaterialReuseGroup,
    id: &str,
) -> &'a crate::render::catalog::ShaderMaterialReuseCandidate {
    group
        .candidates
        .iter()
        .find(|candidate| candidate.record_id == id)
        .unwrap()
}

fn fixture_shader_plan() -> ShaderPlan {
    let shader = ShaderSource::new(
        "litd.catalog.fixture",
        SHADER_SYSTEM_WGSL,
        ShaderEntryPoints::graphics("litd_shader_system_vs", "litd_shader_system_fs"),
    )
    .with_bindings(vec![
        BindingDecl::new(0, 0, vec![ShaderStage::Vertex], BindingKind::UniformBuffer),
        BindingDecl::new(1, 0, vec![ShaderStage::Fragment], BindingKind::Texture),
        BindingDecl::new(1, 1, vec![ShaderStage::Fragment], BindingKind::Sampler),
    ])
    .with_variant_axes(vec![VariantAxis::new("QUALITY", vec!["LOW", "HIGH"])]);
    build_shader_plan(ShaderSystemSettings::default(), &shader, &shader.bindings).unwrap()
}

fn fixture_material(name: &str, roughness: f32) -> PbrMaterial {
    let mut material = PbrMaterial::new(name);
    material.base_color_factor = Vec4::new(0.62, 0.58, 0.51, 1.0);
    material.metallic_factor = 0.12;
    material.roughness_factor = roughness;
    material.normal_scale = 1.0;
    material.occlusion_strength = 0.9;
    material.emissive_factor = Vec3::ZERO;
    material
        .with_texture(TextureBinding::new(TextureSlot::BaseColor, "ash-base"))
        .with_texture(TextureBinding::new(TextureSlot::Normal, "ash-normal"))
}

fn distinct_material() -> PbrMaterial {
    let mut material = PbrMaterial::new("obsidian");
    material.base_color_factor = Vec4::new(0.05, 0.06, 0.08, 1.0);
    material.metallic_factor = 0.75;
    material.roughness_factor = 0.18;
    material.normal_scale = 1.8;
    material.occlusion_strength = 0.65;
    material.emissive_factor = Vec3::new(0.1, 0.2, 0.5);
    material
        .with_texture(TextureBinding::new(TextureSlot::BaseColor, "obsidian-base"))
        .with_texture(TextureBinding::new(TextureSlot::Normal, "obsidian-normal"))
}
