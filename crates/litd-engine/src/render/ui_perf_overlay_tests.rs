use std::fs;

use serde_json::{Value, json};

use crate::{
    fsv_artifacts::begin_issue_run,
    profile::{FrameProfile, FrameProfiler, ProfileSystem},
    render::ui::{
        InputLatencyRecord, PerformanceOverlayImage, UiRenderSettings,
        build_performance_overlay_plan, write_performance_overlay_artifacts,
    },
};

#[test]
fn issue1339_perf_overlay_renders_bmp_and_exports_profile_json() {
    let run = begin_issue_run("issue1339_perf_overlay").unwrap();
    let root = run.run_dir();
    let bmp_path = root.join("perf-overlay.bmp");
    let export_path = root.join("perf-profile-export.json");
    let report_path = run.report_path().to_path_buf();
    let settings = UiRenderSettings {
        viewport_width: 640.0,
        viewport_height: 160.0,
    };
    let profile = profile_with_frame_time(77, 16_666_667);
    let latency = latency(12_000_000, 24_000_000);

    let happy_before = json!({
        "frame_time_ns": profile.frame_time_ns,
        "cpu_spans": profile.cpu_spans.len(),
        "gpu_passes": profile.gpu_passes.len(),
        "latency_input_ns": latency.input_ns,
        "latency_reaction_ns": latency.reaction_ns,
    });
    let plan = build_performance_overlay_plan(settings, &profile, &latency).unwrap();
    let readback = write_performance_overlay_artifacts(&plan, &bmp_path, &export_path).unwrap();
    let bmp_bytes = fs::read(&bmp_path).unwrap();
    let image = PerformanceOverlayImage::from_bmp_bytes(&bmp_bytes).unwrap();
    let exported: FrameProfile = serde_json::from_slice(&fs::read(&export_path).unwrap()).unwrap();
    assert_eq!(exported, profile);
    let bar_readback = bar_widths_from_bmp(&image, &plan);
    assert_eq!(bar_readback["frame"], plan.bars[0].filled_px);
    assert_eq!(bar_readback["input_latency"], plan.bars[1].filled_px);

    let spike_edge = spike_frame_edge(settings);
    let zero_latency_edge = zero_latency_edge(settings, &profile);
    let missing_data_edge = missing_data_edge(settings, &latency);
    let invalid_latency_edge = invalid_latency_edge(settings, &profile);

    let report = json!({
        "schema_version": 1,
        "issue": 1339,
        "source_of_truth": {
            "bmp_path": bmp_path,
            "bmp_bytes": readback.bmp_bytes,
            "bmp_sha256": readback.bmp_sha256,
            "export_path": export_path,
            "export_bytes": readback.export.bytes,
            "export_sha256": readback.export.sha256,
            "report_path": report_path,
        },
        "happy_path": {
            "before": happy_before,
            "after": {
                "frame_time_ns": plan.frame_time_ns,
                "input_latency_ns": plan.input_latency_ns,
                "over_100ms": plan.over_100ms,
                "latency_over_50ms": plan.latency_over_50ms,
                "text": plan.text_runs.iter().map(|run| run.text.clone()).collect::<Vec<_>>(),
                "bars": plan.bars,
                "bmp_bar_readback": bar_readback,
                "exported_frame_time_ns": exported.frame_time_ns,
                "exported_cpu_spans": exported.cpu_spans.len(),
                "exported_gpu_passes": exported.gpu_passes.len(),
            }
        },
        "edges": {
            "spike_frame": spike_edge,
            "zero_latency": zero_latency_edge,
            "missing_data": missing_data_edge,
            "invalid_latency": invalid_latency_edge,
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_bytes = report_readback.bytes;
    let report_json = report_readback.json;
    assert_eq!(report_json["issue"], 1339);
    assert_eq!(
        report_json["happy_path"]["after"]["frame_time_ns"],
        16_666_667
    );
    assert_eq!(
        report_json["edges"]["spike_frame"]["after"]["over_100ms"],
        true
    );
    assert_eq!(
        report_json["edges"]["zero_latency"]["after"]["input_latency_ns"],
        0
    );
    assert_eq!(
        report_json["edges"]["missing_data"]["after"]["error_code"],
        "LITD-RENDER-UI-INVALID-CONFIG"
    );
    println!(
        "ISSUE1339_FSV_REPORT_READBACK path={} bytes={} sha256={} bmp_sha256={} export_sha256={} frame_bar_px={} latency_bar_px={}",
        report_path.display(),
        report_bytes.len(),
        report_readback.sha256,
        readback.bmp_sha256,
        readback.export.sha256,
        bar_readback["frame"],
        bar_readback["input_latency"],
    );
}

fn spike_frame_edge(settings: UiRenderSettings) -> Value {
    let profile = profile_with_frame_time(200, 120_000_000);
    let latency = latency(1_000_000, 20_000_000);
    let before = json!({ "frame_time_ns": profile.frame_time_ns });
    let plan = build_performance_overlay_plan(settings, &profile, &latency).unwrap();
    json!({
        "before": before,
        "after": {
            "over_100ms": plan.over_100ms,
            "frame_bar_px": plan.bars[0].filled_px,
            "frame_bar_max_px": plan.bars[0].max_px,
            "frame_bar_over_budget": plan.bars[0].over_budget,
        }
    })
}

fn zero_latency_edge(settings: UiRenderSettings, profile: &FrameProfile) -> Value {
    let latency = latency(42_000_000, 42_000_000);
    let before = json!({
        "input_ns": latency.input_ns,
        "reaction_ns": latency.reaction_ns,
    });
    let plan = build_performance_overlay_plan(settings, profile, &latency).unwrap();
    json!({
        "before": before,
        "after": {
            "input_latency_ns": plan.input_latency_ns,
            "latency_bar_px": plan.bars[1].filled_px,
            "latency_over_50ms": plan.latency_over_50ms,
        }
    })
}

fn missing_data_edge(settings: UiRenderSettings, latency: &InputLatencyRecord) -> Value {
    let mut profiler = FrameProfiler::disabled();
    profiler.begin_frame_at(300, 0).unwrap();
    let profile = profiler.finish_frame_at(16_000_000).unwrap();
    let before = json!({
        "enabled": profile.enabled,
        "cpu_spans": profile.cpu_spans.len(),
        "systems_present": profile.systems_present.len(),
    });
    let error = build_performance_overlay_plan(settings, &profile, latency).unwrap_err();
    json!({
        "before": before,
        "after": {
            "error_code": error.code(),
            "error_message": error.to_string(),
        }
    })
}

fn invalid_latency_edge(settings: UiRenderSettings, profile: &FrameProfile) -> Value {
    let latency = latency(100, 99);
    let before = json!({
        "input_ns": latency.input_ns,
        "reaction_ns": latency.reaction_ns,
    });
    let error = build_performance_overlay_plan(settings, profile, &latency).unwrap_err();
    json!({
        "before": before,
        "after": {
            "error_code": error.code(),
            "error_message": error.to_string(),
        }
    })
}

fn bar_widths_from_bmp(
    image: &PerformanceOverlayImage,
    plan: &crate::render::ui::PerformanceOverlayPlan,
) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for (index, bar) in plan.bars.iter().enumerate() {
        let y = 28 + index as u32 * 32;
        let mut count = 0_u32;
        for x in 64..64 + bar.max_px {
            if image.pixels()[(y * image.width + x) as usize] == bar.color_rgba {
                count += 1;
            } else {
                break;
            }
        }
        out.insert(bar.id.clone(), json!(count));
    }
    out
}

fn profile_with_frame_time(frame_index: u64, frame_time_ns: u64) -> FrameProfile {
    let mut profiler = FrameProfiler::enabled();
    profiler.begin_frame_at(frame_index, 0).unwrap();
    let sim = frame_time_ns / 5;
    let nav = frame_time_ns / 10;
    let ai = frame_time_ns / 10;
    let render = frame_time_ns / 2;
    let stream = frame_time_ns - sim - nav - ai - render;
    let mut start = 0;
    for (system, name, duration) in [
        (ProfileSystem::Sim, "sim", sim),
        (ProfileSystem::Nav, "nav", nav),
        (ProfileSystem::Ai, "ai", ai),
        (ProfileSystem::Render, "render", render),
        (ProfileSystem::Stream, "stream", stream),
    ] {
        let span = profiler
            .begin_cpu_span_at(system, name, start)
            .unwrap()
            .unwrap();
        start += duration;
        profiler.end_cpu_span_at(span, start).unwrap();
    }
    profiler
        .record_gpu_pass_ticks("main-ui-pass", 1000, 1500, 2000)
        .unwrap();
    profiler.finish_frame_at(frame_time_ns).unwrap()
}

fn latency(input_ns: u64, reaction_ns: u64) -> InputLatencyRecord {
    InputLatencyRecord {
        action_id: "editor.confirm".to_string(),
        input_frame: 76,
        reaction_frame: 77,
        input_ns,
        reaction_ns,
    }
}
