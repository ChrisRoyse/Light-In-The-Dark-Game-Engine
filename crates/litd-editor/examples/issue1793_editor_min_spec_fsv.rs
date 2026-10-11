//! FSV harness for #1793: headless editor min-spec run on a forced adapter with
//! forced low-tier degrade settings. Measures cold-start time, startup working-set
//! RAM, derived VRAM footprint, and interactive frame cost while driving the real
//! editor-world renderer (the representative terrain map) across an edit-navigation
//! loop. Persists the degrade knobs through the REAL Calyx Aster store, gates every
//! measured number against its fail-loud budget ceiling, and writes a verdict
//! artifact under target/fsv/. The RTX 5060 box forces the integrated adapter when
//! one is present, else documents its absence and runs discrete-preferred.
//!
//! Run: cargo run -p litd-editor --example issue1793_editor_min_spec_fsv --features fsv

use std::time::Instant;

use litd_editor::editor_min_spec_store::EditorMinSpecStore;
use litd_engine::{
    EngineError,
    editor_min_spec::{
        AdapterClass, EditorDegradeSettings, EditorMinSpecBudget, EditorMinSpecCeilings,
        process_working_set_bytes,
    },
    fsv_artifacts::begin_issue_run,
    gpu::{AdapterPreference, GpuContext, enumerate_adapters, select_adapter_index},
    math::Vec3,
    render::editor_world::{
        EditorViewportRect, EditorWorldCameraState, EditorWorldLayerVisibility,
        EditorWorldRenderer, PrepareFrameMetricsArgs,
    },
};
use serde_json::{Value, json};

const SLUG: &str = "issue1793_editor_min_spec";
const BASE_WIDTH: u32 = 1600;
const BASE_HEIGHT: u32 = 900;
const SURFACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const INTERACTIVE_FRAMES: u32 = 32;
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.02,
    g: 0.03,
    b: 0.04,
    a: 1.0,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let readback = pollster::block_on(run())?;
    println!(
        "wrote #1793 editor min-spec verdict to {}",
        readback.path.display()
    );
    Ok(())
}

