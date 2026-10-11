//! #1552 FSV — the Ward response policy is sourced from an AUTHORED, persisted corpus, and a
//! degenerate calibration fails loud.
//!
//! This FSV is **deterministic and offline**: it does not call any model provider. That is correct
//! for what #1552 proves. #1552 is not about the transport (that is #1548's live FSV); it is about
//! (1) the authoring path — a Ward response corpus derived from ingested world content, persisted
//! with provenance and a self-hash, read back — and (2) the degenerate-calibration guard. The four
//! admit/refuse cases feed the choke point `admit_agent_response` its adversarial input directly,
//! because the GATE is the system under test and the bytes are the attacker's payload — a real
//! model will not emit off-topic garbage or an out-of-envelope order on command (which is exactly
//! why #1548 had to coax it with an echo service). Constructing the exact adversarial bytes is more
//! honest than coaxing a model, and it makes every case reproducible.
//!
//! # Source of truth
//!
//! Nothing is trusted by return value. The corpus is read back out of the vault and its self-hash
//! re-verified; each admit/refuse outcome is read back out of the admission ledger; the lowered
//! order (or its absence) is read back out of the order store; and the admitted order is applied to
//! a sim whose state hash is compared against a baseline that never saw a proposal.
//!
//! Run:
//! ```text
//! cargo run -p litd-engine --example issue1552_ward_corpus_fsv
//! ```

use std::io;

use litd_engine::{
    agent_response::EngineAgentVerbTag,
    agent_response_corpus::{
        WardResponseCorpusError, assess_corpus_separation, build_corpus, build_corpus_with_texts,
        corpus_source_from_content, persist_corpus, read_corpus,
    },
    agent_response_gate::{admit_agent_response, read_lowered_agent_order},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
};
use serde_json::json;

#[path = "issue1552_ward_corpus_fsv/support.rs"]
mod support;

