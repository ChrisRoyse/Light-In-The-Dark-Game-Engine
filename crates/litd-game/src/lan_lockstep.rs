//! #1787: the deterministic, server-validated lockstep core.
//!
//! [`LockstepMatch`] wraps the composed [`MatchRuntimeSession`] and advances it ONE frame at a
//! time from an authoritative, sequenced command stream. A "frame" for tick `T` is the ordered
//! set of that tick's commands followed by exactly one [`SessionOrder::Tick`], so both peers --
//! applying byte-identical frames -- reproduce an identical `combined_state_hash`. This is the
//! determinism spine: the sim advances on the lockstep frame schedule, NEVER on packet arrival,
//! so network jitter cannot change the outcome (the #1783 / #85 house rule).
//!
//! [`FrameBuffer`] is the host-only sequencer: it collects each slot's submitted orders keyed by
//! execute-tick and emits the canonical, slot-sorted command list for a tick. [`hash_verdict`]
//! is the server-validated half: a client's reported hash for a tick is compared against the
//! host's authoritative hash and a divergence becomes a typed kick.

use std::collections::BTreeMap;

use litd_engine::game_runtime::{
    MatchRuntimeSession, RuntimeDatasetsManifest, SessionOrder, SessionReplay,
};

use crate::lan_error::LanError;
use crate::lan_protocol::FrameCommand;

/// One peer's deterministic view of the match, advanced frame-by-frame.
pub struct LockstepMatch {
    session: MatchRuntimeSession,
    hash_report_interval: u64,
    frames_applied: u64,
    final_hash: u64,
}

impl LockstepMatch {
    /// Build a peer's session from the SAME validated package manifest both sides loaded.
    pub fn new(
        manifest: &RuntimeDatasetsManifest,
        hash_report_interval: u64,
    ) -> Result<Self, LanError> {
        let interval = hash_report_interval.max(1);
        let session = MatchRuntimeSession::from_manifest(manifest)?;
        let final_hash = session.combined_state_hash();
        Ok(Self {
            session,
            hash_report_interval: interval,
            frames_applied: 0,
            final_hash,
        })
    }

    /// The lockstep hash of a fresh session -- the join handshake's build+package proof.
    pub fn lockstep_handshake_hash(manifest: &RuntimeDatasetsManifest) -> Result<u64, LanError> {
        Ok(MatchRuntimeSession::from_manifest(manifest)?.combined_state_hash())
    }

    pub fn current_hash(&self) -> u64 {
        self.final_hash
    }

    pub fn frames_applied(&self) -> u64 {
        self.frames_applied
    }

    /// Whether tick `T` is a hash-report boundary (host and client both key off this so the
    /// comparison cadence is identical on both ends).
    pub fn is_report_tick(&self, tick: u64) -> bool {
        tick.is_multiple_of(self.hash_report_interval)
    }

    /// Apply the authoritative frame for `tick`: its ordered commands, then one deterministic
    /// tick. Returns the post-frame `combined_state_hash`. Fails typed if frames are applied out
    /// of order (a protocol/sequence bug), never silently.
    pub fn apply_frame(&mut self, tick: u64, commands: &[FrameCommand]) -> Result<u64, LanError> {
        if tick != self.frames_applied {
            return Err(LanError::Protocol {
                phase: "lockstep",
                reason: format!(
                    "frame {tick} applied out of order (expected {})",
                    self.frames_applied
                ),
            });
        }
        for command in commands {
            self.session.apply(command.order.clone())?;
        }
        let report = self.session.apply(SessionOrder::Tick)?;
        self.frames_applied = self.frames_applied.saturating_add(1);
        self.final_hash = report.combined_state_hash;
        Ok(self.final_hash)
    }

    /// Inject a rogue LOCAL order that never entered the authoritative frame -- the deliberate
    /// divergence the negative control uses to prove the host's hash check kicks a desynced
    /// client. This is a REAL mutation of this peer's real session (no mock): its hash now
    /// disagrees with the host's for every subsequent report.
    pub fn inject_local_divergence(&mut self, order: SessionOrder) -> Result<(), LanError> {
        let report = self.session.apply(order)?;
        self.final_hash = report.combined_state_hash;
        Ok(())
    }

    /// Capture the applied order stream as a versioned replay (recorded on BOTH ends).
    /// Re-running it headlessly reproduces this peer's exact final hash (feeds #83).
    pub fn capture_replay(
        &self,
        manifest: &RuntimeDatasetsManifest,
    ) -> Result<SessionReplay, LanError> {
        Ok(SessionReplay::capture(manifest, &self.session)?)
    }
}

/// The host-only submission sequencer. Slots submit orders for a future execute-tick; the host
/// drains a tick into the canonical, slot-sorted command list every peer then applies.
#[derive(Debug, Default)]
pub struct FrameBuffer {
    pending: BTreeMap<u64, Vec<FrameCommand>>,
}

impl FrameBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `slot`'s orders for `execute_tick`. Submission order within a slot is preserved;
    /// cross-slot order is fixed later by the slot sort, so the frame is submission-timing
    /// independent (determinism does not depend on which peer's packet arrived first).
    pub fn submit(&mut self, execute_tick: u64, slot: u8, orders: Vec<SessionOrder>) {
        let bucket = self.pending.entry(execute_tick).or_default();
        for order in orders {
            bucket.push(FrameCommand { slot, order });
        }
    }

    /// Drain the canonical command list for `tick`: stable-sorted by slot so both peers receive
    /// and apply an identical stream.
    pub fn drain_frame(&mut self, tick: u64) -> Vec<FrameCommand> {
        let mut commands = self.pending.remove(&tick).unwrap_or_default();
        commands.sort_by_key(|command| command.slot);
        commands
    }
}

/// The server-validated lockstep check: compare a client's reported hash for `tick` against the
/// host's authoritative hash. `Ok(())` accepts; a mismatch is a typed [`LanError::DivergenceKick`].
pub fn hash_verdict(tick: u64, host_hash: u64, client_hash: u64) -> Result<(), LanError> {
    if host_hash == client_hash {
        Ok(())
    } else {
        Err(LanError::DivergenceKick {
            tick,
            expected: host_hash,
            actual: client_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_buffer_sorts_by_slot_regardless_of_submit_order() {
        let mut buffer = FrameBuffer::new();
        buffer.submit(3, 1, vec![SessionOrder::Tick]);
        buffer.submit(3, 0, vec![SessionOrder::SetTimeOfDay { tick_in_day: 0 }]);
        let frame = buffer.drain_frame(3);
        assert_eq!(frame.len(), 2);
        assert_eq!(frame[0].slot, 0);
        assert_eq!(frame[1].slot, 1);
        // Draining a second time yields nothing (the tick was consumed).
        assert!(buffer.drain_frame(3).is_empty());
    }

    #[test]
    fn hash_verdict_kicks_typed_on_divergence() {
        assert!(hash_verdict(7, 10, 10).is_ok());
        let error = hash_verdict(7, 10, 11).unwrap_err();
        assert_eq!(error.code(), "LITD_LAN_DIVERGENCE_KICK");
    }
}
