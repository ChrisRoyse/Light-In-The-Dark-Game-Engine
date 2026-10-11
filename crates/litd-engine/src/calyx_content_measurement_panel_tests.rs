use std::path::PathBuf;

use calyx_core::{
    Asymmetry, LensId, Modality, Placement, QuantPolicy, Slot, SlotId, SlotKey, SlotResource,
    SlotShape, SlotState,
};
use calyx_registry::{
    CapabilitySignalKind, FastembedBgem3Output, LensRuntime, LensSpec, NormPolicy,
    signal_kind_from_spec,
};

use super::register::{
    CpuExplicitTextRuntime, cpu_explicit_text_candidate, cpu_explicit_text_runtime,
    lens_runtime_label,
};

#[test]
fn cpu_explicit_text_runtime_accepts_sparse_and_colbert_a37_families() {
    assert_eq!(
        cpu_explicit_text_runtime(&spec(
            SlotShape::Dense(768),
            LensRuntime::Onnx {
                model_id: "dense".to_string(),
                files: files()
            }
        )),
        Some(CpuExplicitTextRuntime::DenseOnnx)
    );
    assert_eq!(
        cpu_explicit_text_runtime(&spec(
            SlotShape::Sparse(30_522),
            LensRuntime::FastembedSparse {
                model_id: "splade_pp_en_v1".to_string(),
                files: files()
            }
        )),
        Some(CpuExplicitTextRuntime::FastembedSparse)
    );
    assert_eq!(
        cpu_explicit_text_runtime(&spec(
            SlotShape::Multi { token_dim: 384 },
            LensRuntime::OnnxColbert {
                model_id: "answerai-colbert-small-v1".to_string(),
                files: files()
            }
        )),
        Some(CpuExplicitTextRuntime::OnnxColbert)
    );
    assert_eq!(
        cpu_explicit_text_runtime(&spec(
            SlotShape::Multi { token_dim: 1024 },
            LensRuntime::FastembedBgem3 {
                model_id: "bge-m3".to_string(),
                files: files(),
                output: FastembedBgem3Output::Colbert
            }
        )),
        Some(CpuExplicitTextRuntime::FastembedBgem3)
    );
}

#[test]
fn cpu_explicit_text_runtime_rejects_shape_runtime_mismatches() {
    assert_eq!(
        cpu_explicit_text_runtime(&spec(
            SlotShape::Sparse(30_522),
            LensRuntime::Onnx {
                model_id: "dense".to_string(),
                files: files()
            }
        )),
        None
    );
    assert_eq!(
        cpu_explicit_text_runtime(&spec(
            SlotShape::Dense(384),
            LensRuntime::OnnxColbert {
                model_id: "answerai-colbert-small-v1".to_string(),
                files: files()
            }
        )),
        None
    );
}

/// Multi-embedder mandate (#1775): the CPU-explicit content panel now SURFACES
/// every locally-runnable learned text family -- fastembed-qwen3 and candle-local
/// (dense candle encoders on an explicit CPU device policy) and static-lookup (a
/// dense mmap embedding table). Before #1775 these were learned-but-unsurfaceable
/// and the loader failed loud; now they resolve to a concrete CPU runtime so the
/// derived A37 roster includes them.
#[test]
fn cpu_explicit_text_runtime_surfaces_local_learned_text_families() {
    let qwen3 = spec(
        SlotShape::Dense(1024),
        LensRuntime::FastembedQwen3 {
            model_id: "Qwen/Qwen3-Embedding-0.6B".to_string(),
            files: files(),
            dtype: "f32".to_string(),
        },
    );
    assert_eq!(
        signal_kind_from_spec(&qwen3),
        CapabilitySignalKind::LearnedEncoder
    );
    assert_eq!(
        cpu_explicit_text_runtime(&qwen3),
        Some(CpuExplicitTextRuntime::FastembedQwen3)
    );

    let candle = spec(
        SlotShape::Dense(768),
        LensRuntime::CandleLocal {
            model_id: "intfloat/e5-large".to_string(),
            files: files(),
            dtype: "f32".to_string(),
            pooling: "mean".to_string(),
        },
    );
    assert_eq!(
        signal_kind_from_spec(&candle),
        CapabilitySignalKind::LearnedEncoder
    );
    assert_eq!(
        cpu_explicit_text_runtime(&candle),
        Some(CpuExplicitTextRuntime::CandleLocal)
    );

    let static_lookup = spec(
        SlotShape::Dense(256),
        LensRuntime::StaticLookup {
            embeddings_file: PathBuf::from("model2vec.cxlkup"),
            tokenizer: PathBuf::from("tokenizer.json"),
            dim: 256,
        },
    );
    assert_eq!(
        signal_kind_from_spec(&static_lookup),
        CapabilitySignalKind::LearnedEncoder
    );
    assert_eq!(
        cpu_explicit_text_runtime(&static_lookup),
        Some(CpuExplicitTextRuntime::StaticLookup)
    );
}

