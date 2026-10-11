use std::collections::BTreeMap;

use calyx_assay::MIN_ASSAY_SAMPLES;
use calyx_core::{SlotId, SlotVector, SparseEntry};
use serde_json::json;

use crate::{
    calyx_content::{
        EngineCalyxContentError, EngineContentA37AdmissionRef, EngineContentA37FrozenLens,
        EngineContentKind, frozen_roster_hash, read_engine_content_assay,
        store_engine_content_assay, store_engine_content_assay_with_a37_admission,
        write_engine_content_a37_admission,
    },
    calyx_content_a37_identity::roster_hash_from_evidence,
    calyx_content_issue1511_tests::passing_admission,
    calyx_content_tests::{content, fixture_content_store, unique_vault_dir},
    calyx_content_vectors::compact_non_dense_vectors_for_content_row,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
};

#[test]
fn content_assay_consumes_exact_db_admission_for_sparse_multi_roster() {
    let (database, readbacks, labels, sparse_slot, multi_slot, vault_dir) =
        non_dense_roster_fixture("assay-non-dense-roster");
    match store_engine_content_assay(
        &database,
        "issue1479-exact-non-dense-without-admission",
        &readbacks,
        &labels,
    ) {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => {
            assert_eq!(phase, "a37_gate");
            assert!(reason.contains("status=diagnostic_only"));
            assert!(reason.contains("no_collapse_pass=false"));
        }
        other => panic!("expected single-card A37 refusal before admission, got {other:?}"),
    }

    let admission_ref =
        EngineContentA37AdmissionRef::new(vault_dir.join("a37-admission-cf"), "issue1479-exact");
    let first = &readbacks[0].measurement;
    let admission_report = passing_admission(&first.frozen_roster);
    write_engine_content_a37_admission(&admission_ref, &admission_report).unwrap();
    let stored = store_engine_content_assay_with_a37_admission(
        &database,
        "issue1479-exact-non-dense-roster",
        &readbacks,
        &labels,
        &admission_ref,
    )
    .unwrap();

    assert_eq!(stored.report.measured_learned_lenses, 12);
    assert_eq!(stored.report.assayed_dense_lenses, 10);
    assert_eq!(stored.report.assayed_lens_count, 12);
    assert_eq!(
        stored.report.a37_gate_source,
        "db_native_multi_anchor_admission"
    );
    assert_eq!(
        stored.report.a37_admission.as_ref().unwrap().report.status,
        "gate_passed"
    );
    assert_non_dense_lenses(&stored, sparse_slot, multi_slot);

    // FSV: the persisted admission binds the exact frozen roster identity, and it
    // matches the measured roster read back from the assay row.
    let admission = &stored.report.a37_admission.as_ref().unwrap().report;
    assert_eq!(
        admission.panel_state_sha256,
        first.frozen_roster.panel_state_sha256
    );
    assert_eq!(
        admission.registry_roster_sha256,
        stored.report.measured_frozen_roster.registry_roster_sha256
    );
    assert_eq!(admission.lenses.len(), first.frozen_roster.lenses.len());
    for (lens, frozen) in admission
        .lenses
        .iter()
        .zip(first.frozen_roster.lenses.iter())
    {
        assert!(!lens.frozen_lens_id.is_empty());
        assert!(!lens.weights_sha256.is_empty());
        assert!(!lens.runtime_fingerprint.is_empty());
        assert_eq!(lens.frozen_lens_id, frozen.frozen_lens_id);
        assert_eq!(lens.weights_sha256, frozen.weights_sha256);
        assert_eq!(lens.runtime_fingerprint, frozen.runtime_fingerprint);
    }

    println!(
        "issue1479_exact_db_admission samples={} sparse_slot={} multi_slot={} row_sha={} admission_sha={} panel_state_sha={} registry_roster_sha={} first_lens_id={} first_weights={} vault={}",
        readbacks.len(),
        sparse_slot.get(),
        multi_slot.get(),
        stored.database.sha256,
        stored.report.a37_admission.as_ref().unwrap().value_sha256,
        admission.panel_state_sha256,
        admission.registry_roster_sha256,
        admission.lenses[0].frozen_lens_id,
        admission.lenses[0].weights_sha256,
        vault_dir.display()
    );
}

#[test]
fn content_assay_refuses_stale_db_admission_roster() {
    let (database, readbacks, labels, _sparse_slot, _multi_slot, vault_dir) =
        non_dense_roster_fixture("assay-stale-admission");
    let first = &readbacks[0].measurement;
    // A card from a different runtime/weights roster: rename lens 11 and give it a
    // different frozen identity, then re-seal the roster hash so the card is
    // internally self-consistent (passes the write gate) but no longer matches the
    // measured local roster.
    let mut stale = passing_admission(&first.frozen_roster);
    stale.lenses[11].name = "stale-colbert-roster".to_string();
    stale.lenses[11].weights_sha256 = litd_hash::sha256_hex(b"weights:stale-colbert-roster");
    stale.registry_roster_sha256 = roster_hash_from_evidence(&stale.lenses);
    let admission_ref =
        EngineContentA37AdmissionRef::new(vault_dir.join("a37-admission-cf"), "issue1479-stale");
    write_engine_content_a37_admission(&admission_ref, &stale).unwrap();

    match store_engine_content_assay_with_a37_admission(
        &database,
        "issue1479-stale-admission-roster",
        &readbacks,
        &labels,
        &admission_ref,
    ) {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => {
            assert_eq!(phase, "a37_admission");
            assert!(reason.contains("roster mismatch"));
        }
        other => panic!("expected stale A37 admission refusal, got {other:?}"),
    }
    assert!(matches!(
        read_engine_content_assay(&database, "issue1479-stale-admission-roster"),
        Err(EngineCalyxContentError::Persistence(_))
    ));
}

