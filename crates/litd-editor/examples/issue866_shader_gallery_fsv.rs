use std::{fs, path::Path};

use litd_editor::mcp_render_tools::{
    READ_RENDER_TOOL, SET_SHADER_GALLERY_TOOL, call_mcp_render_tool, read_mcp_render_state,
    read_mcp_render_tool_ledger,
};
use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    render::{
        shader::{ShaderEntryPoints, ShaderSystemSettings},
        stylized::{
            StylizedGallerySelection, StylizedGpuScene, StylizedPresetKind, StylizedValue,
            WgslShaderEscape, build_stylized_gallery_readback, build_stylized_shader_library,
            render_stylized_materials_gpu_with_context, validate_wgsl_escape_gpu,
        },
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    pollster::block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let run = begin_issue_run("issue866_shader_gallery")?;
    let report_path = run.report_path();
    let gallery_bmp = run.artifact_path("curated-shader-gallery.bmp")?;
    let editor_state = run.artifact_path("editor-render-state.json")?;
    let editor_ledger = run.artifact_path("editor-render-ledger.json")?;

    let context = GpuContext::new_headless().await?;
    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    let gallery = sample_gallery(&library)?;
    let gallery_readback =
        build_stylized_gallery_readback(ShaderSystemSettings::default(), &library, &gallery)?;
    let custom_wgsl_gpu = validate_wgsl_escape_gpu(
        &context,
        gallery
            .custom_wgsl
            .as_ref()
            .expect("sample gallery includes WGSL escape"),
    )
    .await?;

    let scene = StylizedGpuScene::from_materials(576, 240, gallery.materials.clone());
    let frame = render_stylized_materials_gpu_with_context(&context, &library, &scene).await?;
    frame.image.write_bmp(&gallery_bmp)?;

    let editor_write = call_mcp_render_tool(
        SET_SHADER_GALLERY_TOOL,
        json!({
            "state_path": editor_state,
            "state_id": "issue866-render-state",
            "ledger_path": editor_ledger,
            "ledger_id": "issue866-shader-gallery-ledger",
            "request_id": "issue866-set-shader-gallery",
            "requested_by": "litd-editor-issue866-fsv",
            "gallery": gallery
        }),
    )?;
    let editor_read =
        call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": editor_state }))?;
    let editor_state_readback = read_mcp_render_state(&editor_state)?;
    let editor_ledger_readback = read_mcp_render_tool_ledger(&editor_ledger)?;

    let edges = vec![
        edge_invalid_wave_strength(
            &editor_state,
            &run.artifact_path("edge-invalid-wave-ledger.json")?,
        )?,
        edge_glsl_escape_rejected(&editor_state, &run.artifact_path("edge-glsl-ledger.json")?)?,
        edge_missing_entry_point(
            &editor_state,
            &run.artifact_path("edge-missing-entry-ledger.json")?,
        )?,
        edge_empty_materials(
            &editor_state,
            &run.artifact_path("edge-empty-materials-ledger.json")?,
        )?,
    ];

    let report = json!({
        "schema_version": 1,
        "issue": 866,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "gallery_bmp": gallery_bmp.display().to_string(),
            "editor_state": editor_state.display().to_string(),
            "editor_ledger": editor_ledger.display().to_string(),
            "manual_fsv_readback": "Open the persisted JSON through Synapse, read SHA256 from disk, inspect the shader-gallery BMP, and compare actual preset/WGSL/editor fields to expected."
        },
        "research": [
            {
                "url": "https://www.w3.org/TR/WGSL/",
                "applied": "WGSL is the shader language for the WebGPU renderer path."
            },
            {
                "url": "https://docs.rs/wgpu/latest/wgpu/enum.ShaderSource.html",
                "applied": "wgpu parses and validates WGSL ShaderSource when creating shader modules."
            }
        ],
        "visual_references_reviewed": [
            "docs/readme/ED08-vfx-editor.png",
            "docs/readme/GP04-mage-spell.png",
            "docs/readme/GP07-boss.png",
            "docs/readme/H03-ember-wastes.png"
        ],
        "sufficiency_argument": "The six-stripe scene is the smallest dataset that proves #866 because each required curated shader preset is represented by one validated no-code material and one executed fragment entry. The custom WGSL escape is separately parsed by wgpu from source. The editor MCP write/read proves the selection persists in authoring state. Four edge probes cover invalid tweak range, obsolete GLSL input, missing WGSL entry point, and empty gallery materials without state mutation.",
        "gpu_context": context_state(&context),
        "gallery": {
            "readback": gallery_readback,
            "gpu_stats": frame.stats,
            "bmp_readback": file_state(&gallery_bmp),
            "custom_wgsl_gpu": custom_wgsl_gpu
        },
        "editor_readback": {
            "write_result": editor_write.result,
            "read_result": editor_read.result,
            "state_path": editor_state_readback.path,
            "state_sha256": editor_state_readback.sha256,
            "selected_preset": editor_state_readback.shader_gallery_selected_preset,
            "material_count": editor_state_readback.shader_gallery_material_count,
            "custom_wgsl_id": editor_state_readback.shader_gallery_custom_wgsl_id,
            "ledger_path": editor_ledger_readback.path,
            "ledger_sha256": editor_ledger_readback.sha256,
            "ledger_record_count": editor_ledger_readback.ledger.record_count
        },
        "edges": edges,
        "expected_actuals": {
            "preset_count": 6,
            "material_count": 6,
            "draw_calls": 6,
            "selected_preset": "force_field",
            "custom_wgsl_id": "issue866.custom_wgsl_escape",
            "edge_count": 4,
            "expected_fragment_entries": [
                "litd_stylized_water_fs",
                "litd_stylized_fire_fs",
                "litd_stylized_glow_fs",
                "litd_stylized_toon_fs",
                "litd_stylized_dissolve_fs",
                "litd_stylized_force_field_fs"
            ]
        }
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "issue866_shader_gallery_fsv report={} bytes={} sha256={} presets={} draws={} selected={} custom={} bmp_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["gallery"]["gpu_stats"]["library_preset_count"],
        readback.json["gallery"]["gpu_stats"]["draw_calls"],
        readback.json["editor_readback"]["selected_preset"],
        readback.json["editor_readback"]["custom_wgsl_id"],
        readback.json["gallery"]["bmp_readback"]["sha256"],
    );
    Ok(())
}

