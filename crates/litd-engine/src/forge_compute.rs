//! Engine-facing Forge compute and quantization service.
//!
//! Backend selection is explicit. A CUDA request in a non-CUDA build fails
//! closed; it never silently redirects to CPU.

use calyx_forge::{
    Backend, CpuBackend, DeviceInfo, ForgeError, QuantLevel, QuantizedVec, Quantizer,
    ScalarInt8Codec, TurboQuantCodec, decode_mxfp4, decode_mxfp8, encode_mxfp4, encode_mxfp8,
    new_seed, seed_id_hex,
};

use crate::EngineError;

const ZERO_SEED: [u8; 32] = [0; 32];

mod fixtures;
mod types;

use fixtures::{ProbeFixture, known_probe_fixture, known_quant_vector};
pub use types::*;

pub struct EngineForgeComputeService {
    config: ForgeComputeConfig,
    backend: Box<dyn Backend>,
}

impl EngineForgeComputeService {
    pub fn new(config: ForgeComputeConfig) -> Result<Self, EngineError> {
        config.validate()?;
        let backend = build_backend(&config)?;
        Ok(Self { config, backend })
    }

    pub fn config(&self) -> &ForgeComputeConfig {
        &self.config
    }

    pub fn device_info(&self) -> DeviceInfo {
        self.backend.device_info()
    }

    pub fn run_known_probe(&self) -> Result<ForgeComputeProbeReport, EngineError> {
        let fixture = known_probe_fixture();
        let cpu = CpuBackend::new();

        let cpu_gemm = run_gemm(&cpu, &fixture, "forge.cpu_reference.gemm")?;
        let backend_gemm = run_gemm(self.backend.as_ref(), &fixture, "forge.backend.gemm")?;
        let gemm = verify_parity(
            "gemm",
            &cpu_gemm,
            &backend_gemm,
            self.config.parity_abs_tolerance,
            self.config.parity_rel_tolerance,
        )?;

        let cpu_cosine = run_cosine(&cpu, &fixture, "forge.cpu_reference.cosine")?;
        let backend_cosine = run_cosine(self.backend.as_ref(), &fixture, "forge.backend.cosine")?;
        let cosine = verify_parity(
            "cosine",
            &cpu_cosine,
            &backend_cosine,
            self.config.parity_abs_tolerance,
            self.config.parity_rel_tolerance,
        )?;

        let cpu_topk = run_topk(&cpu, &fixture, "forge.cpu_reference.topk")?;
        let backend_topk = run_topk(self.backend.as_ref(), &fixture, "forge.backend.topk")?;
        let topk = verify_topk(&cpu_topk, &backend_topk)?;

        Ok(ForgeComputeProbeReport {
            backend: self.config.backend,
            device_info: self.device_info(),
            fixture: fixture.report(),
            gemm,
            cosine,
            topk,
        })
    }

    pub fn quantize_known_vector(
        &self,
        level: QuantLevel,
        cosine_loss_bound: f32,
    ) -> Result<ForgeQuantizationReport, EngineError> {
        validate_quant_bound(cosine_loss_bound)?;
        let original = known_quant_vector();
        let (qv, decoded) = quantize_decode(level, &original)?;
        let cosine = cosine_similarity(&original, &decoded)?;
        let cosine_loss = (1.0 - cosine).max(0.0);
        if cosine_loss > cosine_loss_bound {
            let error = EngineError::ForgeQuantizationBoundExceeded {
                level: level.to_string(),
                cosine_loss,
                bound: cosine_loss_bound,
            };
            log_engine_failure("forge.quantization.bound", &error);
            return Err(error);
        }
        Ok(ForgeQuantizationReport {
            requested_level: level,
            encoded_level: qv.level,
            dim: qv.dim,
            encoded_bytes: qv.bytes.len(),
            scale: qv.scale,
            seed_id: seed_id_hex(&qv.seed_id),
            cosine,
            cosine_loss,
            cosine_loss_bound,
            original,
            decoded,
        })
    }

    pub fn probe_vram_request(
        &self,
        requested_bytes: usize,
    ) -> Result<ForgeVramProbeReport, EngineError> {
        if self.config.backend != ForgeComputeBackend::Cuda {
            return Err(invalid_config(
                "backend",
                "VRAM allocation probes require backend=cuda",
            ));
        }
        probe_cuda_vram(self.config.cuda_device_index, requested_bytes)
    }
}

