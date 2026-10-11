use std::path::PathBuf;

use calyx_assay::MIN_ASSAY_SAMPLES;
use calyx_core::{
    Anchor, AnchorKind, AnchorValue, Asymmetry, Input, Lens, LensId, Modality, QuantPolicy, Slot,
    SlotId, SlotKey, SlotShape, SlotState, SlotVector,
};
use calyx_registry::{
    FrozenLensContract, LensDType, LensRuntime, LensSpec, NormPolicy, Registry, VaultPanelState,
};
use serde_json::json;
use sha2::Digest;

use crate::{
    calyx_content::{
        ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA, EngineCalyxContentError, EngineCalyxContentStore,
        EngineContentKind, EngineContentMeasurementConfig, EngineContentRecord,
        EngineContentSource, read_engine_content_assay, store_engine_content_assay,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
};

#[test]
fn calyx_content_constellations_cover_world_content_and_assets() {
    let (store, vault_dir) = content_store("all-kinds");
    let records = [
        content(EngineContentKind::Unit, "unit-footman"),
        content(EngineContentKind::Ability, "ability-firebolt"),
        content(EngineContentKind::Item, "item-iron-sword"),
        content(EngineContentKind::Trigger, "trigger-enter-town"),
        content(EngineContentKind::Terrain, "terrain-ashen-road"),
        content(EngineContentKind::Story, "story-opening"),
        content(EngineContentKind::Lore, "lore-dawnward"),
        content(EngineContentKind::Asset, "asset-knight-mesh"),
    ];

    let mut seqs = Vec::new();
    let mut first_cx = String::new();
    for record in &records {
        let write = store.put_content(record).unwrap();
        let editor = store
            .get_content_for_editor(record.kind, &record.id)
            .unwrap();
        assert_eq!(editor.payload, record.payload);
        assert_eq!(editor.constellation.modality, calyx_core::Modality::Text);
        assert_eq!(editor.constellation.anchors.len(), 0);
        assert_eq!(
            editor.measurement.schema,
            ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA
        );
        assert_eq!(editor.measurement.measured_learned_lenses, 10);
        assert!(editor.measurement.association_family_count >= 2);
        assert_eq!(editor.measurement.dropped_synthetic_anchor_count, 1);
        assert_eq!(editor.lowering.source_cx_id, editor.cx_id);
        assert_eq!(
            editor.lowering.hot_path_policy,
            "frozen_fingerprinted_data_only"
        );
        assert_eq!(editor.database.sha256, write.database.sha256);
        if first_cx.is_empty() {
            first_cx = editor.cx_id.clone();
        }
        seqs.push(editor.database.seq);
    }

    println!(
        "issue1116_content_constellations kinds={} seqs={:?} first_cx={} vault={}",
        records.len(),
        seqs,
        first_cx,
        vault_dir.display()
    );
}

#[test]
fn sub_ten_measurement_panel_fails_closed() {
    let (store, _vault_dir) = content_store_with_lens_count("sub-ten", 9);
    match store.put_content(&content(EngineContentKind::Story, "story-sub-ten")) {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => {
            assert_eq!(phase, "panel_gate");
            assert!(reason.contains("at least 10"));
        }
        other => panic!("expected sub-ten panel gate failure, got {other:?}"),
    }
}

#[test]
fn content_assay_measures_and_persists_sufficiency() {
    let vault_dir = unique_vault_dir("assay");
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    let store = fixture_content_store(database.clone(), "assay", 10);
    let sample_count = MIN_ASSAY_SAMPLES * 2;
    let mut readbacks = Vec::new();
    let mut labels = Vec::new();
    for idx in 0..sample_count {
        let positive = idx % 2 == 0;
        let mut record = content(EngineContentKind::Story, &format!("story-assay-{idx:02}"));
        record.payload = json!({
            "id": record.id,
            "kind": "story",
            "known_value": if positive { 4 } else { 2 },
            "expected_value": if positive { 4 } else { 2 },
            "assay_positive": positive,
            "assay_text": if positive { "ember ward protects the road" } else { "frost gate blocks the road" }
        });
        labels.push(positive);
        readbacks.push(store.put_content(&record).unwrap());
    }

    let stored =
        store_engine_content_assay(&database, "issue1456-fixture-assay", &readbacks, &labels)
            .unwrap();
    let loaded = read_engine_content_assay(&database, "issue1456-fixture-assay").unwrap();

    assert_eq!(stored.report.sample_count, sample_count);
    assert_eq!(stored.report.positive_count, sample_count / 2);
    assert_eq!(stored.report.negative_count, sample_count / 2);
    assert_eq!(stored.report.assayed_dense_lenses, 10);
    assert_eq!(stored.report.sufficiency_state, "measured");
    assert_eq!(stored.report.ensemble.n_samples, sample_count);
    assert_eq!(
        stored.report.ensemble.a37_diversity.status,
        calyx_assay::A37_DIVERSITY_GATE_PASSED
    );
    assert!(stored.report.ensemble.a37_diversity.no_collapse_pass);
    assert_eq!(stored.report.ensemble.keep_count, 10);
    assert_eq!(
        loaded.report.ensemble.sufficiency,
        stored.report.ensemble.sufficiency
    );
    assert_eq!(loaded.database.id, "content_assay/issue1456-fixture-assay");

    println!(
        "issue1456_content_assay samples={} dense_lenses={} sufficient={} panel_bits={:.6} vault={} row_sha={}",
        stored.report.sample_count,
        stored.report.assayed_dense_lenses,
        stored.report.ensemble.sufficient,
        stored.report.ensemble.panel_bits,
        vault_dir.display(),
        loaded.database.sha256
    );
}

#[test]
fn content_assay_rejects_a37_diagnostic_only() {
    let vault_dir = unique_vault_dir("assay-collapsed");
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    let store = fixture_content_store(database.clone(), "assay-collapsed", 10);
    let mut readbacks = Vec::new();
    let mut labels = Vec::new();
    for idx in 0..MIN_ASSAY_SAMPLES {
        let positive = idx % 2 == 0;
        let mut record = content(
            EngineContentKind::Story,
            &format!("story-collapsed-{idx:02}"),
        );
        record.payload = json!({
            "id": record.id,
            "kind": "story",
            "known_value": if positive { 4 } else { 2 },
            "expected_value": if positive { 4 } else { 2 },
            "assay_positive": positive,
            "assay_text": if positive { "same lens signal protects" } else { "same lens signal harms" }
        });
        labels.push(positive);
        readbacks.push(store.put_content(&record).unwrap());
    }

    match store_engine_content_assay(&database, "issue1478-collapsed-assay", &readbacks, &labels) {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => {
            assert_eq!(phase, "a37_gate");
            assert!(reason.contains("status=diagnostic_only"));
            assert!(reason.contains("no_collapse_pass=false"));
        }
        other => panic!("expected A37 diagnostic-only failure, got {other:?}"),
    }

    println!(
        "issue1478_a37_diagnostic_only_rejected samples={} vault={}",
        readbacks.len(),
        vault_dir.display()
    );
}

#[test]
fn editor_and_engine_queries_share_the_same_calyx_constellation() {
    let (store, _vault_dir) = content_store("query");
    let record = content(EngineContentKind::Ability, "ability-frost-nova");
    store.put_content(&record).unwrap();

    let editor = store
        .get_content_for_editor(EngineContentKind::Ability, "ability-frost-nova")
        .unwrap();
    let engine = store
        .lowered_content_for_engine(EngineContentKind::Ability, "ability-frost-nova")
        .unwrap();

    assert_eq!(editor.cx_id, engine.source_cx_id);
    assert_eq!(editor.input_sha256, engine.fingerprint);
    assert_eq!(engine.source_kind, EngineContentKind::Ability);
    assert_eq!(engine.source_id, "ability-frost-nova");
    assert!(
        engine
            .artifact_id
            .starts_with("lowered://engine-content/ability/")
    );
}

#[test]
fn invalid_calyx_content_fails_loud() {
    let (store, _vault_dir) = content_store("invalid");
    let mut missing_id = content(EngineContentKind::Unit, "unit-valid");
    missing_id.id.clear();
    match store.put_content(&missing_id) {
        Err(EngineCalyxContentError::InvalidContent { field, reason }) => {
            assert_eq!(field, "id");
            assert!(reason.contains("non-empty"));
        }
        other => panic!("expected invalid content id, got {other:?}"),
    }

    let mut missing_source = content(EngineContentKind::Story, "story-valid");
    missing_source.source.path.clear();
    match store.put_content(&missing_source) {
        Err(EngineCalyxContentError::InvalidContent { field, reason }) => {
            assert_eq!(field, "source.path");
            assert!(reason.contains("non-empty"));
        }
        other => panic!("expected invalid source path, got {other:?}"),
    }

    match store.get_content_for_editor(EngineContentKind::Asset, "missing-asset") {
        Err(EngineCalyxContentError::Persistence(_)) => {}
        other => panic!("expected missing content persistence error, got {other:?}"),
    }
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    content_store_with_lens_count(label, 10)
}

fn content_store_with_lens_count(
    label: &str,
    lens_count: usize,
) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = unique_vault_dir(label);
    let db = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (fixture_content_store(db, label, lens_count), vault_dir)
}

