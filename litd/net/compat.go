package net

// compat.go: build-compatibility handshake (#85 edge 3; milestones build-compat).
//
// The deterministic-sim contract assumes every peer runs IDENTICAL code and data
// — two peers on different builds would desync the instant their sims diverge.
// The QUIC wire guard (quic.go protocolVersion) only proves the peers speak the
// same transport; it says nothing about whether their SIM would step identically.
// BuildFingerprint closes that gap: it identifies the determinism-relevant build
// (sim code, replay format, ruleset/data), and a joiner whose fingerprint differs
// from the host's is refused BEFORE any turn data flows — the same fail-closed
// point as the session token (auth.go). This is a pure policy primitive; wiring
// it into the live join handshake is the client-shell half, gated like the rest
// of the net runtime.
//
// litd/net stays sim-free (D-5): the caller (api/client) builds the fingerprint
// from sim.ReplayFormatVersion and the sim/data build hashes it owns and passes
// the struct in — this package only compares and (de)serializes it.

import (
	"encoding/binary"
	"fmt"
)

// BuildFingerprint identifies an engine build for netplay compatibility. Two
// peers may share a deterministic match only if their fingerprints are EQUAL.
// Fields are the things whose divergence would desync the sim:
//   - Protocol: the net handshake/turn-wire protocol revision.
//   - Replay:   the .litdreplay format version (sim/replay.go) — a peer that
//     records or verifies replays differently is incompatible.
//   - Sim:      a hash of the deterministic sim code + bundled gameplay data; any
//     change to stepping, RNG, or unit data must move this.
type BuildFingerprint struct {
	Protocol uint16
	Replay   uint16
	Sim      uint64
}

// buildFingerprintSize is the fixed wire size: 2 + 2 + 8 bytes, big-endian.
const buildFingerprintSize = 12

// Equal reports exact fingerprint equality.
func (f BuildFingerprint) Equal(o BuildFingerprint) bool { return f == o }

// CheckBuildCompat reports whether a joiner may share a match with the host.
// Returns nil when the fingerprints are equal, otherwise an error naming the
// FIRST field that differs (a clear refusal message — #85 edge 3). Fail-closed:
// any difference refuses; equality is the only path through.
func CheckBuildCompat(host, joiner BuildFingerprint) error {
	switch {
	case host.Protocol != joiner.Protocol:
		return fmt.Errorf("net: build mismatch: protocol host=%d joiner=%d", host.Protocol, joiner.Protocol)
	case host.Replay != joiner.Replay:
		return fmt.Errorf("net: build mismatch: replay-format host=%d joiner=%d", host.Replay, joiner.Replay)
	case host.Sim != joiner.Sim:
		return fmt.Errorf("net: build mismatch: sim-build host=%016x joiner=%016x", host.Sim, joiner.Sim)
	default:
		return nil
	}
}

// Encode serializes the fingerprint to its fixed 12-byte big-endian form for the
// join handshake.
func (f BuildFingerprint) Encode() []byte {
	b := make([]byte, buildFingerprintSize)
	binary.BigEndian.PutUint16(b[0:2], f.Protocol)
	binary.BigEndian.PutUint16(b[2:4], f.Replay)
	binary.BigEndian.PutUint64(b[4:12], f.Sim)
	return b
}

// DecodeBuildFingerprint parses a fingerprint from the handshake. It fails closed
// on any length other than the exact wire size — a truncated or padded peer
// fingerprint is refused, never silently zero-filled (a zero fingerprint would
// otherwise masquerade as a valid build).
func DecodeBuildFingerprint(b []byte) (BuildFingerprint, error) {
	if len(b) != buildFingerprintSize {
		return BuildFingerprint{}, fmt.Errorf("net: build fingerprint: got %d bytes, want %d", len(b), buildFingerprintSize)
	}
	return BuildFingerprint{
		Protocol: binary.BigEndian.Uint16(b[0:2]),
		Replay:   binary.BigEndian.Uint16(b[2:4]),
		Sim:      binary.BigEndian.Uint64(b[4:12]),
	}, nil
}
