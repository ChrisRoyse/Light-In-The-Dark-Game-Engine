use std::{fs, path::Path};

use litd_editor::mcp_render_tools::{
    READ_RENDER_TOOL, SET_PRIMITIVE_GALLERY_TOOL, call_mcp_render_tool, read_mcp_render_state,
    read_mcp_render_tool_ledger,
};
use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    render::primitive::{
        PrimitiveGallerySelection, PrimitiveGpuScene, PrimitiveKind,
        build_primitive_gallery_readback, render_primitive_gallery_gpu_with_context,
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
    let run = begin_issue_run("issue867_primitive_builder")?;
    let report_path = run.report_path();
    let gallery_bmp = run.artifact_path("primitive-gallery.bmp")?;
    let editor_state = run.artifact_path("editor-render-state.json")?;
    let editor_ledger = run.artifact_path("editor-render-ledger.json")?;

    let context = GpuContext::new_headless().await?;
    let gallery = PrimitiveGallerySelection::blockout_defaults()?;
    let gallery_readback = build_primitive_gallery_readback(&gallery)?;
    let scene = PrimitiveGpuScene {
        width: 720,
        height: 240,
        gallery: gallery.clone(),
    };
    let frame = render_primitive_gallery_gpu_with_context(&context, &scene).await?;
    frame.image.write_bmp(&gallery_bmp)?;

    let editor_write = call_mcp_render_tool(
        SET_PRIMITIVE_GALLERY_TOOL,
        json!({
            "state_path": editor_state,
            "state_id": "issue867-render-state",
            "ledger_path": editor_ledger,
            "ledger_id": "issue867-primitive-gallery-ledger",
            "request_id": "issue867-set-primitive-gallery",
            "requested_by": "litd-editor-issue867-fsv",
            "gallery": gallery
        }),
    )?;
    let editor_read =
        call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": editor_state }))?;
    let editor_state_readback = read_mcp_render_state(&editor_state)?;
    let editor_ledger_readback = read_mcp_render_tool_ledger(&editor_ledger)?;

    let edges = vec![
        edge_duplicate_id(
            &editor_state,
            &run.artifact_path("edge-duplicate-id-ledger.json")?,
        )?,
        edge_bad_sphere_segments(
            &editor_state,
            &run.artifact_path("edge-bad-sphere-ledger.json")?,
        )?,
        edge_odd_line_points(
            &editor_state,
            &run.artifact_path("edge-bad-lines-ledger.json")?,
        )?,
        edge_empty_gallery(
            &editor_state,
            &run.artifact_path("edge-empty-gallery-ledger.json")?,
        )?,
    ];

    let report = json!({
        "schema_version": 1,
        "issue": 867,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "gallery_bmp": gallery_bmp.display().to_string(),
            "editor_state": editor_state.display().to_string(),
            "editor_ledger": editor_ledger.display().to_string(),
            "manual_fsv_readback": "Open the persisted JSON through Synapse, inspect the primitive-gallery BMP, and compare actual primitive/topology/count fields to expected values."
        },
        "research": [
            {
                "url": "https://docs.rs/wgpu/latest/wgpu/enum.PrimitiveTopology.html",
                "applied": "wgpu exposes separate PointList, LineList, and TriangleList topologies for draw calls."
            },
            {
                "url": "https://www.w3.org/TR/webgpu/",
                "applied": "WebGPU primitive topology defines the primitive type assembled by a render pipeline."
            },
            {
                "url": "https://docs.rs/bevy/latest/bevy/mesh/primitives/index.html",
                "applied": "A mature Rust game engine exposes primitive mesh builders for blockout shapes."
            }
        ],
        "visual_references_reviewed": [
            "docs/readme/ED03-object-editor.png",
            "docs/readme/ED11-world-view.png",
            "docs/readme/ED02-terrain-editor.png"
        ],
        "sufficiency_argument": "The nine-panel gallery is the smallest source-of-truth dataset for #867 because it includes every requested primitive family: box, sphere, cylinder, torus, tube, plane, sprite, points, and lines. Deterministic mesh counts prove procedural generation; the real wgpu pass proves triangle, point, and line topologies execute; the MCP write/read proves editor state owns the result; four edge probes prove invalid geometry fails before mutation.",
        "gpu_context": context_state(&context),
        "gallery": {
            "readback": gallery_readback,
            "gpu_stats": frame.stats,
            "bmp_readback": file_state(&gallery_bmp)
        },
        "editor_readback": {
            "write_result": editor_write.result,
            "read_result": editor_read.result,
            "state_path": editor_state_readback.path,
            "state_sha256": editor_state_readback.sha256,
            "selected_primitive_id": editor_state_readback.primitive_gallery_selected_id,
            "primitive_count": editor_state_readback.primitive_gallery_count,
            "topology_ids": editor_state_readback.primitive_gallery_topology_ids,
            "total_vertex_count": editor_state_readback.primitive_gallery_total_vertex_count,
            "total_index_count": editor_state_readback.primitive_gallery_total_index_count,
            "ledger_path": editor_ledger_readback.path,
            "ledger_sha256": editor_ledger_readback.sha256,
            "ledger_record_count": editor_ledger_readback.ledger.record_count
        },
        "edges": edges,
        "expected_actuals": {
            "primitive_ids": [
                "blockout.box",
                "blockout.sphere",
                "blockout.cylinder",
                "blockout.torus",
                "blockout.tube",
                "blockout.plane",
                "blockout.sprite",
                "blockout.points",
                "blockout.lines"
            ],
            "primitive_count": 9,
            "selected_primitive_id": "blockout.box",
            "total_vertex_count": 588,
            "total_index_count": 2284,
            "draw_calls": 9,
            "topology_ids": [
                "triangle_list",
                "triangle_list",
                "triangle_list",
                "triangle_list",
                "triangle_list",
                "triangle_list",
                "triangle_list",
                "point_list",
                "line_list"
            ],
            "edge_count": 4
        }
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "issue867_primitive_builder_fsv report={} bytes={} sha256={} primitives={} draws={} selected={} bmp_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["gallery"]["readback"]["primitive_count"],
        readback.json["gallery"]["gpu_stats"]["draw_calls"],
        readback.json["editor_readback"]["selected_primitive_id"],
        readback.json["gallery"]["bmp_readback"]["sha256"],
    );
    Ok(())
}