pub(crate) fn fixture_content_store(
    database: EngineCalyxDatabase,
    label: &str,
    lens_count: usize,
) -> EngineCalyxContentStore {
    EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(
            format!("preloaded:unit-{label}"),
            fixture_panel_state(lens_count),
        ),
    )
}

pub(crate) fn default_fixture_content_store(
    database: EngineCalyxDatabase,
    label: &str,
) -> EngineCalyxContentStore {
    fixture_content_store(database, label, 10)
}

#[derive(Clone)]
struct FixtureLearnedLens {
    id: LensId,
    dim: u32,
    salt: u8,
}

impl Lens for FixtureLearnedLens {
    fn id(&self) -> LensId {
        self.id
    }

    fn shape(&self) -> SlotShape {
        SlotShape::Dense(self.dim)
    }

    fn modality(&self) -> Modality {
        Modality::Text
    }

    fn measure(&self, input: &Input) -> calyx_core::Result<SlotVector> {
        let digest = sha2::Sha256::digest([&[self.salt], input.bytes.as_slice()].concat());
        let mut data = (0..self.dim as usize)
            .map(|idx| {
                let byte = digest[idx % digest.len()];
                (f32::from(byte) + 1.0) / 256.0
            })
            .collect::<Vec<_>>();
        let text = std::str::from_utf8(&input.bytes).unwrap_or_default();
        let signal_dim = self.salt as usize % data.len();
        if let Some(sample_index) = assay_sample_index(text) {
            data.fill(0.001);
            if (sample_index / 2) % 10 == self.salt as usize % 10 {
                data[signal_dim] = if text.contains("\"assay_positive\":true") {
                    8.0
                } else {
                    -8.0
                };
            }
        } else if text.contains("\"assay_positive\":true") {
            data[signal_dim] += 2.0;
        } else if text.contains("\"assay_positive\":false") {
            data[signal_dim] -= 2.0;
        }
        let norm = data.iter().map(|value| value * value).sum::<f32>().sqrt();
        for value in &mut data {
            *value /= norm;
        }
        Ok(SlotVector::Dense {
            dim: self.dim,
            data,
        })
    }
}

