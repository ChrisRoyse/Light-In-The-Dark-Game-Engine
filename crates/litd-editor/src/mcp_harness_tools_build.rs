use litd_engine::{EngineError, sim::FixedTickSim, sim_replay::SimSnapshot};

use crate::{mcp_project_tools::read_litdworld_project, play::PlaySession};

use super::{
    HarnessRunMode, HarnessRunRecord, HarnessRunRenderMode, HarnessRunUntilCondition,
    HarnessSnapshotBranchRecord, mcp_harness_tools_validation::invalid_harness,
};

pub(super) struct HarnessHeadlessRunEvidence {
    pub run: HarnessRunRecord,
    pub player_id: String,
}

pub(super) fn build_run_record(
    run_id: &str,
    project_path: &str,
    camera_rig: crate::play::PlayCameraRig,
    ticks: u64,
    until_condition: Option<&HarnessRunUntilCondition>,
) -> Result<HarnessRunRecord, EngineError> {
    Ok(build_headless_run_evidence(run_id, project_path, camera_rig, ticks, until_condition)?.run)
}

pub(super) fn build_headless_run_evidence(
    run_id: &str,
    project_path: &str,
    camera_rig: crate::play::PlayCameraRig,
    ticks: u64,
    until_condition: Option<&HarnessRunUntilCondition>,
) -> Result<HarnessHeadlessRunEvidence, EngineError> {
    let project = read_litdworld_project(project_path)?;
    let session = PlaySession::start(&project.document, camera_rig)?;
    let player_id = session.build.camera_pose.player_id.clone();
    let mut sim = session.build.snapshot.restore::<16>()?;
    let (report, run_mode, termination_reason) = if let Some(condition) = until_condition {
        let report = sim.run_until(ticks, |sim| until_condition_satisfied(condition, sim))?;
        if !until_condition_satisfied(condition, &sim) {
            return invalid_harness(
                "run.until_condition",
                format!(
                    "condition {} was not satisfied within {} ticks; final_tick={} final_state_hash={:016X}",
                    condition_label(condition),
                    ticks,
                    report.final_tick,
                    report.state_hash
                ),
            );
        }
        (
            report,
            HarnessRunMode::UntilCondition,
            "until_condition_satisfied".to_string(),
        )
    } else {
        (
            sim.run_ticks(ticks)?,
            HarnessRunMode::FixedTicks,
            "ticks_completed".to_string(),
        )
    };
    let final_snapshot = SimSnapshot::capture(&sim);
    Ok(HarnessHeadlessRunEvidence {
        run: HarnessRunRecord {
            run_id: run_id.to_string(),
            project_id: project.project_id,
            project_path: project_path.to_string(),
            camera_rig,
            run_mode,
            render_mode: HarnessRunRenderMode::HeadlessNoRender,
            until_condition: until_condition.cloned(),
            termination_reason,
            ticks_requested: ticks,
            start_tick: report.start_tick,
            final_tick: report.final_tick,
            steps: report.steps,
            bodies_updated: report.bodies_updated,
            initial_state_hash: session.build.snapshot.state_hash,
            final_state_hash: report.state_hash,
            spawned_bodies: session.build.spawned_bodies.len(),
            visible_instance_count: session.build.visible_instance_ids.len(),
            draw_batches: session.build.draw_batches,
            verdict: "passed".to_string(),
            initial_snapshot: session.build.snapshot,
            final_snapshot,
        },
        player_id,
    })
}

fn until_condition_satisfied<const MAX_BODIES: usize>(
    condition: &HarnessRunUntilCondition,
    sim: &FixedTickSim<MAX_BODIES>,
) -> bool {
    match condition {
        HarnessRunUntilCondition::TickAtLeast { tick } => sim.tick() >= *tick,
        HarnessRunUntilCondition::BodyCountAtLeast { body_count } => {
            sim.body_count() >= *body_count
        }
        HarnessRunUntilCondition::StateHashEquals { state_hash } => sim.state_hash() == *state_hash,
    }
}

fn condition_label(condition: &HarnessRunUntilCondition) -> String {
    serde_json::to_string(condition).unwrap_or_else(|_| "unprintable_until_condition".to_string())
}

pub(super) fn build_snapshot_branch_record(
    run: &HarnessRunRecord,
    snapshot_id: &str,
    branch_id: &str,
    branch_ticks: u64,
) -> Result<HarnessSnapshotBranchRecord, EngineError> {
    let mut sim = run.final_snapshot.restore::<16>()?;
    let restored_hash = sim.state_hash();
    let report = sim.run_ticks(branch_ticks)?;
    Ok(HarnessSnapshotBranchRecord {
        snapshot_id: snapshot_id.to_string(),
        branch_id: branch_id.to_string(),
        source_run_id: run.run_id.clone(),
        restored_tick: run.final_snapshot.tick,
        restored_state_hash: restored_hash,
        branch_ticks,
        branch_final_tick: report.final_tick,
        branch_bodies_updated: report.bodies_updated,
        branch_final_state_hash: report.state_hash,
        branch_final_snapshot: SimSnapshot::capture(&sim),
    })
}
