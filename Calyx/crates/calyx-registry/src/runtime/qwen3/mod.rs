use std::path::PathBuf;
use std::sync::Mutex;

use calyx_core::{CalyxError, Input, Lens, LensId, Modality, Result, SlotShape, SlotVector};
use fastembed::Qwen3TextEmbedding;

use crate::frozen::{FrozenLensContract, LensDType, NormPolicy, sha256_digest};
use crate::runtime::candle::{CandleDevicePolicy, CandlePrecision};
use crate::runtime::common::{hash_files, text_from_input};
use crate::spec::{LensRuntime, LensSpec, default_recall_delta};

mod files;
mod load;

pub use files::Qwen3ModelFiles;
use load::{dense_batch, qwen3_model_id, read_config, read_model, read_tokenizer};

pub const DEFAULT_QWEN3_MODEL: &str = "Qwen/Qwen3-Embedding-0.6B";
pub const DEFAULT_QWEN3_MAX_TOKENS: usize = 32_768;

const OPTIONAL_QWEN3_FILES: &[&str] = &[
    "tokenizer_config.json",
    "special_tokens_map.json",
    "generation_config.json",
    "merges.txt",
    "vocab.json",
    "modules.json",
    "config_sentence_transformers.json",
    "1_Pooling/config.json",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Qwen3FileSpec {
    pub name: String,
    pub model_id: String,
    pub files: Qwen3ModelFiles,
    pub max_tokens: usize,
    pub device_policy: CandleDevicePolicy,
    pub precision: CandlePrecision,
    pub expected_shape: Option<SlotShape>,
    pub expected_weights_sha256: Option<[u8; 32]>,
}

pub struct FastembedQwen3Lens {
    id: LensId,
    dim: u32,
    contract: FrozenLensContract,
    files: Qwen3ModelFiles,
    device_policy: CandleDevicePolicy,
    precision: CandlePrecision,
    max_tokens: usize,
    model: Mutex<Qwen3TextEmbedding>,
}

impl FastembedQwen3Lens {
    pub fn from_model_id_with_policy(
        name: impl Into<String>,
        model_id: &str,
        cache_dir: PathBuf,
        device_policy: CandleDevicePolicy,
        precision: CandlePrecision,
    ) -> Result<Self> {
        let model_id = qwen3_model_id(model_id)?;
        let files = files::fetch_files(&cache_dir, &model_id)?;
        Self::from_files(Qwen3FileSpec {
            name: name.into(),
            model_id,
            files,
            max_tokens: DEFAULT_QWEN3_MAX_TOKENS,
            device_policy,
            precision,
            expected_shape: None,
            expected_weights_sha256: None,
        })
    }

    pub fn from_files(spec: Qwen3FileSpec) -> Result<Self> {
        if spec.max_tokens == 0 {
            return Err(config_invalid("fastembed-qwen3 max_tokens must be > 0"));
        }
        for path in spec.files.artifact_paths() {
            ensure_file("artifact", &path)?;
        }
        let weights_sha256 = hash_files(&spec.files.artifact_paths())?;
        if let Some(expected) = spec.expected_weights_sha256
            && weights_sha256 != expected
        {
            return Err(CalyxError::lens_frozen_violation(format!(
                "fastembed-qwen3 artifact hash drift for {}",
                spec.model_id
            )));
        }
        let config = read_config(&spec.files.config)?;
        let dim = u32::try_from(config.hidden_size).map_err(|_| {
            CalyxError::lens_dim_mismatch(format!(
                "Qwen3 hidden size {} exceeds u32",
                config.hidden_size
            ))
        })?;
        if let Some(expected) = spec.expected_shape
            && expected != SlotShape::Dense(dim)
        {
            return Err(CalyxError::lens_dim_mismatch(format!(
                "Qwen3 output shape Dense({dim}) != declared {expected:?}"
            )));
        }
        let tokenizer = read_tokenizer(&spec.files.tokenizer, spec.max_tokens)?;
        let model = read_model(
            &spec.files.weights,
            config,
            tokenizer,
            spec.device_policy,
            spec.precision,
        )?;
        let max_tokens = spec.max_tokens.to_string();
        let corpus_hash = sha256_digest(&[
            b"fastembed-qwen3-text-v1",
            spec.model_id.as_bytes(),
            spec.precision.as_str().as_bytes(),
            max_tokens.as_bytes(),
            b"left-padding,last-token,l2",
        ]);
        let contract = FrozenLensContract::new(
            spec.name,
            weights_sha256,
            corpus_hash,
            SlotShape::Dense(dim),
            Modality::Text,
            LensDType::F32,
            NormPolicy::unit(),
        );
        Ok(Self {
            id: contract.lens_id(),
            dim,
            contract,
            files: spec.files,
            device_policy: spec.device_policy,
            precision: spec.precision,
            max_tokens: spec.max_tokens,
            model: Mutex::new(model),
        })
    }

    pub fn from_lens_spec(spec: &LensSpec) -> Result<Self> {
        Self::from_lens_spec_with_policy(spec, CandleDevicePolicy::CudaFailLoud { ordinal: 0 })
    }

    /// Build a Qwen3 lens from a persisted spec under an EXPLICIT device policy.
    ///
    /// `from_lens_spec` pins `CudaFailLoud` -- the default no-silent-fallback GPU
    /// contract. The engine CPU-explicit content panel (#1775) calls this with
    /// `CandleDevicePolicy::CpuExplicit` so an active fastembed-qwen3 learned text
    /// slot can be surfaced and measured on CPU. That is an EXPLICIT device choice
    /// selected by the derived CPU-explicit panel, never a silent CPU fallback from
    /// a failed CUDA init -- a CUDA policy still fails loud with `CALYX_LENS_UNREACHABLE`.
    pub fn from_lens_spec_with_policy(
        spec: &LensSpec,
        device_policy: CandleDevicePolicy,
    ) -> Result<Self> {
        Self::from_files(Self::file_spec_from_lens_spec(spec, device_policy)?)
    }

    /// Resolve a persisted `LensSpec` into a concrete `Qwen3FileSpec` under the given
    /// device policy WITHOUT loading the model. Kept separate so the device-policy
    /// threading is unit-testable without a real Qwen3 checkpoint on disk.
    pub(crate) fn file_spec_from_lens_spec(
        spec: &LensSpec,
        device_policy: CandleDevicePolicy,
    ) -> Result<Qwen3FileSpec> {
        let LensRuntime::FastembedQwen3 {
            model_id,
            files,
            dtype,
        } = &spec.runtime
        else {
            return Err(config_invalid("LensSpec runtime is not fastembed-qwen3"));
        };
        let model_id = qwen3_model_id(model_id)?;
        Ok(Qwen3FileSpec {
            name: spec.name.clone(),
            model_id: model_id.clone(),
            files: Qwen3ModelFiles::from_paths(model_id, files.clone())?,
            max_tokens: DEFAULT_QWEN3_MAX_TOKENS,
            device_policy,
            precision: CandlePrecision::parse(dtype)?,
            expected_shape: Some(spec.output),
            expected_weights_sha256: Some(spec.weights_sha256),
        })
    }

    pub fn contract(&self) -> &FrozenLensContract {
        &self.contract
    }

    pub fn files(&self) -> &Qwen3ModelFiles {
        &self.files
    }

    pub const fn device_policy(&self) -> CandleDevicePolicy {
        self.device_policy
    }

    pub const fn precision(&self) -> CandlePrecision {
        self.precision
    }

    pub const fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    pub const fn runtime_name(&self) -> &'static str {
        "fastembed-qwen3"
    }

    pub fn lens_spec(&self) -> LensSpec {
        LensSpec {
            name: self.contract.name().to_string(),
            runtime: LensRuntime::FastembedQwen3 {
                model_id: self.files.model_id.clone(),
                files: self.files.artifact_paths(),
                dtype: self.precision.as_str().to_string(),
            },
            output: self.contract.shape(),
            modality: self.contract.modality(),
            weights_sha256: self.contract.weights_sha256(),
            corpus_hash: self.contract.corpus_hash(),
            norm_policy: self.contract.norm_policy(),
            max_batch: None,
            axis: None,
            asymmetry: calyx_core::Asymmetry::None,
            quant_default: calyx_core::QuantPolicy::turboquant_default(),
            truncate_dim: None,
            recall_delta: default_recall_delta(),
            retrieval_only: false,
            excluded_from_dedup: false,
        }
    }
}

