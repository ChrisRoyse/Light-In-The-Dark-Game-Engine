use super::*;

#[test]
fn culls_and_batches_visible_instances() {
    let plan = fixture_plan(BatchingSettings::default()).unwrap();
    assert_eq!(plan.visible_instance_ids, vec![1, 2, 5, 6]);
    assert_eq!(plan.frustum_culled_ids, vec![3]);
    assert_eq!(plan.occlusion_culled_ids, vec![4]);
    assert_eq!(plan.draw_batches.len(), 2);
    assert_eq!(plan.draw_batches[0].key.mesh_id, "tree");
    assert_eq!(plan.draw_batches[0].instance_ids, vec![1, 2]);
    assert_eq!(plan.indirect_commands[0].instance_count, 2);
}

#[test]
fn draw_limit_fails_loud() {
    let settings = BatchingSettings {
        max_draw_batches: 1,
        ..BatchingSettings::default()
    };
    match fixture_plan(settings) {
        Err(EngineError::BatchingDrawLimitExceeded {
            draw_count,
            max_draws,
        }) => {
            assert_eq!(draw_count, 2);
            assert_eq!(max_draws, 1);
        }
        other => panic!("expected BatchingDrawLimitExceeded, got {other:?}"),
    }
}

#[test]
fn invalid_screen_rect_fails_loud() {
    match ScreenRect::new(Vec2::new(0.8, 0.2), Vec2::new(0.4, 0.9)) {
        Err(EngineError::BatchingInvalidConfig { field, .. }) => {
            assert_eq!(field, "screen_rect");
        }
        other => panic!("expected BatchingInvalidConfig, got {other:?}"),
    }
}

fn fixture_plan(settings: BatchingSettings) -> Result<BatchPlan, EngineError> {
    let frustum = Frustum::orthographic(Aabb::new(
        Vec3::new(-5.0, -5.0, 0.0),
        Vec3::new(5.0, 5.0, 10.0),
    )?)?;
    let occluders = vec![HzbOccluder::new(rect(0.42, 0.42, 0.58, 0.58)?, 0.40)];
    build_batch_plan(settings, &frustum, &occluders, &fixture_instances()?)
}

fn fixture_instances() -> Result<Vec<RenderInstance>, EngineError> {
    Ok(vec![
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
    ])
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
        world_bounds: Aabb::from_center_half_extents(center, Vec3::splat(0.4))?,
        screen_rect,
        nearest_depth,
        index_count,
    })
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Result<ScreenRect, EngineError> {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
}
