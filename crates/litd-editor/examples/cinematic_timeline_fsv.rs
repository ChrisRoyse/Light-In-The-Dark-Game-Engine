//! FSV for #892: no-code cinematic timeline authoring + deterministic evaluation.
//!
//! Drives the REAL editor MCP tools (`author_timeline`, `evaluate_timeline`) against
//! a Calyx Aster vault, reads the stored document back through a FRESH handle, samples
//! the pure engine evaluator at t=0/mid/end with printed values, edits one camera
//! keyframe and proves EXACTLY the affected samples change, exercises three edge cases
//! (zero-length fade rejected typed, overlapping transmissions resolved by the
//! documented rule, missing timeline fails loud), runs a negative control that
//! sabotages the lowered timeline and proves validation rejects it, then writes a
//! report.json whose sha256 is recomputed independently via certutil.

use std::fs;
use std::path::Path;
use std::process::Command;

use litd_editor::cinematic_authoring::{EditorCinematicTimeline, lower_cinematic_timeline};
use litd_editor::history::{EditorDocument, EditorNamedCamera};
use litd_editor::mcp_cinematic_tools::{
    AUTHOR_TIMELINE_TOOL, EVALUATE_TIMELINE_TOOL, call_mcp_cinematic_tool,
};
use litd_editor::mcp_world_tools::{read_mcp_world_document, sample_issue918_document};
use litd_editor::play::{PlayCameraConfig, PlayCameraRig};
use litd_engine::cinematic_timeline::{
    CinematicFade, CinematicSubtitle, CinematicTransmission, EvaluatedCinematic, FadeKind,
};
use litd_engine::math::Vec3;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Fsv {
    issue: u32,
    document_fingerprint: String,
    baseline_samples: Vec<Sample>,
    edited_samples: Vec<Sample>,
    changed_sample_times: Vec<f32>,
    unchanged_sample_times: Vec<f32>,
    edges: Vec<Value>,
    negative_control: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Sample {
    time_seconds: f32,
    camera_distance: f32,
    fade_alpha: f32,
    transmission: Option<String>,
    subtitle: Option<String>,
    music_cue: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_CINEMATIC_TIMELINE_FSV_ROOT",
        "issue892_cinematic_timeline",
        "manual",
    )?;
    reset(&root);
    let doc = root.join("cine-document.json");
    let ledger = root.join("cine-ledger.json");
    let report_path = root.join("cinematic-timeline-fsv-readback.json");

    author(&doc, &ledger, "author-1", baseline_timeline())?;

    // Independent readback of the persisted source of truth.
    let stored = read_mcp_world_document(&doc)?;
    if !stored
        .document
        .cinematic_timelines
        .contains_key("cine.intro")
    {
        return Err("stored document is missing the authored timeline".into());
    }
    let document_fingerprint = stored.document_fingerprint.clone();

    let times = [0.0f32, 2.0, 6.0, 8.0];
    let baseline_samples = evaluate(&doc, &times)?;
    print_samples("BASELINE", &baseline_samples);

    // Edit ONE keyframe: swap the end camera (t=8) for a camera at a different distance.
    author(&doc, &ledger, "author-2", edited_timeline())?;
    let edited_samples = evaluate(&doc, &times)?;
    print_samples("EDITED", &edited_samples);

    // Prove EXACTLY the affected samples change. camera keys are at 0/4/8, so t=0 and t=2
    // (segment [0,4]) are invariant to the end keyframe; t=6 and t=8 (segment [4,8]) change.
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    for (before, after) in baseline_samples.iter().zip(edited_samples.iter()) {
        if before.camera_distance == after.camera_distance {
            unchanged.push(before.time_seconds);
        } else {
            changed.push(before.time_seconds);
        }
    }
    if changed != vec![6.0, 8.0] || unchanged != vec![0.0, 2.0] {
        return Err(format!(
            "keyframe-edit locality violated: changed={changed:?} unchanged={unchanged:?}"
        )
        .into());
    }
    println!("issue892 keyframe_edit changed={changed:?} unchanged={unchanged:?}");

    let edges = vec![
        edge_zero_length_fade(&doc, &ledger)?,
        edge_overlapping_transmissions(&root)?,
        edge_missing_timeline(&doc)?,
    ];
    let negative_control = negative_control(&stored.document)?;

    let fsv = Fsv {
        issue: 892,
        document_fingerprint,
        baseline_samples,
        edited_samples,
        changed_sample_times: changed,
        unchanged_sample_times: unchanged,
        edges,
        negative_control,
    };
    fs::write(&report_path, serde_json::to_vec_pretty(&fsv)?)?;
    let readback: Fsv = serde_json::from_slice(&fs::read(&report_path)?)?;
    if readback != fsv {
        return Err("report readback mismatch".into());
    }
    let recorded = fsv_run.record_json_file(&report_path)?;
    let certutil = certutil_sha256(&report_path)?;
    if certutil != recorded.sha256 {
        return Err(format!(
            "sha256 mismatch: fsv_run={} certutil={certutil}",
            recorded.sha256
        )
        .into());
    }
    println!(
        "issue892 cinematic_timeline_fsv VERDICT=GREEN report={} report_sha256={} certutil_sha256={}",
        report_path.display(),
        recorded.sha256,
        certutil
    );
    Ok(())
}

