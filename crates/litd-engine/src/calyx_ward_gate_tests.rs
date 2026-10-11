use calyx_core::{Anchor, AnchorKind, AnchorValue};
use calyx_ward::CALYX_GUARD_OOD;
use serde_json::{Value, json};

use crate::{
    calyx_agent_runtime::{EngineWorldSignal, EngineWorldSignalKind, activate_calyx_agents},
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_ward_gate::{
        EngineCalyxWardGateError, EngineCalyxWardPolicy, EngineCalyxWardSubject,
        EngineCalyxWardSubjectKind, evaluate_calyx_ward_gate, read_calyx_ward_gate,
        store_calyx_ward_gate, ward_subject_from_agent_activation, ward_subject_from_content,
    },
};

#[test]
fn calyx_ward_gate_admits_grounded_content_and_agent_orders() {
    let (store, database, vault_dir) = content_store("happy");
    let subjects = trusted_subjects(&store);

    let gate =
        evaluate_calyx_ward_gate("issue1121-ward-pass", policy_for(&subjects), &subjects).unwrap();
    let stored = store_calyx_ward_gate(&database, &gate).unwrap();
    let readback = read_calyx_ward_gate(&database, "issue1121-ward-pass").unwrap();

    assert_eq!(gate.subject_count, 2);
    assert_eq!(gate.admitted_count, 2);
    assert_eq!(gate.refused_count, 0);
    assert!(gate.ward_calibration.calibrated);
    assert!(gate.verdicts.iter().all(|verdict| verdict.lowering_allowed));
    assert!(
        gate.verdicts
            .iter()
            .all(|verdict| verdict.ward_overall_pass)
    );
    assert_eq!(stored.gate.gate_fingerprint, gate.gate_fingerprint);
    assert_eq!(readback.gate.admitted_count, 2);

    println!(
        "issue1121_ward_gate admitted={} refused={} gate_sha={} vault={}",
        gate.admitted_count,
        gate.refused_count,
        readback.database.sha256,
        vault_dir.display()
    );
}

#[test]
fn calyx_ward_gate_refuses_ood_ungrounded_and_envelope_violations() {
    let (store, _database, _vault_dir) = content_store("refuse");
    let trusted = trusted_subjects(&store);
    let policy = policy_for(&trusted);
    let ood = store
        .put_content(&content(
            EngineContentKind::Ability,
            "ability-ood",
            "Teleport Nuke",
            json!({
                "ability_id": "teleport-nuke",
                "summary": "nuclear teleport exploit beyond the world envelope",
                "tags": ["nuclear", "teleport", "exploit"]
            }),
        ))
        .unwrap();
    let mut ungrounded = ward_subject_from_content(&ood).unwrap();
    ungrounded.subject_id = "manual/ungrounded".to_string();
    ungrounded.text = trusted[0].text.clone();
    ungrounded.grounding_refs = vec!["single-ref".to_string()];

    let agent = store
        .put_content(&content(
            EngineContentKind::Agent,
            "agent-fast",
            "Fast Guard",
            json!({
                "agent_id": "fast",
                "trigger_terms": ["enemy"],
                "prompt_template": "Agent {agent}: protect gate from {signal}",
                "target_body": 3,
                "order_velocity": [5, 0, 0]
            }),
        ))
        .unwrap();
    let runtime = activate_calyx_agents(
        "issue1121-agent-fast",
        &[agent],
        &[signal("signal-fast", &["enemy"], "enemy at gate")],
    )
    .unwrap();
    let fast = ward_subject_from_agent_activation(&runtime.activations[0]).unwrap();
    let visual_overflow = EngineCalyxWardSubject {
        subject_id: "manual/visual-overflow".to_string(),
        kind: EngineCalyxWardSubjectKind::AgentActivation,
        content_kind: Some(EngineContentKind::Agent),
        text: "agent fire ward gate protect".to_string(),
        grounding_refs: vec!["grounding://one".to_string(), "grounding://two".to_string()],
        provenance_refs: vec!["provenance://visual-overflow".to_string()],
        movement_units_per_tick: 1,
        teleport_requested: false,
        scale_milli: 2500,
        emissive_milli: 2000,
        source_fingerprint: "visual-overflow-fingerprint".to_string(),
    };
    let subjects = vec![
        ward_subject_from_content(&ood).unwrap(),
        ungrounded,
        fast,
        visual_overflow,
    ];

    let gate = evaluate_calyx_ward_gate("issue1121-ward-refuse", policy, &subjects).unwrap();
    let codes = gate
        .verdicts
        .iter()
        .map(|verdict| verdict.code.as_str())
        .collect::<Vec<_>>();

    assert_eq!(gate.admitted_count, 0);
    assert_eq!(gate.refused_count, 4);
    assert!(gate.fail_closed);
    assert!(
        gate.verdicts
            .iter()
            .all(|verdict| !verdict.lowering_allowed)
    );
    assert!(codes.contains(&CALYX_GUARD_OOD));
    assert!(codes.contains(&"LITD-WARD-REFUSED-UNGROUNDED"));
    assert!(codes.contains(&"LITD-WARD-REFUSED-ENVELOPE"));
    assert!(gate.verdicts.iter().any(|verdict| {
        verdict.subject_id == "manual/visual-overflow"
            && verdict.code == "LITD-WARD-REFUSED-ENVELOPE"
            && verdict
                .reasons
                .iter()
                .any(|reason| reason == "visual envelope exceeded")
    }));
}

