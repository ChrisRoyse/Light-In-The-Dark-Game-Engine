use std::{env, fs, path::PathBuf, time::Instant};

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    math::{Aabb, Vec2, Vec3},
    render::batching::{
        BatchPlan, BatchingSettings, Frustum, HzbOccluder, RenderInstance, ScreenRect,
        build_batch_plan,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

#[path = "batching_fsv/gpu.rs"]
mod gpu;

const FRAME_BUDGET_NS: u128 = 16_666_667;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    pollster::block_on(run())
}

async fn run() -> Result<(), EngineError> {
    let root = fsv_root()?;
    fs::create_dir_all(&root).map_err(|error| batching_fsv_error("root", error))?;
    let report_path = root.join("issue878-batching-fsv-readback.json");

    let (settings, frustum, occluders, instances) = fixture()?;
    let plan_started = Instant::now();
    let plan = build_batch_plan(settings, &frustum, &occluders, &instances)?;
    let planning_ns = plan_started.elapsed().as_nanos();
    let gpu_readback =
        gpu::gpu_culling_readback(settings, &frustum, &occluders, &instances).await?;
    let budget = budget_probe()?;
    let edges = vec![
        edge_draw_limit()?,
        edge_zero_draw_budget()?,
        edge_invalid_screen_rect(),
        edge_mismatched_batch_index_count()?,
    ];

    let report = json!({
        "issue": 878,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "source_module": "crates/litd-engine/src/render/batching.rs",
            "wgsl_module": "crates/litd-engine/src/render/batching_culling.wgsl",
            "manual_fsv_readback": "read this persisted JSON, its SHA256, and the target GPU adapter report through Synapse/computer control"
        },
        "sufficiency_argument": "The six-instance fixture covers visible, frustum-culled, HZB-occluded, tree material batch, unit material batch, and indirect command emission with known IDs. The 4096-instance budget probe proves deterministic grouping remains under the 16.67 ms frame budget and below max_draw_batches on the selected target hardware. Four edge probes cover draw-budget overflow, zero budget, malformed screen rect, and incompatible batch index_count.",
        "happy_path": {
            "before": {
                "input_instances": instances.len(),
                "hzb_enabled": settings.hzb_enabled,
                "max_draw_batches": settings.max_draw_batches,
                "frustum_planes": frustum.planes.len(),
                "occluders": occluders.len()
            },
            "after": plan_state(&plan, planning_ns)
        },
        "gpu_culling_shader": gpu_readback,
        "frame_budget_probe": budget,
        "edges": edges,
        "expected_actuals": {
            "visible_ids": [1, 2, 5, 6],
            "frustum_culled_ids": [3],
            "occlusion_culled_ids": [4],
            "draw_batches": 2,
            "indirect_commands": 2,
            "gpu_visibility_words": [1, 1, 0, 0, 1, 1],
            "budget_probe_draw_batches": 32,
            "edge_count": 4
        }
    });
    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| EngineError::SerializationFailed {
            format: "issue878-batching-fsv-json",
            reason: error.to_string(),
        })?;
    fs::write(&report_path, &bytes).map_err(|error| batching_fsv_error("write", error))?;
    let persisted = fs::read(&report_path).map_err(|error| batching_fsv_error("read", error))?;
    if persisted != bytes {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: format!("{} bytes", bytes.len()),
            actual: format!("{} bytes", persisted.len()),
        });
    }
    let persisted_json: serde_json::Value =
        serde_json::from_slice(&persisted).map_err(|error| EngineError::SerializationFailed {
            format: "issue878-batching-fsv-readback-json",
            reason: error.to_string(),
        })?;
    assert_report(&persisted_json)?;
    println!(
        "issue878 batching_fsv report={} bytes={} sha256={} adapter={} visible={} draw_batches={} budget_batches={} planning_ns={}",
        report_path.display(),
        persisted.len(),
        sha256_hex(&persisted),
        persisted_json["gpu_culling_shader"]["adapter"]["name"],
        persisted_json["happy_path"]["after"]["visible_ids"],
        persisted_json["happy_path"]["after"]["draw_batches"],
        persisted_json["frame_budget_probe"]["after"]["draw_batches"],
        persisted_json["happy_path"]["after"]["planning_ns"]
    );
    Ok(())
}

fn budget_probe() -> Result<serde_json::Value, EngineError> {
    let settings = BatchingSettings::default();
    let frustum = Frustum::orthographic(Aabb::new(
        Vec3::new(-10.0, -10.0, 0.0),
        Vec3::new(10.0, 10.0, 10.0),
    )?)?;
    let instances = budget_instances()?;
    let started = Instant::now();
    let plan = build_batch_plan(settings, &frustum, &[], &instances)?;
    let planning_ns = started.elapsed().as_nanos();
    Ok(json!({
        "before": {
            "input_instances": instances.len(),
            "expected_material_mesh_groups": 32,
            "max_draw_batches": settings.max_draw_batches,
            "frame_budget_ns": FRAME_BUDGET_NS
        },
        "after": {
            "visible_instances": plan.visible_instance_ids.len(),
            "draw_batches": plan.draw_batches.len(),
            "indirect_commands": plan.indirect_commands.len(),
            "first_instance_last_batch": plan.draw_batches.last().map(|batch| batch.first_instance),
            "planning_ns": planning_ns,
            "within_16ms_frame_budget": planning_ns <= FRAME_BUDGET_NS,
            "within_draw_batch_budget": plan.draw_batches.len() <= settings.max_draw_batches
        }
    }))
}