fn non_dense_roster_fixture(
    label: &str,
) -> (
    EngineCalyxDatabase,
    Vec<crate::calyx_content::EngineContentConstellationReadback>,
    Vec<bool>,
    SlotId,
    SlotId,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    let store = fixture_content_store(database.clone(), label, 10);
    let mut readbacks = Vec::new();
    let mut labels = Vec::new();
    let sparse_slot = SlotId::new(99);
    let multi_slot = SlotId::new(100);
    for idx in 0..MIN_ASSAY_SAMPLES {
        let positive = idx % 2 == 0;
        let mut record = content(EngineContentKind::Story, &format!("story-assay-{idx:02}"));
        record.payload = json!({
            "id": record.id,
            "kind": "story",
            "assay_positive": positive,
            "assay_text": if positive { "radiant archive ward clue" } else { "ashen ruin false trail" }
        });
        labels.push(positive);
        let mut readback = store.put_content(&record).unwrap();
        readback
            .measurement
            .measured_slot_ids
            .push(sparse_slot.get());
        readback
            .measurement
            .measured_lens_names
            .push("a38_spladepp_v1_sparse".to_string());
        readback
            .measurement
            .measured_slot_ids
            .push(multi_slot.get());
        readback
            .measurement
            .measured_lens_names
            .push("a38_answerai_colbert_small_v1".to_string());
        readback.measurement.measured_learned_lenses += 2;
        // Keep the frozen roster identity in lockstep with the measured roster:
        // the non-dense a38 lenses are real frozen instruments, so their identity
        // must be captured too (#1511).
        readback
            .measurement
            .frozen_roster
            .lenses
            .push(EngineContentA37FrozenLens {
                slot: sparse_slot.get(),
                name: "a38_spladepp_v1_sparse".to_string(),
                frozen_lens_id: litd_hash::sha256_hex(b"lens-id:a38_spladepp_v1_sparse"),
                weights_sha256: litd_hash::sha256_hex(b"weights:a38_spladepp_v1_sparse"),
                runtime_fingerprint: litd_hash::sha256_hex(b"runtime:a38_spladepp_v1_sparse"),
            });
        readback
            .measurement
            .frozen_roster
            .lenses
            .push(EngineContentA37FrozenLens {
                slot: multi_slot.get(),
                name: "a38_answerai_colbert_small_v1".to_string(),
                frozen_lens_id: litd_hash::sha256_hex(b"lens-id:a38_answerai_colbert_small_v1"),
                weights_sha256: litd_hash::sha256_hex(b"weights:a38_answerai_colbert_small_v1"),
                runtime_fingerprint: litd_hash::sha256_hex(
                    b"runtime:a38_answerai_colbert_small_v1",
                ),
            });
        readback.measurement.frozen_roster.registry_roster_sha256 =
            frozen_roster_hash(&readback.measurement.frozen_roster.lenses);

        let mut non_dense_slots = BTreeMap::new();
        non_dense_slots.insert(
            sparse_slot,
            SlotVector::Sparse {
                dim: 30_522,
                entries: sparse_signal_entries(idx, positive),
            },
        );
        non_dense_slots.insert(
            multi_slot,
            SlotVector::Multi {
                token_dim: 4,
                tokens: multi_signal_tokens(idx, positive),
            },
        );
        let mut non_dense_metadata = BTreeMap::new();
        compact_non_dense_vectors_for_content_row(&mut non_dense_slots, &mut non_dense_metadata)
            .unwrap();
        readback.constellation.slots.extend(non_dense_slots);
        readback.constellation.metadata.extend(non_dense_metadata);
        readbacks.push(readback);
    }
    (
        database,
        readbacks,
        labels,
        sparse_slot,
        multi_slot,
        vault_dir,
    )
}

fn assert_non_dense_lenses(
    stored: &crate::calyx_content::EngineContentAssayReadback,
    sparse_slot: SlotId,
    multi_slot: SlotId,
) {
    let sparse = stored
        .report
        .assayed_lenses
        .iter()
        .find(|lens| lens.slot_id == sparse_slot.get())
        .unwrap();
    assert_eq!(sparse.source_vector_kind, "sparse");
    assert_eq!(sparse.assay_projection, "sparse_hash_dense_8192");
    assert_eq!(sparse.assay_dim, 8192);
    let multi = stored
        .report
        .assayed_lenses
        .iter()
        .find(|lens| lens.slot_id == multi_slot.get())
        .unwrap();
    assert_eq!(multi.source_vector_kind, "multi");
    assert_eq!(multi.assay_projection, "multi_mean_dense");
    assert_eq!(multi.assay_dim, 4);
}

fn sparse_signal_entries(idx: usize, positive: bool) -> Vec<SparseEntry> {
    let active = (idx / 2) % 12 == 10;
    vec![
        SparseEntry {
            idx: 30_000,
            val: if active && positive { 8.0 } else { 0.001 },
        },
        SparseEntry {
            idx: 30_001,
            val: if active && !positive { -8.0 } else { -0.001 },
        },
    ]
}

fn multi_signal_tokens(idx: usize, positive: bool) -> Vec<Vec<f32>> {
    let active = (idx / 2) % 12 == 11;
    let signal = if active && positive {
        8.0
    } else if active {
        -8.0
    } else {
        0.001
    };
    vec![
        vec![signal, 0.001, 0.001, 0.001],
        vec![signal, -0.001, 0.001, -0.001],
    ]
}
