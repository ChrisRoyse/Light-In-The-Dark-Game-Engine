use std::{fs, path::Path};

use litd_engine::{
    math::Vec3,
    render::{
        lighting::{AmbientLight, ClusterGrid, RenderLight},
        post::PostProcessSettings,
        shader::{ShaderEntryPoints, ShaderSystemSettings},
        stylized::{
            StylizedGallerySelection, StylizedPresetKind, StylizedValue, WgslShaderEscape,
            build_stylized_shader_library,
        },
    },
};
use serde_json::json;

use super::*;

#[test]
fn issue923_render_mcp_tools_write_and_read_real_state() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue923_mcp_render_tools", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("render-state.json");
    let material_ledger = root.join("material-ledger.json");
    let lighting_ledger = root.join("lighting-ledger.json");
    let scene_ledger = root.join("scene-ledger.json");
    let shader_ledger = root.join("shader-gallery-ledger.json");
    let before = state(&state_path, &material_ledger);
    println!("issue923 before {before}");

    let material = call_mcp_render_tool(
        SET_PBR_MATERIAL_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": material_ledger,
            "ledger_id": "issue923-material-ledger",
            "request_id": "issue923-set-material",
            "requested_by": "litd-editor-issue923-test",
            "binding": sample_issue923_material()
        }),
    )
    .unwrap();
    assert_eq!(material.tool_name, SET_PBR_MATERIAL_TOOL);
    let material_state = read_mcp_render_state(&state_path).unwrap();
    assert_eq!(material_state.material_count, 1);
    assert_eq!(
        material_state.state.materials[0].material_id,
        "mat.issue923.rune_tempered_steel"
    );

    let lighting = call_mcp_render_tool(
        SET_LIGHTING_ANIMATION_POST_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": lighting_ledger,
            "ledger_id": "issue923-lighting-ledger",
            "request_id": "issue923-set-lighting-animation-post",
            "requested_by": "litd-editor-issue923-test",
            "lighting": sample_lighting(),
            "animation": sample_animation(),
            "post_process": PostProcessSettings::default(),
            "viewport_width": 1280,
            "viewport_height": 720
        }),
    )
    .unwrap();
    assert_eq!(lighting.result["light_count"], 3);
    assert_eq!(lighting.result["ambient_name"], "ambient_moonlit_preview");
    assert_eq!(lighting.result["animation_clip_count"], 2);

    let scene = call_mcp_render_tool(
        COMPOSE_SCENE_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": scene_ledger,
            "ledger_id": "issue923-scene-ledger",
            "request_id": "issue923-compose-scene",
            "requested_by": "litd-editor-issue923-test",
            "composition": sample_scene()
        }),
    )
    .unwrap();
    assert_eq!(
        scene.result["scene_id"],
        "scene.issue923.gothic_material_bay"
    );

    let shader_gallery = call_mcp_render_tool(
        SET_SHADER_GALLERY_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": shader_ledger,
            "ledger_id": "issue923-shader-gallery-ledger",
            "request_id": "issue923-set-shader-gallery",
            "requested_by": "litd-editor-issue923-test",
            "gallery": sample_shader_gallery()
        }),
    )
    .unwrap();
    assert_eq!(shader_gallery.tool_name, SET_SHADER_GALLERY_TOOL);
    assert_eq!(
        shader_gallery.result["shader_gallery_selected_preset"],
        "force_field"
    );
    assert_eq!(shader_gallery.result["shader_gallery_material_count"], 6);
    assert_eq!(
        shader_gallery.result["shader_gallery_custom_wgsl_id"],
        "issue923.custom_wgsl_escape"
    );
    let read = call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read.result["material_count"], 1);
    assert_eq!(read.result["light_count"], 3);
    assert_eq!(read.result["ambient_name"], "ambient_moonlit_preview");
    assert_eq!(read.result["scene_placement_count"], 2);
    assert_eq!(read.result["scene_parented_placement_count"], 1);
    assert_eq!(read.result["shader_gallery_selected_preset"], "force_field");
    assert_eq!(
        read.result["shader_gallery_custom_wgsl_id"],
        "issue923.custom_wgsl_escape"
    );

    let after = state(&state_path, &material_ledger);
    println!("issue923 after {after}");
    assert_eq!(after["state_exists"], true);
    assert_eq!(after["state_update_count"], 4);
    assert_eq!(after["ledger_exists"], true);
}

