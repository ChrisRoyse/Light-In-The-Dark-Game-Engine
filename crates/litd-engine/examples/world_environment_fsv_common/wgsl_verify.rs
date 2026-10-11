//! Genuine headless verification of `world_environment.wgsl` (shared by the
//! #808/#814/#815/#816 FSV harnesses). NO substring checks on the source, and NO
//! Rust-transcription-vs-Rust "crosscheck" (a comparison that cannot fail is a
//! defect). Two independent proofs run here:
//!
//!   1. naga IR checks -- the shader is parsed + validated through naga (wgpu's own
//!      frontend); structural facts (entry points, the `env` uniform, the math
//!      helpers) are read from the validated module IR; two negative controls prove
//!      the naga gate rejects a syntax-broken and a type-broken shader.
//!   2. REAL GPU execution -- the actual WGSL `env_fog_factor` / `env_water_tint`
//!      functions are run on this box's GPU via a wgpu compute dispatch (the shader
//!      source is used verbatim; only a thin compute entry point is appended to call
//!      the real helpers), the results are mapped back, and compared against the CPU
//!      oracle (`uniform::cpu_fog_factor` / `cpu_water_tint`) over a grid of inputs.
//!      The GPU is the separate source of truth; the CPU mirror is the oracle. If no
//!      GPU is present the harness fails loud (no software fallback).
//!
//! What this PROVES: the shader parses + type-checks, exposes the `env` uniform, and
//! its fog/water math -- as compiled and executed by a real GPU -- equals the CPU
//! consumer's within a tight float tolerance. What it does NOT prove: rasterized
//! pixels through the vertex/fragment path (that is a live-editor / T4 claim).

use litd_engine::{
    gpu::GpuContext,
    world_environment::{
        WORLD_ENVIRONMENT_WGSL,
        uniform::{EnvironmentUniform, cpu_fog_factor, cpu_water_tint},
    },
};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use serde_json::{Value, json};
use wgpu::util::DeviceExt;

type Err = Box<dyn std::error::Error>;

/// Max absolute delta permitted between the GPU-executed shader math and the CPU
/// oracle. GPU/CPU transcendentals (`exp`) and fused multiply-add are not bit-for-bit
/// identical by any spec, so an exact `== 0.0` comparison would be wrong; this bound
/// is ~1e3x looser than the observed delta (recorded in the report) yet ~1e4x tighter
/// than any real divergence, so a genuine math regression still fails the gate.
const MATH_TOLERANCE: f32 = 1.0e-4;
/// Number of input samples evaluated on the GPU for each of fog and water.
const SAMPLE_COUNT: u32 = 128;

fn parse_and_validate(source: &str) -> Result<naga::Module, Err> {
    let module = naga::front::wgsl::parse_str(source).map_err(|e| {
        format!(
            "world_environment WGSL failed to parse: {}",
            e.emit_to_string(source)
        )
    })?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| format!("world_environment WGSL failed naga validation: {e:?}"))?;
    Ok(module)
}

/// Validate the shader (naga IR) and execute its math on a real GPU, cross-checking
/// against the CPU oracle. Returns the FSV evidence object; any inconsistency returns
/// an error so the harness aborts before writing a verdict.
pub fn verify_world_environment_wgsl() -> Result<Value, Err> {
    let module = parse_and_validate(WORLD_ENVIRONMENT_WGSL)?;

    let has_vertex = module
        .entry_points
        .iter()
        .any(|e| e.stage == naga::ShaderStage::Vertex);
    let has_fragment = module
        .entry_points
        .iter()
        .any(|e| e.stage == naga::ShaderStage::Fragment);
    if !has_vertex || !has_fragment {
        return Err("world_environment WGSL missing vertex/fragment entry point".into());
    }
    // The environment uniform global must exist and be Uniform address space.
    let env_uniform = module
        .global_variables
        .iter()
        .any(|(_, g)| g.name.as_deref() == Some("env") && g.space == naga::AddressSpace::Uniform);
    if !env_uniform {
        return Err("world_environment WGSL missing `env` uniform global".into());
    }
    let fn_exists = |name: &str| {
        module
            .functions
            .iter()
            .any(|(_, f)| f.name.as_deref() == Some(name))
    };
    for helper in ["env_fog_factor", "env_water_tint", "env_apply"] {
        if !fn_exists(helper) {
            return Err(format!("world_environment WGSL missing helper `{helper}`").into());
        }
    }

    let negatives = negative_controls()?;
    let gpu = pollster::block_on(gpu_math_crosscheck())?;

    Ok(json!({
        "naga_parse_ok": true,
        "naga_validate_ok": true,
        "has_vertex_entry": has_vertex,
        "has_fragment_entry": has_fragment,
        "env_uniform_present": env_uniform,
        "helpers_present": true,
        "negative_controls": negatives,
        "gpu_execution": gpu,
        "note": "naga proves parse + type-check + env uniform + math helpers; the GPU \
                 executes the real WGSL env_fog_factor/env_water_tint and its output equals \
                 the CPU oracle within MATH_TOLERANCE. Rasterized vertex/fragment pixels are \
                 a live-editor claim, not proven here.",
    }))
}

