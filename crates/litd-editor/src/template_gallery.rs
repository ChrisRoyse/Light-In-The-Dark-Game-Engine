//! #901: opinionated starter templates users browse, clone, and dissect.
//!
//! Every template is authored through the REAL editor authoring APIs -- the same
//! `create_editor_world_record` terrain valve, `EditorRegion`/`EditorNamedCamera`
//! upserts, start-location objects, and (for the story template) a #892 cinematic
//! timeline. There is no bespoke template format: a template IS an
//! [`EditorDocument`], so "open to learn" dissects the exact structures the author
//! would build by hand, and cloning is a deep copy of a real document.

use litd_engine::EngineError;
use litd_engine::cinematic_timeline::{
    CinematicCue, CinematicFade, CinematicSubtitle, CueKind, FadeKind,
};
use litd_engine::math::{Transform, Vec3};
use serde::{Deserialize, Serialize};

use crate::cinematic_authoring::EditorCinematicTimeline;
use crate::history::{EditorDocument, EditorNamedCamera, EditorObject, EditorRegion};
use crate::play::{PlayCameraConfig, PlayCameraRig};
use crate::world_creation::{
    EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
    create_editor_world_record,
};

pub const MELEE_SKIRMISH_TEMPLATE: &str = "template.melee.skirmish.starter";
pub const STORY_CINEMATIC_TEMPLATE: &str = "template.story.cinematic.starter";

/// Browsable metadata for one starter template (the gallery card).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub template_id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub region_count: usize,
    pub named_camera_count: usize,
    pub cinematic_timeline_count: usize,
    pub start_location_count: usize,
}

/// The opinionated gallery: every shipped starter, newest doctrine first.
pub fn list_templates() -> Result<Vec<TemplateSummary>, EngineError> {
    [MELEE_SKIRMISH_TEMPLATE, STORY_CINEMATIC_TEMPLATE]
        .into_iter()
        .map(template_summary)
        .collect()
}

/// Metadata for one template id (fails loud on an unknown id).
pub fn template_summary(template_id: &str) -> Result<TemplateSummary, EngineError> {
    let document = build_template_document(template_id)?;
    let (name, category, description) = template_labels(template_id)?;
    let start_location_count = document
        .objects
        .values()
        .filter(|object| object.kind == "StartLocation")
        .count();
    Ok(TemplateSummary {
        template_id: template_id.to_string(),
        name: name.to_string(),
        category: category.to_string(),
        description: description.to_string(),
        region_count: document.regions.len(),
        named_camera_count: document.named_cameras.len(),
        cinematic_timeline_count: document.cinematic_timelines.len(),
        start_location_count,
    })
}

fn template_labels(
    template_id: &str,
) -> Result<(&'static str, &'static str, &'static str), EngineError> {
    match template_id {
        MELEE_SKIRMISH_TEMPLATE => Ok((
            "Melee Skirmish Starter",
            "melee",
            "Two-player symmetric skirmish: walkable arena, two start locations, \
             overview and battle cameras.",
        )),
        STORY_CINEMATIC_TEMPLATE => Ok((
            "Story / Cinematic Starter",
            "story",
            "Single-scene story map with named cameras and an authored intro \
             cutscene (camera move, fade-in, subtitle, music cue).",
        )),
        other => Err(unknown_template(other)),
    }
}

/// Build the real [`EditorDocument`] for a template id via the authoring APIs.
pub fn build_template_document(template_id: &str) -> Result<EditorDocument, EngineError> {
    match template_id {
        MELEE_SKIRMISH_TEMPLATE => build_melee_skirmish(),
        STORY_CINEMATIC_TEMPLATE => build_story_cinematic(),
        other => Err(unknown_template(other)),
    }
}

fn base_world(
    calyx_document_id: &str,
    revision_id: &str,
    display_name: &str,
    description: &str,
) -> Result<EditorDocument, EngineError> {
    let spec = EditorWorldCreationSpec {
        calyx_document_id: calyx_document_id.to_string(),
        revision_id: revision_id.to_string(),
        properties: EditorWorldProperties {
            display_name: display_name.to_string(),
            description: description.to_string(),
            width_tiles: 8,
            height_tiles: 8,
            initial_tile_id: "tile.ashen_grass.default".to_string(),
            initial_cliff_type: "cliff.natural".to_string(),
            random_height_seed: Some(901),
            playable_bounds: EditorPlayableBounds {
                min_tile_x: 0,
                min_tile_z: 0,
                max_tile_x_exclusive: 8,
                max_tile_z_exclusive: 8,
            },
            ..EditorWorldProperties::default()
        },
    };
    Ok(create_editor_world_record(spec)?.document)
}

fn start_location(slot: u32, player: &str, id: &str, position: Vec3) -> EditorObject {
    EditorObject::new(
        id,
        "StartLocation",
        None,
        Transform::from_translation(position),
    )
    .with_property("player_id", player)
    .with_property("slot", slot.to_string())
    .with_property("kind", "start_location")
}

