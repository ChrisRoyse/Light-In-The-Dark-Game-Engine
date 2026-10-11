use std::{
    env, fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    math::{Aabb, FixedVec3, Vec2, Vec3},
    presentation::{
        PresentationBodyStyle, PresentationEntityKind, PresentationFrame,
        extract_presentation_frame,
    },
    render::batching::{BatchingSettings, Frustum, ScreenRect, build_batch_plan},
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_replay::SimSnapshot,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env::var("LITD_SIM_PRESENTATION_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1136-sim-presentation-fsv.txt"));
    let frame_path = env::var("LITD_SIM_PRESENTATION_FSV_FRAME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1136-presentation-frame.ron"));

    let mut sim = FixedTickSim::<4>::new(SimConfig::new(64)?, 0x1136)?;
    let knight = sim.spawn_body(v(0, 0, 0), v(64, 0, -128), FixedVec3::ZERO)?;
    let archer = sim.spawn_body(v(-2, 0, 4), FixedVec3::ZERO, FixedVec3::ZERO)?;
    sim.run_ticks(3)?;

    let sim_hash_before = sim.state_hash();
    let snapshot = SimSnapshot::capture(&sim);
    let styles = vec![
        style(
            knight,
            "unit_knight",
            "steel_gold",
            Vec3::splat(0.5),
            rect(0.45, 0.42, 0.55, 0.55)?,
            0.35,
            144,
        ),
        style(
            archer,
            "unit_archer",
            "forest_cloak",
            Vec3::splat(0.4),
            rect(0.58, 0.35, 0.66, 0.52)?,
            0.32,
            96,
        ),
    ];
    let mut frame = extract_presentation_frame(&snapshot, &styles)?;
    let sim_hash_after_extract = sim.state_hash();
    let frame_for_render = frame.clone();
    frame.instances[0].transform.translation.x = 999.0;
    frame.instances[0]
        .render_instance
        .mesh_id
        .push_str("_copy_probe");
    let sim_hash_after_presentation_mutation = sim.state_hash();

    let render_instances = frame_for_render.render_instances();
    let frustum = Frustum::orthographic(Aabb::from_center_half_extents(
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(8.0, 4.0, 12.0),
    )?)?;
    let plan = build_batch_plan(
        BatchingSettings::default(),
        &frustum,
        &[],
        &render_instances,
    )?;

    write_text(&frame_path, &frame_to_ron(&frame_for_render)?)?;

    let restored = snapshot.restore::<4>()?;
    let missing_style_code = match extract_presentation_frame(&snapshot, &styles[0..1]) {
        Err(err @ EngineError::BatchingInvalidConfig { .. }) => err.code(),
        other => panic!("expected BatchingInvalidConfig, got {other:?}"),
    };
    let duplicate_style_code = match extract_presentation_frame(
        &snapshot,
        &[
            styles[0].clone(),
            PresentationBodyStyle {
                material_id: "duplicate_probe".to_string(),
                ..styles[0].clone()
            },
        ],
    ) {
        Err(err @ EngineError::BatchingInvalidConfig { .. }) => err.code(),
        other => panic!("expected BatchingInvalidConfig, got {other:?}"),
    };
    let tampered_snapshot_code = tampered_snapshot_probe(&snapshot);

    write_text(
        &stats_path,
        &format!(
            "source_tick={}\n\
             source_state_hash={:016X}\n\
             sim_hash_before={:016X}\n\
             sim_hash_after_extract={:016X}\n\
             sim_hash_after_presentation_mutation={:016X}\n\
             sim_hash_unchanged={}\n\
             snapshot_restore_hash={:016X}\n\
             snapshot_restore_match={}\n\
             presentation_instance_count={}\n\
             render_instance_count={}\n\
             body_ids={}\n\
             render_instance_ids={}\n\
             body0_translation={}\n\
             body0_velocity={}\n\
             body1_translation={}\n\
             batch_visible_ids={}\n\
             draw_batches={}\n\
             batch_keys={}\n\
             missing_style_code={}\n\
             duplicate_style_code={}\n\
             tampered_snapshot_code={}\n\
             frame_file={}\n\
             frame_file_bytes={}\n\
             render_touched=false\n",
            frame_for_render.source_tick,
            frame_for_render.source_state_hash,
            sim_hash_before,
            sim_hash_after_extract,
            sim_hash_after_presentation_mutation,
            sim_hash_before == sim_hash_after_extract
                && sim_hash_before == sim_hash_after_presentation_mutation,
            restored.state_hash(),
            restored.state_hash() == sim_hash_before,
            frame_for_render.instances.len(),
            render_instances.len(),
            join_body_ids(&frame_for_render),
            join_ids(&plan.visible_instance_ids),
            vec3(frame_for_render.instances[0].transform.translation),
            vec3(frame_for_render.instances[0].velocity),
            vec3(frame_for_render.instances[1].transform.translation),
            join_ids(&plan.visible_instance_ids),
            plan.draw_batches.len(),
            join_batch_keys(&plan.draw_batches),
            missing_style_code,
            duplicate_style_code,
            tampered_snapshot_code,
            frame_path.display(),
            fs::metadata(&frame_path)?.len(),
        ),
    )?;
    println!(
        "wrote sim presentation FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn tampered_snapshot_probe(snapshot: &SimSnapshot) -> &'static str {
    let mut tampered = snapshot.clone();
    tampered.state_hash = 0;
    match tampered.restore::<4>() {
        Err(err @ EngineError::SerializedFingerprintMismatch { .. }) => err.code(),
        Ok(_) => panic!("expected SerializedFingerprintMismatch, got Ok"),
        Err(other) => panic!(
            "expected SerializedFingerprintMismatch, got {}",
            other.code()
        ),
    }
}

fn style(
    body_id: SimBodyId,
    mesh_id: &str,
    material_id: &str,
    half_extents: Vec3,
    screen_rect: ScreenRect,
    nearest_depth: f32,
    index_count: u32,
) -> PresentationBodyStyle {
    PresentationBodyStyle {
        body_id,
        entity_kind: PresentationEntityKind::Unit,
        mesh_id: mesh_id.to_string(),
        material_id: material_id.to_string(),
        half_extents,
        screen_rect,
        nearest_depth,
        index_count,
    }
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Result<ScreenRect, EngineError> {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn frame_to_ron(frame: &PresentationFrame) -> Result<String, EngineError> {
    ron::ser::to_string_pretty(
        frame,
        ron::ser::PrettyConfig::default()
            .new_line("\n".to_string())
            .indentor("  ".to_string())
            .compact_arrays(true),
    )
    .map_err(|error| EngineError::SerializationFailed {
        format: "ron",
        reason: error.to_string(),
    })
}

fn vec3(value: Vec3) -> String {
    format!("{:.3},{:.3},{:.3}", value.x, value.y, value.z)
}

fn join_ids(ids: &[u64]) -> String {
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

fn join_body_ids(frame: &PresentationFrame) -> String {
    frame
        .instances
        .iter()
        .map(|instance| instance.body_id.index().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn join_batch_keys(batches: &[litd_engine::render::batching::DrawBatch]) -> String {
    batches
        .iter()
        .map(|batch| format!("{}:{}", batch.key.mesh_id, batch.key.material_id))
        .collect::<Vec<_>>()
        .join(",")
}
