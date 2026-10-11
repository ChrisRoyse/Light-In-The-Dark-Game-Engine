//! Deterministic hot-path boundary.
//!
//! # Single-threaded-tick contract (#1463)
//!
//! A deterministic sim tick is a **single-threaded, closed computation**. This
//! is not an implementation detail — it is required for determinism: fanning a
//! tick out to worker threads introduces thread-scheduling-dependent ordering
//! (reduction order, map iteration, RNG draw order), which breaks lockstep
//! replay across machines regardless of any I/O. See Gaffer On Games,
//! "Deterministic Lockstep". `FixedTickSim::step` and `AppRuntime::frame`
//! therefore run the whole tick on the calling thread.
//!
//! Because the tick is single-threaded, the hot-path guard stack is
//! `thread_local!`: every nondeterministic access (Calyx query, Rapier physics)
//! that is *part of the tick* runs on the tick thread and is caught by
//! [`record_calyx_access`] / [`record_physics_access`]. Work on other threads is
//! by definition **not** part of the tick and must not feed it.
//!
//! A process-wide "any Calyx access on any thread during a tick is a violation"
//! flag was considered and rejected: the editor and the test harness both run
//! legitimate, tick-independent Calyx work concurrently with sim ticks on other
//! threads, so a global flag produces false violations on correct code. The
//! forward-looking guard against a future tick that *does* fan out is that such
//! a tick would first have to break this documented contract; if the hot path is
//! ever parallelized, the spawned workers must inherit the phase explicitly
//! (a scoped `enter_hot_path` that propagates to child threads) rather than
//! relying on the global flag.

use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::EngineError;

pub const CALYX_DETERMINISM_BOUNDARY_VIOLATION: &str = "CALYX_DETERMINISM_BOUNDARY_VIOLATION";
pub const PHYSICS_DETERMINISM_BOUNDARY_VIOLATION: &str = "PHYSICS_DETERMINISM_BOUNDARY_VIOLATION";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotPathPhase {
    SimTick,
    RenderFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeterminismBoundarySnapshot {
    pub sim_tick_entries: u64,
    pub render_frame_entries: u64,
    pub calyx_access_total: u64,
    pub hot_path_calyx_access_total: u64,
    pub physics_access_total: u64,
    pub hot_path_physics_access_total: u64,
    pub current_hot_path_depth: usize,
    pub current_hot_path_phase: Option<HotPathPhase>,
}

pub struct HotPathGuard {
    phase: HotPathPhase,
    active: bool,
}

thread_local! {
    static HOT_PATH_STACK: RefCell<Vec<HotPathPhase>> = const { RefCell::new(Vec::new()) };
}

static SIM_TICK_ENTRIES: AtomicU64 = AtomicU64::new(0);
static RENDER_FRAME_ENTRIES: AtomicU64 = AtomicU64::new(0);
static CALYX_ACCESS_TOTAL: AtomicU64 = AtomicU64::new(0);
static HOT_PATH_CALYX_ACCESS_TOTAL: AtomicU64 = AtomicU64::new(0);
static PHYSICS_ACCESS_TOTAL: AtomicU64 = AtomicU64::new(0);
static HOT_PATH_PHYSICS_ACCESS_TOTAL: AtomicU64 = AtomicU64::new(0);

impl HotPathPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SimTick => "sim_tick",
            Self::RenderFrame => "render_frame",
        }
    }
}

pub fn enter_hot_path(phase: HotPathPhase) -> HotPathGuard {
    match phase {
        HotPathPhase::SimTick => {
            SIM_TICK_ENTRIES.fetch_add(1, Ordering::Relaxed);
        }
        HotPathPhase::RenderFrame => {
            RENDER_FRAME_ENTRIES.fetch_add(1, Ordering::Relaxed);
        }
    }
    HOT_PATH_STACK.with(|stack| stack.borrow_mut().push(phase));
    HotPathGuard {
        phase,
        active: true,
    }
}

pub fn record_calyx_access(operation: &'static str) -> Result<(), EngineError> {
    CALYX_ACCESS_TOTAL.fetch_add(1, Ordering::Relaxed);
    record_nondeterministic_access(
        operation,
        "Calyx",
        CALYX_DETERMINISM_BOUNDARY_VIOLATION,
        &HOT_PATH_CALYX_ACCESS_TOTAL,
    )
}

pub fn record_physics_access(operation: &'static str) -> Result<(), EngineError> {
    PHYSICS_ACCESS_TOTAL.fetch_add(1, Ordering::Relaxed);
    record_nondeterministic_access(
        operation,
        "Rapier physics",
        PHYSICS_DETERMINISM_BOUNDARY_VIOLATION,
        &HOT_PATH_PHYSICS_ACCESS_TOTAL,
    )
}

fn record_nondeterministic_access(
    operation: &'static str,
    source: &'static str,
    violation_code: &'static str,
    hot_path_counter: &AtomicU64,
) -> Result<(), EngineError> {
    if let Some(phase) = current_hot_path_phase() {
        hot_path_counter.fetch_add(1, Ordering::Relaxed);
        tracing::error!(
            code = violation_code,
            source,
            phase = phase.as_str(),
            operation,
            "Non-deterministic access attempted inside deterministic hot path"
        );
        return Err(EngineError::DeterminismBoundaryViolation {
            phase: phase.as_str().to_string(),
            operation: operation.to_string(),
        });
    }
    Ok(())
}

pub fn snapshot() -> DeterminismBoundarySnapshot {
    DeterminismBoundarySnapshot {
        sim_tick_entries: SIM_TICK_ENTRIES.load(Ordering::Relaxed),
        render_frame_entries: RENDER_FRAME_ENTRIES.load(Ordering::Relaxed),
        calyx_access_total: CALYX_ACCESS_TOTAL.load(Ordering::Relaxed),
        hot_path_calyx_access_total: HOT_PATH_CALYX_ACCESS_TOTAL.load(Ordering::Relaxed),
        physics_access_total: PHYSICS_ACCESS_TOTAL.load(Ordering::Relaxed),
        hot_path_physics_access_total: HOT_PATH_PHYSICS_ACCESS_TOTAL.load(Ordering::Relaxed),
        current_hot_path_depth: current_hot_path_depth(),
        current_hot_path_phase: current_hot_path_phase(),
    }
}

pub fn reset_for_fsv() {
    SIM_TICK_ENTRIES.store(0, Ordering::Relaxed);
    RENDER_FRAME_ENTRIES.store(0, Ordering::Relaxed);
    CALYX_ACCESS_TOTAL.store(0, Ordering::Relaxed);
    HOT_PATH_CALYX_ACCESS_TOTAL.store(0, Ordering::Relaxed);
    PHYSICS_ACCESS_TOTAL.store(0, Ordering::Relaxed);
    HOT_PATH_PHYSICS_ACCESS_TOTAL.store(0, Ordering::Relaxed);
    HOT_PATH_STACK.with(|stack| stack.borrow_mut().clear());
}

pub fn current_hot_path_phase() -> Option<HotPathPhase> {
    HOT_PATH_STACK.with(|stack| stack.borrow().last().copied())
}

pub fn current_hot_path_depth() -> usize {
    HOT_PATH_STACK.with(|stack| stack.borrow().len())
}

impl Drop for HotPathGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        HOT_PATH_STACK.with(|stack| {
            let popped = stack.borrow_mut().pop();
            debug_assert_eq!(popped, Some(self.phase));
        });
        self.active = false;
    }
}
