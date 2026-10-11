use super::*;

const HWND: u64 = 42_424;
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn observation(step: UserJourneyStep, index: usize) -> UserJourneyObservation {
    UserJourneyObservation {
        step,
        exact_hwnd: HWND,
        observed_unix_ms: 1_000 + (index as u64 * 100),
        duration_ms: 100,
        surface_id: format!("editor.{}", step.label()),
        action: format!("complete {}", step.label()),
        outcome: format!("{} visible", step.label()),
        obvious: true,
        fast: true,
        rewarding: true,
        dead_end: false,
        evidence: UserJourneyEvidence {
            bitmap_artifact: format!("issue905-step-{index}.png"),
            bitmap_sha256: SHA.to_string(),
            state_source: "runtime state readback".to_string(),
            state_readback: format!("step_index={index}"),
        },
    }
}

#[test]
fn ordered_walkthrough_produces_passed_summary() {
    let mut session = UserJourneyAuditSession::start("issue905", HWND, 1_000).unwrap();
    for (index, step) in UserJourneyStep::ORDER.into_iter().enumerate() {
        session.record(observation(step, index)).unwrap();
    }

    let summary = session.summary(2_000).unwrap();
    assert_eq!(summary.verdict, UserJourneyVerdict::Passed);
    assert_eq!(summary.recorded_step_count, 7);
    assert_eq!(summary.next_step, None);
    assert!(summary.failures.is_empty());
}

#[test]
fn skipped_step_fails_without_mutating_session() {
    let mut session = UserJourneyAuditSession::start("issue905", HWND, 1_000).unwrap();
    let error = session
        .record(observation(UserJourneyStep::MakeEdit, 0))
        .unwrap_err();

    assert!(error.to_string().contains("expected First launch"));
    assert!(session.observations.is_empty());
}

#[test]
fn wrong_hwnd_and_thin_evidence_fail_loud() {
    let mut session = UserJourneyAuditSession::start("issue905", HWND, 1_000).unwrap();
    let mut wrong_window = observation(UserJourneyStep::FirstLaunch, 0);
    wrong_window.exact_hwnd = 7;
    assert!(
        session
            .record(wrong_window)
            .unwrap_err()
            .to_string()
            .contains("session is bound")
    );

    let mut thin = observation(UserJourneyStep::FirstLaunch, 0);
    thin.evidence.bitmap_sha256 = "not-a-hash".to_string();
    assert!(
        session
            .record(thin)
            .unwrap_err()
            .to_string()
            .contains("64-character SHA-256")
    );
    assert!(session.observations.is_empty());
}

#[test]
fn dead_end_or_weak_quality_produces_failed_summary() {
    let mut session = UserJourneyAuditSession::start("issue905", HWND, 1_000).unwrap();
    let mut first = observation(UserJourneyStep::FirstLaunch, 0);
    first.obvious = false;
    first.rewarding = false;
    first.dead_end = true;
    session.record(first).unwrap();

    let summary = session.summary(1_200).unwrap();
    assert_eq!(summary.verdict, UserJourneyVerdict::Failed);
    assert_eq!(summary.failures.len(), 3);
    assert_eq!(summary.next_step, Some(UserJourneyStep::ChooseTemplate));
}

#[test]
fn deserialized_structural_drift_cannot_produce_a_verdict() {
    let mut session = UserJourneyAuditSession::start("issue905", HWND, 1_000).unwrap();
    session
        .observations
        .push(observation(UserJourneyStep::Share, 0));

    let error = session.summary(1_200).unwrap_err();
    assert!(error.to_string().contains("expected First launch"));
}
