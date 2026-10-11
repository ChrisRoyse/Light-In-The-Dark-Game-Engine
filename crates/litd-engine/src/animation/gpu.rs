use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::{
    EngineError,
    animation::{
        AnimationSampleRequest, MAX_SKINNING_JOINTS, SkeletalAnimationAsset,
        sample_skeletal_animation,
    },
    math::Mat4,
};

pub const SKINNING_PALETTE_BIND_GROUP: u32 = 3;
pub const SKINNING_PALETTE_BINDING: u32 = 0;
pub const SKINNING_SHADER_ID: &str = "litd.skinning.vertex";

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct GpuJointMatrix {
    pub columns: [[f32; 4]; 4],
}

impl GpuJointMatrix {
    pub fn from_mat4(matrix: Mat4) -> Self {
        Self {
            columns: matrix.to_cols_array_2d(),
        }
    }

    pub fn is_finite(self) -> bool {
        self.columns
            .iter()
            .flat_map(|column| column.iter())
            .all(|value| value.is_finite())
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct SkinningVertexInfluences {
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinningPaletteUpload {
    pub schema: String,
    pub source_relative_path: String,
    pub clip_name: String,
    pub playback_time_ms: u32,
    pub skin_index: usize,
    pub joint_count: usize,
    pub matrix_stride_bytes: usize,
    pub byte_count: usize,
    pub bind_group: u32,
    pub binding: u32,
    pub shader_id: String,
    pub matrices: Vec<GpuJointMatrix>,
}

impl SkinningPaletteUpload {
    pub fn bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.matrices)
    }

    pub fn palette_hash(&self) -> String {
        litd_hash::fnv1a64_hex(self.bytes())
    }

    pub fn buffer_label(&self) -> String {
        format!(
            "litd-skinning-palette-{}-skin{}-{}ms",
            self.clip_name, self.skin_index, self.playback_time_ms
        )
    }
}

pub fn build_skinning_palette_upload(
    asset: &SkeletalAnimationAsset,
    request: &AnimationSampleRequest,
) -> Result<SkinningPaletteUpload, EngineError> {
    let pose = sample_skeletal_animation(asset, request.clone())?;
    let matrices = pose
        .joint_matrices
        .iter()
        .copied()
        .map(GpuJointMatrix::from_mat4)
        .collect::<Vec<_>>();
    let joint_count = matrices.len();
    if joint_count == 0 || joint_count > MAX_SKINNING_JOINTS {
        return Err(EngineError::AnimationInvalidConfig {
            field: "animation.gpu_palette.joint_count".to_string(),
            reason: format!("joint count {joint_count} is outside 1..={MAX_SKINNING_JOINTS}"),
        });
    }
    if matrices.iter().any(|matrix| !matrix.is_finite()) {
        return Err(EngineError::AnimationInvalidConfig {
            field: "animation.gpu_palette.matrices".to_string(),
            reason: "GPU joint palette contains non-finite values".to_string(),
        });
    }
    let matrix_stride_bytes = std::mem::size_of::<GpuJointMatrix>();
    Ok(SkinningPaletteUpload {
        schema: "litd.skinning_palette_upload.v1".to_string(),
        source_relative_path: pose.source_relative_path,
        clip_name: pose.clip_name,
        playback_time_ms: pose.playback_time_ms,
        skin_index: pose.skin_index,
        joint_count,
        matrix_stride_bytes,
        byte_count: joint_count * matrix_stride_bytes,
        bind_group: SKINNING_PALETTE_BIND_GROUP,
        binding: SKINNING_PALETTE_BINDING,
        shader_id: SKINNING_SHADER_ID.to_string(),
        matrices,
    })
}

pub fn create_skinning_palette_buffer(
    device: &wgpu::Device,
    upload: &SkinningPaletteUpload,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&upload.buffer_label()),
        contents: upload.bytes(),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}
