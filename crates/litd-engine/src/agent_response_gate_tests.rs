//! Admission choke-point tests (#1548).
//!
//! Every refusal case here checks the **store**, not the return value. `admitted == false` is a
//! claim; `get_record(LoweredAgentOrder, ..) == Err(MissingRecord)` is a fact. The issue's
//! acceptance criterion is explicit that absence must be proved by readback.

use std::collections::BTreeSet;

use crate::{
    agent_response::EngineAgentVerbTag,
    agent_response_gate::{
        CODE_ADMITTED, CODE_REFUSED_BINDING, CODE_REFUSED_ENVELOPE, CODE_REFUSED_SCHEMA,
        CODE_REFUSED_TRUNCATED, EngineAgentAdmissionStage, EngineAgentBinding,
        EngineAgentOrderEnvelope, EngineAgentResponseError, EngineAgentResponseReadback,
        EngineAgentWireResponse, admit_agent_response, read_agent_response_admission,
        read_lowered_agent_order, validate_agent_response_readback,
    },
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseConfig, EngineCalyxPersistenceError,
        EngineCalyxRecordKind,
    },
    calyx_ward_gate::EngineCalyxWardPolicy,
    sim::SimBodyId,
    sim_order::{SimOrderKind, SimOrderSourceKind},
    test_temp::TestTempDir,
};

#[test]
fn agent_response_gate_admits_lowers_and_persists_an_in_contract_response() {
    let (database, _vault) = vault("admit");
    let readback = admit(
        &database,
        "admit-happy",
        &binding(),
        &envelope(),
        wire(
            "stop",
            r#"{"agent_id":"guard","target_body":2,
                "verb":{"set_velocity":{"x_milli":1000,"y_milli":0,"z_milli":0}},
                "rationale":"fire ward gate protect enemy agent advance one unit and hold"}"#,
        ),
    );

    assert!(readback.admitted, "reasons: {:?}", readback.reasons);
    assert!(readback.lowering_allowed);
    assert_eq!(readback.stage, EngineAgentAdmissionStage::Lowered);
    assert_eq!(readback.code, CODE_ADMITTED);

    // The order exists, and every authored field came from the BINDING, not the model.
    let order = readback.order.expect("an admitted proposal lowers");
    assert_eq!(order.source.kind, SimOrderSourceKind::LoweredAgent);
    assert_eq!(order.source.id, 7);
    assert_eq!(order.execute_tick, 41);
    assert_eq!(order.sequence, 3);
    assert_eq!(order.target, SimBodyId::from_raw(2));
    assert!(matches!(order.kind, SimOrderKind::SetVelocity(_)));

    // SOURCE OF TRUTH: read the order back out of the store.
    let stored = read_lowered_agent_order(&database, "admit-happy").expect("order row exists");
    assert_eq!(stored, order);
    assert_eq!(stored.fingerprint(), readback.order_fingerprint.unwrap());

    // And the ledger row round-trips through the invariant check.
    let ledger = read_agent_response_admission(&database, "admit-happy").unwrap();
    assert_eq!(ledger, readback);
}

/// The FSV's core claim, in miniature: a refused response leaves NO order on disk.
#[test]
fn agent_response_gate_refuses_out_of_envelope_and_writes_no_order_row() {
    let (database, _vault) = vault("envelope");
    // 6000 milli = 6 units. Inside the SCHEMA ceiling (8000). Outside the ENVELOPE (2 units).
    // This is exactly the gap the two-ceiling design exists to cover.
    let readback = admit(
        &database,
        "refuse-envelope",
        &binding(),
        &envelope(),
        wire(
            "stop",
            r#"{"agent_id":"guard","target_body":2,
                "verb":{"set_velocity":{"x_milli":6000,"y_milli":0,"z_milli":0}},
                "rationale":"fire ward gate protect enemy agent advance one unit and hold"}"#,
        ),
    );

    assert!(!readback.admitted);
    assert!(!readback.lowering_allowed);
    assert!(readback.order.is_none());
    assert!(readback.order_row_key.is_none());
    // Ward may or may not flag the text; the ENVELOPE must stop it either way.
    assert!(
        readback.stage == EngineAgentAdmissionStage::Envelope
            || readback.stage == EngineAgentAdmissionStage::Ward,
        "stage was {:?}",
        readback.stage
    );

    assert_order_row_absent(&database, "refuse-envelope");
    // ...while the LEDGER row exists, proving the gate ran on exactly these bytes.
    assert!(read_agent_response_admission(&database, "refuse-envelope").is_ok());
}

