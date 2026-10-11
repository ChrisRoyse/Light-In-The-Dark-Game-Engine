//! CPU-explicit panel lens registration helpers (#1791/#1890).

use calyx_core::{
    CalyxError, Input, LensId, Modality, Result as CalyxResult, Slot, SlotShape, SlotState,
    SlotVector,
};
use calyx_registry::{
    CandleDevicePolicy, CandleLens, FastembedBgem3Lens, FastembedQwen3Lens, FastembedSparseLens,
    Lens, LensRuntime, LensSpec, OnnxColbertFileSpec, OnnxColbertLens, OnnxFileSpec, OnnxLens,
    OnnxProviderPolicy, Registry, RegistryLensSnapshot, StaticLookupLens,
};

use crate::calyx_content::EngineCalyxContentError;

use super::measurement_failed;
pub(super) fn register_cpu_explicit_text_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
    runtime: CpuExplicitTextRuntime,
) -> Result<(), EngineCalyxContentError> {
    match runtime {
        CpuExplicitTextRuntime::DenseOnnx => {
            register_cpu_explicit_dense_onnx_lens(registry, snapshot, spec)
        }
        CpuExplicitTextRuntime::FastembedSparse => {
            register_cpu_explicit_fastembed_sparse_lens(registry, snapshot, spec)
        }
        CpuExplicitTextRuntime::FastembedBgem3 => {
            register_cpu_explicit_fastembed_bgem3_lens(registry, snapshot, spec)
        }
        CpuExplicitTextRuntime::OnnxColbert => {
            register_cpu_explicit_onnx_colbert_lens(registry, snapshot, spec)
        }
        CpuExplicitTextRuntime::FastembedQwen3 => {
            register_cpu_explicit_fastembed_qwen3_lens(registry, snapshot, spec)
        }
        CpuExplicitTextRuntime::CandleLocal => {
            register_cpu_explicit_candle_local_lens(registry, snapshot, spec)
        }
        CpuExplicitTextRuntime::StaticLookup => {
            register_cpu_explicit_static_lookup_lens(registry, snapshot, spec)
        }
    }
}

/// Stable short label for a persisted lens runtime, used only for structured
/// fail-loud diagnostics when a learned text family cannot be surfaced.
pub(super) fn lens_runtime_label(runtime: &LensRuntime) -> &'static str {
    match runtime {
        LensRuntime::Algorithmic { .. } => "algorithmic",
        LensRuntime::TeiHttp { .. } => "tei_http",
        LensRuntime::CandleLocal { .. } => "candle_local",
        LensRuntime::Onnx { .. } => "onnx",
        LensRuntime::OnnxColbert { .. } => "onnx_colbert",
        LensRuntime::FastembedSparse { .. } => "fastembed_sparse",
        LensRuntime::FastembedBgem3 { .. } => "fastembed_bgem3",
        LensRuntime::FastembedReranker { .. } => "fastembed_reranker",
        LensRuntime::FastembedQwen3 { .. } => "fastembed_qwen3",
        LensRuntime::StaticLookup { .. } => "static_lookup",
        LensRuntime::MultimodalAdapter { .. } => "multimodal_adapter",
        LensRuntime::ExternalCmd { .. } => "external_cmd",
    }
}

pub(super) fn register_cpu_explicit_dense_onnx_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
) -> Result<(), EngineCalyxContentError> {
    let file_spec = OnnxFileSpec::from_lens_spec(spec)
        .map_err(|error| {
            measurement_failed(
                "load_cpu_explicit_lens",
                format!(
                    "prepare CPU-explicit ONNX lens {} failed: {}: {}",
                    spec.name, error.code, error.message
                ),
            )
        })?
        .with_provider_policy(OnnxProviderPolicy::CpuExplicit);
    let lens = OnnxLens::from_files(file_spec).map_err(|error| {
        measurement_failed(
            "load_cpu_explicit_lens",
            format!(
                "load CPU-explicit ONNX lens {} failed: {}: {}",
                spec.name, error.code, error.message
            ),
        )
    })?;
    register_cpu_explicit_loaded_lens(registry, snapshot, spec, lens, "ONNX dense")
}

pub(super) fn register_cpu_explicit_fastembed_sparse_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
) -> Result<(), EngineCalyxContentError> {
    let lens =
        FastembedSparseLens::from_lens_spec_with_policy(spec, OnnxProviderPolicy::CpuExplicit)
            .map_err(|error| {
                measurement_failed(
                    "load_cpu_explicit_lens",
                    format!(
                        "load CPU-explicit fastembed sparse lens {} failed: {}: {}",
                        spec.name, error.code, error.message
                    ),
                )
            })?;
    register_cpu_explicit_loaded_lens(registry, snapshot, spec, lens, "fastembed sparse")
}

