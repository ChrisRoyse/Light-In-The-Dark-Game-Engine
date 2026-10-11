use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::EngineError;

use super::{EditorWorldRenderer, world_error};

/// Presentation-only visibility for one deterministic simulation fog cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EditorWorldFogCell {
    Masked = 0,
    Explored = 1,
    Visible = 2,
}

/// Borrowed row-major fog grid uploaded to the world renderer.
#[derive(Debug, Clone, Copy)]
pub struct EditorWorldFogFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub cells: &'a [EditorWorldFogCell],
}

impl EditorWorldFogFrame<'_> {
    fn validate(self) -> Result<(), EngineError> {
        if self.width == 0 || self.height == 0 {
            return world_error(
                "replay_fog.dimensions",
                "fog width and height must be non-zero",
            );
        }
        let expected = usize::try_from(u64::from(self.width) * u64::from(self.height))
            .map_err(|_| fog_error("replay_fog.cells", "fog cell count exceeds usize"))?;
        if self.cells.len() != expected {
            return world_error(
                "replay_fog.cells",
                format!(
                    "fog cell count {} does not match {}x{}={expected}",
                    self.cells.len(),
                    self.width,
                    self.height
                ),
            );
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EditorWorldFogUniform {
    // enabled, width, height, reserved
    config: [u32; 4],
}

pub(super) struct EditorWorldFogGpu {
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    _texture: wgpu::Texture,
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
    enabled: bool,
}

impl EditorWorldFogGpu {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = fog_bind_group_layout(device);
        let uniform = EditorWorldFogUniform {
            config: [0, 1, 1, 0],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("litd-editor-world-replay-fog-uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let rgba8 = vec![EditorWorldFogCell::Visible as u8, 0, 0, 0];
        let (texture, bind_group) =
            fog_texture_and_bind_group(device, queue, &layout, &uniform_buffer, 1, 1, &rgba8);
        Self {
            layout,
            bind_group,
            uniform_buffer,
            _texture: texture,
            width: 1,
            height: 1,
            rgba8,
            enabled: false,
        }
    }

    pub(super) fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub(super) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: EditorWorldFogFrame<'_>,
    ) -> Result<(), EngineError> {
        frame.validate()?;
        validate_gpu_dimensions(device, frame.width, frame.height)?;
        if self.width != frame.width || self.height != frame.height {
            self.resize(device, queue, frame.width, frame.height)?;
        }
        encode_fog_cells(&mut self.rgba8, frame.cells)?;
        write_fog_texture(queue, &self._texture, self.width, self.height, &self.rgba8);
        self.enabled = true;
        self.write_uniform(queue);
        Ok(())
    }

    fn disable(&mut self, queue: &wgpu::Queue) {
        if self.enabled {
            self.enabled = false;
            self.write_uniform(queue);
        }
    }

    fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<(), EngineError> {
        let byte_count = fog_byte_count(width, height)?;
        let rgba8 = vec![0; byte_count];
        let (texture, bind_group) = fog_texture_and_bind_group(
            device,
            queue,
            &self.layout,
            &self.uniform_buffer,
            width,
            height,
            &rgba8,
        );
        self._texture = texture;
        self.bind_group = bind_group;
        self.width = width;
        self.height = height;
        self.rgba8 = rgba8;
        Ok(())
    }

    fn write_uniform(&self, queue: &wgpu::Queue) {
        let uniform = EditorWorldFogUniform {
            config: [u32::from(self.enabled), self.width, self.height, 0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }
}

impl EditorWorldRenderer {
    /// Upload a selected player's deterministic visibility grid for world shading.
    pub fn set_fog_presentation(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: EditorWorldFogFrame<'_>,
    ) -> Result<(), EngineError> {
        self.fog.update(device, queue, frame)
    }

    /// Disable gameplay fog for editor and ordinary package rendering.
    pub fn clear_fog_presentation(&mut self, queue: &wgpu::Queue) {
        self.fog.disable(queue);
    }
}

fn fog_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-editor-world-replay-fog-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Uint,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn fog_texture_and_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    width: u32,
    height: u32,
    rgba8: &[u8],
) -> (wgpu::Texture, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-editor-world-replay-fog-grid"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write_fog_texture(queue, &texture, width, height, rgba8);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-editor-world-replay-fog-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });
    (texture, bind_group)
}

fn write_fog_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    rgba8: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba8,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

fn validate_gpu_dimensions(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Result<(), EngineError> {
    let max = device.limits().max_texture_dimension_2d;
    if width > max || height > max {
        return world_error(
            "replay_fog.dimensions",
            format!("fog grid {width}x{height} exceeds GPU 2D texture limit {max}"),
        );
    }
    fog_byte_count(width, height).map(|_| ())
}

fn fog_byte_count(width: u32, height: u32) -> Result<usize, EngineError> {
    usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| fog_error("replay_fog.cells", "fog texture byte count exceeds usize"))
}

fn encode_fog_cells(rgba8: &mut [u8], cells: &[EditorWorldFogCell]) -> Result<(), EngineError> {
    if rgba8.len() != cells.len().saturating_mul(4) {
        return world_error(
            "replay_fog.upload",
            "fog upload buffer must contain exactly four bytes per cell",
        );
    }
    for (pixel, state) in rgba8.chunks_exact_mut(4).zip(cells) {
        pixel[0] = *state as u8;
        pixel[1..].fill(0);
    }
    Ok(())
}

fn fog_error(field: &str, reason: &str) -> EngineError {
    EngineError::EditorWorldRendererInvalidConfig {
        field: field.to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_requires_exact_nonempty_row_major_grid() {
        let cells = [EditorWorldFogCell::Visible; 4];
        EditorWorldFogFrame {
            width: 2,
            height: 2,
            cells: &cells,
        }
        .validate()
        .unwrap();

        let empty = EditorWorldFogFrame {
            width: 0,
            height: 2,
            cells: &[],
        }
        .validate()
        .unwrap_err();
        let wrong_count = EditorWorldFogFrame {
            width: 3,
            height: 2,
            cells: &cells,
        }
        .validate()
        .unwrap_err();
        assert!(empty.to_string().contains("width and height"));
        assert!(wrong_count.to_string().contains("does not match 3x2=6"));
    }

    #[test]
    fn upload_encoding_preserves_visibility_ordinals_without_padding_leaks() {
        let cells = [
            EditorWorldFogCell::Masked,
            EditorWorldFogCell::Explored,
            EditorWorldFogCell::Visible,
        ];
        let mut rgba8 = vec![0xff; cells.len() * 4];
        encode_fog_cells(&mut rgba8, &cells).unwrap();
        assert_eq!(
            rgba8,
            vec![0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0],
            "row-major cells must map exactly to Masked=0, Explored=1, Visible=2"
        );
    }

    #[test]
    fn upload_encoding_rejects_mismatched_storage() {
        let error = encode_fog_cells(&mut [0; 4], &[EditorWorldFogCell::Masked; 2]).unwrap_err();
        assert!(error.to_string().contains("four bytes per cell"));
    }
}