fn author(
    doc: &Path,
    ledger: &Path,
    request_id: &str,
    timeline: EditorCinematicTimeline,
) -> Result<(), Box<dyn std::error::Error>> {
    let initial = if doc.exists() {
        Value::Null
    } else {
        json!(base_document())
    };
    call_mcp_cinematic_tool(
        AUTHOR_TIMELINE_TOOL,
        json!({
            "document_path": doc,
            "ledger_path": ledger,
            "ledger_id": "cine-ledger",
            "request_id": request_id,
            "requested_by": "issue892-fsv",
            "initial_document": initial,
            "timeline": timeline,
        }),
    )?;
    Ok(())
}

fn evaluate(doc: &Path, times: &[f32]) -> Result<Vec<Sample>, Box<dyn std::error::Error>> {
    let output = call_mcp_cinematic_tool(
        EVALUATE_TIMELINE_TOOL,
        json!({
            "document_path": doc,
            "timeline_id": "cine.intro",
            "sample_times": times,
        }),
    )?;
    let samples: Vec<EvaluatedCinematic> =
        serde_json::from_value(output.result["samples"].clone())?;
    Ok(samples.into_iter().map(to_sample).collect())
}

fn to_sample(state: EvaluatedCinematic) -> Sample {
    Sample {
        time_seconds: state.time_seconds,
        camera_distance: state.camera.distance,
        fade_alpha: state.fade.alpha,
        transmission: state.active_transmission.map(|t| t.speaker_unit_type),
        subtitle: state.active_subtitle,
        music_cue: state.active_music_cue,
    }
}

fn print_samples(label: &str, samples: &[Sample]) {
    for sample in samples {
        println!(
            "issue892 {label} t={:.1} camera_distance={:.3} fade_alpha={:.3} transmission={:?} subtitle={:?} music={:?}",
            sample.time_seconds,
            sample.camera_distance,
            sample.fade_alpha,
            sample.transmission,
            sample.subtitle,
            sample.music_cue,
        );
    }
}

fn base_document() -> EditorDocument {
    let mut document = sample_issue918_document();
    document
        .upsert_named_camera(EditorNamedCamera::top_down(
            "camera.a",
            "A",
            Vec3::new(1.0, 0.0, 1.0),
        ))
        .unwrap();
    document
        .upsert_named_camera(EditorNamedCamera::top_down(
            "camera.mid",
            "Mid",
            Vec3::new(2.0, 0.0, 2.0),
        ))
        .unwrap();
    document
        .upsert_named_camera(camera_with_distance("camera.b", "B", 12.0))
        .unwrap();
    document
        .upsert_named_camera(camera_with_distance("camera.b2", "B2", 30.0))
        .unwrap();
    document
}

fn camera_with_distance(id: &str, name: &str, distance: f32) -> EditorNamedCamera {
    let mut config = PlayCameraConfig::preset(PlayCameraRig::TopDown);
    config.distance = distance;
    EditorNamedCamera::cinematic(id, name, config, Vec3::new(3.0, 0.0, 3.0))
}

fn baseline_timeline() -> EditorCinematicTimeline {
    intro_timeline("camera.b")
}

fn edited_timeline() -> EditorCinematicTimeline {
    intro_timeline("camera.b2")
}

fn intro_timeline(end_camera: &str) -> EditorCinematicTimeline {
    EditorCinematicTimeline::new("cine.intro", "Intro", 8.0)
        .with_camera_key(0.0, "camera.a")
        .with_camera_key(4.0, "camera.mid")
        .with_camera_key(8.0, end_camera)
        .with_fade(CinematicFade {
            start_seconds: 0.0,
            duration_seconds: 2.0,
            kind: FadeKind::In,
            color: [0.0, 0.0, 0.0, 1.0],
        })
        .with_transmission(CinematicTransmission {
            start_seconds: 5.0,
            duration_seconds: 2.0,
            speaker_unit_type: "unit.hero".to_string(),
            portrait_ref: "portrait.hero".to_string(),
            text: "Hold.".to_string(),
            track: 0,
        })
        .with_subtitle(CinematicSubtitle {
            start_seconds: 5.0,
            duration_seconds: 2.0,
            text: "Hold.".to_string(),
        })
}

