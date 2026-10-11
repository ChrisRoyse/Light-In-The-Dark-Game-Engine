//! Editor load and frame-budget measurements for large world authoring.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use litd_engine::{
    EngineError,
    math::{Aabb, Transform, Vec2, Vec3},
    render::batching::{BatchingSettings, Frustum, RenderInstance, ScreenRect, build_batch_plan},
};
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorObject, stable_fingerprint},
    world_creation::{
        EditorWorldCreationSpec, EditorWorldRecord, create_editor_world_record,
        validate_world_record,
    },
};

#[path = "editor_performance_support.rs"]
mod editor_performance_support;
use editor_performance_support::{
    collect_violations, load_metrics, measure_heavy_edit_frame, timed_aster_load,
};

pub const PERF_MESH_ID_PROPERTY: &str = "editor.performance.mesh_id";
pub const PERF_MATERIAL_ID_PROPERTY: &str = "editor.performance.material_id";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EditorPerformanceBudget {
    pub target_fps: u32,
    pub max_cold_load_ms: f64,
    pub max_warm_load_ms: f64,
    pub max_heavy_edit_frame_ms: f64,
    pub max_object_count: usize,
    pub max_visible_instances: usize,
    pub max_draw_batches: usize,
}

impl Default for EditorPerformanceBudget {
    fn default() -> Self {
        Self {
            target_fps: 120,
            max_cold_load_ms: 3_000.0,
            max_warm_load_ms: 1_000.0,
            max_heavy_edit_frame_ms: 8.333,
            max_object_count: 50_000,
            max_visible_instances: 50_000,
            max_draw_batches: 128,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorPerformanceWorldSpec {
    pub calyx_document_id: String,
    pub revision_id: String,
    pub display_name: String,
    pub object_count: usize,
    pub grid_width: usize,
    pub mesh_variant_count: usize,
    pub material_variant_count: usize,
    pub unit_stride: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorPerformanceReport {
    pub schema: String,
    pub source_of_truth: String,
    pub budget: EditorPerformanceBudget,
    pub calyx_document_id: String,
    pub source_fingerprint: String,
    pub world_bytes: usize,
    pub object_count: usize,
    pub terrain_cell_count: usize,
    pub cold_load: EditorLoadMetrics,
    pub warm_load: EditorLoadMetrics,
    pub steady_frame: EditorFrameMetrics,
    pub heavy_edit_frame: EditorHeavyEditMetrics,
    pub violations: Vec<EditorPerformanceViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorLoadMetrics {
    pub read_seq: u64,
    pub graph_rows: usize,
    pub bytes: usize,
    pub elapsed_ms: f64,
    pub parsed_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorFrameMetrics {
    pub object_count: usize,
    pub visible_instances: usize,
    pub frustum_culled_instances: usize,
    pub occlusion_culled_instances: usize,
    pub draw_batches: usize,
    pub indirect_commands: usize,
    pub triangle_count: u64,
    pub elapsed_ms: f64,
    pub estimated_fps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorHeavyEditMetrics {
    pub selected_object_count: usize,
    pub edit_elapsed_ms: f64,
    pub render_elapsed_ms: f64,
    pub combined_frame_ms: f64,
    pub estimated_fps: f64,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub changed_object_ids: Vec<String>,
    pub frame: EditorFrameMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorPerformanceViolation {
    pub field: String,
    pub observed: f64,
    pub limit: f64,
    pub reason: String,
}

impl EditorPerformanceBudget {
    pub fn target_frame_ms(&self) -> f64 {
        1_000.0 / f64::from(self.target_fps)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.target_fps == 0 {
            return invalid_performance("budget.target_fps", "must be greater than zero");
        }
        if !self.max_cold_load_ms.is_finite()
            || !self.max_warm_load_ms.is_finite()
            || !self.max_heavy_edit_frame_ms.is_finite()
            || self.max_cold_load_ms <= 0.0
            || self.max_warm_load_ms <= 0.0
            || self.max_heavy_edit_frame_ms <= 0.0
        {
            return invalid_performance(
                "budget.ms",
                "load and frame budgets must be finite and positive",
            );
        }
        if self.max_object_count == 0
            || self.max_visible_instances == 0
            || self.max_draw_batches == 0
        {
            return invalid_performance(
                "budget.counts",
                "object, visible instance, and draw batch budgets must be non-zero",
            );
        }
        Ok(())
    }
}

impl EditorPerformanceWorldSpec {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.calyx_document_id.trim().is_empty()
            || self.revision_id.trim().is_empty()
            || self.display_name.trim().is_empty()
        {
            return invalid_performance(
                "world.identity",
                "document id, revision id, and display name are required",
            );
        }
        if self.object_count == 0 || self.grid_width == 0 {
            return invalid_performance(
                "world.size",
                "object count and grid width must be non-zero",
            );
        }
        if self.mesh_variant_count == 0 || self.material_variant_count == 0 {
            return invalid_performance(
                "world.variants",
                "mesh and material variant counts must be non-zero",
            );
        }
        if self.unit_stride == 0 {
            return invalid_performance("world.unit_stride", "must be non-zero");
        }
        Ok(())
    }
}

pub fn build_editor_performance_world_record(
    spec: EditorPerformanceWorldSpec,
) -> Result<EditorWorldRecord, EngineError> {
    spec.validate()?;
    let mut world_spec = EditorWorldCreationSpec::issue1210_default();
    world_spec.calyx_document_id = spec.calyx_document_id.clone();
    world_spec.revision_id = spec.revision_id.clone();
    world_spec.properties.display_name = spec.display_name.clone();
    world_spec.properties.width_tiles = spec.grid_width.max(16) as u32;
    world_spec.properties.height_tiles = ((spec.object_count / spec.grid_width) + 2).max(12) as u32;
    world_spec.properties.playable_bounds.max_tile_x_exclusive =
        world_spec.properties.width_tiles as i32;
    world_spec.properties.playable_bounds.max_tile_z_exclusive =
        world_spec.properties.height_tiles as i32;
    let mut record = create_editor_world_record(world_spec)?;
    record.source_issue = 1214;

    for index in 0..spec.object_count {
        let x = (index % spec.grid_width) as f32;
        let z = (index / spec.grid_width) as f32;
        let kind = if index % spec.unit_stride == 0 {
            "Unit"
        } else {
            "Doodad"
        };
        let mut object = EditorObject::new(
            format!("perf.object.{index:05}"),
            kind,
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(x * 2.0, 0.0, z * 2.0)),
        );
        object.properties.insert(
            PERF_MESH_ID_PROPERTY.to_string(),
            format!("mesh.variant.{}", index % spec.mesh_variant_count),
        );
        object.properties.insert(
            PERF_MATERIAL_ID_PROPERTY.to_string(),
            format!("material.variant.{}", index % spec.material_variant_count),
        );
        record.document.upsert_object(object)?;
    }
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.summary = format!(
        "world={} editor_performance_fixture objects={} fingerprint={}",
        record.spec.properties.display_name, spec.object_count, record.document_fingerprint
    );
    validate_world_record(&record)?;
    Ok(record)
}

pub fn build_editor_performance_report_from_aster(
    aster_dir: &Path,
    calyx_document_id: &str,
    budget: EditorPerformanceBudget,
    heavy_edit_selection_count: usize,
) -> Result<EditorPerformanceReport, EngineError> {
    budget.validate()?;
    if heavy_edit_selection_count == 0 {
        return invalid_performance("heavy_edit.selection", "selection count must be non-zero");
    }

    let (cold_readback, cold_record, cold_elapsed) =
        timed_aster_load(aster_dir, calyx_document_id)?;
    let (warm_readback, warm_record, warm_elapsed) =
        timed_aster_load(aster_dir, calyx_document_id)?;
    if cold_record.document_fingerprint != warm_record.document_fingerprint {
        return invalid_performance(
            "load.fingerprint",
            "cold and warm Aster reads returned different document fingerprints",
        );
    }

    let steady_frame = measure_editor_frame(&warm_record.document, budget)?;
    let heavy_edit_frame =
        measure_heavy_edit_frame(&warm_record, budget, heavy_edit_selection_count)?;
    let cold_load = load_metrics(
        cold_readback,
        cold_record.document_fingerprint,
        cold_elapsed,
    );
    let warm_load = load_metrics(
        warm_readback,
        warm_record.document_fingerprint.clone(),
        warm_elapsed,
    );
    let mut report = EditorPerformanceReport {
        schema: "litd.editor_performance_budget.v1".to_string(),
        source_of_truth: "Calyx Aster world record row readback plus render batch-plan metrics"
            .to_string(),
        budget,
        calyx_document_id: warm_record.spec.calyx_document_id.clone(),
        source_fingerprint: warm_record.document_fingerprint,
        world_bytes: warm_load.bytes,
        object_count: warm_record.document.objects.len().saturating_sub(1),
        terrain_cell_count: warm_record.document.terrain_cells.len(),
        cold_load,
        warm_load,
        steady_frame,
        heavy_edit_frame,
        violations: Vec::new(),
    };
    report.violations = collect_violations(&report);
    Ok(report)
}

pub fn enforce_editor_performance_budget(
    report: &EditorPerformanceReport,
) -> Result<(), EngineError> {
    if report.violations.is_empty() {
        return Ok(());
    }
    let summary = report
        .violations
        .iter()
        .map(|violation| {
            format!(
                "{} observed {:.3} > limit {:.3}",
                violation.field, violation.observed, violation.limit
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    invalid_performance("budget.enforcement", summary)
}

pub fn measure_editor_frame(
    document: &crate::history::EditorDocument,
    budget: EditorPerformanceBudget,
) -> Result<EditorFrameMetrics, EngineError> {
    budget.validate()?;
    let start = Instant::now();
    let instances = render_instances_for_document(document)?;
    let frustum = whole_world_frustum(&instances)?;
    let plan = build_batch_plan(
        BatchingSettings {
            hzb_enabled: false,
            occlusion_bias: 0.0,
            max_draw_batches: budget.max_draw_batches,
        },
        &frustum,
        &[],
        &instances,
    )?;
    let elapsed = elapsed_ms(start.elapsed());
    let mut triangle_count = 0_u64;
    for batch in &plan.draw_batches {
        let triangles_per_instance = u64::from(batch.index_count / 3);
        let instance_count = u64::try_from(batch.instance_ids.len()).map_err(|_| {
            invalid_performance_error("frame.triangle_count", "instance count overflowed u64")
        })?;
        let batch_triangles = triangles_per_instance
            .checked_mul(instance_count)
            .ok_or_else(|| {
                invalid_performance_error("frame.triangle_count", "triangle count overflowed u64")
            })?;
        triangle_count = triangle_count.checked_add(batch_triangles).ok_or_else(|| {
            invalid_performance_error("frame.triangle_count", "triangle count overflowed u64")
        })?;
    }
    Ok(EditorFrameMetrics {
        object_count: instances.len(),
        visible_instances: plan.visible_instance_ids.len(),
        frustum_culled_instances: plan.frustum_culled_ids.len(),
        occlusion_culled_instances: plan.occlusion_culled_ids.len(),
        draw_batches: plan.draw_batches.len(),
        indirect_commands: plan.indirect_commands.len(),
        triangle_count,
        elapsed_ms: elapsed,
        estimated_fps: fps_from_ms(elapsed),
    })
}

fn render_instances_for_document(
    document: &crate::history::EditorDocument,
) -> Result<Vec<RenderInstance>, EngineError> {
    document.validate()?;
    let mut instances = Vec::new();
    for (index, object) in document
        .objects
        .values()
        .filter(|object| object.id != "world")
        .enumerate()
    {
        let mesh_id = required_render_property(object, PERF_MESH_ID_PROPERTY, "mesh_id")?;
        let material_id =
            required_render_property(object, PERF_MATERIAL_ID_PROPERTY, "material_id")?;
        let world_bounds =
            Aabb::from_center_half_extents(object.transform.translation, Vec3::splat(0.45))?;
        let screen_min = Vec2::new(0.0, 0.0);
        let screen_max = Vec2::new(0.01, 0.01);
        let nearest_depth = ((index % 1_000) as f32 / 1_000.0).clamp(0.0, 1.0);
        instances.push(RenderInstance {
            id: u64::try_from(index + 1).map_err(|_| {
                invalid_performance_error("render.instance_id", "instance id overflowed")
            })?,
            mesh_id,
            material_id,
            world_bounds,
            screen_rect: ScreenRect::new(screen_min, screen_max)?,
            nearest_depth,
            index_count: 36,
        });
    }
    if instances.is_empty() {
        return invalid_performance("document.objects", "no renderable editor objects found");
    }
    Ok(instances)
}

fn required_render_property(
    object: &EditorObject,
    property_key: &str,
    field_name: &str,
) -> Result<String, EngineError> {
    object
        .properties
        .get(property_key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            invalid_performance_error(
                format!("object.{}.{}", object.id, field_name),
                format!("missing required render property {property_key}"),
            )
        })
}

fn whole_world_frustum(instances: &[RenderInstance]) -> Result<Frustum, EngineError> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for instance in instances {
        min = min.min(instance.world_bounds.min);
        max = max.max(instance.world_bounds.max);
    }
    Frustum::orthographic(Aabb::new(min - Vec3::splat(1.0), max + Vec3::splat(1.0))?)
}

fn elapsed_ms(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1_000.0).max(0.001)
}

fn fps_from_ms(ms: f64) -> f64 {
    1_000.0 / ms.max(0.001)
}

fn invalid_performance<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_performance_error(field, reason))
}

fn invalid_performance_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.performance.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "editor_performance_tests.rs"]
mod tests;
