use std::collections::BTreeMap;

use calyx_assay::{EnsembleConfig, EnsembleLensInput, MIN_ASSAY_SAMPLES, ensemble_card};
use calyx_core::SlotId;

use crate::{
    calyx_content::{
        EngineCalyxContentError, EngineContentA37AdmissionRef, EngineContentA37FrozenLens,
        EngineContentA37FrozenRoster, EngineContentA37InputReport,
        EngineContentA37MultiAnchorConfig, build_engine_content_a37_admission_report,
        frozen_roster_hash, verify_admission_against_measured_roster,
        write_engine_content_a37_admission,
    },
    calyx_content_a37_admission::{
        EngineContentA37AdmissionReport, EngineContentA37LensEvidence,
        EngineContentA37TargetLensValue, EngineContentA37TargetSummary,
    },
    calyx_content_a37_identity::{ENGINE_CONTENT_A37_ROSTER_SCHEMA, roster_hash_from_evidence},
    calyx_content_tests::unique_vault_dir,
};

#[test]
fn multi_anchor_builder_admits_exact_real_ensemble_cards() {
    let names = fixture_roster_names();
    let slots = (0..names.len() as u16).collect::<Vec<_>>();
    let roster = fixture_frozen_roster(&slots, &names);
    let cards = (0..2)
        .map(|target| {
            let card = synthetic_multi_anchor_card(target, &names);
            EngineContentA37InputReport {
                target_class: target,
                domain: format!("issue1479-target-{target}"),
                source: format!("memory://issue1479-target-{target}"),
                card,
            }
        })
        .collect::<Vec<_>>();

    let report = build_engine_content_a37_admission_report(
        &cards,
        &EngineContentA37MultiAnchorConfig::default(),
        &roster,
    )
    .unwrap();

    assert!(report.gate_passed);
    assert_eq!(report.status, "gate_passed");
    assert_eq!(report.lens_count, names.len());
    assert_eq!(report.passing_lens_count, names.len());
    assert_eq!(report.report_count, 2);
    assert!(report.association_family_count >= 3);
    assert!(report.min_best_marginal_bits >= report.min_marginal_bits);
    // The builder copies the measured frozen identity onto every lens (#1511).
    assert_eq!(report.registry_roster_sha256, roster.registry_roster_sha256);
    assert_eq!(report.panel_state_sha256, roster.panel_state_sha256);
    for (lens, frozen) in report.lenses.iter().zip(roster.lenses.iter()) {
        assert_eq!(lens.frozen_lens_id, frozen.frozen_lens_id);
        assert_eq!(lens.weights_sha256, frozen.weights_sha256);
        assert_eq!(lens.runtime_fingerprint, frozen.runtime_fingerprint);
    }
}

#[test]
fn assay_rejects_each_flipped_identity_dimension() {
    let names = fixture_roster_names();
    let slots = (0..names.len() as u16).collect::<Vec<_>>();
    let roster = fixture_frozen_roster(&slots, &names);
    let base = passing_admission(&roster);

    // Reordered roster (swap two lenses) — position identity changes.
    let mut reordered = base.clone();
    reordered.lenses.swap(0, 1);
    reordered.registry_roster_sha256 = roster_hash_from_evidence(&reordered.lenses);
    assert_flip_rejected(&roster, &reordered, "reordered");

    // Renamed lens.
    let mut renamed = base.clone();
    renamed.lenses[3].name = "impostor-lens".to_string();
    renamed.registry_roster_sha256 = roster_hash_from_evidence(&renamed.lenses);
    assert_flip_rejected(&roster, &renamed, "renamed");

    // Weight-changed lens.
    let mut reweighted = base.clone();
    reweighted.lenses[4].weights_sha256 = litd_hash::sha256_hex(b"tampered-weights");
    reweighted.registry_roster_sha256 = roster_hash_from_evidence(&reweighted.lenses);
    assert_flip_rejected(&roster, &reweighted, "weight-changed");

    // Runtime/shape-changed lens.
    let mut reshaped = base.clone();
    reshaped.lenses[5].runtime_fingerprint = litd_hash::sha256_hex(b"tampered-runtime");
    reshaped.registry_roster_sha256 = roster_hash_from_evidence(&reshaped.lenses);
    assert_flip_rejected(&roster, &reshaped, "runtime-changed");

    // Frozen-lens-id changed.
    let mut relensid = base.clone();
    relensid.lenses[6].frozen_lens_id = litd_hash::sha256_hex(b"tampered-lens-id");
    relensid.registry_roster_sha256 = roster_hash_from_evidence(&relensid.lenses);
    assert_flip_rejected(&roster, &relensid, "lens-id-changed");

    // Panel-changed (different panel state hash).
    let mut repanel = base.clone();
    repanel.panel_state_sha256 = litd_hash::sha256_hex(b"different-panel-state");
    assert_flip_rejected(&roster, &repanel, "panel-changed");

    // Aggregate roster-hash tampered while lenses match.
    let mut rehash = base.clone();
    rehash.registry_roster_sha256 = litd_hash::sha256_hex(b"tampered-aggregate");
    assert_flip_rejected(&roster, &rehash, "hash-mismatched");

    // The exact roster is still accepted.
    verify_admission_against_measured_roster(&base, &roster)
        .expect("exact frozen roster must be accepted");
}

