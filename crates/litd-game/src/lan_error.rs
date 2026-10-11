//! #1787: typed, fail-loud errors for the LAN match flow.
//!
//! Local to `litd-game` (a `thiserror` enum with stable `LITD_LAN_*` diagnostic codes) so
//! the frozen engine-wide `error.rs` baseline is untouched -- the same pattern
//! `game_runtime::error` uses. Every refusal / kick / protocol failure carries a stable,
//! run-invariant `code()` for the FSV report and logs; the LAN flow NEVER silently
//! continues past a mismatch, a divergence, or a stall it cannot recover from.

use litd_engine::EngineError;
use litd_engine::game_runtime::GameRuntimeError;

/// Every failure mode of the LAN lobby + lockstep match. Codes are the load-bearing typed
/// signal the negative controls and the divergence/refusal gates assert on.
#[derive(Debug, thiserror::Error)]
pub enum LanError {
    /// A joiner's package fingerprint or lockstep hash did not match the host's: the join is
    /// REFUSED (per #1472 divergence response) rather than booting a mismatched match.
    #[error(
        "LAN join refused: {field} mismatch (host {host}, joiner {joiner}) [LITD_LAN_PACKAGE_MISMATCH]"
    )]
    PackageMismatch {
        field: &'static str,
        host: String,
        joiner: String,
    },
    /// The wire protocol version differs between peers.
    #[error(
        "LAN protocol version mismatch: host {host}, joiner {joiner} [LITD_LAN_PROTOCOL_VERSION]"
    )]
    ProtocolVersion { host: u32, joiner: u32 },
    /// A client's periodic state-hash report diverged from the authoritative host: the
    /// client is KICKED typed (#85 server-validated lockstep).
    #[error(
        "LAN client kicked at tick {tick}: state hash diverged (host {expected:016x}, client {actual:016x}) [LITD_LAN_DIVERGENCE_KICK]"
    )]
    DivergenceKick {
        tick: u64,
        expected: u64,
        actual: u64,
    },
    /// A transient network stall exceeded the configured grace window: the peer is KICKED.
    #[error(
        "LAN peer stalled past the {grace_ms} ms grace window at tick {tick} [LITD_LAN_STALL_TIMEOUT]"
    )]
    StallTimeout { tick: u64, grace_ms: u64 },
    /// A protocol message was out of sequence, malformed, or unexpected for the phase.
    #[error("LAN protocol error ({phase}): {reason} [LITD_LAN_PROTOCOL]")]
    Protocol { phase: &'static str, reason: String },
    /// Lobby rule rejected a slot/faction/ready request.
    #[error("LAN lobby rejected {field}: {reason} [LITD_LAN_LOBBY]")]
    Lobby { field: &'static str, reason: String },
    /// Socket / framing i/o failed.
    #[error("LAN i/o failed ({context}): {reason} [LITD_LAN_IO]")]
    Io {
        context: &'static str,
        reason: String,
    },
    /// A serialized message could not be encoded/decoded.
    #[error("LAN codec failed ({context}): {reason} [LITD_LAN_CODEC]")]
    Codec {
        context: &'static str,
        reason: String,
    },
    /// The composed deterministic runtime session failed loud.
    #[error("LAN runtime error: {0}")]
    Runtime(#[from] GameRuntimeError),
    /// The engine (package load / config) failed loud.
    #[error("LAN engine error: {0}")]
    Engine(#[from] EngineError),
}

impl LanError {
    /// Stable, run-invariant diagnostic code (no absolute paths, no line info).
    pub fn code(&self) -> &'static str {
        match self {
            Self::PackageMismatch { .. } => "LITD_LAN_PACKAGE_MISMATCH",
            Self::ProtocolVersion { .. } => "LITD_LAN_PROTOCOL_VERSION",
            Self::DivergenceKick { .. } => "LITD_LAN_DIVERGENCE_KICK",
            Self::StallTimeout { .. } => "LITD_LAN_STALL_TIMEOUT",
            Self::Protocol { .. } => "LITD_LAN_PROTOCOL",
            Self::Lobby { .. } => "LITD_LAN_LOBBY",
            Self::Io { .. } => "LITD_LAN_IO",
            Self::Codec { .. } => "LITD_LAN_CODEC",
            Self::Runtime(_) => "LITD_LAN_RUNTIME",
            Self::Engine(_) => "LITD_LAN_ENGINE",
        }
    }
}

pub(crate) fn io_err(context: &'static str, reason: impl std::fmt::Display) -> LanError {
    LanError::Io {
        context,
        reason: reason.to_string(),
    }
}

pub(crate) fn codec_err(context: &'static str, reason: impl std::fmt::Display) -> LanError {
    LanError::Codec {
        context,
        reason: reason.to_string(),
    }
}

pub(crate) fn protocol_err(phase: &'static str, reason: impl Into<String>) -> LanError {
    LanError::Protocol {
        phase,
        reason: reason.into(),
    }
}