/// The non-local / non-content-embedding learned text families stay unsurfaceable
/// so the loader fails loud rather than silently parking them (#1775 decision):
/// tei-http (remote endpoint), multimodal-adapter (external cross-modal subprocess
/// bridge, no in-process CPU device policy) and fastembed-reranker (a cross-encoder
/// that emits a Dense(1) relevance score, not a standalone content vector).
#[test]
fn non_local_learned_text_families_stay_unsurfaceable_and_fail_loud() {
    let tei = spec(
        SlotShape::Dense(1024),
        LensRuntime::TeiHttp {
            endpoint: "https://tei.internal/embed".to_string(),
        },
    );
    assert_eq!(
        signal_kind_from_spec(&tei),
        CapabilitySignalKind::LearnedEncoder
    );
    assert_eq!(cpu_explicit_text_runtime(&tei), None);

    let reranker = spec(
        SlotShape::Dense(1),
        LensRuntime::FastembedReranker {
            model_id: "bge-reranker-base".to_string(),
            files: files(),
        },
    );
    assert_eq!(
        signal_kind_from_spec(&reranker),
        CapabilitySignalKind::LearnedEncoder
    );
    assert_eq!(cpu_explicit_text_runtime(&reranker), None);

    let multimodal = spec(
        SlotShape::Dense(512),
        LensRuntime::MultimodalAdapter {
            axis: "text".to_string(),
            model_id: "jina-clip-v2".to_string(),
            adapter_config: Some(PathBuf::from("adapter.json")),
            files: files(),
        },
    );
    assert_eq!(
        signal_kind_from_spec(&multimodal),
        CapabilitySignalKind::LearnedEncoder
    );
    assert_eq!(cpu_explicit_text_runtime(&multimodal), None);
}

#[test]
fn lens_runtime_label_is_stable_for_every_runtime_family() {
    assert_eq!(
        lens_runtime_label(&LensRuntime::FastembedQwen3 {
            model_id: "q".to_string(),
            files: files(),
            dtype: "f32".to_string(),
        }),
        "fastembed_qwen3"
    );
    assert_eq!(
        lens_runtime_label(&LensRuntime::Onnx {
            model_id: "d".to_string(),
            files: files(),
        }),
        "onnx"
    );
    assert_eq!(
        lens_runtime_label(&LensRuntime::OnnxColbert {
            model_id: "c".to_string(),
            files: files(),
        }),
        "onnx_colbert"
    );
    assert_eq!(
        lens_runtime_label(&LensRuntime::FastembedSparse {
            model_id: "s".to_string(),
            files: files(),
        }),
        "fastembed_sparse"
    );
    assert_eq!(
        lens_runtime_label(&LensRuntime::CandleLocal {
            model_id: "e".to_string(),
            files: files(),
            dtype: "f32".to_string(),
            pooling: "mean".to_string(),
        }),
        "candle_local"
    );
}

#[test]
fn cpu_explicit_text_candidate_accepts_active_and_parked_text_slots_only() {
    assert!(cpu_explicit_text_candidate(&slot(
        SlotState::Active,
        Modality::Text,
        false,
        false
    )));
    assert!(cpu_explicit_text_candidate(&slot(
        SlotState::Parked,
        Modality::Text,
        false,
        false
    )));
    assert!(!cpu_explicit_text_candidate(&slot(
        SlotState::Retired,
        Modality::Text,
        false,
        false
    )));
    assert!(!cpu_explicit_text_candidate(&slot(
        SlotState::Active,
        Modality::Image,
        false,
        false
    )));
    assert!(!cpu_explicit_text_candidate(&slot(
        SlotState::Active,
        Modality::Text,
        true,
        false
    )));
    assert!(!cpu_explicit_text_candidate(&slot(
        SlotState::Active,
        Modality::Text,
        false,
        true
    )));
}

fn spec(output: SlotShape, runtime: LensRuntime) -> LensSpec {
    LensSpec {
        name: "unit-lens".to_string(),
        runtime,
        output,
        modality: Modality::Text,
        weights_sha256: [1; 32],
        corpus_hash: [2; 32],
        norm_policy: NormPolicy::Finite,
        max_batch: None,
        axis: None,
        asymmetry: Asymmetry::None,
        quant_default: QuantPolicy::turboquant_default(),
        truncate_dim: None,
        recall_delta: 0.02,
        retrieval_only: false,
        excluded_from_dedup: false,
    }
}

fn files() -> Vec<PathBuf> {
    vec![
        PathBuf::from("model.onnx"),
        PathBuf::from("tokenizer.json"),
        PathBuf::from("config.json"),
    ]
}

fn slot(
    state: SlotState,
    modality: Modality,
    retrieval_only: bool,
    excluded_from_dedup: bool,
) -> Slot {
    let slot_id = SlotId::new(7);
    Slot {
        slot_id,
        slot_key: SlotKey::new(slot_id, "unit-text"),
        lens_id: LensId::from_bytes([7; 16]),
        shape: SlotShape::Dense(768),
        modality,
        asymmetry: Asymmetry::None,
        quant: QuantPolicy::None,
        resource: SlotResource {
            cost: Default::default(),
            placement: Placement::Cpu,
        },
        axis: None,
        retrieval_only,
        excluded_from_dedup,
        bits_about: Default::default(),
        state,
        added_at_panel_version: 1,
    }
}
