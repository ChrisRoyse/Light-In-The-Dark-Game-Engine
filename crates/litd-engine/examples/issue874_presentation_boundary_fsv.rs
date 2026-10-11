use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::{Aabb, FixedVec3, Vec2, Vec3},
    presentation::{PresentationBodyStyle, PresentationEntityKind, extract_presentation_frame},
    render::batching::{BatchingSettings, Frustum, ScreenRect, build_batch_plan},
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_replay::SimSnapshot,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue874_presentation_boundary")?;
    let report = build_report(&run.report_path().display().to_string())?;
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE874_PRESENTATION_BOUNDARY_FSV report={} bytes={} sha256={} instances={} draw_batches={} hash_unchanged={} edges={} manifest={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["actual"]["presentation_instance_count"],
        readback.json["actual"]["draw_batches"],
        readback.json["acceptance"]["sim_hash_unchanged"],
        readback.json["acceptance"]["edge_count"],
        readback.latest_manifest_path.display(),
    );
    Ok(())
}

fn build_report(report_path: &str) -> Result<Value, EngineError> {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(64)?, 874)?;
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
    let frame = extract_presentation_frame(&snapshot, &styles)?;
    let sim_hash_after_extract = sim.state_hash();

    let mut mutated_copy = frame.clone();
    mutated_copy.instances[0].transform.translation.x = 999.0;
    mutated_copy.instances[0]
        .render_instance
        .mesh_id
        .push_str("_copy_probe");
    let sim_hash_after_copy_mutation = sim.state_hash();

    let plan = build_batch_plan(
        BatchingSettings::default(),
        &Frustum::orthographic(Aabb::from_center_half_extents(
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(8.0, 4.0, 12.0),
        )?)?,
        &[],
        &frame.render_instances(),
    )?;
    let restored = snapshot.restore::<4>()?;
    let edges = edge_readbacks(&snapshot, &styles, knight)?;
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let sim_hash_unchanged = sim_hash_before == sim_hash_after_extract
        && sim_hash_before == sim_hash_after_copy_mutation;
    ensure_acceptance(sim_hash_unchanged, "issue874.sim_hash_unchanged")?;
    ensure_acceptance(all_edges_matched, "issue874.edges")?;

    Ok(json!({
        "schema": "litd.engine.issue874.presentation_boundary_fsv.v1",
        "schema_version": 1,
        "issue": 874,
        "source_of_truth": {
            "report_path": report_path,
            "engine_boundary": "litd_engine::presentation::extract_presentation_frame",
            "input_source": "FixedTickSim -> SimSnapshot immutable readback",
            "render_consumer": "PresentationFrame::render_instances -> build_batch_plan",
            "superseding_doctrine": "#1071 custom Rust/wgpu engine; g3n wording obsolete",
            "adr": "docs/adr/adr-874-rust-presentation-boundary.md",
            "manual_gate": "manual Synapse/browser readback must inspect this persisted JSON and latest manifest"
        },
        "expected": {
            "presentation_reads_snapshot_only": true,
            "presentation_copy_mutation_does_not_change_sim_hash": true,
            "render_batching_receives_render_instances_only": true,
            "sim_restore_hash_matches": true,
            "edge_count": 4,
        },
        "actual": {
            "source_tick": frame.source_tick,
            "source_state_hash": format!("{:016X}", frame.source_state_hash),
            "sim_hash_before": format!("{sim_hash_before:016X}"),
            "sim_hash_after_extract": format!("{sim_hash_after_extract:016X}"),
            "sim_hash_after_copy_mutation": format!("{sim_hash_after_copy_mutation:016X}"),
            "presentation_instance_count": frame.instances.len(),
            "render_instance_count": frame.render_instances().len(),
            "body_ids": frame.instances.iter().map(|instance| instance.body_id.index()).collect::<Vec<_>>(),
            "render_instance_ids": plan.visible_instance_ids,
            "draw_batches": plan.draw_batches.len(),
            "draw_batch_keys": plan.draw_batches.iter().map(|batch| format!("{}:{}", batch.key.mesh_id, batch.key.material_id)).collect::<Vec<_>>(),
            "copy_mutation_probe_mesh": mutated_copy.instances[0].render_instance.mesh_id,
            "original_mesh_after_copy_mutation": frame.instances[0].render_instance.mesh_id,
            "restored_snapshot_hash": format!("{:016X}", restored.state_hash()),
        },
        "acceptance": {
            "source_hash_matches_snapshot": frame.source_state_hash == snapshot.state_hash,
            "sim_hash_unchanged": sim_hash_unchanged,
            "copy_isolated_from_original_frame": frame.instances[0].render_instance.mesh_id == "unit_knight",
            "render_batcher_consumed_instances": frame.instances.len() == 2 && plan.draw_batches.len() == 2,
            "snapshot_restore_match": restored.state_hash() == sim_hash_before,
            "all_edges_matched": all_edges_matched,
            "edge_count": edges.len(),
        },
        "edges": edges,
        "sufficiency_argument": "The minimal known-I/O set uses two sim bodies to prove snapshot-to-presentation extraction, render-instance handoff, copied-frame isolation, and sim-hash stability. The edge set covers missing style, duplicate style, invalid style data, and tampered snapshot hash. Additional bodies would duplicate the same boundary rule."
    }))
}

