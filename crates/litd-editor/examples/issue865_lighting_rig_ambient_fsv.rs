use std::{fs, path::Path};

use litd_editor::mcp_render_tools::{
    READ_RENDER_TOOL, RenderAnimationBinding, RenderAnimationBlendMode, RenderAnimationClipBlend,
    RenderLightingPlan, SET_LIGHTING_ANIMATION_POST_TOOL, call_mcp_render_tool,
    read_mcp_render_state, read_mcp_render_tool_ledger,
};
use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    math::Vec3,
    render::{
        lighting::{
            AmbientLight, ClusterGrid, DayNightLightingInput, ForwardPlusGpuSettings, LightingRig,
            RenderLight, SpotCone, WeatherLightingState, build_day_night_lighting_plan,
            cluster_lights, render_forward_plus_lighting_rig_gpu_with_context,
        },
        post::PostProcessSettings,
        shadow::{ShadowCamera, ShadowSettings, build_shadow_plan},
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
    let run = begin_issue_run("issue865_lighting_rig_ambient")?;
    let report_path = run.report_path();
    let day_bmp = run.artifact_path("day-rig-forward-plus.bmp")?;
    let night_bmp = run.artifact_path("night-rig-forward-plus.bmp")?;
    let ambient_bmp = run.artifact_path("ambient-only-forward-plus.bmp")?;
    let editor_state = run.artifact_path("editor-render-state.json")?;
    let editor_ledger = run.artifact_path("editor-render-ledger.json")?;

    let context = GpuContext::new_headless().await?;
    let grid = cluster_grid();
    let day_plan =
        build_day_night_lighting_plan(DayNightLightingInput::default(), &local_lights())?;
    let day_rig = day_plan.lighting_rig()?;
    let day_frame =
        render_forward_plus_lighting_rig_gpu_with_context(&context, settings(), grid, &day_rig)
            .await?;
    day_frame.image.write_bmp(&day_bmp)?;
    let day_shadow = build_shadow_plan(shadow_settings(), shadow_camera(), &day_plan.lights)?;

    let night_input = DayNightLightingInput {
        time_of_day_hours: 22.0,
        weather: WeatherLightingState {
            cloud_cover: 0.78,
            precipitation: 0.40,
            fog_density: 0.55,
        },
        ..DayNightLightingInput::default()
    };
    let night_plan = build_day_night_lighting_plan(night_input, &local_lights())?;
    let night_rig = night_plan.lighting_rig()?;
    let night_frame =
        render_forward_plus_lighting_rig_gpu_with_context(&context, settings(), grid, &night_rig)
            .await?;
    night_frame.image.write_bmp(&night_bmp)?;

    let ambient_only = LightingRig::new(
        AmbientLight::new("ambient_probe_fill", Vec3::new(0.070, 0.090, 0.140), 0.80),
        Vec::new(),
    )?;
    let ambient_frame = render_forward_plus_lighting_rig_gpu_with_context(
        &context,
        settings(),
        grid,
        &ambient_only,
    )
    .await?;
    ambient_frame.image.write_bmp(&ambient_bmp)?;

    let editor_write = call_mcp_render_tool(
        SET_LIGHTING_ANIMATION_POST_TOOL,
        json!({
            "state_path": editor_state,
            "state_id": "issue865-render-state",
            "ledger_path": editor_ledger,
            "ledger_id": "issue865-render-ledger",
            "request_id": "issue865-set-lighting-rig",
            "requested_by": "litd-editor-issue865-fsv",
            "lighting": RenderLightingPlan {
                cluster_grid: grid,
                ambient: day_plan.ambient_light.clone(),
                lights: day_plan.lights.clone(),
            },
            "animation": sample_animation(),
            "post_process": PostProcessSettings::default(),
            "viewport_width": 1280,
            "viewport_height": 720
        }),
    )?;
    let editor_read =
        call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": editor_state }))?;
    let editor_state_readback = read_mcp_render_state(&editor_state)?;
    let editor_ledger_readback = read_mcp_render_tool_ledger(&editor_ledger)?;

    let report = json!({
        "schema_version": 1,
        "issue": 865,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "day_bmp": day_bmp.display().to_string(),
            "night_bmp": night_bmp.display().to_string(),
            "ambient_bmp": ambient_bmp.display().to_string(),
            "editor_state": editor_state.display().to_string(),
            "manual_fsv_readback": "open the persisted JSON through Synapse, read SHA256 from disk, inspect the BMPs, and compare ambient/day-night/editor readbacks to expected"
        },
        "sufficiency_argument": "The known scene uses one day/night-owned directional key, one point torch, one spot rune light, one named ambient fill, and one shadow plan. Day and stormy-night prove ambient is tied to time/weather. The ambient-only pass proves the shader uses the ambient uniform even without punctual light refs. The editor MCP write/read proves the same ambient rig is persisted in authoring state. Four edge probes cover invalid ambient name, invalid ambient intensity, invalid time-of-day, and forbidden local directional lights.",
        "visual_references_reviewed": [
            "docs/readme/ED11-world-view.png",
            "docs/readme/GP02-exploration.png",
            "docs/readme/H03-ember-wastes.png"
        ],
        "gpu_context": context_state(&context),
        "day": {
            "lighting": lighting_state(&day_plan),
            "gpu": gpu_state(&day_frame, &day_bmp),
            "cluster": assignment_state(&cluster_lights(grid, &day_plan.lights)?),
            "shadow": {
                "entries": day_shadow.entries.len(),
                "directional_cascades": day_shadow.directional_cascade_count(),
                "point_faces": day_shadow.point_face_count(),
                "spot_maps": day_shadow.spot_map_count()
            }
        },
        "stormy_night": {
            "lighting": lighting_state(&night_plan),
            "gpu": gpu_state(&night_frame, &night_bmp),
            "cluster": assignment_state(&cluster_lights(grid, &night_plan.lights)?)
        },
        "ambient_only": {
            "gpu": gpu_state(&ambient_frame, &ambient_bmp),
            "expected_light_refs": "0:[]|1:[]"
        },
        "editor_readback": {
            "write_result": editor_write.result,
            "read_result": editor_read.result,
            "state_path": editor_state_readback.path,
            "state_sha256": editor_state_readback.sha256,
            "ambient_name": editor_state_readback.state.lighting.as_ref().map(|lighting| lighting.ambient.name.clone()),
            "ambient_intensity": editor_state_readback.state.lighting.as_ref().map(|lighting| lighting.ambient.intensity),
            "light_count": editor_state_readback.light_count,
            "ledger_path": editor_ledger_readback.path,
            "ledger_sha256": editor_ledger_readback.sha256,
            "ledger_record_count": editor_ledger_readback.ledger.record_count
        },
        "edges": [
            edge_bad_ambient_name(),
            edge_bad_ambient_intensity(),
            edge_invalid_time(),
            edge_local_directional()
        ],
        "expected_actuals": {
            "day_ambient": "ambient_day",
            "night_ambient": "ambient_night",
            "editor_ambient": "ambient_day",
            "day_refs": "0:[1]|1:[2]",
            "ambient_only_refs": "0:[]|1:[]",
            "shadow_entries": 9,
            "edge_count": 4
        }
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "issue865_lighting_rig_ambient_fsv report={} bytes={} sha256={} day_ambient={} night_ambient={} editor_ambient={} day_refs={} ambient_luma={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["day"]["gpu"]["stats"]["ambient_name"],
        readback.json["stormy_night"]["gpu"]["stats"]["ambient_name"],
        readback.json["editor_readback"]["ambient_name"],
        readback.json["day"]["cluster"]["refs_summary"],
        readback.json["ambient_only"]["gpu"]["stats"]["metrics"]["average_luma"],
    );
    Ok(())
}