async fn run() -> Result<litd_engine::fsv_artifacts::FsvReportReadback, EngineError> {
    // Force the integrated adapter when present (RTX 5060 box has one); else document.
    let adapters = enumerate_adapters();
    let integrated_present = adapters
        .iter()
        .any(|report| report.device_type == "IntegratedGpu");
    let preference = if integrated_present {
        AdapterPreference::ForceIntegrated
    } else {
        AdapterPreference::DiscretePreferred
    };

    // Cold start: adapter+device acquisition, renderer/pipeline build, first frame.
    let degrade_placeholder = EditorDegradeSettings::forced_low_tier(AdapterClass::Unknown);
    let (width, height) = degrade_placeholder.scaled_dimensions(BASE_WIDTH, BASE_HEIGHT)?;
    let cold_start = Instant::now();
    let context = GpuContext::new_headless_with_preference(preference).await?;
    let adapter_class = AdapterClass::from_report(context.adapter_report());
    let degrade = EditorDegradeSettings::forced_low_tier(adapter_class);
    let mut renderer = EditorWorldRenderer::new(
        context.device(),
        context.queue(),
        SURFACE_FORMAT,
        width,
        height,
    )?;
    let target = offscreen_target(context.device(), width, height);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let viewport = EditorViewportRect::new(0, 0, width, height)?;
    let layers = EditorWorldLayerVisibility::default();
    render_one_frame(
        &context,
        &mut renderer,
        &target_view,
        viewport,
        camera_at(0),
        layers,
    )?;
    let cold_start_ns = cold_start.elapsed().as_nanos();
    let startup_working_set_bytes = process_working_set_bytes()?;

    // Interactive edit-navigation loop: pan/dolly the camera each frame (a full
    // redraw, the dominant cost of an active terrain-edit session) and time the
    // real GPU frame.
    let mut frame_ns: Vec<u128> = Vec::with_capacity(INTERACTIVE_FRAMES as usize);
    for i in 1..=INTERACTIVE_FRAMES {
        let started = Instant::now();
        render_one_frame(
            &context,
            &mut renderer,
            &target_view,
            viewport,
            camera_at(i),
            layers,
        )?;
        frame_ns.push(started.elapsed().as_nanos());
    }
    let interactive_frame_ns_p50 = median(&frame_ns);
    let interactive_frame_ns_max = *frame_ns.iter().max().expect("frames measured");

    // Structural terrain edit apply: rebuilding scene buffers + pipelines is the
    // cost of a topology-changing edit. Measure one apply on the warm device.
    let edit_start = Instant::now();
    let _rebuilt = EditorWorldRenderer::new(
        context.device(),
        context.queue(),
        SURFACE_FORMAT,
        width,
        height,
    )?;
    let structural_edit_apply_ns = edit_start.elapsed().as_nanos();

    // Persist the degrade knobs through the REAL Aster store (no side store, #1348).
    let run = begin_issue_run(SLUG).map_err(fsv_err)?;
    let aster_dir = run.artifact_path("min-spec-aster").map_err(fsv_err)?;
    let store = EditorMinSpecStore::new(aster_dir);
    let persisted = store.save_degrade_settings(&degrade)?;
    let loaded = store.load_degrade_settings()?.ok_or_else(|| {
        render_err(
            "persist.readback",
            "degrade settings missing after Aster put",
        )
    })?;
    if loaded != degrade {
        return Err(render_err(
            "persist.readback",
            "loaded degrade settings differ from the persisted settings",
        ));
    }

    let budget = EditorMinSpecBudget {
        adapter_class,
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        forced_preference: preference.label().to_string(),
        degrade,
        base_surface_width: BASE_WIDTH,
        base_surface_height: BASE_HEIGHT,
        internal_render_width: width,
        internal_render_height: height,
        cold_start_ns,
        startup_working_set_bytes,
        derived_vram_footprint_bytes: degrade
            .derived_vram_footprint_bytes(BASE_WIDTH, BASE_HEIGHT)?,
        interactive_frame_ns_p50,
        interactive_frame_ns_max,
        structural_edit_apply_ns,
        frames_measured: INTERACTIVE_FRAMES,
        ceilings: EditorMinSpecCeilings::editor_min_spec(),
    };
    // The pass/fail gate: every measured number within its fail-loud ceiling.
    budget.validate_within_budget()?;

    let report = json!({
        "schema": "litd.editor.issue1793.editor_min_spec.v1",
        "schema_version": 1,
        "issue": 1793,
        "parent_issue": 1790,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "aster_vault_dir": persisted.vault_dir.display().to_string(),
            "manual_gate": "headless GPU + Aster evidence; live Synapse min-spec editor FSV per #800 still required"
        },
        "research_basis": [
            "wgpu enumerate_adapters gives an explicit, inspectable adapter list; PowerPreference is opaque and can silently deprioritize an integrated GPU",
            "wgpu PowerPreference::LowPower/HighPerformance map to integrated/discrete but expose no chosen/rejected record",
            "community note: preferring integrated when the discrete GPU drives the display can hurt -- hence discrete-preferred default, integrated forced only for min-spec validation"
        ],
        "adapter_selection": {
            "integrated_adapter_present": integrated_present,
            "forced_preference": preference.label(),
            "enumerated_adapters": adapters.iter().map(|r| json!({
                "name": r.name, "device_type": r.device_type, "backend": r.backend
            })).collect::<Vec<_>>(),
            "note": if integrated_present {
                "integrated adapter present: forced the integrated path in reality"
            } else {
                "no integrated adapter enumerated on this box: documented, ran discrete-preferred (discrete is a preference, never a requirement)"
            }
        },
        "wgsl_validation": "editor_world.wgsl validated by REAL wgpu pipeline creation (naga); an invalid shader would fail EditorWorldRenderer::new. No substring checks.",
        "happy_path": {
            "forced_low_tier_degrade": degrade,
            "budget": &budget,
            "budget_within_ceilings": true,
            "aster_persist": {
                "vault_dir": persisted.vault_dir.display().to_string(),
                "sha256": persisted.sha256,
                "seq": persisted.seq,
                "graph_rows": persisted.graph_rows,
                "reloaded_equals_persisted": true
            },
            "budget_numbers": {
                "cold_start_ms": ms(cold_start_ns),
                "startup_working_set_mib": mib(startup_working_set_bytes),
                "derived_vram_footprint_mib": mib(budget.derived_vram_footprint_bytes),
                "interactive_frame_p50_ms": ms(interactive_frame_ns_p50),
                "interactive_frame_max_ms": ms(interactive_frame_ns_max),
                "interactive_fps_p50": if interactive_frame_ns_p50 > 0 {
                    1_000_000_000.0 / interactive_frame_ns_p50 as f64
                } else { 0.0 },
                "structural_edit_apply_ms": ms(structural_edit_apply_ns)
            }
        },
        "edges": edge_proofs()?,
    });
    run.write_report(&report).map_err(fsv_err)
}

