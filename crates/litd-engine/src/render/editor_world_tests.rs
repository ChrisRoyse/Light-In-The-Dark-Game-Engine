use super::*;

#[test]
fn viewport_rejects_zero_size() {
    match EditorViewportRect::new(0, 0, 0, 720) {
        Err(EngineError::EditorWorldRendererInvalidConfig { field, reason }) => {
            assert_eq!(field, "viewport_rect");
            assert!(reason.contains("non-zero"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn viewport_rejects_surface_overflow() {
    match EditorViewportRect::from_float_bounds(12.0, 40.0, 820.0, 720.0, 800, 720) {
        Err(EngineError::EditorWorldRendererInvalidConfig { field, reason }) => {
            assert_eq!(field, "viewport_rect");
            assert!(reason.contains("exceeds surface"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn viewport_float_bounds_round_inside_chrome_edges() {
    let viewport =
        EditorViewportRect::from_float_bounds(410.001, 74.001, 1548.999, 820.999, 1920, 1080)
            .unwrap();

    assert_eq!(viewport.x, 411);
    assert_eq!(viewport.y, 75);
    assert_eq!(viewport.x + viewport.width, 1548);
    assert_eq!(viewport.y + viewport.height, 820);
}

#[test]
fn camera_allows_level_runtime_pitch_for_first_person_playtest() {
    let camera = EditorWorldCameraState {
        target_world: Vec3::ZERO,
        distance: 128.0,
        yaw_degrees: 45.0,
        pitch_degrees: 0.0,
    };
    camera.validate().unwrap();
}

#[test]
fn camera_rejects_extreme_pitch() {
    let camera = EditorWorldCameraState {
        target_world: Vec3::ZERO,
        distance: 128.0,
        yaw_degrees: 45.0,
        pitch_degrees: 92.0,
    };
    match camera.validate() {
        Err(EngineError::EditorWorldRendererInvalidConfig { field, reason }) => {
            assert_eq!(field, "camera");
            assert!(reason.contains("pitch -89..89"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn reference_scene_is_real_indexed_geometry() {
    let metrics = reference_editor_world_scene_metrics().unwrap();
    assert!(metrics.vertex_count > 200);
    assert!(metrics.index_count > 300);
    assert!(metrics.triangle_count > 100);
    assert_eq!(metrics.index_count, metrics.triangle_count * 3);
}

#[test]
fn blank_scene_allows_zero_authored_geometry() {
    let scene = EditorWorldSceneBuffers::new(EditorWorldSceneBuffersNewArgs {
        scene_name: "blank_world.authoring_test",
        source_id: "litd.test.blank_world",
        source_path: "generated://test/blank-world",
        dependency_count: 0,
        material_count: 0,
        texture_count: 0,
        image_count: 0,
        vertices: Vec::new(),
        indices: Vec::new(),
        mesh_primitives: 0,
        base_color_texture: test_texture("base"),
        normal_texture: test_texture("normal"),
        metallic_roughness_texture: test_texture("metallic-roughness"),
    })
    .unwrap();

    let metrics = scene.metrics().unwrap();

    assert_eq!(metrics.vertex_count, 0);
    assert_eq!(metrics.index_count, 0);
    assert_eq!(metrics.triangle_count, 0);
    assert_eq!(metrics.mesh_primitives, 0);
}

#[test]
fn partial_blank_scene_buffers_fail_loud() {
    let vertex = EditorWorldVertex::new(
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0],
        [1.0, 1.0, 1.0, 0.0],
        [0.0, 0.8, 1.0, 0.0],
    );
    let error = match EditorWorldSceneBuffers::new(EditorWorldSceneBuffersNewArgs {
        scene_name: "broken_blank_world",
        source_id: "litd.test.broken_blank_world",
        source_path: "generated://test/broken-blank-world",
        dependency_count: 0,
        material_count: 0,
        texture_count: 0,
        image_count: 0,
        vertices: vec![vertex],
        indices: Vec::new(),
        mesh_primitives: 1,
        base_color_texture: test_texture("base"),
        normal_texture: test_texture("normal"),
        metallic_roughness_texture: test_texture("metallic-roughness"),
    }) {
        Ok(_) => panic!("expected partial blank scene buffers to fail"),
        Err(error) => error,
    };

    match error {
        EngineError::EditorWorldRendererInvalidConfig { field, reason } => {
            assert_eq!(field, "reference_scene");
            assert!(reason.contains("empty scene requires zero vertices"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn oversized_editor_world_scene_budget_fails_loud_before_gpu_allocation() {
    let vertex_count = editor_world_scene::MAX_EDITOR_WORLD_VERTEX_BUFFER_BYTES
        / std::mem::size_of::<EditorWorldVertex>()
        + 1;
    let index_count = editor_world_scene::MAX_EDITOR_WORLD_SCENE_TRIANGLES * 3 + 3;

    let error = match editor_world_scene::validate_scene_budget(vertex_count, index_count) {
        Ok(_) => panic!("expected oversized editor world scene budget to fail"),
        Err(error) => error,
    };

    match error {
        EngineError::EditorWorldRendererInvalidConfig { field, reason } => {
            assert_eq!(field, "reference_scene.budget");
            assert!(reason.contains("scene exceeds editor renderer budget"));
            assert!(reason.contains("vertex_buffer_bytes"));
            assert!(reason.contains("triangles"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn editor_world_shader_keeps_hdr_emissive_and_tonemap_contract() {
    assert!(EDITOR_WORLD_WGSL.contains("litd_editor_world_emissive_fs"));
    assert!(EDITOR_WORLD_WGSL.contains("litd_editor_foliage_vs"));
    assert!(EDITOR_WORLD_WGSL.contains("litd_editor_world_foliage_fs"));
    assert!(EDITOR_WORLD_WGSL.contains("foliage_fade"));
    assert!(EDITOR_WORLD_WGSL.contains("smoothstep(instance.cull_wind.x"));
    assert!(EDITOR_WORLD_WGSL.contains("litd_editor_world_atmosphere_fog"));
    assert!(EDITOR_WORLD_WGSL.contains("litd_editor_replay_fog_color"));
    assert!(EDITOR_WORLD_WGSL.contains("litd_editor_replay_fog_emissive"));
    assert!(EDITOR_WORLD_WGSL.contains("textureLoad(replay_fog_texture"));
    assert!(EDITOR_WORLD_WGSL.contains("emissive_strength"));
    assert!(EDITOR_WORLD_WGSL.contains("bloom_hint"));
    assert!(EDITOR_WORLD_WGSL.contains("alpha_cutoff < -0.5"));
    assert!(EDITOR_WORLD_WGSL.contains("world.atmosphere"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_world_blur_horizontal_fs"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_world_blur_vertical_fs"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_world_composite_fs"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_world_post_tonemap_aces"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_world_color_grade"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_world_ed11_cinematic_veil"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("smoke_band"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_world_background_atmosphere"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_world_clear_background_mask"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("shadow_cool"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("warm_highlights"));
    assert!(EDITOR_WORLD_POST_WGSL.contains("litd_editor_blank_world_grid"));
}

#[test]
fn instanced_foliage_buffers_require_template_geometry() {
    let instance = EditorWorldFoliageInstance::new(
        [0.0, 0.0, 0.0],
        1.5,
        0.0,
        0.4,
        [0.12, 0.16, 0.13, 0.0],
        [70.0, 185.0],
        0.1,
    )
    .unwrap();
    let error = match EditorWorldFoliageBuffers::new(Vec::new(), Vec::new(), vec![instance]) {
        Ok(_) => panic!("expected missing template geometry to fail"),
        Err(error) => error,
    };

    match error {
        EngineError::EditorWorldRendererInvalidConfig { field, reason } => {
            assert_eq!(field, "reference_scene.foliage_buffers");
            assert!(reason.contains("template geometry"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn instanced_foliage_effective_index_count_tracks_instances() {
    let template = vec![test_vertex(); 4];
    let indices = vec![0, 1, 2, 1, 3, 2];
    let instances = vec![
        EditorWorldFoliageInstance::new(
            [0.0, 0.0, 0.0],
            1.5,
            0.0,
            0.4,
            [0.12, 0.16, 0.13, 0.0],
            [70.0, 185.0],
            0.1,
        )
        .unwrap(),
        EditorWorldFoliageInstance::new(
            [2.0, 0.0, 1.0],
            1.8,
            0.4,
            0.5,
            [0.18, 0.14, 0.10, 0.0],
            [78.0, 172.0],
            0.08,
        )
        .unwrap(),
    ];
    let buffers = EditorWorldFoliageBuffers::new(template, indices, instances).unwrap();

    assert_eq!(buffers.template_index_count(), 6);
    assert_eq!(buffers.instance_count(), 2);
    assert_eq!(buffers.effective_index_count(), 12);
    assert_eq!(buffers.effective_triangle_count(), 4);
}

#[test]
fn instanced_foliage_effective_budget_fails_before_gpu_allocation() {
    let over_budget_instances = editor_world_scene::MAX_EDITOR_WORLD_SCENE_TRIANGLES / 2 + 1;

    let error = match editor_world_foliage::validate_foliage_effective_budget(
        4,
        6,
        over_budget_instances,
    ) {
        Ok(_) => panic!("expected oversized instanced foliage budget to fail"),
        Err(error) => error,
    };

    match error {
        EngineError::EditorWorldRendererInvalidConfig { field, reason } => {
            assert_eq!(field, "reference_scene.budget");
            assert!(reason.contains("scene exceeds editor renderer budget"));
            assert!(reason.contains("triangles"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn scene_plus_instanced_foliage_effective_budget_fails_before_gpu_allocation() {
    let base_indices = vec![0; editor_world_scene::MAX_EDITOR_WORLD_SCENE_TRIANGLES * 3 - 2];
    let foliage = EditorWorldFoliageBuffers::new(
        vec![test_vertex(); 3],
        vec![0, 1, 2],
        vec![
            EditorWorldFoliageInstance::new(
                [0.0, 0.0, 0.0],
                1.5,
                0.0,
                0.4,
                [0.12, 0.16, 0.13, 0.0],
                [70.0, 185.0],
                0.1,
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let error = match EditorWorldSceneBuffers::new_with_layers_and_foliage(
        EditorWorldSceneBuffersFullArgs {
            scene_name: "over_budget_world",
            source_id: "litd.test.over_budget_world",
            source_path: "generated://test/over-budget-world",
            dependency_count: 0,
            material_count: 0,
            texture_count: 0,
            image_count: 0,
            vertices: vec![test_vertex()],
            indices: base_indices,
            layers: Vec::new(),
            foliage,
            mesh_primitives: 1,
            base_color_texture: test_texture("base"),
            normal_texture: test_texture("normal"),
            metallic_roughness_texture: test_texture("metallic-roughness"),
        },
    ) {
        Ok(_) => panic!("expected combined scene and foliage budget to fail"),
        Err(error) => error,
    };

    match error {
        EngineError::EditorWorldRendererInvalidConfig { field, reason } => {
            assert_eq!(field, "reference_scene.budget");
            assert!(reason.contains("scene exceeds editor renderer budget"));
            assert!(reason.contains("indices"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn editor_world_uniform_keeps_wgsl_vec4_alignment() {
    assert_eq!(std::mem::size_of::<EditorWorldUniform>(), 144);
    assert_eq!(
        editor_world_evidence::ATMOSPHERE_MODEL,
        "exponential_squared_distance_plus_height_fog"
    );
    assert!(editor_world_evidence::ATMOSPHERE_PROFILE.contains("fog_color=0.018,0.032,0.052"));
    assert!(editor_world_evidence::ATMOSPHERE_PROFILE.contains("height_reference=-12"));
    assert!(editor_world_evidence::ATMOSPHERE_PROFILE.contains("distance_start=96"));
    assert!(editor_world_evidence::ATMOSPHERE_PROFILE.contains("fog_max=0.58"));
}

#[test]
fn editor_world_evidence_reports_background_atmosphere_contract() {
    assert_eq!(
        editor_world_post::BACKGROUND_MODEL_LABEL,
        "ed11_clear_color_screen_space_atmosphere"
    );
    assert!(editor_world_post::BACKGROUND_PROFILE_LABEL.contains("clear=0.022,0.030,0.044"));
    assert!(editor_world_post::BACKGROUND_PROFILE_LABEL.contains("mist_band=0.43"));
    assert!(editor_world_post::EDITOR_WORLD_POST_WGSL.contains("ed11_cinematic_veil"));
}

#[test]
fn scene_layer_validation_rejects_non_triangle_aligned_ranges() {
    let error =
        match EditorWorldSceneBuffers::new_with_layers(EditorWorldSceneBuffersWithLayersArgs {
            scene_name: "layered_world",
            source_id: "litd.test.layered_world",
            source_path: "generated://test/layered-world",
            dependency_count: 0,
            material_count: 0,
            texture_count: 0,
            image_count: 0,
            vertices: vec![test_vertex(); 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            layers: vec![EditorWorldSceneLayer::new(
                EditorWorldSceneLayerKind::Foliage,
                1,
                3,
            )],
            mesh_primitives: 1,
            base_color_texture: test_texture("base"),
            normal_texture: test_texture("normal"),
            metallic_roughness_texture: test_texture("metallic-roughness"),
        }) {
            Ok(_) => panic!("expected misaligned foliage layer range to fail"),
            Err(error) => error,
        };

    match error {
        EngineError::EditorWorldRendererInvalidConfig { field, reason } => {
            assert_eq!(field, "reference_scene.layers");
            assert!(reason.contains("triangle indices"));
        }
        other => panic!("expected EditorWorldRendererInvalidConfig, got {other:?}"),
    }
}

#[test]
fn foliage_visibility_complements_scene_index_ranges() {
    let layers = vec![
        EditorWorldSceneLayer::new(EditorWorldSceneLayerKind::Terrain, 0, 3),
        EditorWorldSceneLayer::new(EditorWorldSceneLayerKind::Foliage, 3, 6),
    ];
    editor_world_layers::validate_scene_layers(&layers, 12).unwrap();

    let visible = editor_world_layers::visible_index_ranges(
        12,
        &layers,
        EditorWorldLayerVisibility::default(),
    );
    let hidden_layers = EditorWorldLayerVisibility {
        terrain: false,
        foliage: false,
        ..Default::default()
    };
    let hidden = editor_world_layers::visible_index_ranges(12, &layers, hidden_layers);

    assert_eq!(visible, vec![0..12]);
    assert_eq!(hidden, vec![9..12]);
    assert_eq!(
        editor_world_layers::hidden_index_count(&layers, hidden_layers),
        9
    );
}

fn test_vertex() -> EditorWorldVertex {
    EditorWorldVertex::new(
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0],
        [1.0, 1.0, 1.0, 0.0],
        [0.0, 0.8, 1.0, 0.0],
    )
}

fn test_texture(label: &'static str) -> EditorWorldTextureData {
    EditorWorldTextureData::new(
        format!("test-{label}"),
        format!("generated://test/{label}"),
        1,
        1,
        vec![255, 255, 255, 255],
    )
    .unwrap()
}
