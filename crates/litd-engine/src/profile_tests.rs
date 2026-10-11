use std::fs;

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    fsv_artifacts::begin_issue_run,
    profile::{FrameProfile, FrameProfiler, ProfileSystem, write_profile_json},
};

#[test]
fn issue1338_frame_profiler_captures_spans_and_writes_source_truth() {
    let run = begin_issue_run("issue1338_frame_profiler").unwrap();
    let root = run.run_dir();
    let profile_path = root.join("frame-profile.json");
    let report_path = run.report_path().to_path_buf();

    let mut profiler = FrameProfiler::enabled();
    profiler.begin_frame_at(77, 0).unwrap();
    let happy_before = profiler_state(&profiler);

    sequential_span(&mut profiler, ProfileSystem::Sim, "fixed-sim", 0, 2_000_000);
    sequential_span(
        &mut profiler,
        ProfileSystem::Nav,
        "navmesh-query",
        2_000_000,
        3_000_000,
    );
    sequential_span(
        &mut profiler,
        ProfileSystem::Ai,
        "behavior-tree",
        3_000_000,
        4_000_000,
    );
    sequential_span(
        &mut profiler,
        ProfileSystem::Render,
        "render-graph",
        4_000_000,
        14_000_000,
    );
    sequential_span(
        &mut profiler,
        ProfileSystem::Stream,
        "asset-stream",
        14_000_000,
        16_666_667,
    );
    profiler
        .record_gpu_pass_ticks("shadow-pass", 1000, 1500, 2000)
        .unwrap();
    profiler
        .record_gpu_pass_ticks("post-pass", 2000, 2600, 2000)
        .unwrap();

    let profile = profiler.finish_frame_at(16_666_667).unwrap();
    assert_happy_profile(&profile);
    let profile_readback = write_profile_json(&profile_path, &profile).unwrap();
    let profile_file = fs::read(&profile_path).unwrap();
    let profile_from_disk: FrameProfile = serde_json::from_slice(&profile_file).unwrap();
    assert_eq!(profile_from_disk, profile);
    assert_eq!(profile_readback.profile, profile);
    assert_eq!(profile_readback.sha256, sha256_hex(&profile_file));

    let zero_edge = zero_work_edge();
    let nested_edge = nested_spans_edge();
    let disabled_edge = disabled_edge();
    let unclosed_edge = unclosed_span_edge();

    let report = json!({
        "schema_version": 1,
        "issue": 1338,
        "source_of_truth": {
            "profile_path": profile_path,
            "profile_bytes": profile_readback.bytes,
            "profile_sha256": profile_readback.sha256,
            "report_path": report_path,
        },
        "happy_path": {
            "before": happy_before,
            "after": {
                "frame_index": profile.frame_index,
                "frame_time_ns": profile.frame_time_ns,
                "cpu_spans": profile.cpu_spans.len(),
                "gpu_passes": profile.gpu_passes.len(),
                "cpu_top_level_ns": profile.cpu_top_level_ns,
                "cpu_span_total_ns": profile.cpu_span_total_ns,
                "gpu_total_ns": profile.gpu_total_ns,
                "systems_present": profile.systems_present,
            }
        },
        "edges": {
            "zero_work_system": zero_edge,
            "nested_spans": nested_edge,
            "disabled_capture": disabled_edge,
            "unclosed_span_error": unclosed_edge,
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_json = report_readback.json;
    assert_eq!(report_json["issue"], 1338);
    assert_eq!(report_json["happy_path"]["after"]["cpu_spans"], 5);
    assert_eq!(
        report_json["edges"]["zero_work_system"]["after"]["duration_ns"],
        0
    );
    assert_eq!(
        report_json["edges"]["unclosed_span_error"]["after"]["error_code"],
        "LITD-PROFILE-INVALID-OPERATION"
    );
    println!(
        "ISSUE1338_FSV_REPORT_READBACK path={} bytes={} sha256={} frame_ns={} cpu_top_level_ns={} gpu_total_ns={}",
        report_path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        profile.frame_time_ns,
        profile.cpu_top_level_ns,
        profile.gpu_total_ns
    );
}

fn sequential_span(
    profiler: &mut FrameProfiler,
    system: ProfileSystem,
    name: &str,
    start_ns: u64,
    end_ns: u64,
) {
    let span = profiler
        .begin_cpu_span_at(system, name, start_ns)
        .unwrap()
        .unwrap();
    profiler.end_cpu_span_at(span, end_ns).unwrap();
}

fn assert_happy_profile(profile: &FrameProfile) {
    assert!(profile.enabled);
    assert_eq!(profile.frame_time_ns, 16_666_667);
    assert_eq!(profile.cpu_spans.len(), 5);
    assert_eq!(profile.gpu_passes.len(), 2);
    assert_eq!(profile.cpu_top_level_ns, profile.frame_time_ns);
    assert_eq!(profile.cpu_span_total_ns, profile.frame_time_ns);
    assert_eq!(profile.gpu_total_ns, 2200);
    assert_eq!(
        profile.systems_present,
        vec![
            ProfileSystem::Sim,
            ProfileSystem::Nav,
            ProfileSystem::Ai,
            ProfileSystem::Render,
            ProfileSystem::Stream,
        ]
    );
    for span in &profile.cpu_spans {
        assert!(span.end_ns >= span.start_ns);
        assert_eq!(span.duration_ns, span.end_ns - span.start_ns);
    }
    assert_eq!(profile.gpu_passes[0].duration_ns, 1000);
    assert_eq!(profile.gpu_passes[1].duration_ns, 1200);
}

fn zero_work_edge() -> Value {
    let mut profiler = FrameProfiler::enabled();
    profiler.begin_frame_at(100, 5).unwrap();
    let before = profiler_state(&profiler);
    let span = profiler
        .begin_cpu_span_at(ProfileSystem::Stream, "idle-stream", 5)
        .unwrap()
        .unwrap();
    profiler.end_cpu_span_at(span, 5).unwrap();
    let profile = profiler.finish_frame_at(5).unwrap();
    json!({
        "before": before,
        "after": {
            "duration_ns": profile.cpu_spans[0].duration_ns,
            "frame_time_ns": profile.frame_time_ns,
            "cpu_top_level_ns": profile.cpu_top_level_ns,
        }
    })
}

fn nested_spans_edge() -> Value {
    let mut profiler = FrameProfiler::enabled();
    profiler.begin_frame_at(101, 100).unwrap();
    let parent = profiler
        .begin_cpu_span_at(ProfileSystem::Ai, "ai-parent", 100)
        .unwrap()
        .unwrap();
    let child = profiler
        .begin_cpu_span_at(ProfileSystem::Ai, "ai-child", 110)
        .unwrap()
        .unwrap();
    let before = profiler_state(&profiler);
    profiler.end_cpu_span_at(child, 130).unwrap();
    profiler.end_cpu_span_at(parent, 200).unwrap();
    let profile = profiler.finish_frame_at(200).unwrap();
    let child_span = profile
        .cpu_spans
        .iter()
        .find(|span| span.name == "ai-child")
        .unwrap();
    json!({
        "before": before,
        "after": {
            "span_count": profile.cpu_spans.len(),
            "child_depth": child_span.depth,
            "child_parent_id": child_span.parent_id,
            "cpu_top_level_ns": profile.cpu_top_level_ns,
            "cpu_span_total_ns": profile.cpu_span_total_ns,
        }
    })
}

fn disabled_edge() -> Value {
    let mut profiler = FrameProfiler::disabled();
    profiler.begin_frame_at(102, 500).unwrap();
    let before = profiler_state(&profiler);
    let span = profiler
        .begin_cpu_span_at(ProfileSystem::Render, "ignored-render", 500)
        .unwrap();
    profiler
        .record_gpu_pass_ticks("ignored-gpu-pass", 1, 2, 1000)
        .unwrap();
    let profile = profiler.finish_frame_at(700).unwrap();
    json!({
        "before": before,
        "after": {
            "span_id": span.map(|id| id.0),
            "enabled": profile.enabled,
            "cpu_spans": profile.cpu_spans.len(),
            "gpu_passes": profile.gpu_passes.len(),
            "frame_time_ns": profile.frame_time_ns,
        }
    })
}

fn unclosed_span_edge() -> Value {
    let mut profiler = FrameProfiler::enabled();
    profiler.begin_frame_at(103, 0).unwrap();
    let _span = profiler
        .begin_cpu_span_at(ProfileSystem::Sim, "left-open", 0)
        .unwrap()
        .unwrap();
    let before = profiler_state(&profiler);
    let error = profiler.finish_frame_at(10).unwrap_err();
    json!({
        "before": before,
        "after": {
            "error_code": error.code(),
            "error_message": error.to_string(),
        }
    })
}

fn profiler_state(profiler: &FrameProfiler) -> Value {
    json!({
        "started": profiler.is_started(),
        "active_spans": profiler.active_span_count(),
        "completed_spans": profiler.completed_span_count(),
        "gpu_passes": profiler.gpu_pass_count(),
    })
}