/// A truncated body must be refused BEFORE it is parsed. `schema_errors.is_empty()` is the proof:
/// if we had parsed it, the fragment would have produced violations.
#[test]
fn agent_response_gate_refuses_a_truncated_body_before_parsing_it() {
    let (database, _vault) = vault("truncated");
    let readback = admit(
        &database,
        "refuse-truncated",
        &binding(),
        &envelope(),
        // A real DeepSeek `finish_reason: "length"` body: valid JSON prefix, cut mid-value.
        wire(
            "length",
            r#"{"agent_id":"guard","target_body":2,"verb":{"set_velo"#,
        ),
    );

    assert_eq!(readback.stage, EngineAgentAdmissionStage::Wire);
    assert_eq!(readback.code, CODE_REFUSED_TRUNCATED);
    assert!(
        readback.schema_errors.is_empty(),
        "empty schema_errors is the proof we never parsed the fragment"
    );
    assert!(readback.proposal.is_none());
    assert!(readback.gate.is_none(), "Ward must never have run");
    assert_order_row_absent(&database, "refuse-truncated");
}

#[test]
fn agent_response_gate_refuses_a_schema_violation_and_writes_no_order_row() {
    let (database, _vault) = vault("schema");
    let readback = admit(
        &database,
        "refuse-schema",
        &binding(),
        &envelope(),
        wire(
            "stop",
            r#"{"agent_id":"guard","target_body":2,
                "verb":{"set_velocity":{"x_milli":250000,"y_milli":0,"z_milli":0}},
                "rationale":"charge","override_ward":true}"#,
        ),
    );

    assert_eq!(readback.stage, EngineAgentAdmissionStage::Schema);
    assert_eq!(readback.code, CODE_REFUSED_SCHEMA);
    assert!(!readback.schema_errors.is_empty());
    assert!(readback.proposal.is_none());
    assert!(readback.gate.is_none());
    assert_order_row_absent(&database, "refuse-schema");
}

/// A model may not decide which agent it is or which body it acts on.
#[test]
fn agent_response_gate_refuses_a_proposal_bound_to_another_body() {
    let (database, _vault) = vault("binding");
    let readback = admit(
        &database,
        "refuse-binding",
        &binding(),
        &envelope(),
        wire(
            "stop",
            r#"{"agent_id":"guard","target_body":9,
                "verb":{"set_velocity":{"x_milli":1000,"y_milli":0,"z_milli":0}},
                "rationale":"fire ward gate protect enemy agent advance one unit and hold"}"#,
        ),
    );

    assert_eq!(readback.stage, EngineAgentAdmissionStage::Binding);
    assert_eq!(readback.code, CODE_REFUSED_BINDING);
    assert!(
        readback.gate.is_none(),
        "Ward must not run on a mis-bound proposal"
    );
    assert_order_row_absent(&database, "refuse-binding");
}

