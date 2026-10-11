//! The deterministic order stream for the economy world (#1785). An [`EconomyOrder`] is a
//! single lockstep-safe operation; the FSV builds a `Vec<EconomyOrder>` and replays it twice
//! to prove the final `state_hash` is identical. Gameplay rejections (insufficient funds,
//! supply cap, unbuildable footprint) are captured as [`OrderOutcome::Rejected`] with the
//! stable engine error code, not as a fatal abort -- so replay reproduces the exact outcome
//! list including the rejections.

use super::structures::RallyPoint;
use super::world::EconomyWorld;

/// A single deterministic operation on the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomyOrder {
    PlaceStructure {
        struct_id: String,
        origin_x: i32,
        origin_z: i32,
    },
    CancelStructure {
        instance_id: u32,
    },
    TrainUnit {
        instance_id: u32,
        unit_id: String,
    },
    SetRallyPoint {
        instance_id: u32,
        rally: RallyPoint,
    },
    Grant {
        key: String,
        amount: u64,
    },
    Spend {
        key: String,
        amount: u64,
    },
    AssignWorker {
        worker_id: u32,
        node_id: String,
    },
    Tick,
}

/// Outcome of applying one order (recorded so a replay can be compared step-by-step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderOutcome {
    Placed { instance_id: u32 },
    Cancelled,
    Trained,
    RallySet,
    Granted(u64),
    Spent(u64),
    Assigned,
    Ticked,
    Rejected(String),
}

impl EconomyWorld {
    /// Apply one order from a stream, returning a comparable outcome.
    pub fn apply_order(&mut self, order: &EconomyOrder) -> OrderOutcome {
        match order {
            EconomyOrder::PlaceStructure {
                struct_id,
                origin_x,
                origin_z,
            } => match self.place_structure(struct_id, *origin_x, *origin_z) {
                Ok(instance_id) => OrderOutcome::Placed { instance_id },
                Err(e) => OrderOutcome::Rejected(e.code().to_string()),
            },
            EconomyOrder::CancelStructure { instance_id } => {
                match self.cancel_structure(*instance_id) {
                    Ok(()) => OrderOutcome::Cancelled,
                    Err(e) => OrderOutcome::Rejected(e.code().to_string()),
                }
            }
            EconomyOrder::TrainUnit {
                instance_id,
                unit_id,
            } => match self.train_unit(*instance_id, unit_id) {
                Ok(()) => OrderOutcome::Trained,
                Err(e) => OrderOutcome::Rejected(e.code().to_string()),
            },
            EconomyOrder::SetRallyPoint { instance_id, rally } => {
                match self.set_rally(*instance_id, *rally) {
                    Ok(()) => OrderOutcome::RallySet,
                    Err(e) => OrderOutcome::Rejected(e.code().to_string()),
                }
            }
            EconomyOrder::Grant { key, amount } => match self.grant(key, *amount) {
                Ok(b) => OrderOutcome::Granted(b),
                Err(e) => OrderOutcome::Rejected(e.code().to_string()),
            },
            EconomyOrder::Spend { key, amount } => match self.spend(key, *amount) {
                Ok(b) => OrderOutcome::Spent(b),
                Err(e) => OrderOutcome::Rejected(e.code().to_string()),
            },
            EconomyOrder::AssignWorker { worker_id, node_id } => {
                match self.reassign_worker(*worker_id, node_id) {
                    Ok(()) => OrderOutcome::Assigned,
                    Err(e) => OrderOutcome::Rejected(e.code().to_string()),
                }
            }
            EconomyOrder::Tick => match self.tick() {
                Ok(_) => OrderOutcome::Ticked,
                Err(e) => OrderOutcome::Rejected(e.code().to_string()),
            },
        }
    }
}
