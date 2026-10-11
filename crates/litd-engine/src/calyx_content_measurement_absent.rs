//! Addon-absent content-measurement panel (#1791 pending path).
//!
//! When the canonical CPU-explicit content panel cannot be surfaced -- the AI
//! addon (an sm_120 ONNX Runtime) is absent, or the canonical panel vault is not
//! materialized on this host -- editor authoring must still SUCCEED and record a
//! `pending_measurement` row, never hard-fail (the #1791 guarantee). This module
//! builds a synthetic panel whose learned lenses are all UNREACHABLE at measure
//! time: persistence + lowering run on the explicit payload while every learned
//! encoder degrades to `absent(LensUnavailable)`, which the content measure path
//! records as `pending_measurement` for a later backfill. This mirrors the
//! world-environment FSV fixture but as a first-class, reusable engine surface so
//! every content-authoring capability shares ONE addon-absent contract.

use std::{collections::BTreeMap, path::PathBuf};

use calyx_core::{
    Asymmetry, CalyxError, Input, Lens, LensId, Modality, Panel, QuantPolicy, Result, Slot, SlotId,
    SlotKey, SlotShape, SlotState, SlotVector,
};
use calyx_registry::{
    FrozenLensContract, LensDType, LensRuntime, LensSpec, NormPolicy, Registry, VaultPanelState,
};
use sha2::Digest;

use crate::calyx_content_measurement::EngineContentMeasurementConfig;

/// Learned-lens count for the addon-absent panel. Must stay >= the diversity gate
/// panel size (`DEFAULT_GATE_PANEL_LENSES` == 10) so the panel carries the full
/// learned-encoder roster the canonical panel would, every slot unreachable.
pub const ADDON_ABSENT_CONTENT_PANEL_LENSES: usize = 12;

/// A learned lens that is always unreachable (AI addon absent).
#[derive(Clone)]
struct UnavailableLens {
    id: LensId,
}

impl Lens for UnavailableLens {
    fn id(&self) -> LensId {
        self.id
    }

    fn shape(&self) -> SlotShape {
        SlotShape::Dense(4)
    }

    fn modality(&self) -> Modality {
        Modality::Text
    }

    fn measure(&self, _input: &Input) -> Result<SlotVector> {
        Err(CalyxError::lens_unreachable(format!(
            "addon-absent content panel learned lens {} unavailable (AI addon absent)",
            self.id
        )))
    }
}

/// Build a content-measurement panel where every learned lens is unreachable, so a
/// measure over it degrades to `pending_measurement` while persistence + lowering
/// still succeed on the explicit payload.
pub fn addon_absent_content_panel_state(lens_count: usize) -> VaultPanelState {
    let mut registry = Registry::new();
    let mut slots = Vec::with_capacity(lens_count);
    for idx in 0..lens_count {
        let name = fixture_lens_name(idx);
        let weights_sha256 = sha32(format!("weights:{name}").as_bytes());
        let corpus_hash = sha32(format!("corpus:{name}").as_bytes());
        let contract = FrozenLensContract::new(
            name.clone(),
            weights_sha256,
            corpus_hash,
            SlotShape::Dense(4),
            Modality::Text,
            LensDType::F32,
            NormPolicy::unit(),
        );
        let lens_id = contract.lens_id();
        let spec = lens_spec(&name, weights_sha256, corpus_hash);
        registry
            .register_frozen_with_spec(UnavailableLens { id: lens_id }, contract, spec)
            .expect("addon-absent frozen lens registration is deterministic");
        let slot_id = SlotId::new(idx as u16);
        slots.push(Slot {
            slot_id,
            slot_key: SlotKey::new(slot_id, name.clone()),
            lens_id,
            shape: SlotShape::Dense(4),
            modality: Modality::Text,
            asymmetry: Asymmetry::None,
            quant: QuantPolicy::turboquant_default(),
            resource: Default::default(),
            axis: Some(name),
            retrieval_only: false,
            excluded_from_dedup: false,
            bits_about: BTreeMap::new(),
            state: SlotState::Active,
            added_at_panel_version: idx as u32 + 1,
        });
    }
    VaultPanelState {
        panel: Panel {
            version: lens_count as u32,
            slots,
            created_at: 1_786_000_000,
            kernel_ref: None,
            guard_ref: None,
        },
        registry,
        registry_snapshot: None,
    }
}

/// A measurement config backed by the addon-absent panel. Persistence + lowering
/// succeed; the row is recorded `pending_measurement` (#1791) for later backfill.
pub fn addon_absent_content_measurement(
    label: impl Into<String>,
) -> EngineContentMeasurementConfig {
    EngineContentMeasurementConfig::from_preloaded_state(
        label,
        addon_absent_content_panel_state(ADDON_ABSENT_CONTENT_PANEL_LENSES),
    )
}

fn lens_spec(name: &str, weights_sha256: [u8; 32], corpus_hash: [u8; 32]) -> LensSpec {
    LensSpec {
        name: name.to_string(),
        runtime: LensRuntime::StaticLookup {
            embeddings_file: PathBuf::from(format!("addon-absent/{name}.cslm")),
            tokenizer: PathBuf::from("addon-absent/tokenizer.json"),
            dim: 4,
        },
        output: SlotShape::Dense(4),
        modality: Modality::Text,
        weights_sha256,
        corpus_hash,
        norm_policy: NormPolicy::unit(),
        max_batch: None,
        axis: Some(name.to_string()),
        asymmetry: Asymmetry::None,
        quant_default: QuantPolicy::turboquant_default(),
        truncate_dim: None,
        recall_delta: 0.02,
        retrieval_only: false,
        excluded_from_dedup: false,
    }
}

fn fixture_lens_name(index: usize) -> String {
    match index {
        0 => "semantic-general-alpha".to_string(),
        1 => "semantic-general-beta".to_string(),
        2 => "legal-domain".to_string(),
        3 => "medical-domain".to_string(),
        4 => "splade-lexical".to_string(),
        5 => "colbert-late-interaction".to_string(),
        6 => "entity-graph".to_string(),
        7 => "structural-style".to_string(),
        8 => "scientific-domain".to_string(),
        9 => "financial-domain".to_string(),
        other => format!("semantic-extra-{other}"),
    }
}

fn sha32(bytes: &[u8]) -> [u8; 32] {
    let digest = sha2::Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}