fn assert_flip_rejected(
    roster: &EngineContentA37FrozenRoster,
    report: &EngineContentA37AdmissionReport,
    dimension: &str,
) {
    match verify_admission_against_measured_roster(report, roster) {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => {
            assert_eq!(phase, "a37_admission", "{dimension}: {reason}");
        }
        other => panic!("{dimension}: expected a37_admission rejection, got {other:?}"),
    }
}

#[test]
fn a37_admission_writer_refuses_diagnostic_report() {
    let vault_dir = unique_vault_dir("a37-writer-refuses-diagnostic");
    let admission_ref =
        EngineContentA37AdmissionRef::new(vault_dir.join("a37-admission-cf"), "diagnostic");
    let slots = (0..12).map(|slot| slot as u16).collect::<Vec<_>>();
    let names = fixture_roster_names();
    let roster = fixture_frozen_roster(&slots, &names);
    let mut report = passing_admission(&roster);
    report.status = "diagnostic_only".to_string();
    report.gate_passed = false;
    report.no_collapse_pass = false;

    match write_engine_content_a37_admission(&admission_ref, &report) {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => {
            assert_eq!(phase, "a37_admission");
            assert!(reason.contains("A37 admission refused"));
        }
        other => panic!("expected diagnostic A37 admission write refusal, got {other:?}"),
    }
}

/// Builds a gate-passing admission card bound to `roster`'s frozen identity.
/// Shared with the store/assay tests in `calyx_content_issue1479_tests`.
pub(crate) fn passing_admission(
    roster: &EngineContentA37FrozenRoster,
) -> EngineContentA37AdmissionReport {
    let lens_count = roster.lenses.len();
    let lenses = roster
        .lenses
        .iter()
        .enumerate()
        .map(|(idx, frozen)| EngineContentA37LensEvidence {
            slot: frozen.slot,
            name: frozen.name.clone(),
            frozen_lens_id: frozen.frozen_lens_id.clone(),
            weights_sha256: frozen.weights_sha256.clone(),
            runtime_fingerprint: frozen.runtime_fingerprint.clone(),
            association_family: if frozen.name.contains("splade") {
                "lexical_sparse"
            } else if frozen.name.contains("colbert") {
                "late_interaction_token"
            } else {
                "dense_semantic_general"
            }
            .to_string(),
            passed: true,
            best_target_class: 1,
            best_domain: "issue1479-fixture".to_string(),
            best_marginal_bits: 0.07 + idx as f32 * 0.001,
            best_solo_bits: 0.1,
            target_values: vec![EngineContentA37TargetLensValue {
                target_class: 1,
                domain: "issue1479-fixture".to_string(),
                marginal_bits: 0.07 + idx as f32 * 0.001,
                solo_bits: 0.1,
                decision: "Keep".to_string(),
            }],
        })
        .collect::<Vec<_>>();
    EngineContentA37AdmissionReport {
        schema_version: 1,
        role: "a37_multi_anchor_admission_card".to_string(),
        status: "gate_passed".to_string(),
        mode: "gate".to_string(),
        panel_template: roster.panel_template.clone(),
        panel_version: roster.panel_version,
        panel_state_sha256: roster.panel_state_sha256.clone(),
        registry_roster_sha256: roster.registry_roster_sha256.clone(),
        source_card_sha256: frozen_roster_hash(&roster.lenses),
        source_of_truth: format!("db://issue1479-fixture@v{}", roster.panel_version),
        gate_passed: true,
        report_count: 2,
        lens_count,
        passing_lens_count: lens_count,
        min_lenses: 10,
        min_marginal_bits: 0.05,
        max_redundancy: 0.6,
        family_span_pass: true,
        redundancy_bound_pass: true,
        no_collapse_pass: true,
        association_family_count: 3,
        association_families: BTreeMap::new(),
        min_best_marginal_bits: 0.07,
        max_best_marginal_bits: 0.09,
        weakest_lens: roster.lenses[0].name.clone(),
        target_summaries: vec![EngineContentA37TargetSummary {
            target_class: 1,
            domain: "issue1479-fixture".to_string(),
            report_path: "db://issue1479-fixture".to_string(),
            status: "diagnostic_only".to_string(),
            no_collapse_pass: false,
            family_span_pass: true,
            redundancy_bound_pass: true,
            n_eff: lens_count as f32,
            panel_bits: 1.0,
            max_marginal_bits: 0.09,
            keep_count: lens_count,
            park_count: 0,
        }],
        lenses,
        source_reports: vec!["db://issue1479-fixture".to_string()],
    }
}