fn edge_draw_limit() -> Result<serde_json::Value, EngineError> {
    let (_, frustum, occluders, instances) = fixture()?;
    let settings = BatchingSettings {
        max_draw_batches: 1,
        ..BatchingSettings::default()
    };
    Ok(edge_result(
        "draw_limit_exceeded",
        json!({ "max_draw_batches": 1, "expected_draw_batches": 2 }),
        build_batch_plan(settings, &frustum, &occluders, &instances),
        "LITD-RENDER-BATCHING-DRAW-LIMIT",
    ))
}

fn edge_zero_draw_budget() -> Result<serde_json::Value, EngineError> {
    let (_, frustum, occluders, instances) = fixture()?;
    let settings = BatchingSettings {
        max_draw_batches: 0,
        ..BatchingSettings::default()
    };
    Ok(edge_result(
        "zero_draw_budget",
        json!({ "max_draw_batches": 0 }),
        build_batch_plan(settings, &frustum, &occluders, &instances),
        "LITD-RENDER-BATCHING-INVALID-CONFIG",
    ))
}

fn edge_invalid_screen_rect() -> serde_json::Value {
    edge_result(
        "invalid_screen_rect",
        json!({ "min": [0.8, 0.2], "max": [0.4, 0.9] }),
        ScreenRect::new(Vec2::new(0.8, 0.2), Vec2::new(0.4, 0.9)),
        "LITD-RENDER-BATCHING-INVALID-CONFIG",
    )
}

fn edge_mismatched_batch_index_count() -> Result<serde_json::Value, EngineError> {
    let (settings, frustum, occluders, mut instances) = fixture()?;
    instances[1].index_count = 97;
    Ok(edge_result(
        "mismatched_batch_index_count",
        json!({ "instance_id": 2, "mesh_id": "tree", "material_id": "leaf", "index_count": 97 }),
        build_batch_plan(settings, &frustum, &occluders, &instances),
        "LITD-RENDER-BATCHING-INVALID-CONFIG",
    ))
}

fn plan_state(plan: &BatchPlan, planning_ns: u128) -> serde_json::Value {
    json!({
        "visible_instances": plan.visible_instance_ids.len(),
        "visible_ids": plan.visible_instance_ids,
        "frustum_culled": plan.frustum_culled_ids.len(),
        "frustum_culled_ids": plan.frustum_culled_ids,
        "occlusion_culled": plan.occlusion_culled_ids.len(),
        "occlusion_culled_ids": plan.occlusion_culled_ids,
        "draw_batches": plan.draw_batches.len(),
        "indirect_commands": plan.indirect_commands.len(),
        "batch_keys": plan.draw_batches.iter().map(|batch| {
            format!("{}:{}:{}", batch.key.mesh_id, batch.key.material_id, batch.instance_ids.len())
        }).collect::<Vec<_>>(),
        "indirect_instance_counts": plan.indirect_commands.iter().map(|command| {
            command.instance_count
        }).collect::<Vec<_>>(),
        "planning_ns": planning_ns,
        "within_16ms_frame_budget": planning_ns <= FRAME_BUDGET_NS
    })
}

fn assert_report(report: &serde_json::Value) -> Result<(), EngineError> {
    let ok = report["happy_path"]["after"]["visible_ids"] == json!([1, 2, 5, 6])
        && report["happy_path"]["after"]["frustum_culled_ids"] == json!([3])
        && report["happy_path"]["after"]["occlusion_culled_ids"] == json!([4])
        && report["happy_path"]["after"]["draw_batches"] == 2
        && report["happy_path"]["after"]["indirect_commands"] == 2
        && report["happy_path"]["after"]["within_16ms_frame_budget"] == true
        && report["gpu_culling_shader"]["compute_pipeline_created"] == true
        && report["gpu_culling_shader"]["visibility_words"] == json!([1, 1, 0, 0, 1, 1])
        && report["gpu_culling_shader"]["visible_ids"] == json!([1, 2, 5, 6])
        && report["gpu_culling_shader"]["device_lost_report"].is_null()
        && report["frame_budget_probe"]["after"]["visible_instances"] == 4096
        && report["frame_budget_probe"]["after"]["draw_batches"] == 32
        && report["frame_budget_probe"]["after"]["within_16ms_frame_budget"] == true
        && report["frame_budget_probe"]["after"]["within_draw_batch_budget"] == true
        && report["edges"].as_array().is_some_and(|edges| {
            edges.len() == 4
                && edges
                    .iter()
                    .all(|edge| edge["matched_expected_code"] == true)
        });
    if ok {
        Ok(())
    } else {
        Err(EngineError::SerializedFingerprintMismatch {
            expected: "issue878 batching report with happy path, GPU readback, budget probe, and four matched edges".to_string(),
            actual: report.to_string(),
        })
    }
}

