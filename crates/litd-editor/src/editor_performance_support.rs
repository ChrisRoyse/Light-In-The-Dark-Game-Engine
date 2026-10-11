use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

use litd_engine::{EngineError, math::Vec3};

use super::{
    EditorHeavyEditMetrics, EditorLoadMetrics, EditorPerformanceBudget, EditorPerformanceReport,
    EditorPerformanceViolation, elapsed_ms, fps_from_ms, invalid_performance,
    invalid_performance_error, measure_editor_frame,
};
use crate::{
    history::SelectionSet,
    shell::SnapSettings,
    viewport_transform::{ViewportTransformOperation, apply_viewport_operation_in_world_record},
    world_creation::{
        EditorWorldRecord, WorldAsterReadback, parse_world_record, read_world_record,
    },
};

pub(super) fn measure_heavy_edit_frame(
    record: &EditorWorldRecord,
    budget: EditorPerformanceBudget,
    selection_count: usize,
) -> Result<EditorHeavyEditMetrics, EngineError> {
    let mut edited = record.clone();
    let ids = edited
        .document
        .objects
        .keys()
        .filter(|id| id.as_str() != "world")
        .take(selection_count)
        .cloned()
        .collect::<BTreeSet<_>>();
    if ids.len() != selection_count {
        return invalid_performance(
            "heavy_edit.selection",
            format!(
                "requested {selection_count} objects but only {} exist",
                ids.len()
            ),
        );
    }
    let before_fingerprint = edited.document_fingerprint.clone();
    let mut selection = SelectionSet::default();
    selection.select_many(ids.iter().cloned());
    let start = Instant::now();
    let transform_report = apply_viewport_operation_in_world_record(
        &mut edited,
        &selection,
        ViewportTransformOperation::Move {
            delta: Vec3::new(0.25, 0.0, 0.25),
            snap: SnapSettings {
                grid_enabled: false,
                grid_size: 1.0,
                rotation_enabled: false,
                rotation_degrees: 15.0,
                scale_enabled: false,
                scale_step: 0.1,
            },
        },
        None,
    )?;
    let edit_elapsed = elapsed_ms(start.elapsed());
    let frame = measure_editor_frame(&edited.document, budget)?;
    let combined = edit_elapsed + frame.elapsed_ms;
    Ok(EditorHeavyEditMetrics {
        selected_object_count: ids.len(),
        edit_elapsed_ms: edit_elapsed,
        render_elapsed_ms: frame.elapsed_ms,
        combined_frame_ms: combined,
        estimated_fps: fps_from_ms(combined),
        before_fingerprint,
        after_fingerprint: transform_report.after_fingerprint,
        changed_object_ids: transform_report.changed_object_ids,
        frame,
    })
}

pub(super) fn timed_aster_load(
    aster_dir: &Path,
    calyx_document_id: &str,
) -> Result<(WorldAsterReadback, EditorWorldRecord, Duration), EngineError> {
    let start = Instant::now();
    let readback = read_world_record(aster_dir, calyx_document_id)?.ok_or_else(|| {
        invalid_performance_error(
            "aster.read",
            format!("missing Aster world record {calyx_document_id}"),
        )
    })?;
    let record = parse_world_record(&readback.bytes)?;
    Ok((readback, record, start.elapsed()))
}

pub(super) fn load_metrics(
    readback: WorldAsterReadback,
    parsed_fingerprint: String,
    elapsed: Duration,
) -> EditorLoadMetrics {
    EditorLoadMetrics {
        read_seq: readback.seq,
        graph_rows: readback.graph_rows,
        bytes: readback.bytes.len(),
        elapsed_ms: elapsed_ms(elapsed),
        parsed_fingerprint,
    }
}

pub(super) fn collect_violations(
    report: &EditorPerformanceReport,
) -> Vec<EditorPerformanceViolation> {
    let mut violations = Vec::new();
    push_violation(
        &mut violations,
        "load.cold_ms",
        report.cold_load.elapsed_ms,
        report.budget.max_cold_load_ms,
        "cold Aster load exceeded budget",
    );
    push_violation(
        &mut violations,
        "load.warm_ms",
        report.warm_load.elapsed_ms,
        report.budget.max_warm_load_ms,
        "warm Aster load exceeded budget",
    );
    push_violation(
        &mut violations,
        "world.object_count",
        report.object_count as f64,
        report.budget.max_object_count as f64,
        "world object count exceeded editor budget",
    );
    push_violation(
        &mut violations,
        "frame.ms",
        report.steady_frame.elapsed_ms,
        report.budget.target_frame_ms(),
        "steady editor frame plan exceeded 120 FPS budget",
    );
    push_violation(
        &mut violations,
        "frame.visible_instances",
        report.steady_frame.visible_instances as f64,
        report.budget.max_visible_instances as f64,
        "visible instances exceeded editor budget",
    );
    push_violation(
        &mut violations,
        "frame.draw_batches",
        report.steady_frame.draw_batches as f64,
        report.budget.max_draw_batches as f64,
        "draw batches exceeded editor budget",
    );
    push_violation(
        &mut violations,
        "heavy_edit.frame_ms",
        report.heavy_edit_frame.combined_frame_ms,
        report.budget.max_heavy_edit_frame_ms,
        "heavy edit plus render planning exceeded frame budget",
    );
    violations
}

fn push_violation(
    violations: &mut Vec<EditorPerformanceViolation>,
    field: &str,
    observed: f64,
    limit: f64,
    reason: &str,
) {
    if observed > limit {
        violations.push(EditorPerformanceViolation {
            field: field.to_string(),
            observed,
            limit,
            reason: reason.to_string(),
        });
    }
}
