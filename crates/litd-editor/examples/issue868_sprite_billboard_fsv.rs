use std::{fs, path::Path};

use litd_editor::mcp_render_tools::{
    READ_RENDER_TOOL, SET_SPRITE_BILLBOARD_LIBRARY_TOOL, call_mcp_render_tool,
    read_mcp_render_state, read_mcp_render_tool_ledger,
};
use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    render::sprite_billboard::{
        SpriteBillboardGpuScene, SpriteBillboardLibrarySelection,
        build_sprite_billboard_library_readback, render_sprite_billboard_library_gpu_with_context,
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
    let run = begin_issue_run("issue868_sprite_billboards")?;
    let report_path = run.report_path();
    let sprite_bmp = run.artifact_path("sprite-billboard-library.bmp")?;
    let editor_state = run.artifact_path("editor-render-state.json")?;
    let editor_ledger = run.artifact_path("editor-render-ledger.json")?;

    let context = GpuContext::new_headless().await?;
    let library = SpriteBillboardLibrarySelection::issue868_defaults()?;
    let library_readback = build_sprite_billboard_library_readback(&library)?;
    let scene = SpriteBillboardGpuScene {
        width: 640,
        height: 240,
        library: library.clone(),
    };
    let frame = render_sprite_billboard_library_gpu_with_context(&context, &scene).await?;
    frame.image.write_bmp(&sprite_bmp)?;

    let editor_write = call_mcp_render_tool(
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL,
        json!({
            "state_path": editor_state,
            "state_id": "issue868-render-state",
            "ledger_path": editor_ledger,
            "ledger_id": "issue868-sprite-billboard-ledger",
            "request_id": "issue868-set-sprite-billboard-library",
            "requested_by": "litd-editor-issue868-fsv",
            "library": library
        }),
    )?;
    let editor_read =
        call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": editor_state }))?;
    let editor_state_readback = read_mcp_render_state(&editor_state)?;
    let editor_ledger_readback = read_mcp_render_tool_ledger(&editor_ledger)?;

    let edges = vec![
        edge_duplicate_content(
            &editor_state,
            &run.artifact_path("edge-duplicate-content.json")?,
        )?,
        edge_bad_hash(&editor_state, &run.artifact_path("edge-bad-hash.json")?)?,
        edge_bad_frame(&editor_state, &run.artifact_path("edge-bad-frame.json")?)?,
        edge_zero_size(&editor_state, &run.artifact_path("edge-zero-size.json")?)?,
    ];

    let report = json!({
        "schema_version": 1,
        "issue": 868,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "sprite_bmp": sprite_bmp.display().to_string(),
            "editor_state": editor_state.display().to_string(),
            "editor_ledger": editor_ledger.display().to_string(),
            "manual_fsv_readback": "Open the persisted JSON through Synapse, inspect the sprite-billboard BMP, and compare material/texture/sheet/billboard counts, content addresses, search rows, and GPU stats to expected values."
        },
        "research": [
            {
                "url": "https://docs.rs/wgpu/latest/wgpu/enum.PrimitiveTopology.html",
                "applied": "Sprite billboards are rendered as triangle-list quads."
            },
            {
                "url": "https://sotrh.github.io/learn-wgpu/beginner/tutorial5-textures/",
                "applied": "The preview renderer samples an RGBA8 texture atlas through a wgpu bind group."
            },
            {
                "url": "https://docs.rs/bevy/latest/bevy/sprite/index.html",
                "applied": "Sprite sheets and texture-atlas frame concepts map to authored sprite materials."
            },
            {
                "url": "https://github.com/bevyengine/bevy/issues/3688",
                "applied": "3D billboard sprites are represented as quads that face or align to the camera mode."
            }
        ],
        "visual_references_reviewed": [
            "docs/readme/ED06-material-assets.png",
            "docs/readme/ED11-world-view.png",
            "docs/readme/ED12-play-topdown.png",
            "docs/readme/GP03-combat.png"
        ],
        "sufficiency_argument": "The #868 dataset is intentionally small but complete: three unique content-addressed RGBA8 textures, three sprite materials, one 2x2 sheet, four billboards covering camera-facing, Y-axis-locked, and screen-aligned modes, deterministic Calyx-search rows, a real wgpu atlas render, and four fail-closed edge probes. That covers the issue gap left after PBR material catalog and texture-set resolution: a general no-code sprite/billboard material+texture library outside VFX particles.",
        "gpu_context": context_state(&context),
        "library": {
            "readback": library_readback,
            "gpu_stats": frame.stats,
            "bmp_readback": file_state(&sprite_bmp)
        },
        "editor_readback": {
            "write_result": editor_write.result,
            "read_result": editor_read.result,
            "state_path": editor_state_readback.path,
            "state_sha256": editor_state_readback.sha256,
            "library_id": editor_state_readback.sprite_billboard_library.as_ref().unwrap().library_id,
            "texture_count": editor_state_readback.sprite_billboard_library.as_ref().unwrap().texture_count,
            "material_count": editor_state_readback.sprite_billboard_library.as_ref().unwrap().material_count,
            "sheet_count": editor_state_readback.sprite_billboard_library.as_ref().unwrap().sheet_count,
            "billboard_count": editor_state_readback.sprite_billboard_library.as_ref().unwrap().billboard_count,
            "search_row_count": editor_state_readback.sprite_billboard_library.as_ref().unwrap().search_row_count,
            "ledger_path": editor_ledger_readback.path,
            "ledger_sha256": editor_ledger_readback.sha256,
            "ledger_record_count": editor_ledger_readback.ledger.record_count
        },
        "edges": edges,
        "expected_actuals": {
            "texture_count": 3,
            "material_count": 3,
            "sheet_count": 1,
            "billboard_count": 4,
            "search_row_count": 11,
            "total_vertex_count": 16,
            "total_index_count": 24,
            "triangle_count": 8,
            "draw_calls": 1,
            "edge_count": 4
        }
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "issue868_sprite_billboard_fsv report={} bytes={} sha256={} textures={} billboards={} search_rows={} draws={} bmp_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["library"]["readback"]["texture_count"],
        readback.json["library"]["readback"]["billboard_count"],
        readback.json["library"]["readback"]["search_row_count"],
        readback.json["library"]["gpu_stats"]["draw_calls"],
        readback.json["library"]["bmp_readback"]["sha256"],
    );
    Ok(())
}