fn build_melee_skirmish() -> Result<EditorDocument, EngineError> {
    let mut document = base_world(
        "calyx://world/template-melee-skirmish",
        "template-melee-skirmish-rev-1",
        "Melee Skirmish Starter",
        "Two-player symmetric skirmish arena.",
    )?;
    document.upsert_named_camera(EditorNamedCamera::top_down(
        "camera.overview",
        "Overview",
        Vec3::new(4.0, 0.0, 4.0),
    ))?;
    document.upsert_named_camera(EditorNamedCamera::cinematic(
        "camera.battle",
        "Battle",
        PlayCameraConfig::preset(PlayCameraRig::Isometric),
        Vec3::new(4.0, 0.0, 4.0),
    ))?;
    document.upsert_region(EditorRegion::new(
        "region.player_one_base",
        "Player One Base",
        Vec3::new(1.5, 0.0, 1.5),
        Vec3::new(1.5, 4.0, 1.5),
    ))?;
    document.upsert_region(EditorRegion::new(
        "region.player_two_base",
        "Player Two Base",
        Vec3::new(6.5, 0.0, 6.5),
        Vec3::new(1.5, 4.0, 1.5),
    ))?;
    document.upsert_object(start_location(
        0,
        "player_1",
        "start.player_1",
        Vec3::new(1.5, 0.0, 1.5),
    ))?;
    document.upsert_object(start_location(
        1,
        "player_2",
        "start.player_2",
        Vec3::new(6.5, 0.0, 6.5),
    ))?;
    document.validate()?;
    Ok(document)
}

fn build_story_cinematic() -> Result<EditorDocument, EngineError> {
    let mut document = base_world(
        "calyx://world/template-story-cinematic",
        "template-story-cinematic-rev-1",
        "Story / Cinematic Starter",
        "Single-scene story map with an authored intro cutscene.",
    )?;
    document.upsert_named_camera(EditorNamedCamera::top_down(
        "camera.establishing",
        "Establishing",
        Vec3::new(2.0, 0.0, 2.0),
    ))?;
    document.upsert_named_camera(EditorNamedCamera::cinematic(
        "camera.hero_closeup",
        "Hero Closeup",
        PlayCameraConfig::preset(PlayCameraRig::Isometric),
        Vec3::new(5.0, 0.0, 5.0),
    ))?;
    document.upsert_region(EditorRegion::new(
        "region.stage",
        "Stage",
        Vec3::new(4.0, 0.0, 4.0),
        Vec3::new(3.5, 4.0, 3.5),
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit.hero",
            "Unit",
            None,
            Transform::from_translation(Vec3::new(5.0, 0.0, 5.0)),
        )
        .with_property("unit_type", "unit.hero"),
    )?;
    document.upsert_cinematic_timeline(intro_cutscene())?;
    document.validate()?;
    Ok(document)
}

/// The authored intro cutscene for the story template: a camera move from the
/// establishing shot to the hero closeup, a fade-in, a subtitle, and a music cue.
fn intro_cutscene() -> EditorCinematicTimeline {
    EditorCinematicTimeline::new("cine.intro", "Intro Cutscene", 8.0)
        .with_camera_key(0.0, "camera.establishing")
        .with_camera_key(8.0, "camera.hero_closeup")
        .with_fade(CinematicFade {
            start_seconds: 0.0,
            duration_seconds: 1.5,
            kind: FadeKind::In,
            color: [0.0, 0.0, 0.0, 1.0],
        })
        .with_subtitle(CinematicSubtitle {
            start_seconds: 1.5,
            duration_seconds: 4.0,
            text: "The ashen dawn breaks over the keep.".to_string(),
        })
        .with_cue(CinematicCue {
            time_seconds: 0.0,
            kind: CueKind::Music,
            cue_ref: "music.story.intro".to_string(),
        })
}

/// Deep-copy a template document into a fresh editable project document: a full
/// value clone with a brand-new Calyx identity (`calyx_document_id` + `revision_id`).
/// The clone shares no mutable state with the source -- `EditorDocument` is a value
/// type, so this is an owned copy, and the fresh identity means it persists to its
/// own vault rows. Re-validates before returning so a bad identity fails loud.
pub fn clone_template_document(
    source: &EditorDocument,
    new_calyx_document_id: &str,
    new_revision_id: &str,
) -> Result<EditorDocument, EngineError> {
    if new_calyx_document_id.trim().is_empty() || new_revision_id.trim().is_empty() {
        return Err(EngineError::UiRenderInvalidConfig {
            field: "editor.template_gallery.clone_identity".to_string(),
            reason: "clone calyx_document_id and revision_id must be non-empty".to_string(),
        });
    }
    let mut clone = source.clone();
    clone.calyx_document_id = new_calyx_document_id.to_string();
    clone.revision_id = new_revision_id.to_string();
    clone.validate()?;
    Ok(clone)
}

pub(crate) fn unknown_template(template_id: &str) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: "editor.template_gallery.template_id".to_string(),
        reason: format!("{template_id} is not a known starter template"),
    }
}

#[cfg(test)]
#[path = "template_gallery_tests.rs"]
mod tests;