/// Combined test uniform: fog config (density/range) AND water config (colours/depths)
/// populated so a single dispatch exercises both math helpers.
fn test_uniform() -> EnvironmentUniform {
    EnvironmentUniform {
        fog_color: [0.6, 0.61, 0.65, 0.08],
        fog_range: [5.0, 90.0, 0.0, 0.0],
        water_shallow: [0.31, 0.55, 0.67, 0.35],
        water_deep: [0.04, 0.12, 0.27, 0.86],
        water_depths: [0.8, 4.0, 0.5, 0.0],
        ..EnvironmentUniform::default()
    }
}

/// The real shader plus a thin compute entry that calls its helpers. The source of
/// truth (`WORLD_ENVIRONMENT_WGSL`) is included byte-for-byte; only the entry point
/// and its I/O bindings are appended, so the math evaluated on the GPU is exactly the
/// renderer's.
fn compute_source() -> String {
    format!(
        "{WORLD_ENVIRONMENT_WGSL}\n\
@group(0) @binding(1) var<storage, read> fog_dist: array<f32>;\n\
@group(0) @binding(2) var<storage, read> tint_depth: array<f32>;\n\
@group(0) @binding(3) var<storage, read_write> fog_out: array<f32>;\n\
@group(0) @binding(4) var<storage, read_write> tint_out: array<vec4<f32>>;\n\
@compute @workgroup_size(64)\n\
fn env_eval_main(@builtin(global_invocation_id) gid: vec3<u32>) {{\n\
    let i = gid.x;\n\
    if (i >= arrayLength(&fog_out)) {{ return; }}\n\
    fog_out[i] = env_fog_factor(fog_dist[i]);\n\
    tint_out[i] = env_water_tint(tint_depth[i]);\n\
}}\n"
    )
}

fn uniform_bytes(u: &EnvironmentUniform) -> Vec<u8> {
    let mut flat = [0f32; 24];
    flat[0..4].copy_from_slice(&u.fog_color);
    flat[4..8].copy_from_slice(&u.fog_range);
    flat[8..12].copy_from_slice(&u.water_shallow);
    flat[12..16].copy_from_slice(&u.water_deep);
    flat[16..20].copy_from_slice(&u.water_depths);
    flat[20..24].copy_from_slice(&u.ambient);
    bytemuck::cast_slice(&flat).to_vec()
}

