//! FSV harness for issue #1740: two-phase Hi-Z occlusion culling + GPU-driven
//! indirect-draw compaction, with the CPU-culled non-indirect fallback path.
//!
//! Source of truth = the persisted JSON under `target/fsv/`. It records, from a REAL
//! headless GPU run (`hzb_build.wgsl` + `two_phase_cull.wgsl`):
//!   * a dense 1024-instance scene, half of it behind a previously-visible wall, whose
//!     known-occluded instances are proven absent from the compacted count buffer;
//!   * the toggle of the CPU fallback flag producing a BYTE-IDENTICAL visible set
//!     (the culling-level cause of a pixel-identical image; pixel equality itself is a
//!     T4/live-editor claim and is called out as remaining work);
//!   * the GPU-driven vs CPU-fallback dispatch time delta;
//!   * the wgpu-27 indirect-draw reality (INDIRECT_EXECUTION + MULTI_DRAW_INDIRECT_COUNT);
//!   * three edge cases: all-visible, all-occluded, and empty scene.
//!
//! The GPU-built pyramid is compared against the CPU pyramid, and every GPU compaction
//! is checked field-by-field against the deterministic CPU oracle -- if the two paths
//! disagree the harness fails loud. #800.

use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{Frustum, RenderInstance, ScreenRect},
        gpu_driven::{CompactionResult, DrawPathMode, TwoPhaseSettings, select_draw_path},
        gpu_driven_cpu::run_two_phase_cpu,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

#[path = "issue1740_two_phase_cull_fsv/gpu.rs"]
mod gpu;

use gpu::{GpuTwoPhaseReadback, run_gpu_two_phase};

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    pollster::block_on(run())
}

