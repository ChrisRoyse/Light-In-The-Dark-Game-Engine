use super::*;
use crate::math::Vec3;

fn pose(target: f32, distance: f32) -> CinematicCameraPose {
    CinematicCameraPose {
        target: Vec3::new(target, 0.0, target),
        distance,
        yaw_degrees: 0.0,
        pitch_degrees: -45.0,
        fov_degrees: 60.0,
    }
}

fn sample_timeline() -> CinematicTimeline {
    CinematicTimeline {
        timeline_id: "cine.intro".to_string(),
        duration_seconds: 10.0,
        camera_track: vec![
            CameraKeyframe {
                time_seconds: 0.0,
                camera_ref: "camera.a".to_string(),
                pose: pose(0.0, 10.0),
            },
            CameraKeyframe {
                time_seconds: 10.0,
                camera_ref: "camera.b".to_string(),
                pose: pose(4.0, 20.0),
            },
        ],
        transmissions: vec![CinematicTransmission {
            start_seconds: 2.0,
            duration_seconds: 4.0,
            speaker_unit_type: "unit.hero".to_string(),
            portrait_ref: "portrait.hero".to_string(),
            text: "Hold the line.".to_string(),
            track: 0,
        }],
        fades: vec![CinematicFade {
            start_seconds: 0.0,
            duration_seconds: 2.0,
            kind: FadeKind::In,
            color: [0.0, 0.0, 0.0, 1.0],
        }],
        subtitles: vec![CinematicSubtitle {
            start_seconds: 2.0,
            duration_seconds: 4.0,
            text: "Hold the line.".to_string(),
        }],
        cues: vec![CinematicCue {
            time_seconds: 0.0,
            kind: CueKind::Music,
            cue_ref: "music.intro".to_string(),
        }],
    }
}

#[test]
fn camera_interpolates_linearly_at_midpoint() {
    let timeline = sample_timeline();
    let mid = evaluate_cinematic(&timeline, 5.0).unwrap();
    assert_eq!(mid.camera.distance, 15.0);
    assert_eq!(mid.camera.target, Vec3::new(2.0, 0.0, 2.0));
    let start = evaluate_cinematic(&timeline, 0.0).unwrap();
    assert_eq!(start.camera.distance, 10.0);
    let end = evaluate_cinematic(&timeline, 10.0).unwrap();
    assert_eq!(end.camera.distance, 20.0);
}

#[test]
fn fade_in_reveals_then_holds_visible() {
    let timeline = sample_timeline();
    assert_eq!(evaluate_cinematic(&timeline, 0.0).unwrap().fade.alpha, 1.0);
    assert_eq!(evaluate_cinematic(&timeline, 1.0).unwrap().fade.alpha, 0.5);
    assert_eq!(evaluate_cinematic(&timeline, 2.0).unwrap().fade.alpha, 0.0);
    assert_eq!(evaluate_cinematic(&timeline, 8.0).unwrap().fade.alpha, 0.0);
}

#[test]
fn transmission_active_only_inside_window() {
    let timeline = sample_timeline();
    assert!(
        evaluate_cinematic(&timeline, 1.0)
            .unwrap()
            .active_transmission
            .is_none()
    );
    let active = evaluate_cinematic(&timeline, 3.0)
        .unwrap()
        .active_transmission
        .unwrap();
    assert_eq!(active.speaker_unit_type, "unit.hero");
    // End boundary is exclusive.
    assert!(
        evaluate_cinematic(&timeline, 6.0)
            .unwrap()
            .active_transmission
            .is_none()
    );
}

#[test]
fn overlapping_transmissions_resolve_by_highest_track() {
    let mut timeline = sample_timeline();
    timeline.transmissions.push(CinematicTransmission {
        start_seconds: 2.0,
        duration_seconds: 4.0,
        speaker_unit_type: "unit.villain".to_string(),
        portrait_ref: "portrait.villain".to_string(),
        text: "Never.".to_string(),
        track: 1,
    });
    let active = evaluate_cinematic(&timeline, 3.0)
        .unwrap()
        .active_transmission
        .unwrap();
    assert_eq!(active.speaker_unit_type, "unit.villain");
    assert_eq!(active.track, 1);
}

#[test]
fn zero_length_fade_is_rejected_typed() {
    let mut timeline = sample_timeline();
    timeline.fades[0].duration_seconds = 0.0;
    let error = timeline.validate().unwrap_err();
    assert_eq!(error.code(), "CINEMATIC_FADE_ZERO_LENGTH");
}

#[test]
fn unsorted_camera_track_is_rejected() {
    let mut timeline = sample_timeline();
    timeline.camera_track[1].time_seconds = 0.0;
    let error = timeline.validate().unwrap_err();
    assert_eq!(error.field, "camera_track.order");
}

#[test]
fn non_finite_sample_time_is_rejected() {
    let timeline = sample_timeline();
    assert!(evaluate_cinematic(&timeline, f32::NAN).is_err());
}

#[test]
fn cue_holds_latest_music_reference() {
    let timeline = sample_timeline();
    assert_eq!(
        evaluate_cinematic(&timeline, 5.0).unwrap().active_music_cue,
        Some("music.intro".to_string())
    );
    assert_eq!(
        evaluate_cinematic(&timeline, 5.0).unwrap().active_voice_cue,
        None
    );
}