use support::{
    CONTENT_ROW_ID, assert_no_order, binding, case_row, ensure, envelope,
    ingest_and_read_agent_content, report_case, run_sim, ward_slots, wire, write_report,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1552_ward_corpus")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;

    // -----------------------------------------------------------------------------------------
    // STEP 1 — INGEST world content, read it back, and derive the corpus source from the readback.
    // The grounded lexicon is read out of a persisted record, never typed into this example.
    // -----------------------------------------------------------------------------------------
    let ingested = ingest_and_read_agent_content(&database)?;
    let source = corpus_source_from_content(
        &ingested,
        "thera",
        "gate-guard",
        vec![EngineAgentVerbTag::SetVelocity, EngineAgentVerbTag::Hold],
    )?;
    println!(
        "STEP1 ingested content row={CONTENT_ROW_ID} grounded_terms={:?}",
        source.grounded_terms
    );

    // -----------------------------------------------------------------------------------------
    // STEP 2 — BUILD + PERSIST the corpus. The separation guard runs inside build/persist, so a
    // degenerate corpus can never reach the store. Read it back and re-verify the self-hash.
    // -----------------------------------------------------------------------------------------
    let built = build_corpus(&source)?;
    let stored = persist_corpus(&database, &built)?;
    let readback = read_corpus(&database, &stored.corpus_id)?;
    ensure(
        readback == built && readback.corpus_sha256 == built.corpus_sha256,
        "STEP2: corpus readback differs from the authored corpus",
    )?;
    let separation = assess_corpus_separation(&readback)?;
    println!(
        "STEP2 corpus_id={} corpus_sha256={} trusted={} ood={}",
        readback.corpus_id,
        readback.corpus_sha256,
        readback.trusted_texts.len(),
        readback.calibration_ood_texts.len(),
    );
    for slot in &separation.slots {
        println!(
            "       separation slot{} auroc={:.4} fpr_at_tpr={:.4} margin={:.4} (HEALTHY)",
            slot.slot, slot.report.auroc, slot.report.fpr_at_tpr, slot.report.separation_margin
        );
    }

    // The Ward RESPONSE policy is built from the READBACK corpus — not from example code.
    let policy = litd_engine::agent_response_corpus::policy_from_corpus(
        &readback,
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
    let envelope = envelope();
    let binding = binding();
    let baseline_hash = run_sim(None)?;
    println!("BEFORE baseline sim state_hash={baseline_hash:#018x} (no agent order applied)");

    let mut cases = Vec::new();

    // CASE (a) — a legitimate grounded response is ADMITTED and MOVES the body.
    let legit = wire(
        "issue1552-legit",
        "{\"agent_id\":\"guard\",\"target_body\":0,\
         \"verb\":{\"set_velocity\":{\"x_milli\":1000,\"y_milli\":0,\"z_milli\":0}},\
         \"rationale\":\"advance one unit toward the gate and hold the line against the enemy\"}",
    );
    let legit_rb = admit_agent_response(
        &database,
        "issue1552-legit",
        &binding,
        &policy,
        &envelope,
        &legit,
    )?;
    ensure(
        legit_rb.admitted && legit_rb.lowering_allowed,
        &format!(
            "CASE a: a legitimate grounded response was REFUSED at {:?} ({}): {:?} | ward {}",
            legit_rb.stage,
            legit_rb.code,
            legit_rb.reasons,
            ward_slots(&legit_rb)
        ),
    )?;
    let order = legit_rb
        .order
        .ok_or_else(|| io::Error::other("CASE a: admitted response lowered no order"))?;
    let stored_order = read_lowered_agent_order(&database, "issue1552-legit")?;
    ensure(
        stored_order == order,
        "CASE a: stored order differs from returned",
    )?;
    let admitted_hash = run_sim(Some(order))?;
    ensure(
        admitted_hash != baseline_hash,
        "CASE a: an admitted order left the sim bit-identical to baseline",
    )?;
    report_case(
        "a legit-admitted",
        &database,
        "issue1552-legit",
        Some(admitted_hash),
    )?;
    cases.push(case_row(&legit_rb, Some(admitted_hash)));

    // CASE (b) — off-topic garbage is REFUSED by the cosine detector (CALYX_GUARD_OOD).
    let garbage = wire(
        "issue1552-garbage",
        "{\"agent_id\":\"guard\",\"target_body\":0,\
         \"verb\":{\"set_velocity\":{\"x_milli\":1000,\"y_milli\":0,\"z_milli\":0}},\
         \"rationale\":\"quarterly revenue grew twelve percent as investors weighed cereal inflation\"}",
    );
    let garbage_rb = admit_agent_response(
        &database,
        "issue1552-garbage",
        &binding,
        &policy,
        &envelope,
        &garbage,
    )?;
    ensure(
        !garbage_rb.admitted && garbage_rb.code == "CALYX_GUARD_OOD",
        &format!(
            "CASE b: off-topic garbage must be refused by the cosine detector (CALYX_GUARD_OOD); \
             got admitted={} code={} stage={:?} ward {}",
            garbage_rb.admitted,
            garbage_rb.code,
            garbage_rb.stage,
            ward_slots(&garbage_rb)
        ),
    )?;
    assert_no_order(&database, "issue1552-garbage", baseline_hash)?;
    report_case(
        "b garbage-refused-by-cosine",
        &database,
        "issue1552-garbage",
        None,
    )?;
    cases.push(case_row(&garbage_rb, None));

    // CASE (c) — THE DOCTRINE PROOF. A fluent, on-topic rationale the cosine gate ADMITS, carrying
    // an out-of-envelope movement (6 units; envelope allows 2). Refused by the numeric envelope,
    // NOT the detector.
    let injection = wire(
        "issue1552-injection",
        "{\"agent_id\":\"guard\",\"target_body\":0,\
         \"verb\":{\"set_velocity\":{\"x_milli\":6000,\"y_milli\":0,\"z_milli\":0}},\
         \"rationale\":\"advance one unit toward the gate and hold the line against the enemy\"}",
    );
    let injection_rb = admit_agent_response(
        &database,
        "issue1552-injection",
        &binding,
        &policy,
        &envelope,
        &injection,
    )?;
    ensure(
        !injection_rb.admitted,
        "CASE c: an out-of-envelope order was ADMITTED — the structural gate failed",
    )?;
    let detector_passed = injection_rb
        .gate
        .as_ref()
        .and_then(|gate| gate.verdicts.first())
        .is_some_and(|verdict| verdict.ward_overall_pass);
    ensure(
        detector_passed,
        &format!(
            "CASE c requires the cosine detector to ADMIT the rationale (that is the doctrine \
             proof). It flagged it instead: {}",
            ward_slots(&injection_rb)
        ),
    )?;
    ensure(
        injection_rb.code.ends_with("REFUSED-ENVELOPE"),
        &format!(
            "CASE c must be refused by a deterministic ENVELOPE bound, not the detector; code={:?}",
            injection_rb.code
        ),
    )?;
    assert_no_order(&database, "issue1552-injection", baseline_hash)?;
    report_case(
        "c injection-refused-by-envelope",
        &database,
        "issue1552-injection",
        None,
    )?;
    println!(
        "       ^ the DETECTOR PASSED this fluent on-topic rationale ({}); the numeric envelope \
         refused it anyway. That is the whole doctrine of #1548/#1552.",
        ward_slots(&injection_rb)
    );
    cases.push(case_row(&injection_rb, None));

    // CASE (d) — a DEGENERATE calibration (near-duplicate negatives) FAILS LOUD, naming the overlap
    // statistic, instead of silently emitting a useless tau.
    let degenerate_ood =
        litd_engine::agent_response_corpus::degenerate_calibration_ood(&readback.trusted_texts);
    let degenerate =
        build_corpus_with_texts(&source, readback.trusted_texts.clone(), degenerate_ood);
    let (statistic, rendered) = match degenerate {
        Err(WardResponseCorpusError::DegenerateCalibration { slot, source }) => {
            let statistic = match &source {
                calyx_ward::WardError::DegenerateCalibration { statistic, .. } => statistic.clone(),
                other => {
                    return Err(io::Error::other(format!(
                        "CASE d: expected a DegenerateCalibration WardError, got {other:?}"
                    ))
                    .into());
                }
            };
            (format!("slot{slot}:{statistic}"), source.to_string())
        }
        Err(other) => {
            return Err(io::Error::other(format!(
                "CASE d: a degenerate calibration must fail with DegenerateCalibration, got {other:?}"
            ))
            .into());
        }
        Ok(_) => {
            return Err(io::Error::other(
                "CASE d: a near-duplicate-negative calibration was ACCEPTED — the guard is broken",
            )
            .into());
        }
    };
    println!("CASE d degenerate calibration FAILED LOUD statistic={statistic}");
    println!("       {rendered}");
    cases.push(json!({
        "case": "d degenerate-calibration-fails-loud",
        "admitted": false,
        "failed_statistic": statistic,
        "error": rendered,
    }));

    // ACCEPTANCE — counted from the rows that actually exist on disk.
    write_report(
        &run,
        &database,
        cases,
        baseline_hash,
        &readback.corpus_sha256,
    )
}
