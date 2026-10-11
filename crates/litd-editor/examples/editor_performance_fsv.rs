use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    editor_performance::{
        EditorPerformanceBudget, EditorPerformanceWorldSpec,
        build_editor_performance_report_from_aster, build_editor_performance_world_record,
        enforce_editor_performance_budget,
    },
    world_creation::{
        EditorWorldRecord, WorldAsterReadback, read_world_record, write_world_record,
    },
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_EDITOR_PERFORMANCE_FSV",
        "issue1214_editor_performance",
        "manual-fsv.json",
    )?;
    let before_artifact = path_state(&path);
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1214 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let aster_dir = root.join("aster");

    let budget = EditorPerformanceBudget {
        max_cold_load_ms: 5_000.0,
        max_warm_load_ms: 5_000.0,
        max_heavy_edit_frame_ms: 8.333,
        max_object_count: 10_000,
        max_visible_instances: 10_000,
        max_draw_batches: 64,
        ..EditorPerformanceBudget::default()
    };
    let record = build_editor_performance_world_record(world_spec("issue1214-large", 384, 64))?;
    let write_readback = write_and_read_world(&root, &aster_dir, &record)?;
    let report = build_editor_performance_report_from_aster(
        &aster_dir,
        &record.spec.calyx_document_id,
        budget,
        16,
    )?;
    enforce_editor_performance_budget(&report)?;

    let bounded_edge = bounded_large_world_edge(&root, &aster_dir)?;
    let mut snapshot = json!({
        "schema": "litd.issue1214.editor_performance_fsv.v1",
        "issue": 1214,
        "source_of_truth": "Aster world_record_json readback plus editor performance metric artifact",
        "artifact_path": path,
        "before": {
            "artifact": before_artifact,
            "world": {
                "document_id": record.spec.calyx_document_id,
                "objects": report.object_count,
                "terrain_cells": report.terrain_cell_count,
                "write_readback": readback_summary(&write_readback)
            },
            "budget": budget
        },
        "happy_path": {
            "after": {
                "report": report,
                "passed": true
            }
        },
        "edges": [
            bounded_edge,
            {
                "name": "heavy edit FPS holds",
                "before": {
                    "selected_object_count": 16,
                    "source_fingerprint": record.document_fingerprint
                },
                "after": {
                    "combined_frame_ms": report.heavy_edit_frame.combined_frame_ms,
                    "estimated_fps": report.heavy_edit_frame.estimated_fps,
                    "render_elapsed_ms": report.heavy_edit_frame.render_elapsed_ms,
                    "changed_object_count": report.heavy_edit_frame.changed_object_ids.len(),
                    "after_fingerprint": report.heavy_edit_frame.after_fingerprint
                }
            },
            {
                "name": "cold vs warm load measured",
                "before": {
                    "aster_bytes": report.world_bytes,
                    "document_id": report.calyx_document_id
                },
                "after": {
                    "cold_ms": report.cold_load.elapsed_ms,
                    "warm_ms": report.warm_load.elapsed_ms,
                    "cold_seq": report.cold_load.read_seq,
                    "warm_seq": report.warm_load.read_seq,
                    "fingerprints_match": report.cold_load.parsed_fingerprint == report.warm_load.parsed_fingerprint
                }
            }
        ],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote editor performance FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!(
        "steady frame {}",
        snapshot["happy_path"]["after"]["report"]["steady_frame"]
    );
    println!("heavy edit {}", snapshot["edges"][1]["after"]);
    println!("cold warm {}", snapshot["edges"][2]["after"]);
    Ok(())
}

fn bounded_large_world_edge(
    root: &PathBuf,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let record = build_editor_performance_world_record(world_spec("issue1214-bounded", 600, 60))?;
    let write_readback = write_and_read_world(root, aster_dir, &record)?;
    let budget = EditorPerformanceBudget {
        max_object_count: 400,
        max_cold_load_ms: 5_000.0,
        max_warm_load_ms: 5_000.0,
        max_heavy_edit_frame_ms: 100.0,
        max_visible_instances: 5_000,
        max_draw_batches: 64,
        ..EditorPerformanceBudget::default()
    };
    let report = build_editor_performance_report_from_aster(
        aster_dir,
        &record.spec.calyx_document_id,
        budget,
        8,
    )?;
    let enforcement_error = enforce_editor_performance_budget(&report)
        .expect_err("bounded edge must exceed the configured object budget")
        .to_string();
    Ok(json!({
        "name": "very large world still bounded",
        "before": {
            "object_count": report.object_count,
            "max_object_count": budget.max_object_count,
            "write_readback": readback_summary(&write_readback)
        },
        "after": {
            "violation_count": report.violations.len(),
            "violations": report.violations,
            "enforcement_error": enforcement_error
        }
    }))
}

fn world_spec(id: &str, object_count: usize, grid_width: usize) -> EditorPerformanceWorldSpec {
    EditorPerformanceWorldSpec {
        calyx_document_id: format!("calyx://world/{id}"),
        revision_id: format!("{id}-rev-1"),
        display_name: format!("Issue 1214 Performance {id}"),
        object_count,
        grid_width,
        mesh_variant_count: 16,
        material_variant_count: 4,
        unit_stride: 9,
    }
}

fn write_and_read_world(
    _root: &PathBuf,
    aster_dir: &Path,
    record: &EditorWorldRecord,
) -> Result<WorldAsterReadback, Box<dyn std::error::Error>> {
    write_world_record(aster_dir, record)?;
    read_world_record(aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id).into())
}

fn readback_summary(readback: &WorldAsterReadback) -> serde_json::Value {
    json!({
        "seq": readback.seq,
        "graph_rows": readback.graph_rows,
        "bytes": readback.bytes.len(),
        "cell_key": readback.cell_key,
        "index_key": readback.index_key
    })
}

fn write_document_with_final_state(
    path: &PathBuf,
    document: &mut serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"]["artifact"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            let disk_len = fs::metadata(path)?.len();
            if disk_len != expected_len {
                return Err(format!(
                    "FSV artifact write verification failed for {}: embedded after.bytes={}, disk bytes={}",
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
        "FSV artifact byte-count stabilization failed for {}",
        path.display()
    )
    .into())
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::metadata(path) {
        Ok(metadata) => json!({"exists": true, "bytes": metadata.len()}),
        Err(_) => json!({"exists": false}),
    }
}