async fn run() -> Result<(), EngineError> {
    let root = fsv_root()?;
    fs::create_dir_all(&root).map_err(|e| io_error("root", e))?;
    let report_path = root.join("issue1740-two-phase-cull-readback.json");

    let settings = TwoPhaseSettings::default();
    let frustum = wide_frustum()?;

    // ---- Main dense case: 1024 instances, left half occluded behind a near wall. ----
    let (instances, prev_visible) = dense_scene()?;
    let cpu = run_two_phase_cpu(settings, &frustum, &instances, &prev_visible)?;
    let cpu_started = std::time::Instant::now();
    let _ = run_two_phase_cpu(settings, &frustum, &instances, &prev_visible)?;
    let cpu_ns = cpu_started.elapsed().as_nanos();
    let gpu = run_gpu_two_phase(settings, &frustum, &instances, &prev_visible).await?;
    assert_paths_agree("dense", &cpu, &gpu)?;

    // The occluded instances are the left-half grid cells; prove they are culled.
    let occluded_ids = occluded_instance_ids(&instances, &cpu);

    // Draw-path selection is explicit and logged, not a silent fallback.
    let gpu_selection =
        select_draw_path(DrawPathMode::GpuIndirect, gpu.indirect_execution_supported)?;
    let cpu_selection =
        select_draw_path(DrawPathMode::CpuFallback, gpu.indirect_execution_supported)?;
    let unsupported_is_hard_error = select_draw_path(DrawPathMode::GpuIndirect, false).is_err();

    let edges = vec![
        edge_all_visible(settings, &frustum).await?,
        edge_all_occluded(settings, &frustum).await?,
        edge_empty_scene(settings, &frustum).await?,
    ];

    let report = json!({
        "issue": 1740,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "source_modules": [
                "crates/litd-engine/src/render/gpu_driven.rs",
                "crates/litd-engine/src/render/gpu_driven_cpu.rs",
                "crates/litd-engine/src/render/hzb_build.wgsl",
                "crates/litd-engine/src/render/two_phase_cull.wgsl"
            ],
            "readback": "read this persisted JSON + its SHA256; the counts come from a real GPU atomic count buffer"
        },
        "sufficiency_argument": "A 1024-instance scene (512 behind a previously-visible wall at depth 0.30, 512 in front) proves known-occluded instances are absent from the GPU-written count buffer. The GPU-built Hi-Z pyramid is byte-compared to the CPU pyramid, and the GPU compaction (count, phase split, visible set, per-instance class) is checked against the deterministic CPU oracle, so the indirect and CPU-fallback paths provably draw the identical set. Three edges cover all-visible, all-occluded, and empty scene.",
        "adapter": {
            "name": gpu.adapter.name,
            "backend": gpu.adapter.backend,
            "device_type": gpu.adapter.device_type
        },
        "wgpu_reality": {
            "wgpu_version": "27.0.1",
            "indirect_execution_supported": gpu.indirect_execution_supported,
            "multi_draw_indirect_count_supported": gpu.multi_draw_indirect_count_supported,
            "notes": "wgpu 27 removed Features::MULTI_DRAW_INDIRECT (now gated only on DownlevelFlags::INDIRECT_EXECUTION); MULTI_DRAW_INDIRECT_COUNT remains a native-only feature enabling the count-buffer draw. No WGSL gl_DrawID builtin (gfx-rs #6823): the compacted command's first_instance carries the instance index instead."
        },
        "draw_path_selection": {
            "gpu": { "selected": format!("{:?}", gpu_selection.selected), "reason": format!("{:?}", gpu_selection.reason) },
            "cpu": { "selected": format!("{:?}", cpu_selection.selected), "reason": format!("{:?}", cpu_selection.reason) },
            "gpu_requested_on_unsupported_hw_is_hard_error": unsupported_is_hard_error
        },
        "dense_case": {
            "before": {
                "instances": instances.len(),
                "previously_visible": prev_visible.iter().filter(|v| **v).count(),
                "wall_depth": 0.30,
                "behind_depth": 0.80,
                "front_depth": 0.10
            },
            "after": {
                "visible_count": gpu.visible_count,
                "phase1_count": gpu.phase1_count,
                "phase2_count": gpu.phase2_count,
                "occluded_count": occluded_ids.len(),
                "sample_occluded_ids": occluded_ids.iter().take(6).collect::<Vec<_>>(),
                "gpu_pyramid_matches_cpu": gpu.pyramid_matches_cpu,
                "gpu_visible_set_equals_cpu": gpu.visible_indices == cpu.visible_indices,
                "gpu_classes_equal_cpu": classes_match(&cpu, &gpu),
                "result_consistent": cpu.is_consistent()
            }
        },
        "frame_time_delta": {
            "gpu_dispatch_ns": gpu.dispatch_ns,
            "cpu_fallback_ns": cpu_ns,
            "note": "wall-clock of build+cull+readback (GPU incl. map stalls) vs the CPU model; both under one frame at this scale"
        },
        "device_lost_report": gpu.device_lost,
        "edges": edges,
        "expected_actuals": {
            "dense_visible_count": 513,
            "dense_phase1_count": 1,
            "dense_phase2_count": 512,
            "dense_occluded_count": 512,
            "edge_all_visible_count": 4,
            "edge_all_occluded_visible_count": 1,
            "edge_empty_visible_count": 0
        }
    });

    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|e| EngineError::SerializationFailed {
            format: "issue1740-two-phase-cull-json",
            reason: e.to_string(),
        })?;
    fs::write(&report_path, &bytes).map_err(|e| io_error("write", e))?;
    let persisted = fs::read(&report_path).map_err(|e| io_error("read", e))?;
    if persisted != bytes {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: format!("{} bytes", bytes.len()),
            actual: format!("{} bytes", persisted.len()),
        });
    }
    let value: serde_json::Value =
        serde_json::from_slice(&persisted).map_err(|e| EngineError::SerializationFailed {
            format: "issue1740-two-phase-cull-json-reparse",
            reason: e.to_string(),
        })?;
    assert_report(&value)?;
    println!(
        "issue1740 two_phase_cull_fsv report={} bytes={} sha256={} adapter={} visible={} phase1={} phase2={} occluded={} gpu_ns={} cpu_ns={}",
        report_path.display(),
        persisted.len(),
        sha256_hex(&persisted),
        value["adapter"]["name"],
        value["dense_case"]["after"]["visible_count"],
        value["dense_case"]["after"]["phase1_count"],
        value["dense_case"]["after"]["phase2_count"],
        value["dense_case"]["after"]["occluded_count"],
        value["frame_time_delta"]["gpu_dispatch_ns"],
        value["frame_time_delta"]["cpu_fallback_ns"]
    );
    Ok(())
}

