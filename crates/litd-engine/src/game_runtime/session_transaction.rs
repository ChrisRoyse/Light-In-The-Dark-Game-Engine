//! Transaction recovery for rejected top-level session orders.
//!
//! The committed order log plus validated immutable manifest are the #85 source
//! of truth. Dispatch stays allocation-free on success; only an error rebuilds
//! and verifies the last committed state before returning the original error.

use super::{GameRuntimeError, MatchRuntimeSession};

impl MatchRuntimeSession {
    pub(super) fn rollback_dispatch_failure(
        &mut self,
        original: GameRuntimeError,
        expected_hash: u64,
    ) -> GameRuntimeError {
        let original_diagnostic = original.to_string();
        match self.rebuild_committed_state(expected_hash) {
            Ok(rebuilt) => {
                *self = rebuilt;
                original
            }
            Err(rollback_diagnostic) => GameRuntimeError::Invalid {
                field: "session.rollback".to_string(),
                reason: format!(
                    "original dispatch error: {original_diagnostic}; \
                     rebuild/replay verification failed: {rollback_diagnostic}"
                ),
            },
        }
    }

    fn rebuild_committed_state(&self, expected_hash: u64) -> Result<Self, String> {
        let committed_orders = self.order_log.clone();
        let mut rebuilt = Self::from_manifest(&self.source_manifest)
            .map_err(|error| format!("manifest rebuild failed: {error}"))?;
        for (index, order) in committed_orders.iter().enumerate() {
            rebuilt
                .dispatch_nontransactional(order)
                .map_err(|error| format!("committed order {index} replay failed: {error}"))?;
            rebuilt.order_log.push(order.clone());
        }
        if rebuilt.order_log != committed_orders {
            return Err(format!(
                "order log mismatch: expected {} entries, rebuilt {}",
                committed_orders.len(),
                rebuilt.order_log.len()
            ));
        }
        let actual_hash = rebuilt.combined_state_hash();
        if actual_hash != expected_hash {
            return Err(format!(
                "combined hash mismatch: expected {expected_hash:016x}, rebuilt {actual_hash:016x}; \
                 committed orders={}",
                committed_orders.len()
            ));
        }
        Ok(rebuilt)
    }
}
