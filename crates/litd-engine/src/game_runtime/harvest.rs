//! Worker harvest loop: gather -> return -> deposit over editor-authored resource nodes
//! (#1785 a). Workers walk baked navmesh paths (pathing-aware) one cell at a time; movement is
//! an integer accumulator so a fractional cells/sec speed is still exactly reproducible.
//!
//! The worker owns only its own kinematic + phase state and emits a [`WorkerEvent`] at each
//! leg boundary; the world (which holds the navmesh, nodes, and accounts) reacts to the event.
//! This keeps resource mutation in one place and the worker deterministic and side-effect-free.

use litd_hash::Fnv1a64;

use crate::nav::NavCellCoord;

/// What a worker did this tick that the world must service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    /// Still walking or waiting; nothing for the world to do.
    None,
    /// Reached the assigned node cell: the world loads `yield_per_trip` and routes home.
    ArrivedAtNode,
    /// Reached the depot cell: deposit countdown has begun.
    ArrivedAtDepot,
    /// Deposit countdown elapsed: the world grants `carried` and routes back to the node.
    DepositComplete(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerPhase {
    ToNode,
    ToDepot,
    Deposit,
    Idle,
}

/// A harvesting worker. `carried` holds resource picked up at the node until a deposit lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worker {
    pub id: u32,
    pub cell: NavCellCoord,
    pub node_id: String,
    pub resource_key: String,
    pub carried: u64,
    pub phase: WorkerPhase,
    pub path: Vec<NavCellCoord>,
    pub path_index: usize,
    pub move_accum: u32,
    pub deposit_cooldown: u32,
}

impl Worker {
    pub fn new(id: u32, cell: NavCellCoord, node_id: String, resource_key: String) -> Self {
        Self {
            id,
            cell,
            node_id,
            resource_key,
            carried: 0,
            phase: WorkerPhase::Idle,
            path: Vec::new(),
            path_index: 0,
            move_accum: 0,
            deposit_cooldown: 0,
        }
    }

    /// Assign a fresh path leg (cell list starts at the worker's current cell).
    pub fn set_path(&mut self, path: Vec<NavCellCoord>, phase: WorkerPhase) {
        self.path = path;
        self.path_index = 0;
        self.move_accum = 0;
        self.phase = phase;
    }

    /// Advance one tick. `cells_per_sec` and `tick_rate` drive the integer movement accumulator
    /// (step one cell whenever the accumulator crosses a full second of travel).
    pub fn tick(&mut self, cells_per_sec: u32, tick_rate: u32, deposit_ticks: u32) -> WorkerEvent {
        match self.phase {
            WorkerPhase::ToNode => {
                if self.advance(cells_per_sec, tick_rate) {
                    WorkerEvent::ArrivedAtNode
                } else {
                    WorkerEvent::None
                }
            }
            WorkerPhase::ToDepot => {
                if self.advance(cells_per_sec, tick_rate) {
                    self.phase = WorkerPhase::Deposit;
                    self.deposit_cooldown = deposit_ticks;
                    WorkerEvent::ArrivedAtDepot
                } else {
                    WorkerEvent::None
                }
            }
            WorkerPhase::Deposit => {
                if self.deposit_cooldown > 0 {
                    self.deposit_cooldown -= 1;
                }
                if self.deposit_cooldown == 0 {
                    let amount = self.carried;
                    self.carried = 0;
                    WorkerEvent::DepositComplete(amount)
                } else {
                    WorkerEvent::None
                }
            }
            WorkerPhase::Idle => WorkerEvent::None,
        }
    }

    /// Step along the current path; returns true when the final cell is reached this tick.
    fn advance(&mut self, cells_per_sec: u32, tick_rate: u32) -> bool {
        if self.path.len() <= 1 {
            // Degenerate leg (already at target): arrive immediately.
            if let Some(last) = self.path.last() {
                self.cell = *last;
            }
            return true;
        }
        self.move_accum = self.move_accum.saturating_add(cells_per_sec.max(1));
        while self.move_accum >= tick_rate && self.path_index + 1 < self.path.len() {
            self.move_accum -= tick_rate;
            self.path_index += 1;
            self.cell = self.path[self.path_index];
        }
        self.path_index + 1 >= self.path.len()
    }

    pub fn hash_into(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.id));
        hasher.update_i64_le(i64::from(self.cell.x));
        hasher.update_i64_le(i64::from(self.cell.z));
        hasher.update(self.node_id.as_bytes());
        hasher.update_u64_le(self.carried);
        hasher.update_u64_le(self.phase as u64);
        hasher.update_u64_le(self.path_index as u64);
        hasher.update_u64_le(u64::from(self.deposit_cooldown));
    }
}
