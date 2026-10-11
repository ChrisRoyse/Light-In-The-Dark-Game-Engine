//! Tests for the authored Ward response corpus + degenerate-calibration guard (#1552).

use serde_json::json;

use calyx_ward::WardError;

use crate::{
    agent_response::EngineAgentVerbTag,
    agent_response_corpus::{
        WardResponseCorpusError, WardResponseCorpusSource, assess_corpus_separation, build_corpus,
        build_corpus_with_texts, corpus_source_from_content, persist_corpus, policy_from_corpus,
        read_corpus,
    },
    agent_response_corpus_support::derive_degenerate_ood,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_ward_gate::evaluate_calyx_ward_gate,
    test_temp::TestTempDir,
};

fn agent_content() -> EngineContentRecord {
    EngineContentRecord {
        kind: EngineContentKind::Agent,
        id: "agent-gate-guard".to_string(),
        display_name: "Gate Guard".to_string(),
        payload: json!({
            "agent_id": "guard",
            "tags": ["fire", "ward", "gate", "protect", "enemy", "intruder", "line"],
            "trigger_terms": ["enemy", "gate"],
        }),
        source: EngineContentSource {
            path: "authored/world/thera/agents/gate_guard.json".to_string(),
            author: "editor".to_string(),
            source_hash: "deadbeef".to_string(),
        },
        anchors: Vec::new(),
    }
}

fn source() -> WardResponseCorpusSource {
    corpus_source_from_content(
        &agent_content(),
        "thera",
        "gate-guard",
        vec![EngineAgentVerbTag::SetVelocity, EngineAgentVerbTag::Hold],
    )
    .expect("source derives from ingested content")
}

#[test]
fn corpus_derives_from_ingested_content_and_is_non_degenerate() {
    let corpus = build_corpus(&source()).expect("healthy authored corpus");
    // Trusted narrations are in the scored shape "<verb_tag> <rationale>".
    assert!(
        corpus
            .trusted_texts
            .iter()
            .all(|text| { text.starts_with("set_velocity ") || text.starts_with("hold ") })
    );
    // Grounded terms came out of the ingested record payload, not from example code.
    assert!(corpus.grounded_terms.contains(&"gate".to_string()));
    assert!(corpus.grounded_terms.contains(&"ward".to_string()));
    // Provenance points back at the source content.
    assert!(
        corpus
            .provenance_refs
            .iter()
            .any(|reference| reference.contains("gate_guard.json")
                || reference.contains("agent-gate-guard"))
    );
    // Separation is healthy on every slot: negatives are genuinely OOD.
    let separation = assess_corpus_separation(&corpus).expect("healthy separation");
    assert_eq!(separation.slots.len(), 3);
    for slot in &separation.slots {
        assert!(
            slot.report.auroc >= 0.90,
            "slot {} auroc {}",
            slot.slot,
            slot.report.auroc
        );
        assert!(
            slot.report.fpr_at_tpr <= 0.05,
            "slot {} fpr {}",
            slot.slot,
            slot.report.fpr_at_tpr
        );
    }
}

#[test]
fn degenerate_near_duplicate_negatives_fail_loud() {
    let source = source();
    let trusted = build_corpus(&source).unwrap().trusted_texts;
    // Near-duplicate negatives: the trusted narrations with their words shuffled — they share the
    // entire grounded vocabulary. This is the #1548 mistake.
    let degenerate_ood = derive_degenerate_ood(&trusted, calyx_ward::MIN_BAD_SCORES);
    let error = build_corpus_with_texts(&source, trusted, degenerate_ood)
        .expect_err("a degenerate calibration must fail loud, not emit a useless tau");
    match error {
        WardResponseCorpusError::DegenerateCalibration { slot: _, source } => {
            assert!(matches!(source, WardError::DegenerateCalibration { .. }));
            assert_eq!(source.code(), "CALYX_GUARD_DEGENERATE_CALIBRATION");
            // The error names the overlap statistic.
            let rendered = source.to_string();
            assert!(
                rendered.contains("fpr_at_tpr")
                    || rendered.contains("auroc")
                    || rendered.contains("separation_margin"),
                "error must name the overlap statistic: {rendered}"
            );
        }
        other => panic!("expected DegenerateCalibration, got {other:?}"),
    }
}

#[test]
fn persist_and_readback_reverify_hash() {
    let temp = TestTempDir::new("issue1552-corpus");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(
        temp.path().join("aster-vault"),
    ))
    .unwrap();
    let corpus = build_corpus(&source()).unwrap();
    let stored = persist_corpus(&database, &corpus).expect("persist + readback");
    assert_eq!(stored.corpus_sha256, corpus.corpus_sha256);

    // Independent readback re-verifies the self-hash.
    let readback = read_corpus(&database, &corpus.corpus_id).expect("readback");
    assert_eq!(readback, corpus);

    // A tampered row fails the hash check on read.
    let mut tampered = corpus.clone();
    tampered
        .trusted_texts
        .push("set_velocity smuggled injection payload".to_string());
    // Hash still the old one → mismatch when persisted then read is simulated by re-persist guard.
    let error = persist_corpus(&database, &tampered)
        .expect_err("a corpus whose hash does not match its texts must be refused");
    assert!(matches!(error, WardResponseCorpusError::Invalid { .. }));
}

#[test]
fn policy_from_corpus_calibrates_a_usable_ward_gate() {
    use crate::calyx_ward_gate::{EngineCalyxWardSubject, EngineCalyxWardSubjectKind};

    let corpus = build_corpus(&source()).unwrap();
    let policy = policy_from_corpus(
        &corpus,
        vec![
            "ignore".to_string(),
            "unbounded".to_string(),
            "exploit".to_string(),
        ],
        2,
        2,
        2,
        1500,
        1000,
    );
    // A legitimate grounded response narration, scored in "<verb_tag> <rationale>" shape, is
    // admitted by the authored-corpus policy — the calibration is usable, not a useless tau.
    let subject = EngineCalyxWardSubject {
        subject_id: "issue1552-legit".to_string(),
        kind: EngineCalyxWardSubjectKind::AgentResponse,
        content_kind: None,
        text: "set_velocity advance one unit toward the gate and hold the line against the enemy"
            .to_string(),
        grounding_refs: vec!["grounding://a".to_string(), "grounding://b".to_string()],
        provenance_refs: vec!["prov://a".to_string()],
        movement_units_per_tick: 1,
        teleport_requested: false,
        scale_milli: 1000,
        emissive_milli: 0,
        source_fingerprint: "fingerprint".to_string(),
    };
    let gate = evaluate_calyx_ward_gate("issue1552-policy", policy, &[subject])
        .expect("authored-corpus policy calibrates and evaluates");
    assert_eq!(gate.subject_count, 1);
    assert!(gate.ward_calibration.calibrated);
    assert_eq!(gate.admitted_count, 1, "verdicts={:?}", gate.verdicts);
}
