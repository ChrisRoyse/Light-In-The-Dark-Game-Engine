//! Shared learned-lens panel fixture for the world-environment FSV harnesses
//! (#808/#814/#815/#816). Copied in spirit from the #1791 fixture: this box has
//! NO ORT and no model cache, so the learned lenses are registered as frozen
//! contracts that are UNAVAILABLE at measure time -- exactly the addon-absent path
//! the real ONNX loader takes, which the content measure path degrades to
//! `absent(LensUnavailable)` and records as `pending_measurement`. Persistence and
//! lowering still succeed on the explicit payload (the #1791 guarantee).

use std::{collections::BTreeMap, path::PathBuf};

use calyx_core::{
    Asymmetry, CalyxError, Input, Lens, LensId, Modality, Panel, QuantPolicy, Result, Slot, SlotId,
    SlotKey, SlotShape, SlotState, SlotVector,
};
use calyx_registry::{
    FrozenLensContract, LensDType, LensRuntime, LensSpec, NormPolicy, Registry, VaultPanelState,
};
use sha2::Digest;

pub const PANEL_LENSES: usize = 12;

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
            "world-environment fixture learned lens {} unavailable (AI addon absent)",
            self.id
        )))
    }
}

/// A panel where every learned lens is unreachable (AI addon absent).
pub fn unavailable_panel_state() -> VaultPanelState {
    let availability = [false; PANEL_LENSES];
    let mut registry = Registry::new();
    let mut slots = Vec::new();
    for (idx, _available) in availability.iter().enumerate() {
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
            .unwrap();
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
            version: availability.len() as u32,
            slots,
            created_at: 1_786_000_000,
            kernel_ref: None,
            guard_ref: None,
        },
        registry,
        registry_snapshot: None,
    }
}

fn lens_spec(name: &str, weights_sha256: [u8; 32], corpus_hash: [u8; 32]) -> LensSpec {
    LensSpec {
        name: name.to_string(),
        runtime: LensRuntime::StaticLookup {
            embeddings_file: PathBuf::from(format!("fixture/{name}.cslm")),
            tokenizer: PathBuf::from("fixture/tokenizer.json"),
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
