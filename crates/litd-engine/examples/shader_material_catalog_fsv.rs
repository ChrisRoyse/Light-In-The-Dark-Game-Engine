use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Vec3, Vec4},
    render::{
        catalog::{
            ShaderMaterialCatalogInput, ShaderMaterialCatalogSettings, ShaderMaterialRecordInput,
            ShaderMaterialReuseRelationship, build_shader_material_catalog,
        },
        material::{PbrMaterial, TextureBinding, TextureSlot},
        shader::{
            BindingDecl, BindingKind, SHADER_SYSTEM_WGSL, ShaderEntryPoints, ShaderSource,
            ShaderStage, ShaderSystemSettings, VariantAxis, build_shader_plan, shader_fingerprint,
        },
    },
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_SHADER_MATERIAL_CATALOG_FSV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/fsv/issue1236_shader_material_catalog/fsv.json"));
    let before = path_state(&path);

    let shader = fixture_shader_plan()?;
    let low = shader
        .variants
        .iter()
        .find(|variant| variant.key == "QUALITY=LOW")
        .expect("fixture variant exists");
    let high = shader
        .variants
        .iter()
        .find(|variant| variant.key == "QUALITY=HIGH")
        .expect("fixture variant exists");
    let settings = ShaderMaterialCatalogSettings {
        near_duplicate_threshold: 0.01,
    };
    let catalog = build_shader_material_catalog(
        settings,
        ShaderMaterialCatalogInput {
            records: vec![
                ShaderMaterialRecordInput::pbr_variant(
                    "ash-base",
                    "Ash Base",
                    &shader,
                    low,
                    material("Ash Base", 0.42),
                ),
                ShaderMaterialRecordInput::pbr_variant(
                    "ash-exact",
                    "Ash Exact",
                    &shader,
                    low,
                    material("Ash Exact", 0.42),
                ),
                ShaderMaterialRecordInput::pbr_variant(
                    "ash-boundary",
                    "Ash Boundary",
                    &shader,
                    low,
                    material("Ash Boundary", 0.43),
                ),
                ShaderMaterialRecordInput::pbr_variant(
                    "ash-param-variant",
                    "Ash Parameterized Variant",
                    &shader,
                    high,
                    material("Ash Variant", 0.42),
                ),
                ShaderMaterialRecordInput::pbr_variant(
                    "obsidian-distinct",
                    "Obsidian Distinct",
                    &shader,
                    low,
                    distinct_material(),
                ),
                ShaderMaterialRecordInput::pbr_variant(
                    "ash-outside-threshold",
                    "Ash Outside Threshold",
                    &shader,
                    low,
                    material("Ash Outside", 0.4311),
                ),
            ],
        },
    )?;
    let catalog_json = serde_json::to_string_pretty(&catalog)?;
    let edge_empty = edge_result(
        "empty_catalog",
        0,
        build_shader_material_catalog(
            settings,
            ShaderMaterialCatalogInput {
                records: Vec::new(),
            },
        ),
    );
    let edge_duplicate_texture = {
        let mut duplicate = material("Duplicate Texture", 0.42);
        duplicate
            .textures
            .push(TextureBinding::new(TextureSlot::BaseColor, "ash-base-copy"));
        edge_result(
            "duplicate_texture_slot",
            1,
            build_shader_material_catalog(
                settings,
                ShaderMaterialCatalogInput {
                    records: vec![ShaderMaterialRecordInput::pbr_variant(
                        "bad-texture",
                        "Bad Texture",
                        &shader,
                        low,
                        duplicate,
                    )],
                },
            ),
        )
    };
    let edge_threshold = json!({
        "edge": "threshold_boundary",
        "before": {"threshold": settings.near_duplicate_threshold, "base_roughness": 0.42, "boundary_roughness": 0.43, "outside_roughness": 0.4311},
        "after": {
            "boundary_relationship": relationship_for(&catalog, "ash-base", "ash-boundary"),
            "outside_flagged": relationship_for(&catalog, "ash-base", "ash-outside-threshold").is_some()
        }
    });
    let happy = json!({
        "before": before,
        "catalog_hash": shader_fingerprint(&catalog_json),
        "record_count": catalog.record_count,
        "reuse_group_count": catalog.reuse_group_count,
        "exact_duplicate": relationship_for(&catalog, "ash-base", "ash-exact"),
        "near_boundary": relationship_for(&catalog, "ash-base", "ash-boundary"),
        "parameterized_variant": relationship_for(&catalog, "ash-base", "ash-param-variant"),
        "distinct_flagged": relationship_for(&catalog, "ash-base", "obsidian-distinct").is_some(),
        "outside_threshold_flagged": relationship_for(&catalog, "ash-base", "ash-outside-threshold").is_some(),
        "catalog": catalog
    });
    let document = json!({
        "schema": "litd.issue1236.shader_material_catalog_fsv.v1",
        "source_of_truth": path,
        "happy_path": happy,
        "edges": [edge_empty, edge_duplicate_texture, edge_threshold],
    });

    write_document_with_final_state(&path, document)?;
    println!("wrote shader/material catalog FSV to {}", path.display());
    Ok(())
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"] = json!({"exists": true, "len": expected_len});
        let bytes = serde_json::to_vec_pretty(&document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            let disk_len = fs::metadata(path)?.len();
            if disk_len != expected_len {
                return Err(format!(
                    "FSV artifact write verification failed for {}: embedded after.len={}, disk bytes={}",
                    path.display(),
                    expected_len,
                    disk_len
                )
                .into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }

    Err(format!(
        "FSV artifact byte-count stabilization failed for {} after 16 attempts",
        path.display()
    )
    .into())
}

fn edge_result(
    name: &str,
    input_record_count: usize,
    result: Result<litd_engine::render::catalog::ShaderMaterialCatalog, EngineError>,
) -> serde_json::Value {
    match result {
        Ok(catalog) => json!({
            "edge": name,
            "before": {"input_record_count": input_record_count},
            "after": {"unexpected_ok": true, "record_count": catalog.record_count}
        }),
        Err(error) => json!({
            "edge": name,
            "before": {"input_record_count": input_record_count},
            "after": {"unexpected_ok": false, "error_code": error.code(), "message": error.to_string()}
        }),
    }
}

fn relationship_for(
    catalog: &litd_engine::render::catalog::ShaderMaterialCatalog,
    canonical: &str,
    candidate: &str,
) -> Option<ShaderMaterialReuseRelationship> {
    catalog
        .reuse_groups
        .iter()
        .find(|group| group.canonical_record_id == canonical)?
        .candidates
        .iter()
        .find(|item| item.record_id == candidate)
        .map(|item| item.relationship)
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::read(path) {
        Ok(bytes) => json!({
            "exists": true,
            "len": bytes.len(),
            "fnv64": shader_fingerprint(&String::from_utf8_lossy(&bytes))
        }),
        Err(_) => json!({"exists": false}),
    }
}

fn fixture_shader_plan() -> Result<litd_engine::render::shader::ShaderPlan, EngineError> {
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
    build_shader_plan(ShaderSystemSettings::default(), &shader, &shader.bindings)
}

fn material(name: &str, roughness: f32) -> PbrMaterial {
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