fn edge_duplicate_id(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let mut gallery = PrimitiveGallerySelection::blockout_defaults()?;
    gallery.primitives[1].primitive_id = gallery.primitives[0].primitive_id.clone();
    let error = call_mcp_render_tool(
        SET_PRIMITIVE_GALLERY_TOOL,
        edge_args(state_path, ledger_path, "duplicate-id", gallery),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "duplicate_id",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_bad_sphere_segments(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let mut gallery = PrimitiveGallerySelection::blockout_defaults()?;
    gallery.primitives[1].kind = PrimitiveKind::Sphere {
        radius: 0.55,
        segments: 2,
        rings: 8,
    };
    let error = call_mcp_render_tool(
        SET_PRIMITIVE_GALLERY_TOOL,
        edge_args(state_path, ledger_path, "bad-sphere", gallery),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "bad_sphere_segments",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_odd_line_points(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let mut gallery = PrimitiveGallerySelection::blockout_defaults()?;
    gallery.primitives[8].kind = PrimitiveKind::Lines {
        points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    };
    let error = call_mcp_render_tool(
        SET_PRIMITIVE_GALLERY_TOOL,
        edge_args(state_path, ledger_path, "bad-lines", gallery),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "odd_line_points",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_empty_gallery(state_path: &Path, ledger_path: &Path) -> Result<Value, EngineError> {
    let before = read_mcp_render_state(state_path)?;
    let mut gallery = PrimitiveGallerySelection::blockout_defaults()?;
    gallery.primitives.clear();
    let error = call_mcp_render_tool(
        SET_PRIMITIVE_GALLERY_TOOL,
        edge_args(state_path, ledger_path, "empty-gallery", gallery),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?;
    Ok(edge_state(
        "empty_gallery",
        before.sha256,
        after.sha256,
        error,
    ))
}

fn edge_args(
    state_path: &Path,
    ledger_path: &Path,
    request: &str,
    gallery: PrimitiveGallerySelection,
) -> Value {
    json!({
        "state_path": state_path,
        "state_id": "issue867-render-state",
        "ledger_path": ledger_path,
        "ledger_id": format!("issue867-edge-{request}"),
        "request_id": format!("issue867-edge-{request}"),
        "requested_by": "litd-editor-issue867-fsv",
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
    assert_eq!(report["issue"], 867);
    assert_eq!(report["gallery"]["readback"]["primitive_count"], 9);
    assert_eq!(
        report["gallery"]["readback"]["primitive_ids"],
        report["expected_actuals"]["primitive_ids"]
    );
    assert_eq!(report["gallery"]["readback"]["total_vertex_count"], 588);
    assert_eq!(report["gallery"]["readback"]["total_index_count"], 2284);
    assert_eq!(report["gallery"]["gpu_stats"]["draw_calls"], 9);
    assert_eq!(
        report["gallery"]["gpu_stats"]["rendered_topologies"],
        report["expected_actuals"]["topology_ids"]
    );
    assert_eq!(
        report["editor_readback"]["selected_primitive_id"],
        "blockout.box"
    );
    assert_eq!(report["editor_readback"]["primitive_count"], 9);
    assert_eq!(report["gallery"]["bmp_readback"]["bmp_signature"], "BM");
    assert_eq!(report["gallery"]["bmp_readback"]["bmp_width"], 720);
    assert_eq!(report["gallery"]["bmp_readback"]["bmp_height"], 240);
    assert!(
        report["gallery"]["gpu_stats"]["output_metrics"]["nonblack_pixel_ratio"]
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
