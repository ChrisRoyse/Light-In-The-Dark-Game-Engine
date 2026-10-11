use crate::{
    EngineError,
    math::{FixedVec3, Vec2, Vec3},
    presentation::{
        PresentationBodyStyle, PresentationEntityKind, diff_presentation_frames,
        extract_presentation_frame,
    },
    render::batching::{BatchingSettings, Frustum, ScreenRect, build_batch_plan},
    sim::{FixedTickSim, SimConfig},
    sim_replay::SimSnapshot,
};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> ScreenRect {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y)).unwrap()
}

fn style(body_id: crate::sim::SimBodyId) -> PresentationBodyStyle {
    PresentationBodyStyle {
        body_id,
        entity_kind: PresentationEntityKind::Unit,
        mesh_id: "unit_knight".to_string(),
        material_id: "steel_gold".to_string(),
        half_extents: Vec3::splat(0.5),
        screen_rect: rect(0.1, 0.1, 0.2, 0.2),
        nearest_depth: 0.3,
        index_count: 144,
    }
}

#[test]
fn snapshot_extracts_copied_render_instances_without_mutating_sim() {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 1).unwrap();
    let body = sim.spawn_body(v(2, 3, 4), v(64, 0, 0), v(0, 0, 0)).unwrap();
    let before = sim.state_hash();
    let snapshot = SimSnapshot::capture(&sim);

    let mut frame = extract_presentation_frame(&snapshot, &[style(body)]).unwrap();
    frame.instances[0].transform.translation.x = 999.0;

    assert_eq!(sim.state_hash(), before);
    assert_eq!(snapshot.state_hash, before);
    assert_eq!(frame.source_state_hash, before);
    assert_eq!(
        frame.instances[0].render_instance.world_bounds.center(),
        Vec3::new(2.0, 3.0, 4.0)
    );
}

#[test]
fn extracted_instances_feed_existing_batcher() {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 2).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let snapshot = SimSnapshot::capture(&sim);
    let frame = extract_presentation_frame(&snapshot, &[style(body)]).unwrap();
    let frustum = Frustum::orthographic(
        crate::math::Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(4.0)).unwrap(),
    )
    .unwrap();

    let plan = build_batch_plan(
        BatchingSettings::default(),
        &frustum,
        &[],
        &frame.render_instances(),
    )
    .unwrap();

    assert_eq!(plan.visible_instance_ids, vec![1]);
    assert_eq!(plan.draw_batches.len(), 1);
}

#[test]
fn missing_or_duplicate_style_fails_loud() {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 3).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let snapshot = SimSnapshot::capture(&sim);

    match extract_presentation_frame(&snapshot, &[]) {
        Err(EngineError::BatchingInvalidConfig { field, reason }) => {
            assert!(field.contains("presentation.styles"));
            assert!(reason.contains("missing"));
        }
        other => panic!("expected BatchingInvalidConfig, got {other:?}"),
    }

    let first = style(body);
    let duplicate = PresentationBodyStyle {
        material_id: "duplicate".to_string(),
        ..first.clone()
    };
    match extract_presentation_frame(&snapshot, &[first, duplicate]) {
        Err(EngineError::BatchingInvalidConfig { field, reason }) => {
            assert!(field.contains("body_id"));
            assert!(reason.contains("unique"));
        }
        other => panic!("expected BatchingInvalidConfig, got {other:?}"),
    }
}

#[test]
fn lifecycle_diff_reports_created_updated_and_destroyed_bodies() {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(64).unwrap(), 4).unwrap();
    let unit = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let doodad = sim.spawn_body(v(2, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let previous = extract_presentation_frame(
        &SimSnapshot::capture(&sim),
        &[
            style(unit),
            style_with_kind(doodad, PresentationEntityKind::Doodad),
        ],
    )
    .unwrap();

    sim.set_body_velocity(unit, v(64, 0, 0)).unwrap();
    sim.step().unwrap();
    sim.despawn_body(doodad).unwrap();
    let effect = sim.spawn_body(v(4, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let current = extract_presentation_frame(
        &SimSnapshot::capture(&sim),
        &[
            style(unit),
            style_with_kind(effect, PresentationEntityKind::Effect),
        ],
    )
    .unwrap();

    let diff = diff_presentation_frames(Some(&previous), &current).unwrap();

    assert_eq!(diff.summary(), "created=1;updated=1;destroyed=1");
    assert_eq!(diff.created[0].body_id, effect);
    assert_eq!(diff.created[0].entity_kind, PresentationEntityKind::Effect);
    assert_eq!(diff.updated[0].body_id, unit);
    assert_eq!(diff.destroyed[0].body_id, doodad);
    assert_eq!(
        diff.destroyed[0].entity_kind,
        PresentationEntityKind::Doodad
    );
}

fn style_with_kind(
    body_id: crate::sim::SimBodyId,
    entity_kind: PresentationEntityKind,
) -> PresentationBodyStyle {
    PresentationBodyStyle {
        entity_kind,
        ..style(body_id)
    }
}
