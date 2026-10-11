//! Low-level wgpu plumbing for the issue #1740 two-phase cull FSV harness: bind-group
//! layout entries, buffer constructors, compute pipelines, and mapped readback. Split
//! out of `gpu.rs` to keep both files under the 500-line ratchet (#800 sec.6).

use std::sync::mpsc;

use litd_engine::EngineError;
use wgpu::util::DeviceExt;

pub fn compute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    source: &str,
    entry: &str,
    tag: &str,
) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("litd-1740-{tag}-shader")),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("litd-1740-{tag}-pl")),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(&format!("litd-1740-{tag}-pipeline")),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some(entry),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

pub fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    buffer_entry(binding, wgpu::BufferBindingType::Storage { read_only })
}

pub fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    buffer_entry(binding, wgpu::BufferBindingType::Uniform)
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

pub fn bind(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

pub fn buffer_init(
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

pub fn zeroed_storage(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

pub fn readback_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

/// wgpu rejects zero-length buffers; pad an empty slice to one element's worth.
pub fn bytes_or_pad(bytes: &[u8], stride: usize) -> Vec<u8> {
    if bytes.is_empty() {
        vec![0u8; stride]
    } else {
        bytes.to_vec()
    }
}

pub fn read_u32(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u32>, EngineError> {
    let bytes = map_readback(device, buffer)?;
    Ok(bytemuck::cast_slice::<u8, u32>(&bytes).to_vec())
}

pub fn map_readback(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u8>, EngineError> {
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| poll_error("map", error))?;
    rx.recv()
        .map_err(|error| poll_error("callback", error))?
        .map_err(|error| EngineError::BatchingInvalidConfig {
            field: "gpu.readback.map".to_string(),
            reason: format!("{error:?}"),
        })?;
    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    Ok(bytes)
}

pub fn poll_error(stage: &str, error: impl std::fmt::Display) -> EngineError {
    EngineError::BatchingInvalidConfig {
        field: format!("gpu.readback.{stage}"),
        reason: error.to_string(),
    }
}