/// Run the real WGSL math on the GPU and compare to the CPU oracle. Fails loud if no
/// GPU exists, the device is lost, or any delta exceeds `MATH_TOLERANCE`.
async fn gpu_math_crosscheck() -> Result<Value, Err> {
    let u = test_uniform();
    // Input grids: fog over 0..SAMPLE_COUNT metres (spans start=5, horizon=90, and the
    // post-horizon cutoff branch); water depth over 0..~6.4 m (spans shallow=0.8,
    // deep=4.0, and the clamp regions on both sides).
    let n = SAMPLE_COUNT as usize;
    let fog_dist: Vec<f32> = (0..n).map(|i| i as f32).collect();
    let tint_depth: Vec<f32> = (0..n).map(|i| i as f32 * 0.05).collect();

    let context = GpuContext::new_headless().await?;
    let device = context.device();
    let queue = context.queue();

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-world-env-eval"),
        source: wgpu::ShaderSource::Wgsl(compute_source().into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-world-env-eval-layout"),
        entries: &[
            uniform_entry(0),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, false),
            storage_entry(4, false),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-world-env-eval-pl"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("litd-world-env-eval-pipeline"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("env_eval_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let env_buffer = buffer_init(
        device,
        "litd-world-env-uniform",
        &uniform_bytes(&u),
        wgpu::BufferUsages::UNIFORM,
    );
    let fog_in = buffer_init(
        device,
        "litd-world-env-fog-in",
        bytemuck::cast_slice(&fog_dist),
        wgpu::BufferUsages::STORAGE,
    );
    let tint_in = buffer_init(
        device,
        "litd-world-env-tint-in",
        bytemuck::cast_slice(&tint_depth),
        wgpu::BufferUsages::STORAGE,
    );
    let fog_bytes = (n * size_of::<f32>()) as u64;
    let tint_bytes = (n * 4 * size_of::<f32>()) as u64;
    let fog_out = storage_out(device, "litd-world-env-fog-out", fog_bytes);
    let tint_out = storage_out(device, "litd-world-env-tint-out", tint_bytes);

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-world-env-eval-bg"),
        layout: &layout,
        entries: &[
            bind(0, &env_buffer),
            bind(1, &fog_in),
            bind(2, &tint_in),
            bind(3, &fog_out),
            bind(4, &tint_out),
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-world-env-eval-encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("litd-world-env-eval-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(SAMPLE_COUNT.div_ceil(64), 1, 1);
    }
    let fog_rb = readback_buffer(device, "litd-world-env-fog-rb", fog_bytes);
    let tint_rb = readback_buffer(device, "litd-world-env-tint-rb", tint_bytes);
    encoder.copy_buffer_to_buffer(&fog_out, 0, &fog_rb, 0, fog_bytes);
    encoder.copy_buffer_to_buffer(&tint_out, 0, &tint_rb, 0, tint_bytes);
    queue.submit([encoder.finish()]);

    let gpu_fog: Vec<f32> = read_f32(device, &fog_rb)?;
    let gpu_tint: Vec<f32> = read_f32(device, &tint_rb)?;
    if let Some(lost) = context.device_lost_report() {
        return Err(format!("GPU device lost during world-environment eval: {lost:?}").into());
    }

    // Compare GPU output (separate source of truth) to the CPU oracle.
    let mut fog_max_delta = 0f32;
    for (i, &dist) in fog_dist.iter().enumerate() {
        let expected = cpu_fog_factor(&u, dist);
        fog_max_delta = fog_max_delta.max((gpu_fog[i] - expected).abs());
    }
    let mut water_max_delta = 0f32;
    for (i, &depth) in tint_depth.iter().enumerate() {
        let expected = cpu_water_tint(&u, depth);
        for (c, &e) in expected.iter().enumerate() {
            water_max_delta = water_max_delta.max((gpu_tint[i * 4 + c] - e).abs());
        }
    }

    let fog_ok = fog_max_delta <= MATH_TOLERANCE;
    let water_ok = water_max_delta <= MATH_TOLERANCE;
    if !fog_ok {
        return Err(format!(
            "GPU fog math diverged from CPU oracle: max_delta={fog_max_delta} > {MATH_TOLERANCE}"
        )
        .into());
    }
    if !water_ok {
        return Err(format!(
            "GPU water math diverged from CPU oracle: max_delta={water_max_delta} > {MATH_TOLERANCE}"
        )
        .into());
    }

    let adapter = context.adapter_report();
    Ok(json!({
        "verified": true,
        "adapter": {
            "name": adapter.name,
            "backend": adapter.backend,
            "device_type": adapter.device_type,
        },
        "tolerance": MATH_TOLERANCE,
        "fog": {
            "samples": SAMPLE_COUNT,
            "max_delta": fog_max_delta,
            "within_tolerance": fog_ok,
        },
        "water": {
            "samples": SAMPLE_COUNT,
            "max_delta": water_max_delta,
            "within_tolerance": water_ok,
        },
    }))
}

fn negative_controls() -> Result<Value, Err> {
    let type_error =
        format!("{WORLD_ENVIRONMENT_WGSL}\nfn _bad() -> f32 {{ return vec2<f32>(1.0, 2.0); }}\n");
    let syntax_error = format!("{WORLD_ENVIRONMENT_WGSL}\nfn _bad() -> {{ }}\n");
    let syntax_rejected = naga::front::wgsl::parse_str(&syntax_error).is_err();
    let type_rejected = parse_and_validate(&type_error).is_err();
    if !syntax_rejected {
        return Err("negative control: a syntax-error shader was NOT rejected".into());
    }
    if !type_rejected {
        return Err("negative control: a type-error shader was NOT rejected".into());
    }
    Ok(json!({
        "syntax_error_rejected": syntax_rejected,
        "type_error_rejected": type_rejected,
    }))
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    buffer_entry(binding, wgpu::BufferBindingType::Uniform)
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    buffer_entry(binding, wgpu::BufferBindingType::Storage { read_only })
}

fn buffer_entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bind(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn buffer_init(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
}

fn storage_out(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn readback_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

fn read_f32(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<f32>, Err> {
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("world-environment GPU readback poll failed: {e}"))?;
    rx.recv()
        .map_err(|e| format!("world-environment GPU readback channel failed: {e}"))?
        .map_err(|e| format!("world-environment GPU buffer map failed: {e:?}"))?;
    let mapped = slice.get_mapped_range();
    let out = bytemuck::cast_slice::<u8, f32>(&mapped).to_vec();
    drop(mapped);
    buffer.unmap();
    Ok(out)
}