/// Damage was NEVER envelope-checked before #1548: `calyx_ward_gate_support::movement_units`
/// returns 0 for an attack, so an attack sailed through the movement envelope untouched.
#[test]
fn agent_response_gate_refuses_damage_above_the_envelope() {
    let (database, _vault) = vault("damage");
    let mut envelope = envelope();
    envelope.allowed_verbs.insert(EngineAgentVerbTag::Attack);
    let readback = admit(
        &database,
        "refuse-damage",
        &binding(),
        &envelope,
        wire(
            "stop",
            r#"{"agent_id":"guard","target_body":2,
                "verb":{"attack":{"attacker_body":5,"damage_milli":4800}},
                "rationale":"fire ward gate protect enemy agent strike the intruder at the gate"}"#,
        ),
    );

    assert!(!readback.admitted);
    assert!(readback.order.is_none());
    assert!(
        readback.stage == EngineAgentAdmissionStage::Envelope
            || readback.stage == EngineAgentAdmissionStage::Ward
    );
    if readback.stage == EngineAgentAdmissionStage::Envelope {
        assert_eq!(readback.code, CODE_REFUSED_ENVELOPE);
    }
    assert_order_row_absent(&database, "refuse-damage");
}

/// A verb the deployment did not allowlist is refused even when it is schema-valid and in range.
#[test]
fn agent_response_gate_refuses_a_verb_outside_the_allowlist() {
    let (database, _vault) = vault("verb");
    let readback = admit(
        &database,
        "refuse-verb",
        &binding(),
        &envelope(), // allows SetVelocity only
        wire(
            "stop",
            r#"{"agent_id":"guard","target_body":2,
                "verb":{"add_velocity":{"x_milli":500,"y_milli":0,"z_milli":0}},
                "rationale":"fire ward gate protect enemy agent advance one unit and hold"}"#,
        ),
    );

    assert!(!readback.admitted);
    assert!(readback.order.is_none());
    assert_order_row_absent(&database, "refuse-verb");
}

/// A row tampered with on disk to flip `lowering_allowed` must not be replayable into an order.
#[test]
fn agent_response_gate_rejects_a_tampered_readback() {
    let (database, _vault) = vault("tamper");
    let mut readback = admit(
        &database,
        "tamper",
        &binding(),
        &envelope(),
        wire(
            "stop",
            r#"{"agent_id":"guard","target_body":2,
                "verb":{"set_velocity":{"x_milli":1000,"y_milli":0,"z_milli":0}},
                "rationale":"fire ward gate protect enemy agent advance one unit and hold"}"#,
        ),
    );
    assert!(readback.admitted);

    readback.admitted = false; // "refused"... but still carrying an order.
    match validate_agent_response_readback(&readback) {
        Err(EngineAgentResponseError::InvalidInput { field, reason }) => {
            assert_eq!(field, "lowering_allowed");
            assert!(reason.contains("only an admitted response may lower"));
        }
        other => panic!("expected the choke-point invariant to reject tampering, got {other:?}"),
    }
}