fn edge_zero_length_fade(doc: &Path, ledger: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let before = read_mcp_world_document(doc)?.document_fingerprint;
    let mut timeline = baseline_timeline();
    timeline.fades[0].duration_seconds = 0.0;
    let error = author(doc, ledger, "edge-zero-fade", timeline).unwrap_err();
    let after = read_mcp_world_document(doc)?.document_fingerprint;
    if before != after {
        return Err("zero-length fade edge mutated the document".into());
    }
    Ok(json!({
        "name": "zero_length_fade_rejected",
        "before_fingerprint": before,
        "after_fingerprint": after,
        "error": error.to_string(),
    }))
}

fn edge_overlapping_transmissions(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    // Two transmissions overlap at t=3; the documented rule is highest-track-wins.
    let doc = root.join("overlap-document.json");
    let ledger = root.join("overlap-ledger.json");
    let timeline = EditorCinematicTimeline::new("cine.overlap", "Overlap", 6.0)
        .with_camera_key(0.0, "camera.a")
        .with_transmission(CinematicTransmission {
            start_seconds: 2.0,
            duration_seconds: 3.0,
            speaker_unit_type: "unit.low".to_string(),
            portrait_ref: "portrait.low".to_string(),
            text: "low".to_string(),
            track: 0,
        })
        .with_transmission(CinematicTransmission {
            start_seconds: 2.0,
            duration_seconds: 3.0,
            speaker_unit_type: "unit.high".to_string(),
            portrait_ref: "portrait.high".to_string(),
            text: "high".to_string(),
            track: 1,
        });
    call_mcp_cinematic_tool(
        AUTHOR_TIMELINE_TOOL,
        json!({
            "document_path": doc,
            "ledger_path": ledger,
            "ledger_id": "overlap-ledger",
            "request_id": "overlap-1",
            "requested_by": "issue892-fsv",
            "initial_document": base_document(),
            "timeline": timeline,
        }),
    )?;
    let output = call_mcp_cinematic_tool(
        EVALUATE_TIMELINE_TOOL,
        json!({ "document_path": doc, "timeline_id": "cine.overlap", "sample_times": [3.0] }),
    )?;
    let samples: Vec<EvaluatedCinematic> =
        serde_json::from_value(output.result["samples"].clone())?;
    let winner = samples[0]
        .active_transmission
        .as_ref()
        .map(|t| t.speaker_unit_type.clone());
    if winner.as_deref() != Some("unit.high") {
        return Err(format!("overlap rule violated: winner={winner:?}").into());
    }
    Ok(json!({
        "name": "overlapping_transmissions_highest_track_wins",
        "at_time": 3.0,
        "winner": winner,
    }))
}

fn edge_missing_timeline(doc: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let error = call_mcp_cinematic_tool(
        EVALUATE_TIMELINE_TOOL,
        json!({ "document_path": doc, "timeline_id": "cine.ghost", "sample_times": [0.0] }),
    )
    .unwrap_err();
    Ok(json!({ "name": "missing_timeline_fails_loud", "error": error.to_string() }))
}

fn negative_control(document: &EditorDocument) -> Result<Value, Box<dyn std::error::Error>> {
    // Lower the REAL authored timeline (validates), then sabotage the frozen lowered
    // artifact and prove the engine validator rejects it. A pass-through validator would
    // make this control silently succeed.
    let timeline = document
        .cinematic_timelines
        .get("cine.intro")
        .ok_or("missing timeline for negative control")?;
    let good = lower_cinematic_timeline(document, timeline)?;
    good.validate()
        .map_err(|error| format!("healthy lowered timeline unexpectedly rejected: {error}"))?;
    let mut sabotaged = good.clone();
    sabotaged.camera_track[1].time_seconds = sabotaged.camera_track[0].time_seconds;
    let rejection = sabotaged
        .validate()
        .err()
        .ok_or("sabotaged timeline passed validation (negative control failed)")?;
    Ok(json!({
        "name": "sabotaged_lowered_timeline_rejected",
        "healthy_fingerprint": litd_editor::history::stable_fingerprint(&good)?,
        "sabotage": "collapsed camera keyframe ordering",
        "rejection_code": rejection.code(),
        "rejection": rejection.to_string(),
    }))
}

fn certutil_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("certutil")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .output()?;
    if !output.status.success() {
        return Err("certutil failed".into());
    }
    let text = String::from_utf8(output.stdout)?;
    let hash = text
        .lines()
        .map(str::trim)
        .find(|line| line.len() == 64 && line.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or("certutil produced no sha256 line")?;
    Ok(hash.to_lowercase())
}

fn reset(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