/// Builds a synthetic frozen roster for the given slots/names (no live panel).
fn fixture_frozen_roster(slots: &[u16], names: &[String]) -> EngineContentA37FrozenRoster {
    let lenses = slots
        .iter()
        .zip(names.iter())
        .map(|(slot, name)| EngineContentA37FrozenLens {
            slot: *slot,
            name: name.clone(),
            frozen_lens_id: litd_hash::sha256_hex(format!("lens-id:{name}").as_bytes()),
            weights_sha256: litd_hash::sha256_hex(format!("weights:{name}").as_bytes()),
            runtime_fingerprint: litd_hash::sha256_hex(format!("runtime:{name}").as_bytes()),
        })
        .collect::<Vec<_>>();
    EngineContentA37FrozenRoster {
        schema: ENGINE_CONTENT_A37_ROSTER_SCHEMA.to_string(),
        panel_template: "constellation-24".to_string(),
        panel_version: 12,
        panel_state_sha256: litd_hash::sha256_hex(b"issue1479-fixture-panel-state"),
        registry_roster_sha256: frozen_roster_hash(&lenses),
        lenses,
    }
}

fn fixture_roster_names() -> Vec<String> {
    [
        "semantic-general-alpha",
        "semantic-general-beta",
        "legal-domain",
        "medical-domain",
        "splade-lexical",
        "colbert-late-interaction",
        "entity-graph",
        "structural-style",
        "scientific-domain",
        "financial-domain",
        "splade-issue1479-sparse",
        "colbert-issue1479-late",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn synthetic_multi_anchor_card(target: usize, names: &[String]) -> calyx_assay::EnsembleCard {
    let sample_count = MIN_ASSAY_SAMPLES * 6;
    let labels = (0..sample_count)
        .map(|idx| idx % 2 == 0)
        .collect::<Vec<_>>();
    let lenses = names
        .iter()
        .enumerate()
        .map(|(slot, name)| {
            let vectors = labels
                .iter()
                .enumerate()
                .map(|(idx, positive)| {
                    let active = ((idx / 2) + target) % names.len() == slot;
                    let signal = if active && *positive {
                        8.0
                    } else if active {
                        -8.0
                    } else {
                        0.001
                    };
                    vec![signal, slot as f32 * 0.01 + 0.001]
                })
                .collect();
            EnsembleLensInput::new(name.clone(), SlotId::new(slot as u16), vectors)
        })
        .collect::<Vec<_>>();
    ensemble_card(
        &lenses,
        &labels,
        None,
        &EnsembleConfig {
            source: format!("issue1479-synthetic-target-{target}"),
            ..EnsembleConfig::default()
        },
    )
    .unwrap()
}