#[test]
fn issue923_render_mcp_tools_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue923_mcp_render_tools", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("render-state.json");
    let edge_ledger = root.join("edge-ledger.json");

    let before_invalid = state(&state_path, &edge_ledger);
    let mut invalid_material = sample_issue923_material();
    invalid_material.material.roughness_factor = 0.0;
    let invalid = call_mcp_render_tool(
        SET_PBR_MATERIAL_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": edge_ledger,
            "ledger_id": "issue923-invalid-material",
            "request_id": "issue923-invalid-material",
            "requested_by": "litd-editor-issue923-test",
            "binding": invalid_material
        }),
    )
    .unwrap_err();
    let after_invalid = state(&state_path, &edge_ledger);
    println!("issue923 invalid before={before_invalid} after={after_invalid} error={invalid}");
    assert_eq!(before_invalid, after_invalid);
    assert!(invalid.to_string().contains("roughness_factor"));

    let before_unknown = state(&state_path, &edge_ledger);
    let unknown = call_mcp_render_tool("litd.engine.render.not_registered", json!({})).unwrap_err();
    let after_unknown = state(&state_path, &edge_ledger);
    println!("issue923 unknown before={before_unknown} after={after_unknown} error={unknown}");
    assert_eq!(before_unknown, after_unknown);
    assert!(
        unknown
            .to_string()
            .contains("editor.mcp_render_tools.tool_name")
    );

    seed_valid_material(&state_path, &root);
    let bad_scene_ledger = root.join("bad-scene-ledger.json");
    let before_scene = state(&state_path, &bad_scene_ledger);
    let mut scene = sample_scene();
    scene.material_assignments[0].material_id = "mat.issue923.missing".to_string();
    let bad_scene = call_mcp_render_tool(
        COMPOSE_SCENE_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": bad_scene_ledger,
            "ledger_id": "issue923-bad-scene",
            "request_id": "issue923-bad-scene",
            "requested_by": "litd-editor-issue923-test",
            "composition": scene
        }),
    )
    .unwrap_err();
    let after_scene = state(&state_path, &bad_scene_ledger);
    println!("issue923 scene before={before_scene} after={after_scene} error={bad_scene}");
    assert_eq!(before_scene, after_scene);
    assert!(
        bad_scene
            .to_string()
            .contains("scene.assignment.material_id")
    );

    let bad_parent_ledger = root.join("bad-scene-parent-ledger.json");
    let before_parent = state(&state_path, &bad_parent_ledger);
    let mut bad_parent_scene = sample_scene();
    bad_parent_scene.placements[1].parent_placement_id =
        Some("placement.issue923.rune_plinth".to_string());
    let bad_parent = call_mcp_render_tool(
        COMPOSE_SCENE_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": bad_parent_ledger,
            "ledger_id": "issue923-bad-scene-parent",
            "request_id": "issue923-bad-scene-parent",
            "requested_by": "litd-editor-issue923-test",
            "composition": bad_parent_scene
        }),
    )
    .unwrap_err();
    let after_parent = state(&state_path, &bad_parent_ledger);
    println!(
        "issue923 scene parent before={before_parent} after={after_parent} error={bad_parent}"
    );
    assert_eq!(before_parent, after_parent);
    assert!(bad_parent.to_string().contains("scene.parent_placement_id"));

    let bad_shader_ledger = root.join("bad-shader-ledger.json");
    let before_shader = state(&state_path, &bad_shader_ledger);
    let mut bad_gallery = sample_shader_gallery();
    bad_gallery.materials[0]
        .values
        .insert("wave_strength".to_string(), StylizedValue::Scalar(9.0));
    let bad_shader = call_mcp_render_tool(
        SET_SHADER_GALLERY_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": bad_shader_ledger,
            "ledger_id": "issue923-bad-shader-gallery",
            "request_id": "issue923-bad-shader-gallery",
            "requested_by": "litd-editor-issue923-test",
            "gallery": bad_gallery
        }),
    )
    .unwrap_err();
    let after_shader = state(&state_path, &bad_shader_ledger);
    println!("issue923 shader before={before_shader} after={after_shader} error={bad_shader}");
    assert_eq!(before_shader, after_shader);
    assert!(bad_shader.to_string().contains("wave_strength"));
}

fn seed_valid_material(state_path: &Path, root: &Path) {
    call_mcp_render_tool(
        SET_PBR_MATERIAL_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue923-render-state",
            "ledger_path": root.join("seed-ledger.json"),
            "ledger_id": "issue923-seed-ledger",
            "request_id": "issue923-seed-material",
            "requested_by": "litd-editor-issue923-test",
            "binding": sample_issue923_material()
        }),
    )
    .unwrap();
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
        layers: vec![
            RenderSceneLayer {
                layer_id: "layer.preview.props".to_string(),
                kind: "doodads".to_string(),
                visible: true,
            },
            RenderSceneLayer {
                layer_id: "layer.preview.lighting".to_string(),
                kind: "lighting".to_string(),
                visible: true,
            },
        ],
        placements: vec![
            RenderScenePlacement {
                placement_id: "placement.issue923.hero_helm".to_string(),
                asset_id: "asset.issue923.hero_helm".to_string(),
                layer_id: "layer.preview.props".to_string(),
                parent_placement_id: Some("placement.issue923.rune_plinth".to_string()),
                translation: Vec3::new(0.0, 1.2, 0.0),
                rotation_y_rad: 0.75,
                scale: Vec3::splat(1.0),
            },
            RenderScenePlacement {
                placement_id: "placement.issue923.rune_plinth".to_string(),
                asset_id: "asset.issue923.rune_plinth".to_string(),
                layer_id: "layer.preview.props".to_string(),
                parent_placement_id: None,
                translation: Vec3::new(0.0, 0.0, 0.0),
                rotation_y_rad: 0.0,
                scale: Vec3::new(1.4, 0.35, 1.4),
            },
        ],
        material_assignments: vec![RenderSceneMaterialAssignment {
            placement_id: "placement.issue923.hero_helm".to_string(),
            material_id: "mat.issue923.rune_tempered_steel".to_string(),
        }],
    }
}

fn sample_shader_gallery() -> StylizedGallerySelection {
    let library = build_stylized_shader_library(ShaderSystemSettings::default()).unwrap();
    let mut gallery = StylizedGallerySelection::curated_defaults(&library).unwrap();
    gallery.selected_preset = StylizedPresetKind::ForceField;
    gallery.custom_wgsl = Some(WgslShaderEscape {
        shader_id: "issue923.custom_wgsl_escape".to_string(),
        source: custom_wgsl_escape(),
        entry_points: ShaderEntryPoints::graphics("issue923_vs", "issue923_fs"),
        bindings: Vec::new(),
        variant_axes: Vec::new(),
    });
    gallery
}

fn custom_wgsl_escape() -> String {
    r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn issue923_vs(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VertexOut;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    return out;
}

@fragment
fn issue923_fs() -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.85, 1.0, 1.0);
}
"#
    .to_string()
}

fn state(state_path: &Path, ledger_path: &Path) -> serde_json::Value {
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
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