impl Lens for FastembedQwen3Lens {
    fn id(&self) -> LensId {
        self.id
    }

    fn shape(&self) -> SlotShape {
        SlotShape::Dense(self.dim)
    }

    fn modality(&self) -> Modality {
        Modality::Text
    }

    fn measure(&self, input: &Input) -> Result<SlotVector> {
        let mut batch = self.measure_batch(std::slice::from_ref(input))?;
        batch.pop().ok_or_else(|| {
            CalyxError::lens_dim_mismatch(format!("lens {} returned no Qwen3 vector", self.id))
        })
    }

    fn measure_batch(&self, inputs: &[Input]) -> Result<Vec<SlotVector>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let texts = inputs
            .iter()
            .map(|input| text_from_input(self, input).map(str::to_string))
            .collect::<Result<Vec<_>>>()?;
        let model = self
            .model
            .lock()
            .map_err(|_| CalyxError::lens_unreachable("Qwen3 model mutex was poisoned"))?;
        let rows = model.embed(&texts).map_err(qwen3_error)?;
        let vectors = dense_batch(self.dim, rows, inputs.len())?;
        for vector in &vectors {
            self.contract.verify_vector(self.id, vector)?;
        }
        Ok(vectors)
    }
}

fn ensure_file(label: &str, path: &std::path::Path) -> Result<()> {
    if path.is_file() {
        return Ok(());
    }
    Err(config_invalid(format!(
        "fastembed-qwen3 {label} file {} is missing",
        path.display()
    )))
}

