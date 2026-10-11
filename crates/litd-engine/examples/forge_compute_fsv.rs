use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use calyx_forge::QuantLevel;
use litd_engine::EngineError;
use litd_engine::forge_compute::{
    EngineForgeComputeService, ForgeComputeBackend, ForgeComputeConfig, ForgeComputeProbeReport,
    ForgeQuantizationReport, verify_parity,
};
use serde::{Deserialize, Serialize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let artifact_path = artifact_path();
    let backend = requested_backend()?;
    let config = match backend {
        ForgeComputeBackend::Cpu => ForgeComputeConfig::cpu_reference(),
        ForgeComputeBackend::Cuda => ForgeComputeConfig::cuda_required(0),
    };
    let service = EngineForgeComputeService::new(config)?;

    let probe = service.run_known_probe()?;
    let quant_bits8 = service.quantize_known_vector(QuantLevel::Bits8, 0.001)?;
    let quant_mxfp8 = service.quantize_known_vector(QuantLevel::Bits8Fp, 0.001)?;
    let quant_mxfp4 = service.quantize_known_vector(QuantLevel::Bits4Fp, 0.05)?;
    let mut state = RuntimeState {
        successful_probe_reports: 1,
        successful_quant_reports: 3,
        artifact_exists: artifact_path.exists(),
        artifact_bytes: file_len(&artifact_path),
    };

    let edges = vec![
        run_unsupported_quant_edge(&service, &artifact_path, &state),
        run_parity_tolerance_edge(&artifact_path, &state),
        run_vram_edge(&service, &artifact_path, &state),
    ];
    for edge in &edges {
        println!(
            "FSV_EDGE name={} before={:?} after={:?} error_code={}",
            edge.name, edge.before, edge.after, edge.error.code
        );
    }

    let mut artifact = ManualFsvArtifact {
        issue: 1200,
        source_of_truth: artifact_path.display().to_string(),
        backend_requested: backend,
        happy_path: HappyPath {
            probe,
            quant_bits8,
            quant_mxfp8,
            quant_mxfp4,
        },
        edges,
        source_truth_readback: None,
    };
    write_artifact(&artifact_path, &artifact)?;

    let readback = read_artifact(&artifact_path)?;
    assert_eq!(readback.issue, 1200);
    assert_eq!(readback.happy_path.probe.fixture.gemm_shape_mkn, [2, 3, 2]);
    assert_eq!(readback.edges.len(), 3);
    state.artifact_exists = artifact_path.exists();
    state.artifact_bytes = file_len(&artifact_path);

    artifact.source_truth_readback = Some(SourceTruthReadback {
        artifact_bytes: state.artifact_bytes,
        backend_requested: readback.backend_requested,
        gemm_actual: readback.happy_path.probe.gemm.actual.clone(),
        quant_levels: vec![
            readback.happy_path.quant_bits8.encoded_level,
            readback.happy_path.quant_mxfp8.encoded_level,
            readback.happy_path.quant_mxfp4.encoded_level,
        ],
        edge_error_codes: readback
            .edges
            .iter()
            .map(|edge| edge.error.code.clone())
            .collect(),
    });
    write_artifact(&artifact_path, &artifact)?;

    let final_readback = read_artifact(&artifact_path)?;
    let proof = final_readback
        .source_truth_readback
        .as_ref()
        .expect("readback summary must be persisted");
    println!(
        "FSV_SOURCE_OF_TRUTH_READBACK path={} bytes={} backend={:?} gemm={:?} quant_levels={:?} edge_codes={:?}",
        artifact_path.display(),
        file_len(&artifact_path),
        proof.backend_requested,
        proof.gemm_actual,
        proof.quant_levels,
        proof.edge_error_codes
    );
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct ManualFsvArtifact {
    issue: u32,
    source_of_truth: String,
    backend_requested: ForgeComputeBackend,
    happy_path: HappyPath,
    edges: Vec<EdgeCase>,
    source_truth_readback: Option<SourceTruthReadback>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HappyPath {
    probe: ForgeComputeProbeReport,
    quant_bits8: ForgeQuantizationReport,
    quant_mxfp8: ForgeQuantizationReport,
    quant_mxfp4: ForgeQuantizationReport,
}

#[derive(Debug, Deserialize, Serialize)]
struct EdgeCase {
    name: String,
    input: String,
    expected: String,
    before: RuntimeState,
    after: RuntimeState,
    error: ErrorEvidence,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RuntimeState {
    successful_probe_reports: usize,
    successful_quant_reports: usize,
    artifact_exists: bool,
    artifact_bytes: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct ErrorEvidence {
    code: String,
    message: String,
    remediation: String,
    source_error: Option<String>,
    forge_source_code: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SourceTruthReadback {
    artifact_bytes: u64,
    backend_requested: ForgeComputeBackend,
    gemm_actual: Vec<f32>,
    quant_levels: Vec<QuantLevel>,
    edge_error_codes: Vec<String>,
}

fn run_unsupported_quant_edge(
    service: &EngineForgeComputeService,
    artifact_path: &Path,
    state: &RuntimeState,
) -> EdgeCase {
    let before = refresh_state(artifact_path, state);
    let error = capture_error(
        service.quantize_known_vector(QuantLevel::F32, 0.001),
        "unsupported quant level must fail closed",
    );
    EdgeCase {
        name: "unsupported_dtype_rejected".to_string(),
        input: "QuantLevel::F32".to_string(),
        expected: "LITD-FORGE-INVALID-CONFIG".to_string(),
        after: refresh_state(artifact_path, state),
        before,
        error,
    }
}

fn run_parity_tolerance_edge(artifact_path: &Path, state: &RuntimeState) -> EdgeCase {
    let before = refresh_state(artifact_path, state);
    let error = capture_error(
        verify_parity("manual_edge", &[1.0], &[1.5], 0.0, 0.0),
        "parity violation must fail closed",
    );
    EdgeCase {
        name: "parity_tolerance_enforced".to_string(),
        input: "expected=[1.0], actual=[1.5], tolerances=0".to_string(),
        expected: "LITD-FORGE-PARITY-VIOLATION".to_string(),
        after: refresh_state(artifact_path, state),
        before,
        error,
    }
}

fn run_vram_edge(
    service: &EngineForgeComputeService,
    artifact_path: &Path,
    state: &RuntimeState,
) -> EdgeCase {
    let before = refresh_state(artifact_path, state);
    let error = capture_error(
        service.probe_vram_request(usize::MAX),
        "VRAM probe must fail closed for impossible request or non-CUDA backend",
    );
    EdgeCase {
        name: "vram_oom_fails_closed".to_string(),
        input: format!("requested_bytes={}", usize::MAX),
        expected: "CUDA: LITD-FORGE-OPERATION-FAILED with CALYX_FORGE_DEVICE_UNAVAILABLE; CPU: LITD-FORGE-INVALID-CONFIG".to_string(),
        after: refresh_state(artifact_path, state),
        before,
        error,
    }
}

fn capture_error<T>(result: Result<T, EngineError>, ok_message: &str) -> ErrorEvidence {
    match result {
        Ok(_) => panic!("{ok_message}"),
        Err(error) => error_evidence(error),
    }
}

fn error_evidence(error: EngineError) -> ErrorEvidence {
    let report = error.report();
    let forge_source_code = match &error {
        EngineError::ForgeOperationFailed { source, .. } => Some(source.code().to_string()),
        _ => None,
    };
    ErrorEvidence {
        code: report.code.to_string(),
        message: report.message,
        remediation: report.remediation.to_string(),
        source_error: report.source_error,
        forge_source_code,
    }
}

fn refresh_state(path: &Path, state: &RuntimeState) -> RuntimeState {
    RuntimeState {
        artifact_exists: path.exists(),
        artifact_bytes: file_len(path),
        ..state.clone()
    }
}

fn requested_backend() -> Result<ForgeComputeBackend, EngineError> {
    match env::var("LITD_FORGE_BACKEND")
        .unwrap_or_else(|_| "cuda".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "cpu" => Ok(ForgeComputeBackend::Cpu),
        "cuda" => Ok(ForgeComputeBackend::Cuda),
        other => Err(EngineError::ForgeInvalidConfig {
            field: "LITD_FORGE_BACKEND".to_string(),
            reason: format!("expected 'cpu' or 'cuda', got '{other}'"),
        }),
    }
}

fn artifact_path() -> PathBuf {
    env::var("LITD_FORGE_FSV_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new("target/fsv/issue1200_forge_compute/manual-fsv.json").into())
}

fn write_artifact(
    path: &Path,
    artifact: &ManualFsvArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(artifact)?)?;
    Ok(())
}

fn read_artifact(path: &Path) -> Result<ManualFsvArtifact, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}
