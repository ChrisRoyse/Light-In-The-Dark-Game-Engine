//! Full-state verification for the no-code PBR material editor (#862).
//!
//! Proves the complete vertical slice over the SHARED capability layer (#915):
//! author a metallic-roughness material through the material MCP tool -> persist
//! it through the REAL Calyx `put_content` path -> read the persisted record back
//! and confirm every field equals what was set -> lower it to a fingerprinted
//! GPU uniform and render a live preview headless (render-to-texture + pixel
//! readback) confirming the ball actually renders (not blank / not pink) -> edit
//! ONE factor and confirm BOTH the persisted record AND the preview pixels
//! change. The preview WGSL is genuinely naga-parsed/validated with negative
//! controls and a CPU-model cross-check (NO substring checks).
//!
//! Verdict artifact lands under `target/fsv/issue862_pbr_material_editor/` and is
//! independently re-read + hash-checked before the run is declared green.

use litd_editor::material_editor::read_pbr_material;
use litd_editor::material_editor_mcp::{AUTHOR_PBR_MATERIAL_TOOL, McpMaterialToolCallOutput};
use litd_editor::mcp_tool_registry::{
    McpToolRegistry, default_mcp_tool_registry, dispatch_mcp_tool_call,
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_engine::render::material::PbrMaterial;
use litd_engine::render::material_preview::{
    MaterialPreviewFrame, MaterialPreviewSettings, render_material_preview_gpu,
};
use litd_engine::render::material_preview_fsv::verify_preview_wgsl;
use litd_engine::test_temp::TestTempDir;
use serde_json::{Value, json};

const PINK_ERROR: [f32; 3] = [255.0, 0.0, 255.0];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    pollster::block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let wgsl = verify_preview_wgsl()?;

    let vault = TestTempDir::new("issue862_pbr_material");
    let vault_path = vault.path().to_path_buf();

    // Drive authoring through the REAL external MCP integration boundary: build the callable tool
    // registry and dispatch by tool name, exactly as an external agent hits the engine-as-MCP-server
    // (#915). dispatch_mcp_tool_call validates registration, routes by domain, and returns the
    // serialized tool output -- proving the tool is genuinely reachable, not just callable directly.
    let registry = default_mcp_tool_registry()?;
    if registry.entry(AUTHOR_PBR_MATERIAL_TOOL).is_err() {
        return Err("author_pbr_material is not in the callable MCP registry".into());
    }

    // ---- Author v1 through the MCP registry dispatch (agent front-end over the shared layer).
    let author_v1 = author(
        &registry,
        &vault_path,
        "ash_iron",
        0.5,
        0.35,
        [0.80, 0.20, 0.10, 1.0],
    )?;
    let av1 = author_v1
        .author_readback
        .as_ref()
        .ok_or("author v1 produced no readback")?;

    // ---- Read the persisted record back and confirm fields == what was set.
    let read_v1 = read_pbr_material(&vault_path, "ash_iron")?;
    let persist_fields_match = read_v1.metallic == 0.5
        && read_v1.roughness == 0.35
        && read_v1.base_color == [0.80, 0.20, 0.10, 1.0]
        && read_v1.texture_mask == av1.texture_mask
        && read_v1.content_input_sha256 == av1.content_input_sha256
        && read_v1.preview_uniform_fingerprint == av1.preview_uniform_fingerprint;
    if !persist_fields_match {
        return Err("persisted material fields did not equal the authored values".into());
    }

    // ---- Render the live preview headless from the lowered uniform.
    let material_v1 = build_material("Ash Iron", 0.5, 0.35, [0.80, 0.20, 0.10, 1.0])?;
    let frame_v1 =
        render_material_preview_gpu(&material_v1, MaterialPreviewSettings::default()).await?;
    assert_ball_rendered(&frame_v1)?;
    if frame_v1.uniform_fingerprint != av1.preview_uniform_fingerprint {
        return Err("preview uniform fingerprint disagreed with the persisted readback".into());
    }

    // ---- Edit ONE factor (roughness) and re-author the SAME material id.
    let author_v2 = author(
        &registry,
        &vault_path,
        "ash_iron",
        0.5,
        0.70,
        [0.80, 0.20, 0.10, 1.0],
    )?;
    let av2 = author_v2
        .author_readback
        .as_ref()
        .ok_or("author v2 produced no readback")?;
    let read_v2 = read_pbr_material(&vault_path, "ash_iron")?;

    let record_changed = av2.content_input_sha256 != av1.content_input_sha256
        && av2.lowered_fingerprint != av1.lowered_fingerprint
        && av2.preview_uniform_fingerprint != av1.preview_uniform_fingerprint
        && read_v2.roughness == 0.70;
    if !record_changed {
        return Err("editing roughness did not change the persisted record".into());
    }

    // ---- Re-render; the preview pixels must change with the edit.
    let material_v2 = build_material("Ash Iron", 0.5, 0.70, [0.80, 0.20, 0.10, 1.0])?;
    let frame_v2 =
        render_material_preview_gpu(&material_v2, MaterialPreviewSettings::default()).await?;
    assert_ball_rendered(&frame_v2)?;
    let image_changed = frame_v2.image_sha256() != frame_v1.image_sha256()
        && frame_v2.foreground_mean_rgb != frame_v1.foreground_mean_rgb;
    if !image_changed {
        return Err("editing roughness did not change the rendered preview".into());
    }

    let edges = edge_cases(&registry, &vault_path).await?;

    let report = json!({
        "schema_version": 1,
        "issue": 862,
        "capability_layer": "litd_editor::material_editor (put_content + lowering)",
        "surfaces": {
            "mcp_tool": AUTHOR_PBR_MATERIAL_TOOL,
            "mcp_dispatch_boundary": "litd_editor::mcp_tool_registry::dispatch_mcp_tool_call",
            "mcp_registry_tool_count": registry.entries.len(),
            "mcp_registry_domain": registry.entry(AUTHOR_PBR_MATERIAL_TOOL)?.domain_id.clone(),
            "gui_panel": "litd_editor::material_editor_panel::MaterialInspectorPanel",
        },
        "wgsl": wgsl,
        "author_v1": author_summary(av1),
        "read_v1": read_summary(&read_v1),
        "persist_fields_match": persist_fields_match,
        "preview_v1": frame_summary(&frame_v1),
        "edit": {
            "field": "roughness",
            "from": 0.35,
            "to": 0.70,
        },
        "author_v2": author_summary(av2),
        "read_v2": read_summary(&read_v2),
        "record_changed": record_changed,
        "preview_v2": frame_summary(&frame_v2),
        "preview_image_changed": image_changed,
        "edge_cases": edges,
    });

    verify(&report)?;

    let run = begin_issue_run("issue862_pbr_material_editor")?;
    let readback = run.write_report(&report)?;
    let persisted = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&persisted)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&persisted) != readback.sha256 {
        return Err("persisted verdict hash does not match manifest".into());
    }

    println!(
        "wrote issue862 PBR material editor FSV report to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}

fn author(
    registry: &McpToolRegistry,
    vault_path: &std::path::Path,
    id: &str,
    metallic: f32,
    roughness: f32,
    base_color: [f32; 4],
) -> Result<McpMaterialToolCallOutput, Box<dyn std::error::Error>> {
    let value = dispatch_mcp_tool_call(
        registry,
        AUTHOR_PBR_MATERIAL_TOOL,
        json!({
            "vault_path": vault_path,
            "request_id": format!("req-{id}-{roughness}"),
            "requested_by": "issue862-fsv-agent",
            "material": {
                "material_id": id,
                "display_name": "Ash Iron",
                "author": "issue862-fsv",
                "base_color": base_color,
                "metallic": metallic,
                "roughness": roughness,
                "normal_scale": 1.0,
                "occlusion": 0.9,
                "emissive": [0.02, 0.01, 0.0],
                "maps": [
                    {"slot": "base_color", "asset_id": "ash-iron-base"},
                    {"slot": "metallic_roughness", "asset_id": "ash-iron-mr"},
                    {"slot": "normal", "asset_id": "ash-iron-normal"}
                ]
            }
        }),
    )?;
    let output: McpMaterialToolCallOutput = serde_json::from_value(value)?;
    if output.tool_name != AUTHOR_PBR_MATERIAL_TOOL {
        return Err("dispatch returned a different tool's output".into());
    }
    Ok(output)
}

fn build_material(
    name: &str,
    metallic: f32,
    roughness: f32,
    base_color: [f32; 4],
) -> Result<PbrMaterial, Box<dyn std::error::Error>> {
    use litd_engine::math::{Vec3, Vec4};
    use litd_engine::render::material::{TextureBinding, TextureSlot};
    let mut material = PbrMaterial::new(name);
    material.base_color_factor = Vec4::from_array(base_color);
    material.metallic_factor = metallic;
    material.roughness_factor = roughness;
    material.occlusion_strength = 0.9;
    material.emissive_factor = Vec3::new(0.02, 0.01, 0.0);
    material = material
        .with_texture(TextureBinding::new(TextureSlot::BaseColor, "ash-iron-base"))
        .with_texture(TextureBinding::new(
            TextureSlot::MetallicRoughness,
            "ash-iron-mr",
        ))
        .with_texture(TextureBinding::new(TextureSlot::Normal, "ash-iron-normal"));
    material.validate()?;
    Ok(material)
}

fn assert_ball_rendered(frame: &MaterialPreviewFrame) -> Result<(), Box<dyn std::error::Error>> {
    if frame.foreground_pixels == 0 {
        return Err("preview rendered a blank frame (no foreground sphere pixels)".into());
    }
    // A 128x128 frame with a 0.85-radius ball must cover a large minority of pixels.
    let total = frame.width * frame.height;
    if frame.foreground_pixels * 4 < total {
        return Err("preview sphere coverage implausibly small".into());
    }
    if frame.foreground_mean_rgb == PINK_ERROR {
        return Err("preview rendered the pink error material".into());
    }
    if frame.mean_luminance <= 0.0 {
        return Err("preview mean luminance was zero (nothing lit)".into());
    }
    Ok(())
}

async fn edge_cases(
    registry: &McpToolRegistry,
    vault_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    // Edge 1: roughness below the PBR floor is rejected BEFORE any persistence.
    let before_missing = read_pbr_material(vault_path, "bad_rough").is_ok();
    let invalid_roughness = dispatch_mcp_tool_call(
        registry,
        AUTHOR_PBR_MATERIAL_TOOL,
        json!({
            "vault_path": vault_path, "request_id": "req-bad", "requested_by": "fsv",
            "material": {
                "material_id": "bad_rough", "display_name": "Bad", "author": "fsv",
                "base_color": [1.0, 1.0, 1.0, 1.0], "metallic": 0.0, "roughness": 0.0,
                "normal_scale": 1.0, "occlusion": 1.0, "emissive": [0.0, 0.0, 0.0]
            }
        }),
    )
    .is_err();
    let after_missing = read_pbr_material(vault_path, "bad_rough").is_ok();

    // Edge 2: two maps in the same slot is rejected.
    let dup_slot = dispatch_mcp_tool_call(
        registry,
        AUTHOR_PBR_MATERIAL_TOOL,
        json!({
            "vault_path": vault_path, "request_id": "req-dup", "requested_by": "fsv",
            "material": {
                "material_id": "dup", "display_name": "Dup", "author": "fsv",
                "base_color": [1.0, 1.0, 1.0, 1.0], "metallic": 0.0, "roughness": 0.5,
                "normal_scale": 1.0, "occlusion": 1.0, "emissive": [0.0, 0.0, 0.0],
                "maps": [
                    {"slot": "base_color", "asset_id": "a"},
                    {"slot": "base_color", "asset_id": "b"}
                ]
            }
        }),
    )
    .is_err();

    // Edge 3: a zero-size preview target is rejected before touching the GPU.
    let zero_settings = MaterialPreviewSettings {
        width: 0,
        ..MaterialPreviewSettings::default()
    };
    let material = build_material("edge", 0.0, 0.5, [1.0, 1.0, 1.0, 1.0])?;
    let zero_preview = render_material_preview_gpu(&material, zero_settings)
        .await
        .is_err();

    if before_missing || !invalid_roughness || after_missing || !dup_slot || !zero_preview {
        return Err("an edge case did not fail loud as required".into());
    }
    Ok(json!({
        "invalid_roughness_rejected": invalid_roughness,
        "invalid_roughness_not_persisted": !after_missing,
        "duplicate_map_slot_rejected": dup_slot,
        "zero_size_preview_rejected": zero_preview,
        "before_bad_rough_absent": !before_missing,
    }))
}

fn author_summary(readback: &litd_editor::material_editor::PbrMaterialAuthorReadback) -> Value {
    json!({
        "material_id": readback.material_id,
        "cx_id": readback.cx_id,
        "content_input_sha256": readback.content_input_sha256,
        "lowered_artifact_id": readback.lowered_artifact_id,
        "lowered_fingerprint": readback.lowered_fingerprint,
        "lowered_field_count": readback.lowered_field_count,
        "measurement_state": readback.measurement_state,
        "pending_learned_lens_count": readback.pending_learned_lens_count,
        "preview_uniform_fingerprint": readback.preview_uniform_fingerprint,
        "texture_mask": readback.texture_mask,
        "map_count": readback.map_count,
    })
}

fn read_summary(readback: &litd_editor::material_editor::PbrMaterialReadback) -> Value {
    json!({
        "material_id": readback.material_id,
        "cx_id": readback.cx_id,
        "content_input_sha256": readback.content_input_sha256,
        "measurement_state": readback.measurement_state,
        "base_color": readback.base_color,
        "metallic": readback.metallic,
        "roughness": readback.roughness,
        "texture_mask": readback.texture_mask,
        "preview_uniform_fingerprint": readback.preview_uniform_fingerprint,
    })
}

fn frame_summary(frame: &MaterialPreviewFrame) -> Value {
    json!({
        "width": frame.width,
        "height": frame.height,
        "foreground_pixels": frame.foreground_pixels,
        "background_pixels": frame.background_pixels,
        "foreground_mean_rgb": frame.foreground_mean_rgb,
        "mean_luminance": frame.mean_luminance,
        "uniform_fingerprint": frame.uniform_fingerprint,
        "image_sha256": frame.image_sha256(),
        "adapter_name": frame.adapter_name,
        "backend": frame.backend,
        "texture_format": frame.texture_format,
        "readback_bytes": frame.readback_bytes,
    })
}

fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let checks = [
        "persist_fields_match",
        "record_changed",
        "preview_image_changed",
    ];
    for key in checks {
        if report.get(key).and_then(Value::as_bool) != Some(true) {
            return Err(format!("verdict check `{key}` was not true").into());
        }
    }
    let wgsl = report.get("wgsl").ok_or("missing wgsl section")?;
    if wgsl.get("naga_validate_ok").and_then(Value::as_bool) != Some(true) {
        return Err("wgsl naga validation not ok".into());
    }
    let negatives = wgsl.get("negative_controls").ok_or("missing negatives")?;
    if negatives
        .get("syntax_error_rejected")
        .and_then(Value::as_bool)
        != Some(true)
        || negatives
            .get("type_error_rejected")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return Err("wgsl negative controls did not bite".into());
    }
    Ok(())
}
