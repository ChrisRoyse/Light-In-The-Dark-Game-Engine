use std::{collections::BTreeMap, path::PathBuf};

use calyx_core::{
    Asymmetry, Input, Lens, LensId, Modality, QuantPolicy, Slot, SlotId, SlotKey, SlotShape,
    SlotState, SlotVector,
};
use calyx_registry::{
    FrozenLensContract, LensDType, LensRuntime, LensSpec, NormPolicy, Registry, VaultPanelState,
};
use sha2::Digest;

#[derive(Clone)]
struct FixtureLearnedLens {
    id: LensId,
    salt: u8,
}

impl Lens for FixtureLearnedLens {
    fn id(&self) -> LensId {
        self.id
    }

    fn shape(&self) -> SlotShape {
        SlotShape::Dense(4)
    }

    fn modality(&self) -> Modality {
        Modality::Text
    }

    fn measure(&self, input: &Input) -> calyx_core::Result<SlotVector> {
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

pub fn fixture_panel_state(lens_count: usize) -> VaultPanelState {
    let mut registry = Registry::new();
    let mut slots = Vec::new();
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
        registry
            .register_frozen_with_spec(
                FixtureLearnedLens {
                    id: lens_id,
                    salt: idx as u8,
                },
                contract,
                lens_spec(&name, weights_sha256, corpus_hash),
            )
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
        panel: calyx_core::Panel {
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
        other => format!("semantic-extra-{other}"),
    }
}

fn sha32(bytes: &[u8]) -> [u8; 32] {
    let digest = sha2::Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}