fn classes_match(cpu: &CompactionResult, gpu: &GpuTwoPhaseReadback) -> bool {
    cpu.classes.len() == gpu.classes.len()
        && cpu
            .classes
            .iter()
            .zip(gpu.classes.iter())
            .all(|(c, g)| *c as u32 == *g)
}

/// The GPU compaction MUST equal the CPU oracle in every field, or fail loud.
fn assert_paths_agree(
    case: &str,
    cpu: &CompactionResult,
    gpu: &GpuTwoPhaseReadback,
) -> Result<(), EngineError> {
    let ok = gpu.device_lost.is_none()
        && gpu.pyramid_matches_cpu
        && gpu.visible_count == cpu.visible_count
        && gpu.phase1_count == cpu.phase1_count
        && gpu.phase2_count == cpu.phase2_count
        && gpu.visible_indices == cpu.visible_indices
        && classes_match(cpu, gpu)
        && cpu.is_consistent();
    if ok {
        Ok(())
    } else {
        Err(EngineError::SerializedFingerprintMismatch {
            expected: format!(
                "{case}: gpu==cpu (visible {} phase1 {} phase2 {})",
                cpu.visible_count, cpu.phase1_count, cpu.phase2_count
            ),
            actual: format!(
                "gpu(visible {} phase1 {} phase2 {} pyramid_match {} device_lost {:?})",
                gpu.visible_count,
                gpu.phase1_count,
                gpu.phase2_count,
                gpu.pyramid_matches_cpu,
                gpu.device_lost
            ),
        })
    }
}

fn occluded_instance_ids(instances: &[RenderInstance], cpu: &CompactionResult) -> Vec<u64> {
    instances
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            matches!(
                cpu.classes[*index],
                litd_engine::render::gpu_driven::VisibilityClass::Culled
            )
        })
        .map(|(_, instance)| instance.id)
        .collect()
}

async fn edge_all_visible(
    settings: TwoPhaseSettings,
    frustum: &Frustum,
) -> Result<serde_json::Value, EngineError> {
    // No previously-visible occluders -> empty pyramid -> nothing occluded.
    let instances = vec![
        instance(1, rect(0.05, 0.05, 0.09, 0.09)?, 0.5)?,
        instance(2, rect(0.20, 0.20, 0.24, 0.24)?, 0.5)?,
        instance(3, rect(0.60, 0.60, 0.64, 0.64)?, 0.5)?,
        instance(4, rect(0.80, 0.80, 0.84, 0.84)?, 0.5)?,
    ];
    let prev = vec![false; instances.len()];
    edge("all_visible", settings, frustum, instances, prev, 4).await
}

async fn edge_all_occluded(
    settings: TwoPhaseSettings,
    frustum: &Frustum,
) -> Result<serde_json::Value, EngineError> {
    // A full-screen near wall (prev-visible) hides every other instance.
    let mut instances = vec![instance(1, rect(0.0, 0.0, 1.0, 1.0)?, 0.20)?];
    let mut prev = vec![true];
    for i in 0..8u64 {
        let x = 0.1 + (i as f32) * 0.09;
        instances.push(instance(i + 2, rect(x, 0.4, x + 0.03, 0.46)?, 0.90)?);
        prev.push(false);
    }
    // Only the wall survives (visible_count == 1); the eight behind are all culled.
    edge("all_occluded", settings, frustum, instances, prev, 1).await
}

async fn edge_empty_scene(
    settings: TwoPhaseSettings,
    frustum: &Frustum,
) -> Result<serde_json::Value, EngineError> {
    edge("empty_scene", settings, frustum, Vec::new(), Vec::new(), 0).await
}

async fn edge(
    name: &str,
    settings: TwoPhaseSettings,
    frustum: &Frustum,
    instances: Vec<RenderInstance>,
    prev: Vec<bool>,
    expected_visible: u32,
) -> Result<serde_json::Value, EngineError> {
    let cpu = run_two_phase_cpu(settings, frustum, &instances, &prev)?;
    let gpu = run_gpu_two_phase(settings, frustum, &instances, &prev).await?;
    assert_paths_agree(name, &cpu, &gpu)?;
    Ok(json!({
        "name": name,
        "before": { "instances": instances.len(), "previously_visible": prev.iter().filter(|v| **v).count() },
        "after": {
            "visible_count": gpu.visible_count,
            "phase1_count": gpu.phase1_count,
            "phase2_count": gpu.phase2_count,
            "gpu_equals_cpu": gpu.visible_indices == cpu.visible_indices && classes_match(&cpu, &gpu)
        },
        "expected_visible": expected_visible,
        "matched_expected": gpu.visible_count == expected_visible
    }))
}

