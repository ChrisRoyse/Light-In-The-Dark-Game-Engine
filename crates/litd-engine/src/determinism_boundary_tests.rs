//! #1463 defect 3: the deterministic hot-path boundary rejects nondeterministic
//! access (Calyx / Rapier physics) on the tick thread, in both the `SimTick` and
//! `RenderFrame` phases, and permits it outside any hot path. These assertions
//! read the `Result` of `record_*_access`, which depends only on the *current*
//! thread's `thread_local!` guard stack — so they are race-free even though
//! Cargo runs tests in parallel and the snapshot counters are process-global.
//!
//! The cross-thread case documents the single-threaded-tick contract: a worker
//! thread is not part of the tick, so its access is (correctly) not flagged.

use crate::{
    EngineError,
    determinism_boundary::{
        HotPathPhase, enter_hot_path, record_calyx_access, record_physics_access,
    },
};

fn is_boundary_violation(result: Result<(), EngineError>, expected_phase: HotPathPhase) -> bool {
    match result {
        Err(EngineError::DeterminismBoundaryViolation { phase, .. }) => {
            phase == expected_phase.as_str()
        }
        _ => false,
    }
}

#[test]
fn calyx_and_physics_access_inside_sim_tick_fail_loud() {
    // Outside any hot path: nondeterministic access is allowed.
    assert!(record_calyx_access("test.calyx.outside").is_ok());
    assert!(record_physics_access("test.physics.outside").is_ok());

    let guard = enter_hot_path(HotPathPhase::SimTick);
    let calyx = record_calyx_access("test.calyx.sim_tick");
    let physics = record_physics_access("test.physics.sim_tick");
    drop(guard);

    assert!(
        is_boundary_violation(calyx, HotPathPhase::SimTick),
        "Calyx access inside a sim tick must be a SimTick boundary violation"
    );
    assert!(
        is_boundary_violation(physics, HotPathPhase::SimTick),
        "physics access inside a sim tick must be a SimTick boundary violation"
    );

    // Guard dropped: back outside the hot path, access is allowed again.
    assert!(record_calyx_access("test.calyx.after").is_ok());
}

#[test]
fn calyx_access_inside_render_frame_fails_loud() {
    // #1463: the engine render dispatch is now guarded as a RenderFrame hot path,
    // mirroring litd-game's runtime renderer. Prove the RenderFrame phase rejects
    // nondeterministic access on the rendering thread.
    let guard = enter_hot_path(HotPathPhase::RenderFrame);
    let result = record_calyx_access("test.calyx.render_frame");
    drop(guard);

    assert!(
        is_boundary_violation(result, HotPathPhase::RenderFrame),
        "Calyx access inside a render frame must be a RenderFrame boundary violation"
    );
}

#[test]
fn nested_render_inside_sim_tick_reports_innermost_phase() {
    // Guard stack is a stack: the innermost phase is the reported violator.
    let sim = enter_hot_path(HotPathPhase::SimTick);
    let render = enter_hot_path(HotPathPhase::RenderFrame);
    let inner = record_calyx_access("test.calyx.nested_inner");
    drop(render);
    let outer = record_calyx_access("test.calyx.nested_outer");
    drop(sim);

    assert!(is_boundary_violation(inner, HotPathPhase::RenderFrame));
    assert!(is_boundary_violation(outer, HotPathPhase::SimTick));
}

#[test]
fn worker_thread_access_is_outside_the_single_threaded_tick_contract() {
    // The single-threaded-tick contract: the tick runs entirely on the calling
    // thread. A worker thread's guard stack is empty, so its Calyx access is
    // NOT part of the tick and is correctly permitted. (A tick that fans out to
    // Calyx-touching workers would first break the documented contract; the
    // boundary is thread-local precisely because the tick is single-threaded.)
    let guard = enter_hot_path(HotPathPhase::SimTick);
    let worker = std::thread::spawn(|| record_calyx_access("test.calyx.worker"));
    let worker_result = worker.join().expect("worker thread joins");
    // Same-thread access during the same tick still fails loud.
    let owner_result = record_calyx_access("test.calyx.owner");
    drop(guard);

    assert!(
        worker_result.is_ok(),
        "worker-thread access is outside the tick's single-threaded boundary"
    );
    assert!(is_boundary_violation(owner_result, HotPathPhase::SimTick));
}

#[test]
fn violation_error_carries_the_stable_code() {
    let guard = enter_hot_path(HotPathPhase::SimTick);
    let result = record_calyx_access("test.calyx.code");
    drop(guard);
    match result {
        Err(error) => assert_eq!(error.code(), "LITD-DETERMINISM-BOUNDARY-VIOLATION"),
        Ok(()) => panic!("expected a determinism boundary violation"),
    }
}
