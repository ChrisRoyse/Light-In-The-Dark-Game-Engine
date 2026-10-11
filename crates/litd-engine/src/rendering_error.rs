//! Rendering and presentation error taxonomy for the engine boundary.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderingError {
    #[error("PBR material factor {field}={value} is outside [{min}, {max}]")]
    PbrMaterialInvalidFactor {
        field: &'static str,
        value: f32,
        min: f32,
        max: f32,
    },

    #[error("PBR vector {field} is invalid: {value:?}")]
    PbrInvalidVector {
        field: &'static str,
        value: [f32; 3],
    },

    #[error("clustered lighting config field {field} is invalid: {reason}")]
    ClusteredLightingInvalidConfig { field: String, reason: String },

    #[error(
        "clustered lighting cluster {cluster_index} exceeded its max of {max_lights} light refs"
    )]
    ClusteredLightingClusterOverflow {
        cluster_index: usize,
        max_lights: usize,
    },

    #[error(
        "clustered lighting GPU readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    ClusteredLightingGpuReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("shader config field {field} is invalid: {reason}")]
    ShaderInvalidConfig { field: String, reason: String },

    #[error("shader '{shader_id}' binding {binding} failed static validation: {reason}")]
    ShaderBindingValidationFailed {
        shader_id: String,
        binding: String,
        reason: String,
    },

    #[error("stylized shader preset '{preset}' field {field} is invalid: {reason}")]
    StylizedShaderInvalidConfig {
        preset: String,
        field: String,
        reason: String,
    },

    #[error("shader/material catalog field {field} is invalid: {reason}")]
    MaterialCatalogInvalidConfig { field: String, reason: String },

    #[error("camera config field {field} is invalid: {reason}")]
    CameraInvalidConfig { field: String, reason: String },
}
