use litd_engine::{
    EngineError,
    math::Vec3,
    presentation::{PresentationBodyStyle, PresentationFrame, extract_presentation_frame},
    render::batching::{BatchingSettings, Frustum, build_batch_plan},
    sim::SimBodyId,
    sim_replay::SimSnapshot,
};

use crate::history::EditorDocument;

use super::{PlayBodyBinding, PlayBuildArtifact, invalid_play, style_for};

pub(super) fn apply_runtime_snapshot_to_build(
    build: &mut PlayBuildArtifact,
    document: &EditorDocument,
    snapshot: SimSnapshot,
) -> Result<(), EngineError> {
    let styles = styles_for_bindings(&build.spawned_bodies, document)?;
    let (presentation, visible_instance_ids, draw_batches) =
        presentation_readback(&snapshot, &styles)?;
    build.snapshot = snapshot;
    build.presentation = presentation;
    build.visible_instance_ids = visible_instance_ids;
    build.draw_batches = draw_batches;
    Ok(())
}

pub(super) fn presentation_readback(
    snapshot: &SimSnapshot,
    styles: &[PresentationBodyStyle],
) -> Result<(PresentationFrame, Vec<u64>, usize), EngineError> {
    let presentation = extract_presentation_frame(snapshot, styles)?;
    let frustum = Frustum::orthographic(litd_engine::math::Aabb::from_center_half_extents(
        Vec3::ZERO,
        Vec3::splat(256.0),
    )?)?;
    let plan = build_batch_plan(
        BatchingSettings::default(),
        &frustum,
        &[],
        &presentation.render_instances(),
    )?;
    Ok((
        presentation,
        plan.visible_instance_ids,
        plan.draw_batches.len(),
    ))
}

fn styles_for_bindings(
    bindings: &[PlayBodyBinding],
    document: &EditorDocument,
) -> Result<Vec<PresentationBodyStyle>, EngineError> {
    bindings
        .iter()
        .map(|binding| {
            let object = document.objects.get(&binding.object_id).ok_or_else(|| {
                EngineError::UiRenderInvalidConfig {
                    field: "editor.play.binding.object".to_string(),
                    reason: format!("missing object {}", binding.object_id),
                }
            })?;
            if object.kind != "Unit" {
                return invalid_play(
                    "binding.kind",
                    "play body binding must target a Unit object",
                );
            }
            style_for(SimBodyId::from_raw(binding.body_id), object, document)
        })
        .collect()
}