pub(super) fn register_cpu_explicit_fastembed_bgem3_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
) -> Result<(), EngineCalyxContentError> {
    let lens =
        FastembedBgem3Lens::from_lens_spec_with_policy(spec, OnnxProviderPolicy::CpuExplicit)
            .map_err(|error| {
                measurement_failed(
                    "load_cpu_explicit_lens",
                    format!(
                        "load CPU-explicit fastembed BGE-M3 lens {} failed: {}: {}",
                        spec.name, error.code, error.message
                    ),
                )
            })?;
    register_cpu_explicit_loaded_lens(registry, snapshot, spec, lens, "fastembed BGE-M3")
}

pub(super) fn register_cpu_explicit_onnx_colbert_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
) -> Result<(), EngineCalyxContentError> {
    let file_spec = OnnxColbertFileSpec::from_lens_spec(spec)
        .map_err(|error| {
            measurement_failed(
                "load_cpu_explicit_lens",
                format!(
                    "prepare CPU-explicit ONNX ColBERT lens {} failed: {}: {}",
                    spec.name, error.code, error.message
                ),
            )
        })?
        .with_provider_policy(OnnxProviderPolicy::CpuExplicit);
    let lens = OnnxColbertLens::from_files(file_spec).map_err(|error| {
        measurement_failed(
            "load_cpu_explicit_lens",
            format!(
                "load CPU-explicit ONNX ColBERT lens {} failed: {}: {}",
                spec.name, error.code, error.message
            ),
        )
    })?;
    register_cpu_explicit_loaded_lens(registry, snapshot, spec, lens, "ONNX ColBERT")
}

pub(super) fn register_cpu_explicit_fastembed_qwen3_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
) -> Result<(), EngineCalyxContentError> {
    let lens =
        FastembedQwen3Lens::from_lens_spec_with_policy(spec, CandleDevicePolicy::CpuExplicit)
            .map_err(|error| {
                measurement_failed(
                    "load_cpu_explicit_lens",
                    format!(
                        "load CPU-explicit fastembed qwen3 lens {} failed: {}: {}",
                        spec.name, error.code, error.message
                    ),
                )
            })?;
    register_cpu_explicit_loaded_lens(registry, snapshot, spec, lens, "fastembed qwen3")
}

pub(super) fn register_cpu_explicit_candle_local_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
) -> Result<(), EngineCalyxContentError> {
    let lens = CandleLens::from_lens_spec_with_policy(spec, CandleDevicePolicy::CpuExplicit)
        .map_err(|error| {
            measurement_failed(
                "load_cpu_explicit_lens",
                format!(
                    "load CPU-explicit candle-local lens {} failed: {}: {}",
                    spec.name, error.code, error.message
                ),
            )
        })?;
    register_cpu_explicit_loaded_lens(registry, snapshot, spec, lens, "candle-local")
}

pub(super) fn register_cpu_explicit_static_lookup_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
) -> Result<(), EngineCalyxContentError> {
    let lens = StaticLookupLens::from_lens_spec(spec).map_err(|error| {
        measurement_failed(
            "load_cpu_explicit_lens",
            format!(
                "load CPU-explicit static-lookup lens {} failed: {}: {}",
                spec.name, error.code, error.message
            ),
        )
    })?;
    register_cpu_explicit_loaded_lens(registry, snapshot, spec, lens, "static-lookup")
}