pub fn verify_parity(
    op: &str,
    expected: &[f32],
    actual: &[f32],
    tolerance_abs: f32,
    tolerance_rel: f32,
) -> Result<OperationParityReport, EngineError> {
    validate_tolerance("tolerance_abs", tolerance_abs)?;
    validate_tolerance("tolerance_rel", tolerance_rel)?;
    if expected.len() != actual.len() {
        return Err(parity_error(
            op,
            format!(
                "length mismatch expected_len={} actual_len={}",
                expected.len(),
                actual.len()
            ),
        ));
    }

    let mut max_abs_error = 0.0_f32;
    let mut max_rel_error = 0.0_f32;
    for (idx, (left, right)) in expected.iter().zip(actual.iter()).enumerate() {
        if !left.is_finite() || !right.is_finite() {
            return Err(parity_error(
                op,
                format!("non-finite value at index {idx}: expected={left} actual={right}"),
            ));
        }
        let abs_error = (*left - *right).abs();
        let rel_error = abs_error / left.abs().max(1.0);
        max_abs_error = max_abs_error.max(abs_error);
        max_rel_error = max_rel_error.max(rel_error);
        let allowed = tolerance_abs.max(tolerance_rel * left.abs().max(1.0));
        if abs_error > allowed {
            return Err(parity_error(
                op,
                format!(
                    "index={idx} expected={left} actual={right} abs_error={abs_error} allowed={allowed}"
                ),
            ));
        }
    }

    Ok(OperationParityReport {
        op: op.to_string(),
        expected: expected.to_vec(),
        actual: actual.to_vec(),
        max_abs_error,
        max_rel_error,
        tolerance_abs,
        tolerance_rel,
        passed: true,
    })
}

fn build_backend(config: &ForgeComputeConfig) -> Result<Box<dyn Backend>, EngineError> {
    match config.backend {
        ForgeComputeBackend::Cpu => Ok(Box::new(CpuBackend::new())),
        ForgeComputeBackend::Cuda => build_cuda_backend(config.cuda_device_index),
    }
}

#[cfg(feature = "cuda")]
fn build_cuda_backend(device_index: u32) -> Result<Box<dyn Backend>, EngineError> {
    let ctx = run_forge("forge.cuda.init", || {
        calyx_forge::init_cuda(device_index, true)
    })?;
    Ok(Box::new(calyx_forge::CudaBackend::with_context(ctx)))
}

#[cfg(not(feature = "cuda"))]
fn build_cuda_backend(_device_index: u32) -> Result<Box<dyn Backend>, EngineError> {
    let error = EngineError::ForgeBackendUnavailable {
        backend: "cuda".to_string(),
        reason: "litd-engine was built without the cuda feature; rebuild with --features cuda"
            .to_string(),
    };
    log_engine_failure("forge.cuda.unavailable", &error);
    Err(error)
}

#[cfg(feature = "cuda")]
fn probe_cuda_vram(
    cuda_device_index: u32,
    requested_bytes: usize,
) -> Result<ForgeVramProbeReport, EngineError> {
    let ctx = run_forge("forge.cuda.init", || {
        calyx_forge::init_cuda(cuda_device_index, true)
    })?;
    let live_free_bytes_before = run_forge("forge.cuda.free_device_vram_bytes", || {
        ctx.free_device_vram_bytes()
    })?;
    run_forge("forge.cuda.probe_allocation", || {
        calyx_forge::cuda::probe_allocation(&ctx, requested_bytes)
    })?;
    Ok(ForgeVramProbeReport {
        backend: ForgeComputeBackend::Cuda,
        cuda_device_index,
        requested_bytes,
        live_free_bytes_before,
        accepted: true,
    })
}

#[cfg(not(feature = "cuda"))]
fn probe_cuda_vram(
    _cuda_device_index: u32,
    _requested_bytes: usize,
) -> Result<ForgeVramProbeReport, EngineError> {
    build_cuda_backend(0).map(|_| unreachable!())
}

fn quantize_decode(
    level: QuantLevel,
    original: &[f32],
) -> Result<(QuantizedVec, Vec<f32>), EngineError> {
    match level {
        QuantLevel::Bits8 => {
            let codec = ScalarInt8Codec::new(original.len());
            let qv = run_forge("forge.quant.bits8.encode", || codec.encode(original))?;
            let decoded = run_forge("forge.quant.bits8.decode", || codec.decode(&qv))?;
            Ok((qv, decoded))
        }
        QuantLevel::Bits8Fp => {
            let blocks = run_forge("forge.quant.mxfp8.encode", || encode_mxfp8(original))?;
            let decoded = decode_mxfp8(&blocks, original.len());
            let qv = QuantizedVec {
                level,
                dim: original.len(),
                bytes: calyx_forge::quant::mxfp4_codec::serialize_mxfp8_blocks(&blocks),
                scale: 0.0,
                seed_id: ZERO_SEED,
            };
            Ok((qv, decoded))
        }
        QuantLevel::Bits4Fp => {
            let blocks = run_forge("forge.quant.mxfp4.encode", || encode_mxfp4(original))?;
            let decoded = decode_mxfp4(&blocks, original.len());
            let qv = QuantizedVec {
                level,
                dim: original.len(),
                bytes: calyx_forge::quant::mxfp4_codec::serialize_blocks(&blocks),
                scale: 0.0,
                seed_id: ZERO_SEED,
            };
            Ok((qv, decoded))
        }
        QuantLevel::Bits3p5 | QuantLevel::Bits2p5 => {
            let seed = new_seed(original.len(), b"litd-engine-issue1200-known-quant-vector");
            let codec = run_forge("forge.quant.turbo.new", || {
                TurboQuantCodec::new(seed, level)
            })?;
            let qv = run_forge("forge.quant.turbo.encode", || codec.encode(original))?;
            let decoded = run_forge("forge.quant.turbo.decode", || codec.decode(&qv))?;
            Ok((qv, decoded))
        }
        QuantLevel::F32 | QuantLevel::Bits1 => Err(invalid_config(
            "quant_level",
            format!("unsupported Forge quantization level for engine service: {level}"),
        )),
    }
}