fn local_lights() -> Vec<RenderLight> {
    vec![
        RenderLight::point(
            "left_torch",
            Vec3::new(-1.0, 0.0, 1.0),
            0.25,
            Vec3::new(1.0, 0.38, 0.08),
            6.0,
        ),
        RenderLight::spot(
            "right_rune_spot",
            Vec3::new(1.0, 0.0, 1.0),
            -Vec3::Z,
            0.25,
            SpotCone::new(0.2, 0.5),
            Vec3::new(0.25, 0.75, 1.0),
            5.0,
        ),
    ]
}

fn cluster_grid() -> ClusterGrid {
    ClusterGrid::new(
        2,
        1,
        1,
        Vec3::new(-2.0, -1.0, 0.0),
        Vec3::new(2.0, 1.0, 2.0),
        4,
    )
}

fn settings() -> ForwardPlusGpuSettings {
    ForwardPlusGpuSettings {
        width: 640,
        height: 360,
        shading_z_slice: 0,
    }
}

fn shadow_settings() -> ShadowSettings {
    ShadowSettings {
        atlas_size: 512,
        tile_size: 128,
        cascade_count: 2,
        max_shadowed_point_lights: 1,
        max_shadowed_spot_lights: 1,
        ..ShadowSettings::default()
    }
}

fn shadow_camera() -> ShadowCamera {
    ShadowCamera {
        position: Vec3::new(0.0, 8.0, -12.0),
        forward: Vec3::new(0.0, -0.35, 1.0),
        up: Vec3::Y,
        fov_y_rad: 60.0_f32.to_radians(),
        aspect: 16.0 / 9.0,
        near: 0.5,
        far: 80.0,
    }
}