fn sample_gallery(
    library: &litd_engine::render::stylized::StylizedShaderLibrary,
) -> Result<StylizedGallerySelection, EngineError> {
    let mut gallery = StylizedGallerySelection::curated_defaults(library)?;
    gallery.selected_preset = StylizedPresetKind::ForceField;
    for material in &mut gallery.materials {
        match material.preset {
            StylizedPresetKind::Water => {
                material
                    .values
                    .insert("wave_strength".to_string(), StylizedValue::Scalar(2.0));
            }
            StylizedPresetKind::Fire => {
                material
                    .values
                    .insert("intensity".to_string(), StylizedValue::Scalar(5.5));
            }
            StylizedPresetKind::ForceField => {
                material
                    .values
                    .insert("opacity".to_string(), StylizedValue::Scalar(0.72));
            }
            _ => {}
        }
    }
    gallery.custom_wgsl = Some(wgsl_escape(
        "issue866.custom_wgsl_escape",
        "issue866_vs",
        "issue866_fs",
    ));
    Ok(gallery)
}

fn edge_invalid_wave_strength(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    let mut gallery = sample_gallery(&library)?;
    gallery.materials[0]
        .values
        .insert("wave_strength".to_string(), StylizedValue::Scalar(9.0));
    let error = call_mcp_render_tool(
        SET_SHADER_GALLERY_TOOL,
        edge_args(state_path, ledger_path, "invalid-wave", gallery),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "invalid_wave_strength",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_glsl_escape_rejected(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    let mut gallery = sample_gallery(&library)?;
    gallery.custom_wgsl = Some(WgslShaderEscape {
        shader_id: "issue866.glsl_escape".to_string(),
        source: "#version 450\nvoid main() { gl_FragColor = vec4(1.0); }".to_string(),
        entry_points: ShaderEntryPoints::graphics("issue866_vs", "issue866_fs"),
        bindings: Vec::new(),
        variant_axes: Vec::new(),
    });
    let error = call_mcp_render_tool(
        SET_SHADER_GALLERY_TOOL,
        edge_args(state_path, ledger_path, "glsl", gallery),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "glsl_escape_rejected",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_missing_entry_point(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    let mut gallery = sample_gallery(&library)?;
    gallery.custom_wgsl = Some(wgsl_escape(
        "issue866.missing_entry",
        "missing_vs",
        "missing_fs",
    ));
    let error = call_mcp_render_tool(
        SET_SHADER_GALLERY_TOOL,
        edge_args(state_path, ledger_path, "missing-entry", gallery),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "missing_entry_point",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_empty_materials(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    let mut gallery = sample_gallery(&library)?;
    gallery.materials.clear();
    let error = call_mcp_render_tool(
        SET_SHADER_GALLERY_TOOL,
        edge_args(state_path, ledger_path, "empty-materials", gallery),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "empty_materials",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_args(
    state_path: &Path,
    ledger_path: &Path,
    request: &str,
    gallery: StylizedGallerySelection,
) -> Value {
    json!({
        "state_path": state_path,
        "state_id": "issue866-render-state",
        "ledger_path": ledger_path,
        "ledger_id": format!("issue866-edge-{request}"),
        "request_id": format!("issue866-edge-{request}"),
        "requested_by": "litd-editor-issue866-fsv",
        "gallery": gallery
    })
}

fn edge_state(name: &str, before_sha: String, after_sha: String, error: EngineError) -> Value {
    let report = error.report();
    json!({
        "name": name,
        "before_state_sha256": before_sha,
        "after_state_sha256": after_sha,
        "state_unchanged": before_sha == after_sha,
        "error": {
            "code": report.code,
            "message": report.message,
            "remediation": report.remediation
        }
    })
}

fn wgsl_escape(shader_id: &str, vertex: &str, fragment: &str) -> WgslShaderEscape {
    WgslShaderEscape {
        shader_id: shader_id.to_string(),
        source: custom_wgsl_escape(),
        entry_points: ShaderEntryPoints::graphics(vertex, fragment),
        bindings: Vec::new(),
        variant_axes: Vec::new(),
    }
}

fn custom_wgsl_escape() -> String {
    r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn issue866_vs(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
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
fn issue866_fs() -> @location(0) vec4<f32> {
    return vec4<f32>(0.18, 0.72, 1.0, 1.0);
}
"#
    .to_string()
}

fn context_state(context: &GpuContext) -> Value {
    let report = context.report();
    json!({
        "context_id": report.context_id,
        "mode": format!("{:?}", report.mode),
        "adapter_name": report.adapter.name,
        "backend": report.adapter.backend,
        "device_type": report.adapter.device_type,
        "driver": report.adapter.driver,
        "device_lost_report": context.device_lost_report()
    })
}

fn file_state(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap();
    json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
        "bmp_signature": String::from_utf8_lossy(&bytes[0..2]).to_string(),
        "bmp_width": u32::from_le_bytes(bytes[18..22].try_into().unwrap()),
        "bmp_height": u32::from_le_bytes(bytes[22..26].try_into().unwrap())
    })
}

fn assert_report(report: &Value) -> Result<(), EngineError> {
    assert_eq!(report["issue"], 866);
    assert_eq!(report["gallery"]["gpu_stats"]["library_preset_count"], 6);
    assert_eq!(report["gallery"]["gpu_stats"]["draw_calls"], 6);
    assert_eq!(
        report["gallery"]["gpu_stats"]["executed_fragment_entries"],
        report["expected_actuals"]["expected_fragment_entries"]
    );
    assert_eq!(report["editor_readback"]["selected_preset"], "force_field");
    assert_eq!(
        report["editor_readback"]["custom_wgsl_id"],
        "issue866.custom_wgsl_escape"
    );
    assert!(
        report["gallery"]["gpu_stats"]["output_metrics"]["nonblack_pixel_ratio"]
            .as_f64()
            .unwrap()
            > 0.96
    );
    assert_eq!(report["edges"].as_array().unwrap().len(), 4);
    for edge in report["edges"].as_array().unwrap() {
        assert_eq!(edge["state_unchanged"], true);
        assert!(edge["error"]["code"].as_str().unwrap().starts_with("LITD-"));
    }
    Ok(())
}
