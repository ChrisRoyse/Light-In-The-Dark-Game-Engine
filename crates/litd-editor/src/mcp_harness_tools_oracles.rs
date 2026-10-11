use litd_engine::{EngineError, math::Fixed};

use crate::{history::EditorDocument, mcp_project_tools::read_litdworld_project};

use super::{
    HarnessOracleCheck, HarnessOracleKind, HarnessOracleRunRecord, HarnessRunRecord,
    mcp_harness_tools_validation::invalid_harness,
};

const DEFAULT_MAX_DRAW_BATCHES: u64 = 512;
const DEFAULT_MAX_VISIBLE_INSTANCES: u64 = 4096;
const DEFAULT_PHYSICS_MAX_ABS_MILLI: i64 = 10_000_000;
const DEFAULT_MAX_REWARD_DELTA_MILLI: i64 = 100_000;
const DEFAULT_SOAK_MIN_TICKS: u64 = 0;
const DEFAULT_SOAK_MAX_BODY_UPDATES_PER_STEP: u64 = 16;
const REWARD_KEYS: &[&str] = &["reward_milli", "gold_reward_milli", "economy_delta_milli"];

pub(super) fn build_oracle_record(
    oracle_run_id: &str,
    run: &HarnessRunRecord,
    oracles: &[HarnessOracleKind],
) -> Result<HarnessOracleRunRecord, EngineError> {
    let mut checks = Vec::with_capacity(oracles.len());
    for oracle in oracles {
        checks.push(evaluate_oracle(*oracle, run)?);
    }
    let passed_count = checks.iter().filter(|check| check.passed).count();
    let failed_count = checks.len() - passed_count;
    Ok(HarnessOracleRunRecord {
        oracle_run_id: oracle_run_id.to_string(),
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

pub(super) fn validate_oracle_run_record(
    record: &HarnessOracleRunRecord,
) -> Result<(), EngineError> {
    if record.oracle_run_id.trim().is_empty()
        || record.run_id.trim().is_empty()
        || record.check_count != record.checks.len()
        || record.passed_count + record.failed_count != record.check_count
        || record.checks.is_empty()
    {
        return invalid_harness("oracle.record", "oracle run counts or ids are invalid");
    }
    let expected = if record.failed_count == 0 {
        "passed"
    } else {
        "failed"
    };
    if record.verdict != expected {
        return invalid_harness("oracle.verdict", "oracle verdict state is inconsistent");
    }
    for check in &record.checks {
        if check.actual.trim().is_empty() || check.reason.trim().is_empty() {
            return invalid_harness("oracle.check", "oracle check details are invalid");
        }
    }
    Ok(())
}

fn evaluate_oracle(
    oracle: HarnessOracleKind,
    run: &HarnessRunRecord,
) -> Result<HarnessOracleCheck, EngineError> {
    match oracle {
        HarnessOracleKind::DeterministicReplay => deterministic_replay(oracle, run),
        HarnessOracleKind::PresentationNonEmpty => Ok(presentation_non_empty(oracle, run)),
        HarnessOracleKind::NoErrorVerdict => Ok(no_error_verdict(oracle, run)),
        HarnessOracleKind::CrashFree => Ok(crash_free(oracle, run)),
        HarnessOracleKind::SoftLockFree => soft_lock_free(oracle, run),
        HarnessOracleKind::PhysicsBounds => physics_bounds(oracle, run),
        HarnessOracleKind::PerfBudget => perf_budget(oracle, run),
        HarnessOracleKind::EconomyRewardSanity => economy_reward_sanity(oracle, run),
        HarnessOracleKind::SoakStable => soak_stable(oracle, run),
    }
}

fn deterministic_replay(
    oracle: HarnessOracleKind,
    run: &HarnessRunRecord,
) -> Result<HarnessOracleCheck, EngineError> {
    let mut sim = run.initial_snapshot.restore::<16>()?;
    let report = sim.run_ticks(run.steps)?;
    let passed = report.state_hash == run.final_state_hash;
    Ok(HarnessOracleCheck {
        oracle,
        passed,
        actual: format!("{:016X}", report.state_hash),
        reason: format!("expected final hash {:016X}", run.final_state_hash),
    })
}

fn presentation_non_empty(oracle: HarnessOracleKind, run: &HarnessRunRecord) -> HarnessOracleCheck {
    HarnessOracleCheck {
        oracle,
        passed: run.visible_instance_count > 0 && run.draw_batches > 0,
        actual: format!(
            "visible_instances={} draw_batches={}",
            run.visible_instance_count, run.draw_batches
        ),
        reason: "presentation had visible instances and draw batches".to_string(),
    }
}

fn no_error_verdict(oracle: HarnessOracleKind, run: &HarnessRunRecord) -> HarnessOracleCheck {
    HarnessOracleCheck {
        oracle,
        passed: run.verdict == "passed",
        actual: run.verdict.clone(),
        reason: "run verdict was persisted as passed".to_string(),
    }
}

fn crash_free(oracle: HarnessOracleKind, run: &HarnessRunRecord) -> HarnessOracleCheck {
    HarnessOracleCheck {
        oracle,
        passed: run.verdict == "passed",
        actual: format!("verdict={}", run.verdict),
        reason: "frame watcher expected no crash/error verdict".to_string(),
    }
}

fn soft_lock_free(
    oracle: HarnessOracleKind,
    run: &HarnessRunRecord,
) -> Result<HarnessOracleCheck, EngineError> {
    let project = read_litdworld_project(&run.project_path)?;
    let min_updates =
        read_u64_property(&project.document, "oracle.soft_lock_min_bodies_updated", 1)?;
    let locked = run.steps > 0
        && (run.spawned_bodies == 0
            || run.bodies_updated < min_updates
            || run.final_tick <= run.start_tick
            || run.final_state_hash == run.initial_state_hash);
    Ok(HarnessOracleCheck {
        oracle,
        passed: !locked,
        actual: format!(
            "steps={} spawned_bodies={} bodies_updated={} min_bodies_updated={} start_tick={} final_tick={} initial_hash={:016X} final_hash={:016X}",
            run.steps,
            run.spawned_bodies,
            run.bodies_updated,
            min_updates,
            run.start_tick,
            run.final_tick,
            run.initial_state_hash,
            run.final_state_hash
        ),
        reason: "frame watcher expected tick, body, and state progress".to_string(),
    })
}

fn physics_bounds(
    oracle: HarnessOracleKind,
    run: &HarnessRunRecord,
) -> Result<HarnessOracleCheck, EngineError> {
    let project = read_litdworld_project(&run.project_path)?;
    let limit_milli = read_i64_property(
        &project.document,
        "oracle.physics_max_abs_milli",
        DEFAULT_PHYSICS_MAX_ABS_MILLI,
    )?;
    if limit_milli < 0 {
        return invalid_harness(
            "oracle.physics_max_abs_milli",
            "physics absolute bound must be non-negative",
        );
    }
    let limit_raw = fixed_milli_to_abs_raw(limit_milli)?;
    let mut max_abs_raw = 0_i128;
    let mut offender = "none".to_string();
    for body in &run.final_snapshot.bodies {
        for (metric, vector) in [
            ("position", body.position),
            ("velocity", body.velocity),
            ("acceleration", body.acceleration),
        ] {
            for (axis, value) in [("x", vector.x), ("y", vector.y), ("z", vector.z)] {
                let abs_raw = fixed_abs_raw(value);
                if abs_raw > max_abs_raw {
                    max_abs_raw = abs_raw;
                }
                if abs_raw > limit_raw && offender == "none" {
                    offender = format!("body={} {metric}.{axis} raw={abs_raw}", body.id.index());
                }
            }
        }
    }
    Ok(HarnessOracleCheck {
        oracle,
        passed: offender == "none",
        actual: format!(
            "body_count={} max_abs_raw={} limit_raw={} limit_milli={} offender={}",
            run.final_snapshot.bodies.len(),
            max_abs_raw,
            limit_raw,
            limit_milli,
            offender
        ),
        reason: "frame watcher expected fixed-point body vectors inside physics bounds".to_string(),
    })
}

fn perf_budget(
    oracle: HarnessOracleKind,
    run: &HarnessRunRecord,
) -> Result<HarnessOracleCheck, EngineError> {
    let project = read_litdworld_project(&run.project_path)?;
    let max_draw_batches = read_u64_property(
        &project.document,
        "oracle.max_draw_batches",
        DEFAULT_MAX_DRAW_BATCHES,
    )?;
    let max_visible_instances = read_u64_property(
        &project.document,
        "oracle.max_visible_instances",
        DEFAULT_MAX_VISIBLE_INSTANCES,
    )?;
    let draw_batches = run.draw_batches as u64;
    let visible_instances = run.visible_instance_count as u64;
    Ok(HarnessOracleCheck {
        oracle,
        passed: draw_batches <= max_draw_batches && visible_instances <= max_visible_instances,
        actual: format!(
            "draw_batches={} max_draw_batches={} visible_instances={} max_visible_instances={}",
            draw_batches, max_draw_batches, visible_instances, max_visible_instances
        ),
        reason: "frame watcher expected presentation telemetry inside perf budget".to_string(),
    })
}

fn economy_reward_sanity(
    oracle: HarnessOracleKind,
    run: &HarnessRunRecord,
) -> Result<HarnessOracleCheck, EngineError> {
    let project = read_litdworld_project(&run.project_path)?;
    let limit = read_i64_property(
        &project.document,
        "oracle.max_reward_delta_milli",
        DEFAULT_MAX_REWARD_DELTA_MILLI,
    )?;
    if limit < 0 {
        return invalid_harness(
            "oracle.max_reward_delta_milli",
            "reward delta budget must be non-negative",
        );
    }
    let mut field_count = 0_usize;
    let mut max_abs_delta = 0_i128;
    let mut invalid_fields = Vec::new();
    for object in project.document.objects.values() {
        for key in REWARD_KEYS {
            if let Some(value) = object.properties.get(*key) {
                field_count += 1;
                match value.parse::<i64>() {
                    Ok(delta) => max_abs_delta = max_abs_delta.max((delta as i128).abs()),
                    Err(_) => invalid_fields.push(format!("{}.{}", object.id, key)),
                }
            }
        }
    }
    let limit_abs = limit as i128;
    Ok(HarnessOracleCheck {
        oracle,
        passed: invalid_fields.is_empty() && max_abs_delta <= limit_abs,
        actual: format!(
            "reward_field_count={} max_abs_delta_milli={} limit_milli={} invalid_fields={}",
            field_count,
            max_abs_delta,
            limit,
            invalid_fields.join("|")
        ),
        reason: "frame watcher expected authored reward/economy deltas inside budget".to_string(),
    })
}

fn soak_stable(
    oracle: HarnessOracleKind,
    run: &HarnessRunRecord,
) -> Result<HarnessOracleCheck, EngineError> {
    let project = read_litdworld_project(&run.project_path)?;
    let min_ticks = read_u64_property(
        &project.document,
        "oracle.soak_min_ticks",
        DEFAULT_SOAK_MIN_TICKS,
    )?;
    let max_updates_per_step = read_u64_property(
        &project.document,
        "oracle.soak_max_body_updates_per_step",
        DEFAULT_SOAK_MAX_BODY_UPDATES_PER_STEP,
    )?;
    let update_budget = run.steps.checked_mul(max_updates_per_step).ok_or_else(|| {
        invalid_harness::<()>("oracle.soak_budget", "soak update budget overflowed").unwrap_err()
    })?;
    Ok(HarnessOracleCheck {
        oracle,
        passed: run.steps >= min_ticks && run.bodies_updated <= update_budget,
        actual: format!(
            "steps={} min_ticks={} bodies_updated={} update_budget={} max_updates_per_step={}",
            run.steps, min_ticks, run.bodies_updated, update_budget, max_updates_per_step
        ),
        reason: "frame watcher expected sustained soak duration and bounded body updates"
            .to_string(),
    })
}

fn read_u64_property(
    document: &EditorDocument,
    key: &str,
    default: u64,
) -> Result<u64, EngineError> {
    match document_property(document, key) {
        Some(value) => value.parse::<u64>().map_err(|_| {
            invalid_harness::<()>(
                format!("oracle.{key}"),
                format!("property {key} must be an unsigned integer"),
            )
            .unwrap_err()
        }),
        None => Ok(default),
    }
}

fn read_i64_property(
    document: &EditorDocument,
    key: &str,
    default: i64,
) -> Result<i64, EngineError> {
    match document_property(document, key) {
        Some(value) => value.parse::<i64>().map_err(|_| {
            invalid_harness::<()>(
                format!("oracle.{key}"),
                format!("property {key} must be an integer"),
            )
            .unwrap_err()
        }),
        None => Ok(default),
    }
}

fn document_property<'a>(document: &'a EditorDocument, key: &str) -> Option<&'a str> {
    document
        .objects
        .values()
        .find_map(|object| object.properties.get(key).map(String::as_str))
}

fn fixed_milli_to_abs_raw(value: i64) -> Result<i128, EngineError> {
    (value as i128)
        .checked_mul(Fixed::ONE_RAW as i128)
        .and_then(|raw| raw.checked_div(1000))
        .ok_or_else(|| {
            invalid_harness::<()>("oracle.physics_max_abs_milli", "physics bound overflowed")
                .unwrap_err()
        })
}

fn fixed_abs_raw(value: Fixed) -> i128 {
    let raw = value.raw() as i128;
    if raw < 0 { -raw } else { raw }
}
