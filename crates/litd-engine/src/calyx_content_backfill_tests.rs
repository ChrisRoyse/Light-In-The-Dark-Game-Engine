//! Unit coverage for #1791: pending persist + addon-time backfill.

use std::{collections::BTreeMap, path::PathBuf};

use calyx_core::{
    AbsentReason, Asymmetry, CalyxError, Input, Lens, LensId, Modality, Panel, QuantPolicy, Result,
    Slot, SlotId, SlotKey, SlotShape, SlotState, SlotVector,
};
use calyx_registry::{
    FrozenLensContract, LensDType, LensRuntime, LensSpec, NormPolicy, Registry, VaultPanelState,
};
use serde_json::json;
use sha2::Digest;

use crate::{
    calyx_content::{
        ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE, ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentKind,
        EngineContentMeasurementConfig, EngineContentRecord, EngineContentSource,
    },
    calyx_lowering::EngineLoweringValve,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    test_temp::TestTempDir,
};

const PANEL_LENSES: usize = 12;

#[test]
fn unavailable_lenses_persist_pending_measurement() {
    let (store, _dir) = store_with(unavailable_panel(PANEL_LENSES), "pending");
    let row = store.put_content(&record("area-pending")).unwrap();

    assert_eq!(
        row.measurement.measurement_state,
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING
    );
    assert_eq!(row.measurement.measured_learned_lenses, 0);
    assert_eq!(row.measurement.declared_learned_lenses, PANEL_LENSES);
    assert_eq!(row.measurement.pending_learned_lenses.len(), PANEL_LENSES);
    assert!(row.measurement.measured_slot_ids.is_empty());
    for pending in &row.measurement.pending_learned_lenses {
        assert!(matches!(
            row.constellation.slots.get(&SlotId::new(pending.slot)),
            Some(SlotVector::Absent {
                reason: AbsentReason::LensUnavailable
            })
        ));
    }
    // Durable: reads back identically.
    let read = store
        .get_content_for_editor(EngineContentKind::Terrain, "area-pending")
        .unwrap();
    assert_eq!(read.database.sha256, row.database.sha256);
    assert_eq!(
        read.measurement.measurement_state,
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING
    );
}

#[test]
fn pending_row_lowers_without_learned_lenses() {
    let (store, _dir) = store_with(unavailable_panel(PANEL_LENSES), "pending-lower");
    let row = store.put_content(&record("area-lower")).unwrap();
    let package = EngineLoweringValve::lower_content_for_sim(&row).unwrap();
    let lowered = EngineLoweringValve::verify_lowered_package(&package).unwrap();
    assert_eq!(lowered.source_cx_id, row.cx_id);
    assert!(lowered.field_count >= 1);
}

#[test]
fn backfill_completes_pending_row_with_two_phase_provenance() {
    let dir = TestTempDir::new("issue1791-backfill");
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(dir.path())).unwrap();
    let absent = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "absent",
            unavailable_panel(PANEL_LENSES),
        ),
    );
    let pending = absent.put_content(&record("area-bf")).unwrap();
    assert_eq!(
        pending.measurement.measurement_state,
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING
    );

    let present = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "present",
            available_panel(PANEL_LENSES),
        ),
    );
    let outcome = present
        .backfill_pending_content(EngineContentKind::Terrain, "area-bf")
        .unwrap();
    assert!(outcome.was_pending);
    assert!(outcome.completed);
    assert_eq!(
        outcome.phase1_state,
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING
    );
    assert_eq!(
        outcome.phase2_state,
        ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE
    );

    let complete = present
        .get_content_for_editor(EngineContentKind::Terrain, "area-bf")
        .unwrap();
    assert_eq!(
        complete.measurement.measurement_state,
        ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE
    );
    assert!(complete.measurement.measured_learned_lenses >= 10);
    assert!(complete.measurement.pending_learned_lenses.is_empty());
    // Same authored input -> same identity, higher ledger seq.
    assert_eq!(complete.cx_id, pending.cx_id);
    assert!(complete.database.seq > pending.database.seq);
    let backfill = complete.database.provenance.get("backfill").unwrap();
    assert_eq!(
        backfill["phase1_state"],
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING
    );
    assert_eq!(
        backfill["phase2_state"],
        ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE
    );

    // Idempotent: re-running on the completed row is a no-op.
    let again = present
        .backfill_pending_content(EngineContentKind::Terrain, "area-bf")
        .unwrap();
    assert!(!again.was_pending);
    assert!(!again.completed);
    let after = present
        .get_content_for_editor(EngineContentKind::Terrain, "area-bf")
        .unwrap();
    assert_eq!(after.database.seq, complete.database.seq);
}