pub(super) fn register_cpu_explicit_loaded_lens<L>(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
    lens: L,
    runtime_label: &str,
) -> Result<(), EngineCalyxContentError>
where
    L: Lens + 'static,
{
    registry
        .register_frozen_with_spec(lens, snapshot.contract.clone(), spec.clone())
        .map_err(|error| {
            measurement_failed(
                "load_cpu_explicit_lens",
                format!(
                    "register CPU-explicit {runtime_label} lens {} failed: {}: {}",
                    spec.name, error.code, error.message
                ),
            )
        })?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CpuExplicitTextRuntime {
    DenseOnnx,
    FastembedSparse,
    FastembedBgem3,
    OnnxColbert,
    FastembedQwen3,
    CandleLocal,
    StaticLookup,
}

pub(super) fn cpu_explicit_text_runtime(spec: &LensSpec) -> Option<CpuExplicitTextRuntime> {
    match (&spec.output, &spec.runtime) {
        (SlotShape::Dense(_), LensRuntime::Onnx { .. }) => Some(CpuExplicitTextRuntime::DenseOnnx),
        (SlotShape::Sparse(_), LensRuntime::FastembedSparse { .. }) => {
            Some(CpuExplicitTextRuntime::FastembedSparse)
        }
        (
            SlotShape::Dense(_) | SlotShape::Sparse(_) | SlotShape::Multi { .. },
            LensRuntime::FastembedBgem3 { .. },
        ) => Some(CpuExplicitTextRuntime::FastembedBgem3),
        (SlotShape::Multi { .. }, LensRuntime::OnnxColbert { .. }) => {
            Some(CpuExplicitTextRuntime::OnnxColbert)
        }
        // #1775: the remaining locally-runnable learned text families. qwen3 and
        // candle-local are dense candle encoders run on an explicit CPU device
        // policy; static-lookup is a dense mmap embedding table (CPU by nature).
        (SlotShape::Dense(_), LensRuntime::FastembedQwen3 { .. }) => {
            Some(CpuExplicitTextRuntime::FastembedQwen3)
        }
        (SlotShape::Dense(_), LensRuntime::CandleLocal { .. }) => {
            Some(CpuExplicitTextRuntime::CandleLocal)
        }
        (SlotShape::Dense(_), LensRuntime::StaticLookup { .. }) => {
            Some(CpuExplicitTextRuntime::StaticLookup)
        }
        _ => None,
    }
}

pub(super) fn cpu_explicit_text_candidate(slot: &Slot) -> bool {
    matches!(slot.state, SlotState::Active | SlotState::Parked)
        && slot.modality == Modality::Text
        && !slot.retrieval_only
        && !slot.excluded_from_dedup
}

/// True when a lens-load failure is the addon-absent signature (ORT unreachable
/// or missing on-disk weights/embeddings), not a genuine corruption/regression.
/// The load helpers embed `error.code` in the reason string (#1791/#1890).
pub(super) fn is_lens_unreachable(error: &EngineCalyxContentError) -> bool {
    match error {
        EngineCalyxContentError::Measurement { reason, .. } => {
            if reason.contains("CALYX_LENS_UNREACHABLE") {
                return true;
            }
            // Static-lookup / ONNX config points at files the AI addon never
            // installed: treat as pending_measurement, not hard authoring fail.
            let missing_asset = reason.contains("is not a file")
                || reason.contains("not found")
                || reason.contains("No such file")
                || reason.contains("os error 2")
                || reason.contains("The system cannot find");
            reason.contains("CALYX_LENS_CONFIG_INVALID") && missing_asset
        }
        _ => false,
    }
}

/// A declared-but-unavailable learned lens. It occupies the panel slot (so the
/// declared-lens gate and A37 roster identity are unchanged) yet always reports
/// `CALYX_LENS_UNREACHABLE` at measure time, which the content measure path
/// degrades to `absent(LensUnavailable)` and records as `pending_measurement`
/// for addon-time backfill (#1791).
#[derive(Clone)]
struct PendingUnavailableLens {
    id: LensId,
    shape: SlotShape,
    modality: Modality,
}

impl Lens for PendingUnavailableLens {
    fn id(&self) -> LensId {
        self.id
    }

    fn shape(&self) -> SlotShape {
        self.shape
    }

    fn modality(&self) -> Modality {
        self.modality
    }

    fn measure(&self, _input: &Input) -> CalyxResult<SlotVector> {
        Err(CalyxError::lens_unreachable(format!(
            "learned lens {} is unavailable (AI addon absent); measurement deferred for backfill",
            self.id
        )))
    }
}

pub(super) fn register_pending_unavailable_lens(
    registry: &mut Registry,
    snapshot: &RegistryLensSnapshot,
    spec: &LensSpec,
) -> Result<(), EngineCalyxContentError> {
    let lens = PendingUnavailableLens {
        id: snapshot.contract.lens_id(),
        shape: snapshot.contract.shape(),
        modality: snapshot.contract.modality(),
    };
    registry
        .register_frozen_with_spec(lens, snapshot.contract.clone(), spec.clone())
        .map_err(|error| {
            measurement_failed(
                "load_cpu_explicit_lens",
                format!(
                    "register pending-unavailable lens {} failed: {}: {}",
                    spec.name, error.code, error.message
                ),
            )
        })?;
    Ok(())
}
