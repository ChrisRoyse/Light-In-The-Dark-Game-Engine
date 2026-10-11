//! #1787: the LAN wire protocol -- message types, length-prefixed framing, and the UDP
//! discovery beacon.
//!
//! TRANSPORT CHOICE (justified): the lobby + lockstep command channel runs over TCP. Lockstep
//! is only correct if EVERY command frame is delivered exactly once, in order -- a single
//! dropped or reordered frame is an instant desync. `net_authority.rs` already models exactly
//! this contract: it assigns monotonic `sequence` numbers and REJECTS any input whose sequence
//! does not strictly increase (`submit_input`), i.e. it assumes a reliable ordered channel and
//! does not reconstruct loss. TCP provides that channel natively; a UDP command path would have
//! to re-implement TCP's reliability/ordering to stay deterministic, buying nothing on a LAN.
//! Discovery is the one fire-and-forget case (a host beacon a joiner may miss and re-hear), so
//! THAT uses a UDP broadcast -- the classic split.

use std::io::{Read, Write};

use litd_engine::game_runtime::SessionOrder;
use serde::{Deserialize, Serialize};

use crate::lan_error::{LanError, codec_err, io_err, protocol_err};

/// Bumped whenever the wire format or lockstep contract changes; peers with different values
/// are refused typed BEFORE any match state is built.
pub const LAN_PROTOCOL_VERSION: u32 = 1;

/// The UDP port the optional discovery beacon broadcasts on.
pub const LAN_DISCOVERY_PORT: u16 = 47810;

/// Cap on a single framed message (guards against a hostile/garbled length prefix).
pub const MAX_FRAME_BYTES: u32 = 4 * 1024 * 1024;

/// The join handshake: proves the joiner carries the SAME packaged `.litdgame` AND the same
/// build (identical hashing code) as the host. Both fields are compared at the host; any
/// mismatch is a typed refusal (#1472 / same-package enforcement).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinHello {
    pub protocol_version: u32,
    /// The loaded package's `RuntimeDatasetsManifest::datasets_fingerprint` (package identity).
    pub datasets_fingerprint: String,
    /// The combined_state_hash of a FRESH session built from the manifest. Identical package +
    /// identical build => identical value; a divergent build (different hashing) is caught here
    /// even if the package bytes match.
    pub lockstep_hash: u64,
    pub player_name: String,
    pub requested_slot: u8,
    pub requested_faction: u8,
}

/// The host's verdict on a join. `Refused` carries the typed code as a string so the joiner
/// logs the exact reason it was turned away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinVerdict {
    Welcome {
        assigned_slot: u8,
        host_slot: u8,
        lobby: LobbySnapshot,
    },
    Refused {
        code: String,
        reason: String,
    },
}

/// A read-only snapshot of the lobby broadcast to every peer on each change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbySnapshot {
    pub slots: Vec<LobbySlot>,
    pub all_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbySlot {
    pub slot: u8,
    pub faction: u8,
    pub occupied: bool,
    pub is_host: bool,
    pub ready: bool,
    pub player_name: String,
}

/// The parameters the host locks in when all slots are ready; every peer runs the match with
/// the identical schedule so ticks advance on the lockstep clock, not on packet arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchStart {
    pub first_tick: u64,
    pub total_ticks: u64,
    pub input_delay_ticks: u64,
    pub grace_ms: u64,
    pub hash_report_interval: u64,
}

/// Every message on the established TCP channel (lobby + match). Serde-tagged so an unexpected
/// variant for the current phase is a typed protocol error, never a silent misread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LanMessage {
    /// Client -> host: mark this peer ready in the lobby.
    Ready,
    /// Host -> all: the lobby changed.
    Lobby(LobbySnapshot),
    /// Host -> all: all ready; begin the match with these locked parameters.
    Start(MatchStart),
    /// Client -> host: the client's orders for `execute_tick` (its controlled slot only).
    OrderSubmit {
        execute_tick: u64,
        orders: Vec<SessionOrder>,
    },
    /// Host -> all: the authoritative, sequenced command frame for `tick`. Both peers apply
    /// these commands in the given order, then advance one deterministic tick.
    Frame {
        tick: u64,
        commands: Vec<FrameCommand>,
    },
    /// Client -> host: the client's `combined_state_hash` after applying the frame for `tick`.
    HashReport { tick: u64, hash: u64 },
    /// Host -> client: you are kicked (divergence or stall). Carries the typed code.
    Kick { code: String, reason: String },
    /// Either -> either: an orderly end-of-match marker with the final hash for the record.
    Finished { tick: u64, final_hash: u64 },
}

