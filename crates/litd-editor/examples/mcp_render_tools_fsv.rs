use std::fs;

use litd_editor::mcp_render_tools::{
    COMPOSE_SCENE_TOOL, READ_RENDER_TOOL, RenderAnimationBinding, RenderAnimationBlendMode,
    RenderAnimationClipBlend, RenderLightingPlan, RenderSceneComposition, RenderSceneLayer,
    RenderSceneMaterialAssignment, RenderScenePlacement, SET_LIGHTING_ANIMATION_POST_TOOL,
    SET_PBR_MATERIAL_TOOL, call_mcp_render_tool, read_mcp_render_state, sample_issue923_material,
};
use litd_engine::{
    math::Vec3,
    render::{
        lighting::{AmbientLight, ClusterGrid, RenderLight},
        post::PostProcessSettings,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue923Fsv {
    issue: u32,
    source_of_truth: Value,
    states: Value,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_MCP_RENDER_TOOLS_FSV_ROOT",
        "issue923_mcp_render_tools",
        "manual",
    )?;

    let state_path = root.join("render-state.json");
    let material_ledger = root.join("material-ledger.json");
    let lighting_ledger = root.join("lighting-ledger.json");
    let scene_ledger = root.join("scene-ledger.json");
    let report_path = root.join("mcp-render-tools-fsv-readback.json");
    let before = state(&state_path, &material_ledger);

    let material = call_mcp_render_tool(
        SET_PBR_MATERIAL_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": material_ledger,
            "ledger_id": "issue923-material-ledger",
            "request_id": "issue923-set-material",
            "requested_by": "litd-editor-issue923-fsv",
            "binding": sample_issue923_material()
        }),
    )?;
    let lighting = call_mcp_render_tool(
        SET_LIGHTING_ANIMATION_POST_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": lighting_ledger,
            "ledger_id": "issue923-lighting-ledger",
            "request_id": "issue923-set-lighting-animation-post",
            "requested_by": "litd-editor-issue923-fsv",
            "lighting": sample_lighting(),
            "animation": sample_animation(),
            "post_process": PostProcessSettings::default(),
            "viewport_width": 1280,
            "viewport_height": 720
        }),
    )?;
    let scene = call_mcp_render_tool(
        COMPOSE_SCENE_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": scene_ledger,
            "ledger_id": "issue923-scene-ledger",
            "request_id": "issue923-compose-scene",
            "requested_by": "litd-editor-issue923-fsv",
            "composition": sample_scene()
        }),
    )?;
    let read = call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": state_path }))?;
    let after = state(&state_path, &material_ledger);
    let edges = vec![
        edge_invalid_material(&root, &state_path)?,
        edge_unknown_tool(&state_path, &material_ledger),
        edge_missing_material_assignment(&root, &state_path)?,
    ];

    let fsv = Issue923Fsv {
        issue: 923,
        source_of_truth: json!({
            "state_path": state_path,
            "material_ledger": material_ledger,
            "lighting_ledger": lighting_ledger,
            "scene_ledger": scene_ledger
        }),
        states: json!({ "before": before, "after": after }),
        happy_path: json!({
            "set_pbr_material": material,
            "set_lighting_animation_post": lighting,
            "compose_scene": scene,
            "read_render": read
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    let readback = read_json::<Issue923Fsv>(&report_path)?;
    if readback != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue923 mcp_render_tools_fsv report={} report_sha256={} state_sha256={} material_ledger_sha256={} lighting_ledger_sha256={} scene_ledger_sha256={}",
        report_path.display(),
        report_readback.sha256,
        sha256_file(&state_path)?,
        sha256_file(&material_ledger)?,
        sha256_file(&lighting_ledger)?,
        sha256_file(&scene_ledger)?,
    );
    Ok(())
}

fn edge_invalid_material(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-invalid-material-ledger.json");
    let before = state(state_path, &ledger);
    let mut material = sample_issue923_material();
    material.material.roughness_factor = 0.0;
    let error = call_mcp_render_tool(
        SET_PBR_MATERIAL_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": ledger,
            "ledger_id": "issue923-invalid-material",
            "request_id": "issue923-invalid-material",
            "requested_by": "litd-editor-issue923-fsv",
            "binding": material
        }),
    )
    .unwrap_err();
    let after = state(state_path, &ledger);
    Ok(json!({
        "name": "invalid_material_roughness",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn edge_unknown_tool(state_path: &std::path::Path, ledger: &std::path::Path) -> Value {
    let before = state(state_path, ledger);
    let error = call_mcp_render_tool("litd.engine.render.not_registered", json!({})).unwrap_err();
    let after = state(state_path, ledger);
    json!({
        "name": "unknown_tool",
        "before": before,
        "after": after,
        "error": error.to_string()
    })
}

fn edge_missing_material_assignment(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-bad-scene-ledger.json");
    let before = state(state_path, &ledger);
    let mut scene = sample_scene();
    scene.material_assignments[0].material_id = "mat.issue923.missing".to_string();
    let error = call_mcp_render_tool(
        COMPOSE_SCENE_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": ledger,
            "ledger_id": "issue923-bad-scene",
            "request_id": "issue923-bad-scene",
            "requested_by": "litd-editor-issue923-fsv",
            "composition": scene
        }),
    )
    .unwrap_err();
    let after = state(state_path, &ledger);
    Ok(json!({
        "name": "missing_material_assignment",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn sample_lighting() -> RenderLightingPlan {
    RenderLightingPlan {
        cluster_grid: ClusterGrid::new(
            2,
            2,
            1,
            Vec3::new(-8.0, -4.0, 0.0),
            Vec3::new(8.0, 8.0, 12.0),
            8,
        ),
        ambient: AmbientLight::new(
            "ambient_moonlit_preview",
            Vec3::new(0.050, 0.076, 0.120),
            0.22,
        ),
        lights: vec![
            RenderLight::directional("moon key", Vec3::new(-0.3, -0.8, -0.4), Vec3::ONE, 2.2),
            RenderLight::point(
                "blue rune bounce",
                Vec3::new(-2.0, 1.5, 4.0),
                5.0,
                Vec3::new(0.1, 0.65, 1.0),
                8.0,
            ),
            RenderLight::point(
                "ember forge fill",
                Vec3::new(3.5, 1.0, 6.0),
                4.0,
                Vec3::new(1.0, 0.42, 0.12),
                4.5,
            ),
        ],
    }
}

fn sample_animation() -> RenderAnimationBinding {
    RenderAnimationBinding {
        target_entity_id: "entity.issue923.helm_preview".to_string(),
        rig_id: "rig.issue923.preview_turntable".to_string(),
        blend_mode: RenderAnimationBlendMode::LocomotionBlendSpace,
        clips: vec![
            RenderAnimationClipBlend {
                clip_id: "clip.issue923.idle_breathe".to_string(),
                source_relative_path: "sources/issue923/animations/idle_breathe.gltf".to_string(),
                weight: 0.7,
                normalized_time: 0.25,
            },
            RenderAnimationClipBlend {
                clip_id: "clip.issue923.rune_pulse".to_string(),
                source_relative_path: "sources/issue923/animations/rune_pulse.gltf".to_string(),
                weight: 0.3,
                normalized_time: 0.5,
            },
        ],
    }
}

fn sample_scene() -> RenderSceneComposition {
    RenderSceneComposition {
        scene_id: "scene.issue923.gothic_material_bay".to_string(),
        source_scene_id: "litd.asset.composition.stylized_nature_ed11_v1".to_string(),
        camera_mode: "material_orbit_preview".to_string(),
        layers: vec![RenderSceneLayer {
            layer_id: "layer.preview.props".to_string(),
            kind: "doodads".to_string(),
            visible: true,
        }],
        placements: vec![RenderScenePlacement {
            placement_id: "placement.issue923.hero_helm".to_string(),
            asset_id: "asset.issue923.hero_helm".to_string(),
            layer_id: "layer.preview.props".to_string(),
            parent_placement_id: None,
            translation: Vec3::new(0.0, 1.2, 0.0),
            rotation_y_rad: 0.75,
            scale: Vec3::splat(1.0),
        }],
        material_assignments: vec![RenderSceneMaterialAssignment {
            placement_id: "placement.issue923.hero_helm".to_string(),
            material_id: "mat.issue923.rune_tempered_steel".to_string(),
        }],
    }
}

fn state(state_path: &std::path::Path, ledger_path: &std::path::Path) -> Value {
    let state_readback = if state_path.exists() {
        Some(read_mcp_render_state(state_path).unwrap())
    } else {
        None
    };
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "state_update_count": state_readback.as_ref().map(|readback| readback.state.update_count).unwrap_or(0),
        "material_count": state_readback.as_ref().map(|readback| readback.material_count).unwrap_or(0),
        "light_count": state_readback.as_ref().map(|readback| readback.light_count).unwrap_or(0),
        "scene_placement_count": state_readback.as_ref().map(|readback| readback.scene_placement_count).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &std::path::Path,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    Ok(serde_json::from_slice(&serde_json::to_vec_pretty(value)?)?)
}

fn sha256_file(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