fn edge_proofs() -> Result<Value, EngineError> {
    Ok(json!({
        "invalid_viewport_scale": invalid_viewport_scale_edge(),
        "non_power_of_two_atlas": non_po2_atlas_edge(),
        "force_integrated_absent": force_integrated_absent_edge(),
    }))
}

fn invalid_viewport_scale_edge() -> Value {
    let bad = EditorDegradeSettings {
        adapter_class: AdapterClass::Integrated,
        shadow_atlas_size: 1024,
        viewport_resolution_scale_percent: 150,
    };
    let error = bad.validate().unwrap_err();
    json!({ "before": bad, "after": engine_error_report(&error) })
}

fn non_po2_atlas_edge() -> Value {
    let bad = EditorDegradeSettings {
        adapter_class: AdapterClass::Integrated,
        shadow_atlas_size: 1000,
        viewport_resolution_scale_percent: 75,
    };
    let error = bad.validate().unwrap_err();
    json!({ "before": bad, "after": engine_error_report(&error) })
}

fn force_integrated_absent_edge() -> Value {
    let discrete_only = [wgpu::AdapterInfo {
        name: "RTX 5060 Laptop GPU".to_string(),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::DiscreteGpu,
        driver: String::new(),
        driver_info: String::new(),
        backend: wgpu::Backend::Vulkan,
    }];
    let error = select_adapter_index(AdapterPreference::ForceIntegrated, &discrete_only)
        .expect_err("force-integrated over a discrete-only list must fail loud");
    json!({
        "before": { "enumerated": ["RTX 5060 Laptop GPU (DiscreteGpu/Vulkan)"], "preference": "force-integrated" },
        "after": engine_error_report(&error)
    })
}

fn camera_at(frame: u32) -> EditorWorldCameraState {
    EditorWorldCameraState {
        target_world: Vec3::ZERO,
        distance: 140.0 + (frame % 8) as f32 * 6.0,
        yaw_degrees: 30.0 + frame as f32 * 4.0,
        pitch_degrees: -35.0,
    }
}

fn render_one_frame(
    context: &GpuContext,
    renderer: &mut EditorWorldRenderer,
    target_view: &wgpu::TextureView,
    viewport: EditorViewportRect,
    camera: EditorWorldCameraState,
    layers: EditorWorldLayerVisibility,
) -> Result<(), EngineError> {
    renderer.prepare_frame_metrics(PrepareFrameMetricsArgs {
        device: context.device(),
        queue: context.queue(),
        surface_width: viewport.width,
        surface_height: viewport.height,
        viewport,
        camera,
        layers,
    })?;
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("issue1793-min-spec-frame"),
        });
    renderer.render(&mut encoder, target_view, viewport, CLEAR, layers);
    context.queue().submit([encoder.finish()]);
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| render_err("frame.poll", format!("device poll failed: {error}")))?;
    Ok(())
}

fn offscreen_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("issue1793-min-spec-target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SURFACE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn median(values: &[u128]) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn ms(ns: u128) -> f64 {
    ns as f64 / 1_000_000.0
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn engine_error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "error_code": report.code,
        "message": report.message,
        "remediation": report.remediation,
        "source_error": report.source_error,
    })
}

fn render_err(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("issue1793.{field}"),
        reason: reason.into(),
    }
}

fn fsv_err(error: std::io::Error) -> EngineError {
    EngineError::SerializationFailed {
        format: "issue1793-editor-min-spec",
        reason: error.to_string(),
    }
}
