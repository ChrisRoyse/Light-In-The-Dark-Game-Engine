use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GltfAssetImportReceipt {
    pub schema: String,
    pub source_relative_path: String,
    pub source_absolute_path: String,
    pub source_sha256: String,
    pub verified_file_sha256: String,
    pub source_bytes: u64,
    pub extension: String,
    pub dependency_count: usize,
    pub dependencies: Vec<String>,
    pub scene_count: usize,
    pub default_scene_index: Option<usize>,
    pub node_count: usize,
    pub mesh_count: usize,
    pub primitive_count: usize,
    pub material_count: usize,
    pub texture_count: usize,
    pub sampler_count: usize,
    pub image_count: usize,
    pub unique_material_texture_set_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub skin_count: usize,
    pub joint_count: usize,
    pub skinned_node_count: usize,
    pub skinned_vertex_count: usize,
    pub weighted_vertex_count: usize,
    pub inverse_bind_matrix_count: usize,
    pub animation_count: usize,
    pub animation_channel_count: usize,
    pub animation_sampler_count: usize,
    pub morph_target_count: usize,
    pub morph_position_count: usize,
    pub morph_normal_count: usize,
    pub morph_tangent_count: usize,
    pub materials: Vec<GltfMaterialImport>,
    pub primitives: Vec<GltfPrimitiveImport>,
    pub skins: Vec<GltfSkinImport>,
    pub animations: Vec<GltfAnimationImport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GltfMaterialImport {
    pub material_index: usize,
    pub name: Option<String>,
    pub alpha_mode: String,
    pub double_sided: bool,
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub base_color_texture_source_path: Option<String>,
    pub normal_texture_source_path: Option<String>,
    pub metallic_roughness_texture_source_path: Option<String>,
    pub emissive_texture_source_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GltfPrimitiveImport {
    pub mesh_index: usize,
    pub primitive_index: usize,
    pub material_index: Option<usize>,
    pub mode: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub has_normals: bool,
    pub has_tangents: bool,
    pub texcoord_0_count: usize,
    pub joint_set_count: usize,
    pub weight_set_count: usize,
    pub skinned_vertex_count: usize,
    pub weighted_vertex_count: usize,
    pub morph_target_count: usize,
    pub morph_position_count: usize,
    pub morph_normal_count: usize,
    pub morph_tangent_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GltfSkinImport {
    pub skin_index: usize,
    pub name: Option<String>,
    pub skeleton_root: Option<String>,
    pub joint_count: usize,
    pub inverse_bind_matrix_count: usize,
    pub joint_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GltfAnimationImport {
    pub clip_index: usize,
    pub name: String,
    pub duration_ms: u32,
    pub channel_count: usize,
    pub sampler_count: usize,
    pub node_transform_channel_count: usize,
    pub morph_weight_channel_count: usize,
    pub output_sample_count: usize,
    pub target_summary: String,
}
