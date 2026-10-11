use calyx_forge::QuantLevel;

use crate::EngineError;
use crate::forge_compute::{
    EngineForgeComputeService, ForgeComputeBackend, ForgeComputeConfig, verify_parity,
};

#[test]
fn cpu_service_runs_known_probe_with_real_outputs() -> Result<(), EngineError> {
    let service = EngineForgeComputeService::new(ForgeComputeConfig::cpu_reference())?;
    let report = service.run_known_probe()?;

    assert_eq!(report.backend, ForgeComputeBackend::Cpu);
    assert_eq!(report.fixture.gemm_shape_mkn, [2, 3, 2]);
    assert_eq!(report.gemm.actual, vec![-1.25, 0.125, 7.0, 6.0]);
    assert_eq!(report.cosine.actual.len(), 3);
    assert_eq!(
        report
            .topk
            .actual
            .iter()
            .map(|entry| entry.index)
            .collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
    println!(
        "FORGE_CPU_PROBE_READBACK backend={:?} gemm={:?} topk={:?}",
        report.backend, report.gemm.actual, report.topk.actual
    );
    Ok(())
}

#[test]
fn quant_bits8_known_vector_decodes_within_bound() -> Result<(), EngineError> {
    let service = EngineForgeComputeService::new(ForgeComputeConfig::cpu_reference())?;
    let report = service.quantize_known_vector(QuantLevel::Bits8, 0.001)?;

    assert_eq!(report.requested_level, QuantLevel::Bits8);
    assert_eq!(report.encoded_level, QuantLevel::Bits8);
    assert_eq!(report.dim, 128);
    assert_eq!(report.encoded_bytes, 128);
    assert!(report.cosine_loss <= report.cosine_loss_bound);
    assert_eq!(report.original.len(), report.decoded.len());
    println!(
        "FORGE_QUANT_BITS8_READBACK level={:?} bytes={} cosine={:.9} loss={:.9}",
        report.encoded_level, report.encoded_bytes, report.cosine, report.cosine_loss
    );
    Ok(())
}

#[test]
fn quant_mxfp4_known_vector_decodes_within_bound() -> Result<(), EngineError> {
    let service = EngineForgeComputeService::new(ForgeComputeConfig::cpu_reference())?;
    let report = service.quantize_known_vector(QuantLevel::Bits4Fp, 0.05)?;

    assert_eq!(report.requested_level, QuantLevel::Bits4Fp);
    assert_eq!(report.encoded_level, QuantLevel::Bits4Fp);
    assert!(report.encoded_bytes > 0);
    assert!(report.cosine_loss <= report.cosine_loss_bound);
    println!(
        "FORGE_QUANT_MXFP4_READBACK bytes={} cosine={:.9} loss={:.9}",
        report.encoded_bytes, report.cosine, report.cosine_loss
    );
    Ok(())
}

#[test]
fn unsupported_quant_level_fails_closed() -> Result<(), EngineError> {
    let service = EngineForgeComputeService::new(ForgeComputeConfig::cpu_reference())?;
    let err = service
        .quantize_known_vector(QuantLevel::F32, 0.001)
        .expect_err("F32 quant level must be rejected");

    assert!(matches!(err, EngineError::ForgeInvalidConfig { .. }));
    assert_eq!(err.code(), "LITD-FORGE-INVALID-CONFIG");
    println!(
        "FORGE_UNSUPPORTED_QUANT_FAIL_CLOSED {}",
        err.report().as_key_values()
    );
    Ok(())
}

#[test]
fn parity_tolerance_is_enforced() {
    let err = verify_parity("test_parity", &[1.0], &[1.5], 0.0, 0.0)
        .expect_err("parity mismatch must fail closed");

    assert!(matches!(err, EngineError::ForgeParityViolation { .. }));
    assert_eq!(err.code(), "LITD-FORGE-PARITY-VIOLATION");
    println!("FORGE_PARITY_FAIL_CLOSED {}", err.report().as_key_values());
}

#[cfg(not(feature = "cuda"))]
#[test]
fn cuda_backend_without_feature_fails_closed() {
    let err = match EngineForgeComputeService::new(ForgeComputeConfig::cuda_required(0)) {
        Ok(_) => panic!("non-CUDA build must reject CUDA backend requests"),
        Err(error) => error,
    };

    assert!(matches!(err, EngineError::ForgeBackendUnavailable { .. }));
    assert_eq!(err.code(), "LITD-FORGE-BACKEND-UNAVAILABLE");
    println!(
        "FORGE_CUDA_FEATURE_FAIL_CLOSED {}",
        err.report().as_key_values()
    );
}
