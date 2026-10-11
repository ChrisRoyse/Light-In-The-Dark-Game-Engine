//! Deterministic panel fixtures for the issue #1791 pending/backfill FSV.
//!
//! This box has NO ORT_DYLIB_PATH and no model cache, so we cannot load the real
//! learned lenses. Instead we register learned-encoder lenses (real frozen
//! contracts + StaticLookup specs, so `signal_kind == LearnedEncoder` and the
//! declared >=10 gate is exercised) in two flavours:
//!
//! - Available: measures a deterministic dense vector without any runtime -- this
//!   stands in for "the AI addon is installed".
//! - Unavailable: always returns `CALYX_LENS_UNREACHABLE` at measure time, which
//!   the content measure path degrades to `absent(LensUnavailable)` and records as
//!   `pending_measurement` -- exactly the addon-absent code path the real ONNX
//!   loader takes, proven here without ONNX.

use std::{collections::BTreeMap, path::PathBuf};

use calyx_core::{
    Asymmetry, CalyxError, Input, Lens, LensId, Modality, Panel, QuantPolicy, Result, Slot, SlotId,
    SlotKey, SlotShape, SlotState, SlotVector,
};
use calyx_registry::{
    FrozenLensContract, LensDType, LensRuntime, LensSpec, NormPolicy, Registry, VaultPanelState,
};
use sha2::Digest;

#[derive(Clone)]
struct AvailableLens {
    id: LensId,
    salt: u8,
}

impl Lens for AvailableLens {
    fn id(&self) -> LensId {
        self.id
    }

    fn shape(&self) -> SlotShape {
        SlotShape::Dense(4)
    }

    fn modality(&self) -> Modality {
        Modality::Text
    }

    fn measure(&self, input: &Input) -> Result<SlotVector> {
        let digest = sha2::Sha256::digest([&[self.salt], input.bytes.as_slice()].concat());
        let mut data = (0..4)
            .map(|idx| (f32::from(digest[idx]) + 1.0) / 256.0)
            .collect::<Vec<_>>();
        let norm = data.iter().map(|value| value * value).sum::<f32>().sqrt();
        for value in &mut data {
            *value /= norm;
        }
        Ok(SlotVector::Dense { dim: 4, data })
    }
}

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
            "fixture learned lens {} unavailable (AI addon absent)",
            self.id
        )))
    }
}

/// A panel where every learned lens measures (AI addon installed).
pub fn available_panel_state(lens_count: usize) -> VaultPanelState {
    build(&vec![true; lens_count])
}

/// A panel where every learned lens is unreachable (AI addon absent).
pub fn unavailable_panel_state(lens_count: usize) -> VaultPanelState {
    build(&vec![false; lens_count])
}

/// A panel with a mix of available and unavailable learned lenses -- an
/// inconsistent runtime that the measure path must reject (all-or-none).
pub fn mixed_panel_state(available: usize, unavailable: usize) -> VaultPanelState {
    let mut flags = vec![true; available];
    flags.extend(vec![false; unavailable]);
    build(&flags)
}

fn build(availability: &[bool]) -> VaultPanelState {
    let mut registry = Registry::new();
    let mut slots = Vec::new();
    for (idx, &available) in availability.iter().enumerate() {
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
        if available {
            registry
                .register_frozen_with_spec(
                    AvailableLens {
                        id: lens_id,
                        salt: idx as u8,
                    },
                    contract,
                    spec,
                )
                .unwrap();
        } else {
            registry
                .register_frozen_with_spec(UnavailableLens { id: lens_id }, contract, spec)
                .unwrap();
        }
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
