use std::thread;

use litd_engine::{
    EngineError,
    math::{Fixed, FixedVec3},
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::SimSnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

use super::{
    HarnessMutationContext, HarnessRunRecord, McpHarnessToolCallReadback, find_run,
    finish_mutation, load_state,
    mcp_harness_tools_helpers::{parse_args, upsert_by, validate_mutating_route},
    mcp_harness_tools_validation::{
        invalid_harness, validate_context, validate_path_string, validate_token,
    },
};

pub const MAX_SANDBOXES_PER_BATCH: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSandboxesHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub sandbox_batch_id: String,
    pub run_id: String,
    pub sandboxes: Vec<HarnessSandboxInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSandboxInput {
    pub sandbox_id: String,
    pub ticks: u64,
    pub target_body_id: Option<u32>,
    pub velocity_x_milli: i64,
    pub velocity_z_milli: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessSandboxBatchRecord {
    pub sandbox_batch_id: String,
    pub source_run_id: String,
    pub source_final_tick: u64,
    pub source_final_state_hash: u64,
    pub source_of_truth: String,
    pub requested_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub max_sandbox_count: usize,
    pub completed_final_hashes_unique: bool,
    pub sandboxes: Vec<HarnessSandboxResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessSandboxResult {
    pub sandbox_id: String,
    pub status: HarnessSandboxStatus,
    pub target_body_id: Option<u32>,
    pub ticks: u64,
    pub velocity_x_milli: i64,
    pub velocity_z_milli: i64,
    pub order_count: u64,
    pub initial_state_hash: u64,
    pub final_state_hash: Option<u64>,
    pub final_tick: Option<u64>,
    pub bodies_updated: Option<u64>,
    pub final_snapshot: Option<SimSnapshot>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessSandboxStatus {
    Completed,
    Failed,
}

pub(super) fn run_sandboxes(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: RunSandboxesHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_sandbox_args(&args)?;
    let mut state = load_state(&args.context)?;
    let run = find_run(&state, &args.run_id)?.clone();
    let record = build_sandbox_batch_record(&args, &run)?;
    upsert_by(
        &mut state.sandbox_batches,
        |item| item.sandbox_batch_id == record.sandbox_batch_id,
        record.clone(),
    );
    finish_mutation(
        route,
        args.context,
        state,
        "ran isolated deterministic sandbox simulations in parallel",
        vec![record.sandbox_batch_id.clone()],
        json!({ "sandbox_batch": record }),
    )
}

pub(super) fn validate_sandbox_batch_record(
    record: &HarnessSandboxBatchRecord,
) -> Result<(), EngineError> {
    validate_token("sandbox_batch_id", &record.sandbox_batch_id)?;
    validate_token("sandbox.source_run_id", &record.source_run_id)?;
    if record.requested_count == 0
        || record.requested_count > MAX_SANDBOXES_PER_BATCH
        || record.requested_count != record.sandboxes.len()
        || record.completed_count + record.failed_count != record.requested_count
        || record.source_of_truth.trim().is_empty()
        || record.max_sandbox_count != MAX_SANDBOXES_PER_BATCH
    {
        return invalid_harness("sandbox.batch", "sandbox batch counts are invalid");
    }
    let mut completed_hashes = Vec::new();
    for sandbox in &record.sandboxes {
        validate_token("sandbox.sandbox_id", &sandbox.sandbox_id)?;
        if sandbox.initial_state_hash != record.source_final_state_hash {
            return invalid_harness(
                "sandbox.initial_state_hash",
                "sandbox did not restore source",
            );
        }
        match sandbox.status {
            HarnessSandboxStatus::Completed => {
                if sandbox.final_state_hash.is_none()
                    || sandbox.final_tick.is_none()
                    || sandbox.bodies_updated.is_none()
                    || sandbox.final_snapshot.is_none()
                    || sandbox.error.is_some()
                {
                    return invalid_harness("sandbox.completed", "completed fields are invalid");
                }
                completed_hashes.push(sandbox.final_state_hash.unwrap_or_default());
            }
            HarnessSandboxStatus::Failed => {
                if sandbox
                    .error
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                    || sandbox.final_snapshot.is_some()
                {
                    return invalid_harness("sandbox.failed", "failed fields are invalid");
                }
            }
        }
    }
    let unique = unique_count(&completed_hashes) == completed_hashes.len();
    if record.completed_final_hashes_unique != unique {
        return invalid_harness("sandbox.hashes", "unique hash flag is invalid");
    }
    Ok(())
}

fn build_sandbox_batch_record(
    args: &RunSandboxesHarnessArgs,
    run: &HarnessRunRecord,
) -> Result<HarnessSandboxBatchRecord, EngineError> {
    let mut results = Vec::with_capacity(args.sandboxes.len());
    thread::scope(|scope| {
        let handles = args
            .sandboxes
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, input)| {
                let snapshot = run.final_snapshot.clone();
                scope.spawn(move || (index, run_one_sandbox(input, snapshot)))
            })
            .collect::<Vec<_>>();
        for handle in handles {
            let (index, result) = handle.join().unwrap_or_else(|_| {
                (
                    usize::MAX,
                    failed_worker_result("sandbox-worker-panic", "sandbox worker panicked"),
                )
            });
            results.push((index, result));
        }
    });
    results.sort_by_key(|(index, _)| *index);
    let sandboxes = results
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Vec<_>>();
    let completed_count = sandboxes
        .iter()
        .filter(|result| result.status == HarnessSandboxStatus::Completed)
        .count();
    let failed_count = sandboxes.len() - completed_count;
    let completed_hashes = sandboxes
        .iter()
        .filter_map(|result| result.final_state_hash)
        .collect::<Vec<_>>();
    let record = HarnessSandboxBatchRecord {
        sandbox_batch_id: args.sandbox_batch_id.clone(),
        source_run_id: run.run_id.clone(),
        source_final_tick: run.final_tick,
        source_final_state_hash: run.final_state_hash,
        source_of_truth: "Calyx Aster MCP harness state row + sandbox ledger rows".to_string(),
        requested_count: sandboxes.len(),
        completed_count,
        failed_count,
        max_sandbox_count: MAX_SANDBOXES_PER_BATCH,
        completed_final_hashes_unique: unique_count(&completed_hashes) == completed_hashes.len(),
        sandboxes,
    };
    validate_sandbox_batch_record(&record)?;
    Ok(record)
}

fn run_one_sandbox(input: HarnessSandboxInput, snapshot: SimSnapshot) -> HarnessSandboxResult {
    let initial_state_hash = snapshot.state_hash;
    match execute_sandbox(&input, &snapshot) {
        Ok(success) => success,
        Err(error) => HarnessSandboxResult {
            sandbox_id: input.sandbox_id,
            status: HarnessSandboxStatus::Failed,
            target_body_id: input.target_body_id,
            ticks: input.ticks,
            velocity_x_milli: input.velocity_x_milli,
            velocity_z_milli: input.velocity_z_milli,
            order_count: 0,
            initial_state_hash,
            final_state_hash: None,
            final_tick: None,
            bodies_updated: None,
            final_snapshot: None,
            error: Some(error.to_string()),
        },
    }
}

fn execute_sandbox(
    input: &HarnessSandboxInput,
    snapshot: &SimSnapshot,
) -> Result<HarnessSandboxResult, EngineError> {
    let mut sim = snapshot.restore::<16>()?;
    let target_body_id = input
        .target_body_id
        .or_else(|| snapshot.bodies.first().map(|body| body.id.index()))
        .ok_or_else(|| {
            invalid_harness::<()>("sandbox.target", "source run has no body").unwrap_err()
        })?;
    let mut queue = SimOrderQueue::<1>::default();
    queue.enqueue(SimOrder::new(
        sim.tick() + 1,
        SimOrderSource::lowered_agent(1),
        1,
        SimBodyId::from_raw(target_body_id),
        SimOrderKind::SetVelocity(FixedVec3::new(
            Fixed::from_ratio(input.velocity_x_milli, 1000)?,
            Fixed::from_raw(0),
            Fixed::from_ratio(input.velocity_z_milli, 1000)?,
        )),
    ))?;
    let report = queue.run_ticks(&mut sim, input.ticks)?;
    let final_snapshot = SimSnapshot::capture(&sim);
    Ok(HarnessSandboxResult {
        sandbox_id: input.sandbox_id.clone(),
        status: HarnessSandboxStatus::Completed,
        target_body_id: Some(target_body_id),
        ticks: input.ticks,
        velocity_x_milli: input.velocity_x_milli,
        velocity_z_milli: input.velocity_z_milli,
        order_count: queue.applied_total(),
        initial_state_hash: snapshot.state_hash,
        final_state_hash: Some(report.state_hash),
        final_tick: Some(report.final_tick),
        bodies_updated: Some(report.bodies_updated),
        final_snapshot: Some(final_snapshot),
        error: None,
    })
}

fn validate_sandbox_args(args: &RunSandboxesHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("sandbox_batch_id", &args.sandbox_batch_id)?;
    validate_token("run_id", &args.run_id)?;
    if args.sandboxes.is_empty() || args.sandboxes.len() > MAX_SANDBOXES_PER_BATCH {
        return invalid_harness("sandboxes", "sandbox count must be 1..=8");
    }
    let mut ids = Vec::with_capacity(args.sandboxes.len());
    for sandbox in &args.sandboxes {
        validate_token("sandbox.sandbox_id", &sandbox.sandbox_id)?;
        validate_ticks(sandbox.ticks, "sandbox.ticks")?;
        ids.push(sandbox.sandbox_id.as_str());
    }
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return invalid_harness("sandbox.sandbox_id", "sandbox ids must be unique");
    }
    validate_path_string("state_path", &args.context.state_path)
}

fn validate_ticks(ticks: u64, field: &str) -> Result<(), EngineError> {
    if ticks > 1_000_000 {
        return invalid_harness(field, "tick count exceeds the harness safety limit");
    }
    Ok(())
}

fn unique_count(values: &[u64]) -> usize {
    let mut unique = values.to_vec();
    unique.sort_unstable();
    unique.dedup();
    unique.len()
}

fn failed_worker_result(sandbox_id: &str, error: &str) -> HarnessSandboxResult {
    HarnessSandboxResult {
        sandbox_id: sandbox_id.to_string(),
        status: HarnessSandboxStatus::Failed,
        target_body_id: None,
        ticks: 0,
        velocity_x_milli: 0,
        velocity_z_milli: 0,
        order_count: 0,
        initial_state_hash: 0,
        final_state_hash: None,
        final_tick: None,
        bodies_updated: None,
        final_snapshot: None,
        error: Some(error.to_string()),
    }
}
