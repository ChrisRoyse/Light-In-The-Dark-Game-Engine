use litd_engine::{EngineError, math::Fixed, sim::SimBodyState};

use super::{
    HarnessAssertion, HarnessAssertionAxis, HarnessAssertionCheck, HarnessAssertionKind,
    HarnessAssertionMetric, HarnessAssertionVerdict, HarnessRunRecord,
    mcp_harness_tools_validation::invalid_harness,
};

pub(super) fn build_assertion_verdict(
    verdict_id: &str,
    run: &HarnessRunRecord,
    assertions: &[HarnessAssertion],
) -> Result<HarnessAssertionVerdict, EngineError> {
    let mut checks = Vec::with_capacity(assertions.len());
    for assertion in assertions {
        checks.push(evaluate_assertion(assertion, run)?);
    }
    let passed_count = checks.iter().filter(|check| check.passed).count();
    let failed_count = checks.len() - passed_count;
    Ok(HarnessAssertionVerdict {
        verdict_id: verdict_id.to_string(),
        run_id: run.run_id.clone(),
        check_count: checks.len(),
        passed_count,
        failed_count,
        verdict: if failed_count == 0 {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        checks,
    })
}

fn evaluate_assertion(
    assertion: &HarnessAssertion,
    run: &HarnessRunRecord,
) -> Result<HarnessAssertionCheck, EngineError> {
    let (actual, expected, passed, code, location, reason) = match assertion.kind {
        HarnessAssertionKind::FinalTickEquals => {
            let (actual, expected, passed) = compare_u64(run.final_tick, assertion.expected_u64);
            summary_check(actual, expected, passed, "run.final_tick")
        }
        HarnessAssertionKind::FinalHashEquals => {
            let (actual, expected, passed) =
                compare_u64(run.final_state_hash, assertion.expected_u64);
            summary_check(actual, expected, passed, "run.final_state_hash")
        }
        HarnessAssertionKind::BodiesUpdatedAtLeast => {
            let min = assertion.min_u64.unwrap_or_default();
            summary_check(
                run.bodies_updated.to_string(),
                format!(">= {min}"),
                run.bodies_updated >= min,
                "run.bodies_updated",
            )
        }
        HarnessAssertionKind::SpawnedBodiesEquals => {
            let (actual, expected, passed) =
                compare_u64(run.spawned_bodies as u64, assertion.expected_u64);
            summary_check(actual, expected, passed, "run.spawned_bodies")
        }
        HarnessAssertionKind::VerdictEquals => {
            let expected = assertion.expected_string.clone().unwrap_or_default();
            summary_check(
                run.verdict.clone(),
                expected.clone(),
                run.verdict == expected,
                "run.verdict",
            )
        }
        HarnessAssertionKind::BodyExists => evaluate_body_exists(assertion, run),
        HarnessAssertionKind::BodyScalarInRange => evaluate_body_scalar_in_range(assertion, run)?,
    };
    Ok(HarnessAssertionCheck {
        assertion_id: assertion.assertion_id.clone(),
        kind: assertion.kind,
        passed,
        expected,
        actual,
        reason,
        code,
        source_of_truth: "persisted harness run final_snapshot".to_string(),
        location,
    })
}

fn summary_check(
    actual: String,
    expected: String,
    passed: bool,
    location: &'static str,
) -> (String, String, bool, String, String, String) {
    (
        actual,
        expected,
        passed,
        if passed {
            "LITD_HARNESS_ASSERT_MATCHED"
        } else {
            "LITD_HARNESS_ASSERT_VALUE_MISMATCH"
        }
        .to_string(),
        location.to_string(),
        if passed {
            "matched persisted run state"
        } else {
            "actual value differed"
        }
        .to_string(),
    )
}

fn evaluate_body_exists(
    assertion: &HarnessAssertion,
    run: &HarnessRunRecord,
) -> (String, String, bool, String, String, String) {
    let body_id = assertion
        .body_id
        .expect("body_exists declarations are validated before evaluation");
    let location = body_location(body_id);
    let present = run
        .final_snapshot
        .bodies
        .iter()
        .any(|body| body.id.index() == body_id);
    (
        format!(
            "body_id={body_id} present={present} body_count={}",
            run.final_snapshot.bodies.len()
        ),
        format!("body_id={body_id} present=true"),
        present,
        if present {
            "LITD_HARNESS_ASSERT_BODY_FOUND"
        } else {
            "LITD_HARNESS_ASSERT_BODY_NOT_FOUND"
        }
        .to_string(),
        location,
        if present {
            "body exists in persisted final snapshot"
        } else {
            "body was absent from persisted final snapshot"
        }
        .to_string(),
    )
}

fn evaluate_body_scalar_in_range(
    assertion: &HarnessAssertion,
    run: &HarnessRunRecord,
) -> Result<(String, String, bool, String, String, String), EngineError> {
    let body_id = assertion
        .body_id
        .expect("body_scalar_in_range declarations are validated before evaluation");
    let metric = assertion
        .metric
        .expect("body_scalar_in_range metric is validated before evaluation");
    let axis = assertion
        .axis
        .expect("body_scalar_in_range axis is validated before evaluation");
    let location = scalar_location(body_id, metric, axis);
    let Some(body) = run
        .final_snapshot
        .bodies
        .iter()
        .find(|body| body.id.index() == body_id)
    else {
        return Ok((
            format!(
                "body_id={body_id} present=false body_count={}",
                run.final_snapshot.bodies.len()
            ),
            format!("body_id={body_id} present=true {location} in range"),
            false,
            "LITD_HARNESS_ASSERT_BODY_NOT_FOUND".to_string(),
            location,
            "body was absent from persisted final snapshot".to_string(),
        ));
    };
    let min_milli = assertion
        .min_milli
        .expect("body_scalar_in_range min_milli is validated before evaluation");
    let max_milli = assertion
        .max_milli
        .expect("body_scalar_in_range max_milli is validated before evaluation");
    let min = Fixed::from_ratio(min_milli, 1000).map_err(|error| {
        invalid_harness::<()>("assertion.min_milli", error.to_string()).unwrap_err()
    })?;
    let max = Fixed::from_ratio(max_milli, 1000).map_err(|error| {
        invalid_harness::<()>("assertion.max_milli", error.to_string()).unwrap_err()
    })?;
    let actual_value = select_body_scalar(body, metric, axis);
    let passed = actual_value >= min && actual_value <= max;
    Ok((
        format!(
            "{}={}milli raw={}",
            location,
            fixed_to_milli(actual_value),
            actual_value.raw()
        ),
        format!(
            "{} in [{}..={}]milli raw=[{}..={}]",
            location,
            min_milli,
            max_milli,
            min.raw(),
            max.raw()
        ),
        passed,
        if passed {
            "LITD_HARNESS_ASSERT_BODY_SCALAR_IN_RANGE"
        } else {
            "LITD_HARNESS_ASSERT_BODY_SCALAR_OUT_OF_RANGE"
        }
        .to_string(),
        location,
        if passed {
            "body scalar matched expected persisted-state range"
        } else {
            "body scalar was outside expected persisted-state range"
        }
        .to_string(),
    ))
}

fn select_body_scalar(
    body: &SimBodyState,
    metric: HarnessAssertionMetric,
    axis: HarnessAssertionAxis,
) -> Fixed {
    let vector = match metric {
        HarnessAssertionMetric::Position => body.position,
        HarnessAssertionMetric::Velocity => body.velocity,
        HarnessAssertionMetric::Acceleration => body.acceleration,
    };
    match axis {
        HarnessAssertionAxis::X => vector.x,
        HarnessAssertionAxis::Y => vector.y,
        HarnessAssertionAxis::Z => vector.z,
    }
}

fn fixed_to_milli(value: Fixed) -> i128 {
    (value.raw() as i128 * 1000) / Fixed::ONE_RAW as i128
}

fn body_location(body_id: u32) -> String {
    format!("final_snapshot.bodies[id={body_id}]")
}

fn scalar_location(
    body_id: u32,
    metric: HarnessAssertionMetric,
    axis: HarnessAssertionAxis,
) -> String {
    format!(
        "{}.{}.{}",
        body_location(body_id),
        metric_label(metric),
        axis_label(axis)
    )
}

fn metric_label(metric: HarnessAssertionMetric) -> &'static str {
    match metric {
        HarnessAssertionMetric::Position => "position",
        HarnessAssertionMetric::Velocity => "velocity",
        HarnessAssertionMetric::Acceleration => "acceleration",
    }
}

fn axis_label(axis: HarnessAssertionAxis) -> &'static str {
    match axis {
        HarnessAssertionAxis::X => "x",
        HarnessAssertionAxis::Y => "y",
        HarnessAssertionAxis::Z => "z",
    }
}

fn compare_u64(actual: u64, expected: Option<u64>) -> (String, String, bool) {
    let expected = expected.unwrap_or_default();
    (actual.to_string(), expected.to_string(), actual == expected)
}
