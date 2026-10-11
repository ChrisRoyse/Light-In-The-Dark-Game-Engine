use super::*;

use litd_engine::cinematic_timeline::{CueKind, FadeKind, evaluate_cinematic};
use litd_engine::math::Vec3;

use crate::history::EditorDocument;
use crate::play::{PlayCameraConfig, PlayCameraRig};

fn document_with_cameras() -> EditorDocument {
    let mut document = EditorDocument::new("cine.doc", "rev.1").unwrap();
    document
        .upsert_named_camera(EditorNamedCamera::top_down(
            "camera.wide",
            "Wide",
            Vec3::new(2.0, 0.0, 2.0),
        ))
        .unwrap();
    document
        .upsert_named_camera(EditorNamedCamera::cinematic(
            "camera.close",
            "Close",
            PlayCameraConfig::preset(PlayCameraRig::Isometric),
            Vec3::new(3.0, 0.0, 3.0),
        ))
        .unwrap();
    document
}

fn authored_timeline() -> EditorCinematicTimeline {
    EditorCinematicTimeline::new("cine.intro", "Intro", 8.0)
        .with_camera_key(0.0, "camera.wide")
        .with_camera_key(8.0, "camera.close")
        .with_fade(CinematicFade {
            start_seconds: 0.0,
            duration_seconds: 2.0,
            kind: FadeKind::In,
            color: [0.0, 0.0, 0.0, 1.0],
        })
        .with_cue(CinematicCue {
            time_seconds: 0.0,
            kind: CueKind::Music,
            cue_ref: "music.intro".to_string(),
        })
}

#[test]
fn lowering_resolves_named_cameras_to_poses() {
    let document = document_with_cameras();
    let lowered = lower_cinematic_timeline(&document, &authored_timeline()).unwrap();
    assert_eq!(lowered.camera_track.len(), 2);
    // camera.wide look_at (2,0,2) + top-down target_offset -> concrete pose.
    let wide = &document.named_cameras["camera.wide"];
    let expected = wide.look_at + wide.config.target_offset;
    assert_eq!(lowered.camera_track[0].pose.target, expected);
    let evaluated = evaluate_cinematic(&lowered, 0.0).unwrap();
    assert_eq!(evaluated.fade.alpha, 1.0);
}

#[test]
fn dangling_camera_ref_fails_loud() {
    let document = document_with_cameras();
    let timeline =
        EditorCinematicTimeline::new("cine.bad", "Bad", 4.0).with_camera_key(0.0, "camera.missing");
    let error = lower_cinematic_timeline(&document, &timeline).unwrap_err();
    assert!(matches!(error, EngineError::UiRenderInvalidConfig { .. }));
}

#[test]
fn upsert_rejects_timeline_with_missing_camera() {
    let mut document = document_with_cameras();
    let timeline =
        EditorCinematicTimeline::new("cine.bad", "Bad", 4.0).with_camera_key(0.0, "camera.ghost");
    assert!(document.upsert_cinematic_timeline(timeline).is_err());
    assert!(document.cinematic_timelines.is_empty());
}

#[test]
fn upsert_stores_valid_timeline_and_document_revalidates() {
    let mut document = document_with_cameras();
    document
        .upsert_cinematic_timeline(authored_timeline())
        .unwrap();
    assert_eq!(document.cinematic_timelines.len(), 1);
    document.validate().unwrap();
}