fn sample_animation() -> RenderAnimationBinding {
    RenderAnimationBinding {
        target_entity_id: "entity.issue865.preview".to_string(),
        rig_id: "rig.issue865.preview".to_string(),
        blend_mode: RenderAnimationBlendMode::Replace,
        clips: vec![RenderAnimationClipBlend {
            clip_id: "clip.issue865.idle".to_string(),
            source_relative_path: "sources/issue865/idle.gltf".to_string(),
            weight: 1.0,
            normalized_time: 0.25,
        }],
    }
}

fn lighting_state(plan: &litd_engine::render::lighting::DayNightLightingPlan) -> Value {
    json!({
        "time_of_day_hours": plan.input.time_of_day_hours,
        "phase": plan.phase.label(),
        "primary_light_name": plan.primary_light_name,
        "primary_intensity": plan.primary_intensity,
        "ambient_name": plan.ambient_light.name,
        "ambient_color": plan.ambient_light.color,
        "ambient_intensity": plan.ambient_light.intensity,
        "fog_max": plan.fog_max,
        "light_names": plan.lights.iter().map(|light| light.name.clone()).collect::<Vec<_>>()
    })
}

fn gpu_state(frame: &litd_engine::render::lighting::ForwardPlusGpuFrame, bmp_path: &Path) -> Value {
    json!({
        "stats": frame.stats,
        "bmp_readback": image_state(bmp_path)
    })
}

fn assignment_state(assignment: &litd_engine::render::lighting::ClusteredLightAssignment) -> Value {
    json!({
        "directional_indices": assignment.directional_light_indices,
        "refs_summary": assignment.refs_summary(),
        "total_refs": assignment.total_light_refs()
    })
}

fn context_state(context: &GpuContext) -> Value {
    let report = context.report();
    json!({
        "context_id": report.context_id,
        "adapter_name": report.adapter.name,
        "backend": report.adapter.backend,
        "driver": report.adapter.driver,
        "device_lost_report": context.device_lost_report()
    })
}

fn edge_bad_ambient_name() -> Value {
    edge_result(
        "invalid_ambient_name",
        json!({"ambient_name": ""}),
        LightingRig::new(AmbientLight::new("", Vec3::ONE, 1.0), Vec::new()),
        "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG",
    )
}

fn edge_bad_ambient_intensity() -> Value {
    edge_result(
        "invalid_ambient_intensity",
        json!({"ambient_intensity": -0.1}),
        LightingRig::new(
            AmbientLight::new("bad_ambient", Vec3::ONE, -0.1),
            Vec::new(),
        ),
        "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG",
    )
}

fn edge_invalid_time() -> Value {
    let input = DayNightLightingInput {
        time_of_day_hours: 24.0,
        ..DayNightLightingInput::default()
    };
    edge_result(
        "invalid_time_of_day",
        json!({"time_of_day_hours": 24.0}),
        build_day_night_lighting_plan(input, &[]),
        "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG",
    )
}

fn edge_local_directional() -> Value {
    let locals = vec![RenderLight::directional(
        "rogue_directional",
        Vec3::Y,
        Vec3::ONE,
        1.0,
    )];
    edge_result(
        "local_directional_rejected",
        json!({"local_light": "rogue_directional"}),
        build_day_night_lighting_plan(DayNightLightingInput::default(), &locals),
        "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG",
    )
}

fn edge_result<T>(
    name: &'static str,
    before: Value,
    result: Result<T, EngineError>,
    expected_code: &'static str,
) -> Value {
    match result {
        Ok(_) => json!({
            "name": name,
            "before": before,
            "after": {"status": "accepted"},
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
                    "error": {"code": report.code, "message": report.message}
                },
                "expected_code": expected_code,
                "matched_expected_code": report.code == expected_code
            })
        }
    }
}

fn image_state(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap();
    json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
        "signature": String::from_utf8_lossy(&bytes[0..2]).to_string()
    })
}

fn assert_report(report: &Value) -> Result<(), EngineError> {
    let ambient_luma = report["ambient_only"]["gpu"]["stats"]["metrics"]["average_luma"]
        .as_f64()
        .unwrap_or_default();
    let ok = report["issue"] == 865
        && report["day"]["gpu"]["stats"]["ambient_name"] == "ambient_day"
        && report["stormy_night"]["gpu"]["stats"]["ambient_name"] == "ambient_night"
        && report["editor_readback"]["ambient_name"] == "ambient_day"
        && report["day"]["cluster"]["refs_summary"] == "0:[1]|1:[2]"
        && report["ambient_only"]["gpu"]["stats"]["gpu_refs_summary"] == "0:[]|1:[]"
        && ambient_luma > 0.05
        && report["day"]["shadow"]["entries"] == 9
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
            expected: "issue865 ambient lighting rig report with GPU/editor readbacks and four matched edges".to_string(),
            actual: report.to_string(),
        })
    }
}
