use std::fmt;

use calyx_forge::{DeviceInfo, QuantLevel};
use serde::{Deserialize, Serialize};

use crate::EngineError;

const DEFAULT_PARITY_ABS_TOLERANCE: f32 = 1.0e-5;
const DEFAULT_PARITY_REL_TOLERANCE: f32 = 1.0e-4;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ForgeComputeBackend {
    Cpu,
    Cuda,
}

impl fmt::Display for ForgeComputeBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu => f.write_str("cpu"),
            Self::Cuda => f.write_str("cuda"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ForgeComputeConfig {
    pub backend: ForgeComputeBackend,
    pub cuda_device_index: u32,
    pub parity_abs_tolerance: f32,
    pub parity_rel_tolerance: f32,
}

impl ForgeComputeConfig {
    pub fn cpu_reference() -> Self {
        Self {
            backend: ForgeComputeBackend::Cpu,
            cuda_device_index: 0,
            parity_abs_tolerance: DEFAULT_PARITY_ABS_TOLERANCE,
            parity_rel_tolerance: DEFAULT_PARITY_REL_TOLERANCE,
        }
    }

    pub fn cuda_required(device_index: u32) -> Self {
        Self {
            backend: ForgeComputeBackend::Cuda,
            cuda_device_index: device_index,
            parity_abs_tolerance: DEFAULT_PARITY_ABS_TOLERANCE,
            parity_rel_tolerance: DEFAULT_PARITY_REL_TOLERANCE,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), EngineError> {
        super::validate_tolerance("parity_abs_tolerance", self.parity_abs_tolerance)?;
        super::validate_tolerance("parity_rel_tolerance", self.parity_rel_tolerance)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ForgeComputeProbeReport {
    pub backend: ForgeComputeBackend,
    pub device_info: DeviceInfo,
    pub fixture: ProbeFixtureReport,
    pub gemm: OperationParityReport,
    pub cosine: OperationParityReport,
    pub topk: TopkParityReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProbeFixtureReport {
    pub gemm_shape_mkn: [usize; 3],
    pub cosine_dim: usize,
    pub cosine_candidates: usize,
    pub topk_k: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OperationParityReport {
    pub op: String,
    pub expected: Vec<f32>,
    pub actual: Vec<f32>,
    pub max_abs_error: f32,
    pub max_rel_error: f32,
    pub tolerance_abs: f32,
    pub tolerance_rel: f32,
    pub passed: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TopkParityReport {
    pub expected: Vec<TopkEntry>,
    pub actual: Vec<TopkEntry>,
    pub exact_match: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TopkEntry {
    pub index: usize,
    pub score: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ForgeQuantizationReport {
    pub requested_level: QuantLevel,
    pub encoded_level: QuantLevel,
    pub dim: usize,
    pub encoded_bytes: usize,
    pub scale: f32,
    pub seed_id: String,
    pub cosine: f32,
    pub cosine_loss: f32,
    pub cosine_loss_bound: f32,
    pub original: Vec<f32>,
    pub decoded: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ForgeVramProbeReport {
    pub backend: ForgeComputeBackend,
    pub cuda_device_index: u32,
    pub requested_bytes: usize,
    pub live_free_bytes_before: usize,
    pub accepted: bool,
}