fn edge_readbacks(
    snapshot: &SimSnapshot,
    styles: &[PresentationBodyStyle],
    knight: SimBodyId,
) -> Result<Vec<Value>, EngineError> {
    let duplicate = PresentationBodyStyle {
        material_id: "duplicate_probe".to_string(),
        ..styles[0].clone()
    };
    let invalid = PresentationBodyStyle {
        mesh_id: String::new(),
        ..styles[0].clone()
    };
    Ok(vec![
        expect_code(
            "missing_style_rejected",
            extract_presentation_frame(snapshot, &styles[0..1]),
            "LITD-RENDER-BATCHING-INVALID-CONFIG",
            "presentation.styles",
        ),
        expect_code(
            "duplicate_style_rejected",
            extract_presentation_frame(snapshot, &[styles[0].clone(), duplicate]),
            "LITD-RENDER-BATCHING-INVALID-CONFIG",
            "presentation.styles[1].body_id",
        ),
        expect_code(
            "invalid_style_rejected",
            extract_presentation_frame(snapshot, &[invalid, style_for_edge(knight)?]),
            "LITD-RENDER-BATCHING-INVALID-CONFIG",
            "presentation.styles[0].mesh_id",
        ),
        tampered_snapshot_edge(snapshot),
    ])
}

fn expect_code<T>(
    result_label: &str,
    result: Result<T, EngineError>,
    code: &str,
    field: &str,
) -> Value {
    match result {
        Err(error) => json!({
            "label": result_label,
            "matched": error.code() == code && error.to_string().contains(field),
            "error_code": error.code(),
            "error": error.to_string(),
            "expected_code": code,
            "expected_field": field,
        }),
        Ok(_) => json!({
            "label": result_label,
            "matched": false,
            "unexpected": "operation succeeded",
            "expected_code": code,
        }),
    }
}

fn tampered_snapshot_edge(snapshot: &SimSnapshot) -> Value {
    let mut tampered = snapshot.clone();
    tampered.state_hash = 0;
    expect_code(
        "tampered_snapshot_rejected",
        tampered.restore::<4>(),
        "LITD-REFLECT-FINGERPRINT-MISMATCH",
        "expected",
    )
}

fn style_for_edge(body_id: SimBodyId) -> Result<PresentationBodyStyle, EngineError> {
    Ok(style(
        body_id,
        "edge_mesh",
        "edge_material",
        Vec3::splat(0.25),
        rect(0.1, 0.1, 0.2, 0.2)?,
        0.5,
        12,
    ))
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

fn ensure_acceptance(condition: bool, field: &str) -> Result<(), EngineError> {
    if condition {
        Ok(())
    } else {
        Err(EngineError::BatchingInvalidConfig {
            field: field.to_string(),
            reason: "FSV acceptance condition was false".to_string(),
        })
    }
}