fn edge_duplicate_content(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let mut library = SpriteBillboardLibrarySelection::issue868_defaults()?;
    let mut cloned = library.textures[0].clone();
    cloned.texture_id = "texture.issue868.duplicate_pixels".to_string();
    library.textures.push(cloned);
    let error = call_mcp_render_tool(
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL,
        edge_args(state_path, ledger_path, "duplicate-content", library),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "duplicate_content",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_bad_hash(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let mut library = SpriteBillboardLibrarySelection::issue868_defaults()?;
    library.textures[0].rgba8_sha256 =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    let error = call_mcp_render_tool(
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL,
        edge_args(state_path, ledger_path, "bad-hash", library),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state("bad_hash", before.sha256, after.sha256, error))
}

fn edge_bad_frame(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let mut library = SpriteBillboardLibrarySelection::issue868_defaults()?;
    library.billboards[0].frame_index = 4;
    let error = call_mcp_render_tool(
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL,
        edge_args(state_path, ledger_path, "bad-frame", library),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state("bad_frame", before.sha256, after.sha256, error))
}

fn edge_zero_size(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let mut library = SpriteBillboardLibrarySelection::issue868_defaults()?;
    library.billboards[0].size[0] = 0.0;
    let error = call_mcp_render_tool(
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL,
        edge_args(state_path, ledger_path, "zero-size", library),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state("zero_size", before.sha256, after.sha256, error))
}

fn edge_args(
    state_path: &Path,
    ledger_path: &Path,
    request: &str,
    library: SpriteBillboardLibrarySelection,
) -> Value {
    json!({
        "state_path": state_path,
        "state_id": "issue868-render-state",
        "ledger_path": ledger_path,
        "ledger_id": format!("issue868-edge-{request}"),
        "request_id": format!("issue868-edge-{request}"),
        "requested_by": "litd-editor-issue868-fsv",
        "library": library
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
    assert_eq!(report["issue"], 868);
    assert_eq!(report["library"]["readback"]["texture_count"], 3);
    assert_eq!(report["library"]["readback"]["material_count"], 3);
    assert_eq!(report["library"]["readback"]["sheet_count"], 1);
    assert_eq!(report["library"]["readback"]["billboard_count"], 4);
    assert_eq!(report["library"]["readback"]["search_row_count"], 11);
    assert_eq!(report["library"]["readback"]["total_vertex_count"], 16);
    assert_eq!(report["library"]["readback"]["total_index_count"], 24);
    assert_eq!(report["library"]["gpu_stats"]["draw_calls"], 1);
    assert_eq!(
        report["editor_readback"]["library_id"],
        "library.issue868.sprite_billboards"
    );
    assert_eq!(report["editor_readback"]["billboard_count"], 4);
    assert_eq!(report["library"]["bmp_readback"]["bmp_signature"], "BM");
    assert_eq!(report["library"]["bmp_readback"]["bmp_width"], 640);
    assert_eq!(report["library"]["bmp_readback"]["bmp_height"], 240);
    assert!(
        report["library"]["gpu_stats"]["output_metrics"]["nonblack_pixel_ratio"]
            .as_f64()
            .unwrap()
            > 0.015
    );
    assert_eq!(report["edges"].as_array().unwrap().len(), 4);
    for edge in report["edges"].as_array().unwrap() {
        assert_eq!(edge["state_unchanged"], true);
        assert!(edge["error"]["code"].as_str().unwrap().starts_with("LITD-"));
    }
    Ok(())
}