pub(crate) fn qwen3_error(err: candle_core::Error) -> CalyxError {
    let message = format!("Qwen3 runtime failed: {err}");
    let lower = message.to_ascii_lowercase();
    if lower.contains("out of memory") || lower.contains("memoryallocation") {
        return CalyxError {
            code: "CALYX_VRAM_OOM",
            message,
            remediation: "free VRAM, reduce batch size, or evict lower-priority GPU lenses",
        };
    }
    CalyxError::lens_unreachable(message)
}

pub(crate) fn config_invalid(message: impl Into<String>) -> CalyxError {
    CalyxError {
        code: "CALYX_LENS_CONFIG_INVALID",
        message: message.into(),
        remediation: "fix Qwen3 model/tokenizer/config or register a supported lens spec",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frozen::NormPolicy;
    use calyx_core::{Asymmetry, QuantPolicy};
    use std::path::PathBuf;

    fn qwen3_spec() -> LensSpec {
        LensSpec {
            name: "a38-qwen3-embedding".to_string(),
            runtime: LensRuntime::FastembedQwen3 {
                model_id: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                files: vec![
                    PathBuf::from("model.safetensors"),
                    PathBuf::from("tokenizer.json"),
                    PathBuf::from("config.json"),
                ],
                dtype: "f32".to_string(),
            },
            output: SlotShape::Dense(1024),
            modality: Modality::Text,
            weights_sha256: [7; 32],
            corpus_hash: [9; 32],
            norm_policy: NormPolicy::unit(),
            max_batch: None,
            axis: None,
            asymmetry: Asymmetry::None,
            quant_default: QuantPolicy::turboquant_default(),
            truncate_dim: None,
            recall_delta: default_recall_delta(),
            retrieval_only: false,
            excluded_from_dedup: false,
        }
    }

    #[test]
    fn from_lens_spec_defaults_to_cuda_fail_loud_device_policy() {
        // The default persisted-spec path stays on the no-silent-fallback GPU
        // contract; nothing about surfacing a CPU-explicit panel changes it (#1775).
        let file_spec = FastembedQwen3Lens::file_spec_from_lens_spec(
            &qwen3_spec(),
            CandleDevicePolicy::CudaFailLoud { ordinal: 0 },
        )
        .unwrap();
        assert_eq!(
            file_spec.device_policy,
            CandleDevicePolicy::CudaFailLoud { ordinal: 0 }
        );
        assert_eq!(file_spec.expected_shape, Some(SlotShape::Dense(1024)));
        assert_eq!(file_spec.precision, CandlePrecision::F32);
    }

    #[test]
    fn cpu_explicit_policy_threads_through_to_file_spec() {
        // #1775: the engine CPU-explicit content panel surfaces an active
        // fastembed-qwen3 slot by resolving the spec under CpuExplicit -- an
        // explicit CPU device choice that never touches CUDA.
        let file_spec = FastembedQwen3Lens::file_spec_from_lens_spec(
            &qwen3_spec(),
            CandleDevicePolicy::CpuExplicit,
        )
        .unwrap();
        assert_eq!(file_spec.device_policy, CandleDevicePolicy::CpuExplicit);
        assert_eq!(file_spec.device_policy.as_str(), "cpu_explicit,no_cuda");
    }

    #[test]
    fn file_spec_rejects_non_qwen3_runtime() {
        let mut spec = qwen3_spec();
        spec.runtime = LensRuntime::Onnx {
            model_id: "dense".to_string(),
            files: vec![PathBuf::from("model.onnx")],
        };
        let error =
            FastembedQwen3Lens::file_spec_from_lens_spec(&spec, CandleDevicePolicy::CpuExplicit)
                .unwrap_err();
        assert_eq!(error.code, "CALYX_LENS_CONFIG_INVALID");
    }
}