/// One command inside an authoritative frame: which slot submitted it and the order itself.
/// Ordering within a frame is fixed by the host (sorted by slot) so both peers apply an
/// identical stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameCommand {
    pub slot: u8,
    pub order: SessionOrder,
}

/// The UDP discovery beacon a host may broadcast so joiners can find it without a typed IP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryBeacon {
    pub protocol_version: u32,
    pub share_id: String,
    pub tcp_port: u16,
    pub datasets_fingerprint: String,
}

impl DiscoveryBeacon {
    pub fn encode(&self) -> Result<Vec<u8>, LanError> {
        serde_json::to_vec(self).map_err(|error| codec_err("discovery.encode", error))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, LanError> {
        serde_json::from_slice(bytes).map_err(|error| codec_err("discovery.decode", error))
    }
}

/// Write one length-prefixed (u32-LE) JSON frame. Reliable+ordered because the caller holds a
/// TCP stream; the length prefix delimits messages within that byte stream.
pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, message: &T) -> Result<(), LanError> {
    let body = serde_json::to_vec(message).map_err(|error| codec_err("frame.encode", error))?;
    let len = u32::try_from(body.len())
        .map_err(|_| codec_err("frame.encode", "message exceeds u32 length"))?;
    if len > MAX_FRAME_BYTES {
        return Err(codec_err("frame.encode", "message exceeds MAX_FRAME_BYTES"));
    }
    writer
        .write_all(&len.to_le_bytes())
        .map_err(|error| io_err("frame.write_len", error))?;
    writer
        .write_all(&body)
        .map_err(|error| io_err("frame.write_body", error))?;
    writer
        .flush()
        .map_err(|error| io_err("frame.flush", error))?;
    Ok(())
}

/// Read one length-prefixed JSON frame. A read timeout (used for stall detection) surfaces as
/// an `Io` error the caller inspects via [`is_timeout`].
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(reader: &mut R) -> Result<T, LanError> {
    let mut len_bytes = [0u8; 4];
    reader
        .read_exact(&mut len_bytes)
        .map_err(|error| io_err("frame.read_len", error))?;
    let len = u32::from_le_bytes(len_bytes);
    if len > MAX_FRAME_BYTES {
        return Err(protocol_err(
            "frame",
            "declared length exceeds MAX_FRAME_BYTES",
        ));
    }
    let mut body = vec![0u8; len as usize];
    reader
        .read_exact(&mut body)
        .map_err(|error| io_err("frame.read_body", error))?;
    serde_json::from_slice(&body).map_err(|error| codec_err("frame.decode", error))
}

/// Whether an `Io` error is a socket read timeout (WouldBlock / TimedOut) -- the signal the
/// lockstep loop uses to poll for a stalled peer without conflating it with a hard failure.
pub fn is_timeout(error: &LanError) -> bool {
    matches!(
        error,
        LanError::Io { reason, .. }
            if reason.contains("timed out")
                || reason.contains("would block")
                || reason.contains("timedout")
                || reason.contains("os error 10060")
                || reason.contains("os error 10035")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrips_over_a_byte_buffer() {
        let message = LanMessage::HashReport {
            tick: 42,
            hash: 0xdead_beef,
        };
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &message).unwrap();
        let mut cursor = std::io::Cursor::new(buffer);
        let decoded: LanMessage = read_frame(&mut cursor).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn discovery_beacon_roundtrips() {
        let beacon = DiscoveryBeacon {
            protocol_version: LAN_PROTOCOL_VERSION,
            share_id: "demo".to_string(),
            tcp_port: 12345,
            datasets_fingerprint: "abc123".to_string(),
        };
        let bytes = beacon.encode().unwrap();
        assert_eq!(DiscoveryBeacon::decode(&bytes).unwrap(), beacon);
    }

    #[test]
    fn oversized_length_prefix_is_rejected_typed() {
        let mut framed = (MAX_FRAME_BYTES + 1).to_le_bytes().to_vec();
        framed.extend_from_slice(b"{}");
        let mut cursor = std::io::Cursor::new(framed);
        let error = read_frame::<_, LanMessage>(&mut cursor).unwrap_err();
        assert_eq!(error.code(), "LITD_LAN_PROTOCOL");
    }
}