#[test]
fn calyx_ward_gate_rejects_legacy_token_overlap_adversary() {
    let (store, _database, _vault_dir) = content_store("issue1457");
    let trusted = trusted_subjects(&store);
    let mut adversary = trusted[0].clone();
    adversary.subject_id = "manual/legacy-token-overlap-adversary".to_string();
    adversary.text = adversarial_overlap_text();
    adversary.grounding_refs = vec![
        "grounding://trusted-one".to_string(),
        "grounding://trusted-two".to_string(),
    ];
    adversary.source_fingerprint = "issue1457-adversary".to_string();

    let gate = evaluate_calyx_ward_gate(
        "issue1457-ward-adversary",
        policy_for(&trusted),
        &[adversary],
    )
    .unwrap();
    let verdict = &gate.verdicts[0];

    assert_eq!(gate.admitted_count, 0);
    assert_eq!(gate.refused_count, 1);
    assert_eq!(verdict.code, CALYX_GUARD_OOD);
    assert!(verdict.legacy_token_admission_would_pass);
    assert!(!verdict.ward_overall_pass);
    assert!(verdict.ward_slots.iter().any(|slot| !slot.pass));
}

#[test]
fn calyx_ward_gate_invalid_policy_fails_loud() {
    let subject = EngineCalyxWardSubject {
        subject_id: "manual/subject".to_string(),
        kind: EngineCalyxWardSubjectKind::Content,
        content_kind: Some(EngineContentKind::Lore),
        text: "fire ward gate".to_string(),
        grounding_refs: vec!["grounding://one".to_string(), "grounding://two".to_string()],
        provenance_refs: vec!["provenance://subject".to_string()],
        movement_units_per_tick: 0,
        teleport_requested: false,
        scale_milli: 1000,
        emissive_milli: 0,
        source_fingerprint: "fingerprint".to_string(),
    };
    let mut bad_policy = policy_for(std::slice::from_ref(&subject));
    bad_policy.allowed_terms.clear();
    match evaluate_calyx_ward_gate("bad-policy", bad_policy, &[subject]) {
        Err(EngineCalyxWardGateError::InvalidInput { field, reason }) => {
            assert_eq!(field, "policy");
            assert!(reason.contains("required"));
        }
        other => panic!("expected policy rejection, got {other:?}"),
    }
}

fn trusted_subjects(store: &EngineCalyxContentStore) -> Vec<EngineCalyxWardSubject> {
    let ability = store
        .put_content(&content(
            EngineContentKind::Ability,
            "ability-fire-ward",
            "Fire Ward",
            json!({
                "ability_id": "fire-ward",
                "summary": "grounded fire ward protects the gate",
                "tags": ["fire", "ward", "gate", "protect"]
            }),
        ))
        .unwrap();
    let agent = store
        .put_content(&content(
            EngineContentKind::Agent,
            "agent-guard",
            "Gate Guard",
            json!({
                "agent_id": "guard",
                "trigger_terms": ["enemy", "gate"],
                "prompt_template": "Agent {agent}: protect gate from {signal}",
                "target_body": 2,
                "order_velocity": [1, 0, 0]
            }),
        ))
        .unwrap();
    let runtime = activate_calyx_agents(
        "issue1121-agent",
        &[agent],
        &[signal("signal-enemy", &["enemy", "gate"], "enemy at gate")],
    )
    .unwrap();
    vec![
        ward_subject_from_content(&ability).unwrap(),
        ward_subject_from_agent_activation(&runtime.activations[0]).unwrap(),
    ]
}

fn policy_for(trusted: &[EngineCalyxWardSubject]) -> EngineCalyxWardPolicy {
    EngineCalyxWardPolicy {
        policy_id: "ward-issue1121".to_string(),
        allowed_terms: vec![
            "fire".to_string(),
            "ward".to_string(),
            "gate".to_string(),
            "protect".to_string(),
            "enemy".to_string(),
            "agent".to_string(),
        ],
        blocked_terms: vec![
            "nuclear".to_string(),
            "teleport".to_string(),
            "exploit".to_string(),
        ],
        trusted_texts: trusted.iter().map(|subject| subject.text.clone()).collect(),
        calibration_ood_texts: calibration_ood_texts(),
        min_grounding_refs: 2,
        min_matched_terms: 2,
        max_move_units_per_tick: 2,
        allow_teleport: false,
        max_scale_milli: 1500,
        max_emissive_milli: 1000,
    }
}

fn calibration_ood_texts() -> Vec<String> {
    let mut rows = (0..calyx_ward::MIN_BAD_SCORES)
        .map(|index| {
            format!(
                "fire ward gate protect enemy agent contradictory route {} void bargain cold ledger",
                index
            )
        })
        .collect::<Vec<_>>();
    rows[0] = adversarial_overlap_text();
    rows
}

fn adversarial_overlap_text() -> String {
    "fire ward gate protect enemy agent protect gate enemy ward fire agent reversed bargain"
        .to_string()
}

fn content_store(
    label: &str,
) -> (
    EngineCalyxContentStore,
    EngineCalyxDatabase,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database.clone(), label),
        database,
        vault_dir,
    )
}

fn content(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        payload,
        source: EngineContentSource {
            path: format!("synthetic://issue1121/{id}"),
            author: "issue1121-test".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1121:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1121_grounding_anchor".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn signal(signal_id: &str, terms: &[&str], summary: &str) -> EngineWorldSignal {
    EngineWorldSignal {
        signal_id: signal_id.to_string(),
        world_id: "world-ashen-reach".to_string(),
        tick: 12,
        kind: EngineWorldSignalKind::Combat,
        strength_milli: 900,
        terms: terms.iter().map(|term| term.to_string()).collect(),
        summary: summary.to_string(),
        provenance_refs: vec![
            "grounding://signal/source".to_string(),
            format!("signal://issue1121/{signal_id}"),
        ],
    }
}

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1121-{label}"))
}
