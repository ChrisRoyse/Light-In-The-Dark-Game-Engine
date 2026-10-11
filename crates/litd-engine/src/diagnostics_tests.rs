use super::*;

#[test]
fn duplicate_module_report_is_structured_and_fix_oriented() {
    let report = EngineError::DuplicateModule("editor.runtime".to_string()).report();
    assert_eq!(report.code, "LITD-APP-DUPLICATE-MODULE");
    assert!(report.message.contains("editor.runtime"));
    assert!(report.remediation.contains("unique stable name"));
    assert!(
        report
            .as_key_values()
            .contains("error_code=LITD-APP-DUPLICATE-MODULE")
    );
}

#[test]
fn frame_before_start_report_says_how_to_fix_it() {
    let report = EngineError::AppRuntimeNotStarted.report();
    assert_eq!(report.code, "LITD-APP-FRAME-BEFORE-START");
    assert!(report.message.contains("before module startup"));
    assert!(report.remediation.contains("AppRuntime::start"));
}

#[test]
fn logging_invalid_config_report_is_structured() {
    let report = EngineError::LoggingInvalidConfig {
        field: "log_file".to_string(),
        reason: "not writable".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-FOUNDATION-LOGGING-INVALID-CONFIG");
    assert!(report.message.contains("log_file"));
    assert!(report.remediation.contains("writable local log path"));
}

#[test]
fn animation_invalid_config_report_is_structured() {
    let report = EngineError::AnimationInvalidConfig {
        field: "animation.gpu_palette.joint_count".to_string(),
        reason: "outside palette budget".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-ANIMATION-INVALID-CONFIG");
    assert!(report.message.contains("joint_count"));
    assert!(report.remediation.contains("glTF skin"));
}

#[test]
fn invalid_surface_size_report_preserves_dimensions() {
    let report = EngineError::InvalidSurfaceSize {
        width: 0,
        height: 720,
    }
    .report();
    assert_eq!(report.code, "LITD-RENDER-INVALID-SURFACE-SIZE");
    assert!(report.message.contains("0x720"));
    assert!(report.remediation.contains("zero-size"));
}

#[test]
fn fixed_point_divide_by_zero_report_is_structured() {
    let report = EngineError::FixedPointDivideByZero.report();
    assert_eq!(report.code, "LITD-MATH-FIXED-DIVIDE-BY-ZERO");
    assert!(report.remediation.contains("denominators"));
}

#[test]
fn sim_capacity_report_is_structured() {
    let report = EngineError::SimCapacityExceeded { capacity: 4 }.report();
    assert_eq!(report.code, "LITD-SIM-CAPACITY-EXCEEDED");
    assert!(report.message.contains("4"));
    assert!(report.remediation.contains("capacity"));
}

#[test]
fn sim_order_missing_body_report_is_structured() {
    let report = EngineError::SimBodyMissing { id: 42 }.report();
    assert_eq!(report.code, "LITD-SIM-BODY-MISSING");
    assert!(report.message.contains("42"));
    assert!(report.remediation.contains("orders"));
}

#[test]
fn nav_readback_report_is_structured() {
    let report = EngineError::NavReadbackMismatch {
        field: "fnv1a64".to_string(),
        expected: "1234".to_string(),
        actual: "5678".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-NAV-READBACK-MISMATCH");
    assert!(report.message.contains("fnv1a64"));
    assert!(report.remediation.contains("serialized source of truth"));
}

#[test]
fn nav_path_reports_are_structured() {
    let not_found = EngineError::NavPathNotFound {
        from: "0,0".to_string(),
        to: "9,9".to_string(),
        expansions: 17,
    }
    .report();
    assert_eq!(not_found.code, "LITD-NAV-PATH-NOT-FOUND");
    assert!(not_found.message.contains("17"));
    assert!(not_found.remediation.contains("blocked cells"));

    let budget = EngineError::NavPathBudgetExceeded {
        max_expansions: 4,
        expanded: 4,
    }
    .report();
    assert_eq!(budget.code, "LITD-NAV-BUDGET-EXCEEDED");
    assert!(budget.remediation.contains("hot tick"));
}

#[test]
fn ai_behavior_tree_reports_are_structured() {
    let invalid = EngineError::AiInvalidConfig {
        field: "tree.root".to_string(),
        reason: "missing".to_string(),
    }
    .report();
    assert_eq!(invalid.code, "LITD-AI-INVALID-CONFIG");
    assert!(invalid.message.contains("tree.root"));
    assert!(invalid.remediation.contains("lowered AI data"));

    let mismatch = EngineError::AiReadbackMismatch {
        field: "behavior_run.fnv1a64".to_string(),
        expected: "aaaa".to_string(),
        actual: "bbbb".to_string(),
    }
    .report();
    assert_eq!(mismatch.code, "LITD-AI-READBACK-MISMATCH");
    assert!(mismatch.remediation.contains("FNV-1a hashes"));
}

#[test]
fn world_stream_reports_are_structured() {
    let invalid = EngineError::StreamInvalidConfig {
        field: "anchor.position".to_string(),
        reason: "outside world bounds".to_string(),
    }
    .report();
    assert_eq!(invalid.code, "LITD-STREAM-INVALID-CONFIG");
    assert!(invalid.message.contains("anchor.position"));
    assert!(invalid.remediation.contains("Aster world record"));

    let persistence = EngineError::StreamPersistenceFailed {
        op: "aster.chunk.missing".to_string(),
        reason: "chunk row absent".to_string(),
    }
    .report();
    assert_eq!(persistence.code, "LITD-STREAM-PERSISTENCE-FAILED");
    assert!(persistence.remediation.contains("Aster vault"));

    let mismatch = EngineError::StreamReadbackMismatch {
        field: "aster.sha256".to_string(),
        expected: "aaaa".to_string(),
        actual: "bbbb".to_string(),
    }
    .report();
    assert_eq!(mismatch.code, "LITD-STREAM-READBACK-MISMATCH");
    assert!(mismatch.remediation.contains("manifest/chunk"));
}

#[test]
fn asset_stream_reports_are_structured() {
    let invalid = EngineError::AssetStreamInvalidConfig {
        field: "asset_lod.distance_bands".to_string(),
        reason: "missing LOD1".to_string(),
    }
    .report();
    assert_eq!(invalid.code, "LITD-ASSET-STREAM-INVALID-CONFIG");
    assert!(invalid.message.contains("asset_lod.distance_bands"));
    assert!(invalid.remediation.contains("texture-mip product"));

    let persistence = EngineError::AssetStreamPersistenceFailed {
        op: "asset_lod.selected.read".to_string(),
        reason: "file missing".to_string(),
    }
    .report();
    assert_eq!(persistence.code, "LITD-ASSET-STREAM-PERSISTENCE-FAILED");
    assert!(persistence.remediation.contains("source file"));

    let mismatch = EngineError::AssetStreamReadbackMismatch {
        field: "asset_lod.selected.sha256".to_string(),
        expected: "aaaa".to_string(),
        actual: "bbbb".to_string(),
    }
    .report();
    assert_eq!(mismatch.code, "LITD-ASSET-STREAM-READBACK-MISMATCH");
    assert!(mismatch.remediation.contains("FNV-1a"));
}

#[test]
fn stale_ecs_entity_report_is_structured() {
    let report = EngineError::EcsEntityNotAlive {
        index: 2,
        generation: 1,
    }
    .report();
    assert_eq!(report.code, "LITD-ECS-ENTITY-NOT-ALIVE");
    assert!(report.message.contains("#2v1"));
    assert!(report.remediation.contains("stale entity"));
}

#[test]
fn ecs_free_list_failures_are_structured() {
    let index_report = EngineError::EcsEntityIndexOverflow { slot_count: 9 }.report();
    assert_eq!(index_report.code, "LITD-ECS-ENTITY-INDEX-OVERFLOW");
    assert!(index_report.remediation.contains("u32 entity indices"));

    let generation_report = EngineError::EcsEntityGenerationOverflow {
        index: 2,
        generation: u32::MAX,
    }
    .report();
    assert_eq!(
        generation_report.code,
        "LITD-ECS-ENTITY-GENERATION-OVERFLOW"
    );
    assert!(generation_report.remediation.contains("stale handle"));

    let corrupt_report = EngineError::EcsFreeListCorrupt {
        index: 7,
        reason: "free-list index is outside the slot table".to_string(),
    }
    .report();
    assert_eq!(corrupt_report.code, "LITD-ECS-FREE-LIST-CORRUPT");
    assert!(corrupt_report.remediation.contains("slot/free-list"));
}

#[test]
fn scene_parent_cycle_report_is_structured() {
    let report = EngineError::SceneParentCycle {
        child: "#1v0".to_string(),
        parent: "#2v0".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-SCENE-PARENT-CYCLE");
    assert!(report.message.contains("#1v0"));
    assert!(report.remediation.contains("descendants"));
}

#[test]
fn reflection_type_mismatch_report_is_structured() {
    let report = EngineError::SerializedTypeMismatch {
        expected: "expected.Type".to_string(),
        actual: "actual.Type".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-REFLECT-TYPE-MISMATCH");
    assert!(report.message.contains("expected.Type"));
    assert!(report.remediation.contains("reject"));
}

#[test]
fn pbr_invalid_factor_report_is_structured() {
    let report = EngineError::Rendering(
        litd_engine::error::RenderingError::PbrMaterialInvalidFactor {
            field: "roughness_factor",
            value: 0.0,
            min: 0.04,
            max: 1.0,
        },
    )
    .report();
    assert_eq!(report.code, "LITD-RENDER-PBR-INVALID-FACTOR");
    assert!(report.message.contains("roughness_factor"));
    assert!(report.remediation.contains("authoring UI"));
}

#[test]
fn clustered_lighting_overflow_report_is_structured() {
    let report = EngineError::Rendering(
        litd_engine::error::RenderingError::ClusteredLightingClusterOverflow {
            cluster_index: 3,
            max_lights: 8,
        },
    )
    .report();
    assert_eq!(report.code, "LITD-RENDER-CLUSTERED-LIGHTING-OVERFLOW");
    assert!(report.message.contains("cluster 3"));
    assert!(report.remediation.contains("max_lights_per_cluster"));
}

#[test]
fn clustered_lighting_gpu_readback_mismatch_report_is_structured() {
    let report = EngineError::Rendering(
        litd_engine::error::RenderingError::ClusteredLightingGpuReadbackMismatch {
            field: "refs_summary".to_string(),
            expected: "0:[1]".to_string(),
            actual: "0:[]".to_string(),
        },
    )
    .report();
    assert_eq!(
        report.code,
        "LITD-RENDER-CLUSTERED-LIGHTING-GPU-READBACK-MISMATCH"
    );
    assert!(report.message.contains("refs_summary"));
    assert!(report.remediation.contains("GPU readback"));
}

#[test]
fn shadow_atlas_overflow_report_is_structured() {
    let report = EngineError::ShadowAtlasOverflow {
        required_tiles: 11,
        available_tiles: 4,
    }
    .report();
    assert_eq!(report.code, "LITD-RENDER-SHADOW-ATLAS-OVERFLOW");
    assert!(report.message.contains("11"));
    assert!(report.remediation.contains("shadow atlas"));
}

#[test]
fn post_process_invalid_config_report_is_structured() {
    let report = EngineError::PostProcessInvalidConfig {
        field: "color_grade.lut_size".to_string(),
        reason: "must be in range".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-RENDER-POST-INVALID-CONFIG");
    assert!(report.message.contains("lut_size"));
    assert!(report.remediation.contains("color-grade"));
}

#[test]
fn screen_space_invalid_config_report_is_structured() {
    let report = EngineError::ScreenSpaceInvalidConfig {
        field: "aa.msaa_samples".to_string(),
        reason: "must be supported".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-RENDER-SCREEN-SPACE-INVALID-CONFIG");
    assert!(report.message.contains("msaa_samples"));
    assert!(report.remediation.contains("MSAA"));
}

#[test]
fn batching_draw_limit_report_is_structured() {
    let report = EngineError::BatchingDrawLimitExceeded {
        draw_count: 9,
        max_draws: 4,
    }
    .report();
    assert_eq!(report.code, "LITD-RENDER-BATCHING-DRAW-LIMIT");
    assert!(report.message.contains("9"));
    assert!(report.remediation.contains("draw batch budget"));
}

#[test]
fn shader_binding_report_is_structured() {
    let report = EngineError::Rendering(
        litd_engine::error::RenderingError::ShaderBindingValidationFailed {
            shader_id: "litd.test.shader".to_string(),
            binding: "group=1,binding=0".to_string(),
            reason: "missing from pipeline layout".to_string(),
        },
    )
    .report();
    assert_eq!(report.code, "LITD-RENDER-SHADER-BINDING-VALIDATION");
    assert!(report.message.contains("litd.test.shader"));
    assert!(report.remediation.contains("pipeline layout"));
}

#[test]
fn stylized_shader_report_is_structured() {
    let report = EngineError::Rendering(
        litd_engine::error::RenderingError::StylizedShaderInvalidConfig {
            preset: "water".to_string(),
            field: "wave_strength".to_string(),
            reason: "outside range".to_string(),
        },
    )
    .report();
    assert_eq!(report.code, "LITD-RENDER-STYLIZED-SHADER-INVALID-CONFIG");
    assert!(report.message.contains("water"));
    assert!(report.remediation.contains("preset range"));
}

#[test]
fn camera_invalid_config_report_is_structured() {
    let report = EngineError::Rendering(litd_engine::error::RenderingError::CameraInvalidConfig {
        field: "near_far".to_string(),
        reason: "near must be less than far".to_string(),
    })
    .report();
    assert_eq!(report.code, "LITD-RENDER-CAMERA-INVALID-CONFIG");
    assert!(report.message.contains("near_far"));
    assert!(report.remediation.contains("clipping"));
}

#[test]
fn ui_render_invalid_config_report_is_structured() {
    let report = EngineError::UiRenderInvalidConfig {
        field: "viewport".to_string(),
        reason: "width must be non-zero".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(report.message.contains("viewport"));
    assert!(report.remediation.contains("fog grids"));
}

#[test]
fn text_shaping_reports_are_structured() {
    let invalid = EngineError::TextShapingInvalidConfig {
        field: "text.direction".to_string(),
        reason: "auto direction could not resolve a strong bidi level".to_string(),
    }
    .report();
    assert_eq!(invalid.code, "LITD-RENDER-TEXT-SHAPING-INVALID-CONFIG");
    assert!(invalid.message.contains("text.direction"));
    assert!(invalid.remediation.contains("single-line Unicode text"));

    let mismatch = EngineError::TextShapingReadbackMismatch {
        field: "shaped_text_run.fnv1a64".to_string(),
        expected: "aaaa".to_string(),
        actual: "bbbb".to_string(),
    }
    .report();
    assert_eq!(mismatch.code, "LITD-RENDER-TEXT-SHAPING-READBACK-MISMATCH");
    assert!(mismatch.remediation.contains("source-of-truth JSON"));
}

#[test]
fn reference_scene_invalid_config_report_is_structured() {
    let report = EngineError::ReferenceSceneInvalidConfig {
        field: "visual_target.width".to_string(),
        reason: "must be non-zero".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG");
    assert!(report.message.contains("visual_target.width"));
    assert!(report.remediation.contains("README-quality"));
}

#[test]
fn editor_world_renderer_invalid_config_report_is_structured() {
    let report = EngineError::EditorWorldRendererInvalidConfig {
        field: "viewport_rect".to_string(),
        reason: "width and height must be non-zero".to_string(),
    }
    .report();
    assert_eq!(report.code, "LITD-RENDER-EDITOR-WORLD-INVALID-CONFIG");
    assert!(report.message.contains("viewport_rect"));
    assert!(report.remediation.contains("bird-eye camera"));
}