fn edge_result<T>(
    name: &'static str,
    before: serde_json::Value,
    result: Result<T, EngineError>,
    expected_code: &'static str,
) -> serde_json::Value {
    match result {
        Ok(_) => json!({
            "name": name,
            "before": before,
            "after": { "status": "accepted" },
            "expected_code": expected_code,
            "matched_expected_code": false
        }),
        Err(error) => {
            let report: ErrorReport = error.report();
            json!({
                "name": name,
                "before": before,
                "after": {
                    "status": "rejected",
                    "error": {
                        "code": report.code,
                        "message": report.message,
                        "remediation": report.remediation,
                        "source_error": report.source_error
                    }
                },
                "expected_code": expected_code,
                "matched_expected_code": report.code == expected_code
            })
        }
    }
}

fn fixture() -> Result<
    (
        BatchingSettings,
        Frustum,
        Vec<HzbOccluder>,
        Vec<RenderInstance>,
    ),
    EngineError,
> {
    let settings = BatchingSettings::default();
    let frustum = Frustum::orthographic(Aabb::new(
        Vec3::new(-5.0, -5.0, 0.0),
        Vec3::new(5.0, 5.0, 10.0),
    )?)?;
    let occluders = vec![HzbOccluder::new(rect(0.42, 0.42, 0.58, 0.58)?, 0.40)];
    let instances = vec![
        instance(
            1,
            "tree",
            "leaf",
            Vec3::new(-2.0, 0.0, 3.0),
            rect(0.10, 0.10, 0.20, 0.25)?,
            0.35,
            96,
        )?,
        instance(
            2,
            "tree",
            "leaf",
            Vec3::new(-1.0, 0.0, 4.0),
            rect(0.24, 0.10, 0.34, 0.24)?,
            0.37,
            96,
        )?,
        instance(
            3,
            "tree",
            "leaf",
            Vec3::new(12.0, 0.0, 4.0),
            rect(0.50, 0.10, 0.58, 0.20)?,
            0.30,
            96,
        )?,
        instance(
            4,
            "unit",
            "blue",
            Vec3::new(0.0, 0.0, 4.0),
            rect(0.46, 0.46, 0.54, 0.54)?,
            0.82,
            128,
        )?,
        instance(
            5,
            "unit",
            "blue",
            Vec3::new(1.0, 0.0, 4.0),
            rect(0.62, 0.44, 0.70, 0.55)?,
            0.22,
            128,
        )?,
        instance(
            6,
            "unit",
            "blue",
            Vec3::new(2.0, 0.0, 4.0),
            rect(0.72, 0.44, 0.78, 0.55)?,
            0.24,
            128,
        )?,
    ];
    Ok((settings, frustum, occluders, instances))
}

fn budget_instances() -> Result<Vec<RenderInstance>, EngineError> {
    (0..4096)
        .map(|index| {
            let mesh = format!("mesh{:02}", index % 16);
            let material = format!("mat{:02}", (index / 16) % 2);
            let x = -8.0 + (index % 64) as f32 * 0.25;
            let y = -8.0 + (index / 64) as f32 * 0.25;
            let sx = 0.01 + (index % 32) as f32 * 0.003;
            let sy = 0.01 + ((index / 32) % 32) as f32 * 0.003;
            instance(
                index as u64 + 1,
                &mesh,
                &material,
                Vec3::new(x, y, 4.0),
                rect(sx, sy, (sx + 0.002).min(0.99), (sy + 0.002).min(0.99))?,
                0.25,
                64 + (index % 16) as u32,
            )
        })
        .collect()
}

fn instance(
    id: u64,
    mesh_id: &str,
    material_id: &str,
    center: Vec3,
    screen_rect: ScreenRect,
    nearest_depth: f32,
    index_count: u32,
) -> Result<RenderInstance, EngineError> {
    Ok(RenderInstance {
        id,
        mesh_id: mesh_id.to_string(),
        material_id: material_id.to_string(),
        world_bounds: Aabb::from_center_half_extents(center, Vec3::splat(0.04))?,
        screen_rect,
        nearest_depth,
        index_count,
    })
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Result<ScreenRect, EngineError> {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
}

fn fsv_root() -> Result<PathBuf, EngineError> {
    match env::var("LITD_BATCHING_FSV_ROOT") {
        Ok(value) if value.trim().is_empty() => Err(EngineError::BatchingInvalidConfig {
            field: "render.batching.fsv.root".to_string(),
            reason: "LITD_BATCHING_FSV_ROOT must not be empty".to_string(),
        }),
        Ok(value) => Ok(PathBuf::from(value)),
        Err(_) => Ok(PathBuf::from("target")
            .join("fsv")
            .join("issue878_batching")
            .join("manual")),
    }
}

fn batching_fsv_error(field: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::BatchingInvalidConfig {
        field: format!("render.batching.fsv.{field}"),
        reason: error.to_string(),
    }
}
