use std::{fs, path::PathBuf};

use super::*;
use crate::world_creation::write_world_record;

#[test]
fn large_world_performance_report_reads_aster_and_enforces_budget() {
    let root = temp_root("large_world_performance_report");
    let record = build_editor_performance_world_record(world_spec("large", 512, 64)).unwrap();
    let aster_dir = write_record(&root, &record);
    let budget = EditorPerformanceBudget {
        max_cold_load_ms: 5_000.0,
        max_warm_load_ms: 5_000.0,
        max_heavy_edit_frame_ms: 100.0,
        ..EditorPerformanceBudget::default()
    };

    let report = build_editor_performance_report_from_aster(
        &aster_dir,
        &record.spec.calyx_document_id,
        budget,
        32,
    )
    .unwrap();
    enforce_editor_performance_budget(&report).unwrap();

    assert_eq!(report.object_count, 512);
    assert_eq!(report.steady_frame.visible_instances, 512);
    assert!(report.steady_frame.draw_batches <= 32);
    assert_eq!(
        report.cold_load.parsed_fingerprint,
        record.document_fingerprint
    );
    assert_eq!(
        report.warm_load.parsed_fingerprint,
        record.document_fingerprint
    );
    assert_eq!(report.heavy_edit_frame.selected_object_count, 32);
    assert_ne!(
        report.heavy_edit_frame.before_fingerprint,
        report.heavy_edit_frame.after_fingerprint
    );
}

#[test]
fn object_budget_violation_is_reported_and_enforced() {
    let root = temp_root("object_budget_violation");
    let record = build_editor_performance_world_record(world_spec("bounded", 64, 16)).unwrap();
    let aster_dir = write_record(&root, &record);
    let budget = EditorPerformanceBudget {
        max_object_count: 10,
        max_cold_load_ms: 5_000.0,
        max_warm_load_ms: 5_000.0,
        max_heavy_edit_frame_ms: 100.0,
        ..EditorPerformanceBudget::default()
    };

    let report = build_editor_performance_report_from_aster(
        &aster_dir,
        &record.spec.calyx_document_id,
        budget,
        8,
    )
    .unwrap();
    assert!(
        report
            .violations
            .iter()
            .any(|violation| violation.field == "world.object_count")
    );
    let error = enforce_editor_performance_budget(&report).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("editor.performance.budget.enforcement")
    );
}

#[test]
fn invalid_budget_and_missing_aster_fail_loud() {
    let invalid_budget = EditorPerformanceBudget {
        target_fps: 0,
        ..Default::default()
    };
    let error = invalid_budget.validate().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("editor.performance.budget.target_fps")
    );

    let root = temp_root("missing_aster");
    let error = build_editor_performance_report_from_aster(
        &root.join("aster"),
        "calyx://world/missing-performance-world",
        EditorPerformanceBudget::default(),
        1,
    )
    .unwrap_err();
    assert!(error.to_string().contains("editor.performance.aster.read"));
}

#[test]
fn missing_render_identity_fails_loud_without_synthetic_frame() {
    let root = temp_root("missing_render_identity");
    let mut record =
        build_editor_performance_world_record(world_spec("missing-render", 16, 8)).unwrap();
    record
        .document
        .objects
        .get_mut("perf.object.00000")
        .unwrap()
        .properties
        .remove(PERF_MESH_ID_PROPERTY);
    record.document_fingerprint = stable_fingerprint(&record.document).unwrap();
    let aster_dir = write_record(&root, &record);

    let error = build_editor_performance_report_from_aster(
        &aster_dir,
        &record.spec.calyx_document_id,
        EditorPerformanceBudget {
            max_cold_load_ms: 5_000.0,
            max_warm_load_ms: 5_000.0,
            max_heavy_edit_frame_ms: 100.0,
            ..EditorPerformanceBudget::default()
        },
        4,
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("editor.performance.object.perf.object.00000.mesh_id")
    );
}

fn world_spec(id: &str, object_count: usize, grid_width: usize) -> EditorPerformanceWorldSpec {
    EditorPerformanceWorldSpec {
        calyx_document_id: format!("calyx://world/perf-{id}"),
        revision_id: format!("perf-{id}-rev-1"),
        display_name: format!("Performance {id}"),
        object_count,
        grid_width,
        mesh_variant_count: 8,
        material_variant_count: 4,
        unit_stride: 9,
    }
}

fn write_record(root: &std::path::Path, record: &EditorWorldRecord) -> PathBuf {
    fs::create_dir_all(root).unwrap();
    let aster_dir = root.join("aster");
    write_world_record(&aster_dir, record).unwrap();
    aster_dir
}

fn temp_root(label: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(&format!("editor-performance-{label}"))
}