fn run_gemm(
    backend: &dyn Backend,
    fixture: &ProbeFixture,
    op: &'static str,
) -> Result<Vec<f32>, EngineError> {
    let mut out = vec![0.0; fixture.gemm_m * fixture.gemm_n];
    run_forge(op, || {
        backend.gemm(
            &fixture.gemm_a,
            &fixture.gemm_b,
            fixture.gemm_m,
            fixture.gemm_k,
            fixture.gemm_n,
            &mut out,
        )
    })?;
    Ok(out)
}

fn run_cosine(
    backend: &dyn Backend,
    fixture: &ProbeFixture,
    op: &'static str,
) -> Result<Vec<f32>, EngineError> {
    let mut out = vec![0.0; fixture.cosine_candidates];
    run_forge(op, || {
        backend.cosine(
            &fixture.cosine_query,
            &fixture.cosine_matrix,
            fixture.cosine_dim,
            &mut out,
        )
    })?;
    Ok(out)
}

fn run_topk(
    backend: &dyn Backend,
    fixture: &ProbeFixture,
    op: &'static str,
) -> Result<Vec<TopkEntry>, EngineError> {
    let ranked = run_forge(op, || backend.topk(&fixture.topk_scores, fixture.topk_k))?;
    Ok(ranked
        .into_iter()
        .map(|(index, score)| TopkEntry { index, score })
        .collect())
}

fn verify_topk(
    expected: &[TopkEntry],
    actual: &[TopkEntry],
) -> Result<TopkParityReport, EngineError> {
    if expected != actual {
        return Err(parity_error(
            "topk",
            format!("expected={expected:?} actual={actual:?}"),
        ));
    }
    Ok(TopkParityReport {
        expected: expected.to_vec(),
        actual: actual.to_vec(),
        exact_match: true,
    })
}

fn run_forge<T>(
    op: impl Into<String>,
    action: impl FnOnce() -> Result<T, ForgeError>,
) -> Result<T, EngineError> {
    let op = op.into();
    action().map_err(|source| {
        tracing::error!(
            op,
            forge_error_code = source.code(),
            remediation = source.remediation(),
            source_error = %source,
            "forge operation failed"
        );
        EngineError::ForgeOperationFailed {
            op,
            source: Box::new(source),
        }
    })
}

fn validate_tolerance(field: &'static str, value: f32) -> Result<(), EngineError> {
    if value.is_finite() && value >= 0.0 {
        return Ok(());
    }
    Err(invalid_config(
        field,
        format!("value must be finite and non-negative, got {value}"),
    ))
}

fn validate_quant_bound(value: f32) -> Result<(), EngineError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        return Ok(());
    }
    Err(invalid_config(
        "cosine_loss_bound",
        format!("value must be finite and inside [0, 1], got {value}"),
    ))
}

fn invalid_config(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let error = EngineError::ForgeInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    };
    log_engine_failure("forge.invalid_config", &error);
    error
}

fn parity_error(op: &str, detail: String) -> EngineError {
    let error = EngineError::ForgeParityViolation {
        op: op.to_string(),
        detail,
    };
    log_engine_failure("forge.parity", &error);
    error
}

fn log_engine_failure(context: &'static str, error: &EngineError) {
    tracing::error!(
        context,
        error_code = error.code(),
        error_message = %error,
        remediation = error.remediation(),
        "engine forge failure"
    );
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> Result<f32, EngineError> {
    if left.len() != right.len() {
        return Err(parity_error(
            "quant_cosine",
            format!("length mismatch left={} right={}", left.len(), right.len()),
        ));
    }
    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;
    for (idx, (a, b)) in left.iter().zip(right.iter()).enumerate() {
        if !a.is_finite() || !b.is_finite() {
            return Err(parity_error(
                "quant_cosine",
                format!("non-finite coefficient at index {idx}: left={a} right={b}"),
            ));
        }
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm <= 0.0 || right_norm <= 0.0 {
        return Err(parity_error(
            "quant_cosine",
            format!("zero norm left_norm={left_norm} right_norm={right_norm}"),
        ));
    }
    Ok(dot / (left_norm.sqrt() * right_norm.sqrt()))
}