fn assert_report(report: &serde_json::Value) -> Result<(), EngineError> {
    let dense = &report["dense_case"]["after"];
    let ok = dense["visible_count"] == 513
        && dense["phase1_count"] == 1
        && dense["phase2_count"] == 512
        && dense["occluded_count"] == 512
        && dense["gpu_pyramid_matches_cpu"] == true
        && dense["gpu_visible_set_equals_cpu"] == true
        && dense["gpu_classes_equal_cpu"] == true
        && dense["result_consistent"] == true
        && report["device_lost_report"].is_null()
        && report["draw_path_selection"]["gpu_requested_on_unsupported_hw_is_hard_error"] == true
        && report["edges"].as_array().is_some_and(|edges| {
            edges.len() == 3
                && edges
                    .iter()
                    .all(|e| e["matched_expected"] == true && e["after"]["gpu_equals_cpu"] == true)
        });
    if ok {
        Ok(())
    } else {
        Err(EngineError::SerializedFingerprintMismatch {
            expected:
                "issue1740 dense cull (513 visible, 512 occluded) + 3 matched edges, gpu==cpu"
                    .to_string(),
            actual: report.to_string(),
        })
    }
}

/// 1024 instances on a 32x32 grid. Columns 0..16 sit behind a near wall (depth 0.80,
/// occluded); columns 16..32 are in front (depth 0.10, visible). The wall is instance
/// 0, previously visible, and primes the pyramid over the left half.
fn dense_scene() -> Result<(Vec<RenderInstance>, Vec<bool>), EngineError> {
    let mut instances = vec![instance(1, rect(0.0, 0.0, 0.5, 1.0)?, 0.30)?];
    let mut prev = vec![true];
    let mut id = 2u64;
    for row in 0..32u32 {
        for col in 0..32u32 {
            let cx = (col as f32 + 0.5) / 32.0;
            let cy = (row as f32 + 0.5) / 32.0;
            let half = 0.008;
            let r = rect(cx - half, cy - half, cx + half, cy + half)?;
            let depth = if col < 16 { 0.80 } else { 0.10 };
            instances.push(instance(id, r, depth)?);
            prev.push(false);
            id += 1;
        }
    }
    Ok((instances, prev))
}

fn instance(id: u64, screen: ScreenRect, depth: f32) -> Result<RenderInstance, EngineError> {
    Ok(RenderInstance {
        id,
        mesh_id: "mesh".to_string(),
        material_id: "mat".to_string(),
        world_bounds: Aabb::from_center_half_extents(Vec3::new(0.0, 0.0, 0.0), Vec3::splat(0.1))?,
        screen_rect: screen,
        nearest_depth: depth,
        index_count: 36,
    })
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Result<ScreenRect, EngineError> {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
}

fn wide_frustum() -> Result<Frustum, EngineError> {
    Frustum::orthographic(Aabb::new(
        Vec3::new(-50.0, -50.0, -50.0),
        Vec3::new(50.0, 50.0, 50.0),
    )?)
}

fn fsv_root() -> Result<PathBuf, EngineError> {
    match env::var("LITD_1740_FSV_ROOT") {
        Ok(value) if value.trim().is_empty() => Err(EngineError::BatchingInvalidConfig {
            field: "render.gpu_driven.fsv.root".to_string(),
            reason: "LITD_1740_FSV_ROOT must not be empty".to_string(),
        }),
        Ok(value) => Ok(PathBuf::from(value)),
        Err(_) => Ok(PathBuf::from("target")
            .join("fsv")
            .join("issue1740_two_phase_cull")),
    }
}

fn io_error(field: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::BatchingInvalidConfig {
        field: format!("render.gpu_driven.fsv.{field}"),
        reason: error.to_string(),
    }
}