fn assay_sample_index(text: &str) -> Option<usize> {
    let marker = "story-assay-";
    let start = text.find(marker)? + marker.len();
    text.get(start..start + 2)?.parse().ok()
}

fn fixture_panel_state(lens_count: usize) -> VaultPanelState {
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
        let spec = LensSpec {
            name: name.clone(),
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
            axis: Some(name.clone()),
            asymmetry: Asymmetry::None,
            quant_default: QuantPolicy::turboquant_default(),
            truncate_dim: None,
            recall_delta: 0.02,
            retrieval_only: false,
            excluded_from_dedup: false,
        };
        registry
            .register_frozen_with_spec(
                FixtureLearnedLens {
                    id: lens_id,
                    dim: 4,
                    salt: idx as u8,
                },
                contract,
                spec,
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
            bits_about: Default::default(),
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

pub(crate) fn content(kind: EngineContentKind, id: &str) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: id.replace('-', " "),
        payload: json!({
            "id": id,
            "kind": kind.as_str(),
            "known_value": 4,
            "expected_value": 4
        }),
        source: EngineContentSource {
            path: format!("synthetic://issue1116/{}", id),
            author: "issue1116-test".to_string(),
            source_hash: stable_source_hash(id),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1116_known_io".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn stable_source_hash(id: &str) -> String {
    litd_hash::sha256_hex(format!("issue1116:{id}").as_bytes())
}

fn sha32(bytes: &[u8]) -> [u8; 32] {
    let digest = sha2::Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub(crate) fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1116-{label}"))
}