#[test]
fn agent_response_gate_rejects_an_ungrounded_binding() {
    let (database, _vault) = vault("ungrounded");
    let mut binding = binding();
    binding.grounding_refs.clear();
    match admit_agent_response(
        &database,
        "ungrounded",
        &binding,
        &policy(),
        &envelope(),
        &wire(
            "stop",
            r#"{"agent_id":"guard","target_body":2,"verb":{"hold":{}},"rationale":"x"}"#,
        ),
    ) {
        Err(EngineAgentResponseError::InvalidInput { field, .. }) => {
            assert_eq!(field, "binding.grounding_refs");
        }
        other => panic!("expected an ungrounded binding to fail loud, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

/// The absence proof. A `MissingRecord` on the order store is what "no order was lowered" means.
fn assert_order_row_absent(database: &EngineCalyxDatabase, admission_id: &str) {
    match database.get_record(EngineCalyxRecordKind::LoweredAgentOrder, admission_id) {
        Err(EngineCalyxPersistenceError::MissingRecord { kind, id }) => {
            assert_eq!(kind, EngineCalyxRecordKind::LoweredAgentOrder);
            assert_eq!(id, admission_id);
        }
        Ok(row) => panic!("a refused response lowered an order into the store: {row:?}"),
        Err(other) => panic!("order store readback failed for an unexpected reason: {other:?}"),
    }
}

fn admit(
    database: &EngineCalyxDatabase,
    admission_id: &str,
    binding: &EngineAgentBinding,
    envelope: &EngineAgentOrderEnvelope,
    wire: EngineAgentWireResponse,
) -> EngineAgentResponseReadback {
    admit_agent_response(database, admission_id, binding, &policy(), envelope, &wire)
        .expect("the gate itself must be usable")
}

fn wire(finish_reason: &str, content: &str) -> EngineAgentWireResponse {
    EngineAgentWireResponse {
        response_id: "req-issue1548-fixture".to_string(),
        model: "deepseek-v4-flash".to_string(),
        finish_reason: finish_reason.to_string(),
        content: content.to_string(),
        prompt_sha256: litd_hash::sha256_hex(b"issue1548 prompt"),
    }
}

fn binding() -> EngineAgentBinding {
    EngineAgentBinding {
        agent_id: "guard".to_string(),
        target_body: SimBodyId::from_raw(2),
        source_id: 7,
        execute_tick: 41,
        sequence: 3,
        grounding_refs: vec![
            "grounding://signal/source".to_string(),
            "signal://issue1548/enemy-at-gate".to_string(),
        ],
    }
}

fn envelope() -> EngineAgentOrderEnvelope {
    EngineAgentOrderEnvelope {
        allowed_verbs: BTreeSet::from([EngineAgentVerbTag::Hold, EngineAgentVerbTag::SetVelocity]),
        max_move_units_per_tick: 2,
        max_damage_milli: 1_200,
        max_rationale_chars: 320,
        allow_self_attack: false,
    }
}

/// Ward policy for RESPONSES. `trusted_texts` are authored exemplar *response narrations* — what
/// an in-contract rationale looks like — NOT prompts. Ward is a cosine gate over the subject text;
/// trusting prompt text would make every response novel.
fn policy() -> EngineCalyxWardPolicy {
    EngineCalyxWardPolicy {
        policy_id: "ward-agent-response-issue1548".to_string(),
        allowed_terms: vec![
            "fire".to_string(),
            "ward".to_string(),
            "gate".to_string(),
            "protect".to_string(),
            "enemy".to_string(),
            "agent".to_string(),
            "hold".to_string(),
            "advance".to_string(),
        ],
        blocked_terms: vec![
            "ignore".to_string(),
            "instructions".to_string(),
            "unbounded".to_string(),
            "teleport".to_string(),
            "exploit".to_string(),
        ],
        trusted_texts: trusted_response_texts(),
        calibration_ood_texts: calibration_ood_texts(),
        min_grounding_refs: 2,
        min_matched_terms: 2,
        max_move_units_per_tick: 2,
        allow_teleport: false,
        max_scale_milli: 1500,
        max_emissive_milli: 1000,
    }
}

/// Exemplar in-contract responses, in the exact shape `ward_subject_from_agent_response` scores:
/// `"<verb_tag> <rationale>"`.
fn trusted_response_texts() -> Vec<String> {
    vec![
        "set_velocity fire ward gate protect enemy agent advance one unit and hold".to_string(),
        "hold fire ward gate protect enemy agent hold the line at the gate".to_string(),
        "attack fire ward gate protect enemy agent strike the intruder at the gate".to_string(),
    ]
}

fn calibration_ood_texts() -> Vec<String> {
    let mut rows = (0..calyx_ward::MIN_BAD_SCORES)
        .map(|index| {
            format!(
                "fire ward gate protect enemy agent contradictory route {index} void bargain cold ledger"
            )
        })
        .collect::<Vec<_>>();
    rows[0] =
        "fire ward gate protect enemy agent protect gate enemy ward fire agent reversed bargain"
            .to_string();
    rows
}

fn vault(label: &str) -> (EngineCalyxDatabase, TestTempDir) {
    let temp = TestTempDir::new(&format!("issue1548-{label}"));
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(
        temp.path().join("aster-vault"),
    ))
    .expect("vault opens");
    (database, temp)
}