#[test]
fn partial_learned_availability_fails_closed() {
    let (store, _dir) = store_with(mixed_panel(8, 4), "partial");
    match store.put_content(&record("area-partial")) {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => {
            assert_eq!(phase, "measurement_availability");
            assert!(reason.contains("partial"), "reason: {reason}");
        }
        other => panic!("expected partial-availability failure, got {other:?}"),
    }
}

#[test]
fn under_declared_panel_still_fails_closed() {
    // A panel that DECLARES < 10 learned lenses fails the gate even when they are
    // all measurable -- the >=10 declaration gate is unweakened (#778).
    let (store, _dir) = store_with(available_panel(9), "under-declared");
    match store.put_content(&record("area-under")) {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => {
            assert_eq!(phase, "panel_gate");
            assert!(reason.contains("at least 10"), "reason: {reason}");
        }
        other => panic!("expected panel-gate failure, got {other:?}"),
    }
}

fn store_with(state: VaultPanelState, label: &str) -> (EngineCalyxContentStore, TestTempDir) {
    let dir = TestTempDir::new(&format!("issue1791-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(dir.path())).unwrap();
    let store = EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(format!("preloaded:{label}"), state),
    );
    (store, dir)
}

fn record(id: &str) -> EngineContentRecord {
    EngineContentRecord {
        kind: EngineContentKind::Terrain,
        id: id.to_string(),
        display_name: id.replace('-', " "),
        payload: json!({
            "schema": "litd.rpg.area.v1",
            "surface_kind": "area_region",
            "id": id,
            "region_name": "Ashen Vale",
            "terrain_tag": "ashen_road"
        }),
        source: EngineContentSource {
            path: format!("editor://areas/{id}"),
            author: "issue1791-test".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1791:{id}").as_bytes()),
        },
        anchors: Vec::new(),
    }
}

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
            "test lens {} unavailable",
            self.id
        )))
    }
}

fn available_panel(n: usize) -> VaultPanelState {
    build(&vec![true; n])
}

fn unavailable_panel(n: usize) -> VaultPanelState {
    build(&vec![false; n])
}

fn mixed_panel(available: usize, unavailable: usize) -> VaultPanelState {
    let mut flags = vec![true; available];
    flags.extend(vec![false; unavailable]);
    build(&flags)
}

fn build(availability: &[bool]) -> VaultPanelState {
    let mut registry = Registry::new();
    let mut slots = Vec::new();
    for (idx, &available) in availability.iter().enumerate() {
        let name = lens_name(idx);
        let weights = sha32(format!("w:{name}").as_bytes());
        let corpus = sha32(format!("c:{name}").as_bytes());
        let contract = FrozenLensContract::new(
            name.clone(),
            weights,
            corpus,
            SlotShape::Dense(4),
            Modality::Text,
            LensDType::F32,
            NormPolicy::unit(),
        );
        let lens_id = contract.lens_id();
        let spec = spec(&name, weights, corpus);
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

fn spec(name: &str, weights: [u8; 32], corpus: [u8; 32]) -> LensSpec {
    LensSpec {
        name: name.to_string(),
        runtime: LensRuntime::StaticLookup {
            embeddings_file: PathBuf::from(format!("fixture/{name}.cslm")),
            tokenizer: PathBuf::from("fixture/tokenizer.json"),
            dim: 4,
        },
        output: SlotShape::Dense(4),
        modality: Modality::Text,
        weights_sha256: weights,
        corpus_hash: corpus,
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

fn lens_name(index: usize) -> String {
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
