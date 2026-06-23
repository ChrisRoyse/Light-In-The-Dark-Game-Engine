package net

// #85 edge-3 FSV: a build-mismatch join must be refused before any turn data,
// naming what differs. SoT = CheckBuildCompat's verdict + the wire round-trip.
// X+X=Y: encode a fingerprint, decode it, get the same struct; compare equal vs
// each single-field difference and read the refusal reason.

import (
	"strings"
	"testing"
)

func TestBuildCompatFSV(t *testing.T) {
	host := BuildFingerprint{Protocol: 1, Replay: 3, Sim: 0xDEADBEEFCAFEF00D}

	// Happy path: identical builds are compatible.
	if err := CheckBuildCompat(host, host); err != nil {
		t.Fatalf("identical builds refused: %v", err)
	}
	if !host.Equal(host) {
		t.Fatal("Equal(self) false")
	}

	// Each determinism-relevant field differing → refused, naming that field.
	cases := []struct {
		name   string
		joiner BuildFingerprint
		want   string
	}{
		{"protocol", BuildFingerprint{Protocol: 2, Replay: 3, Sim: host.Sim}, "protocol"},
		{"replay", BuildFingerprint{Protocol: 1, Replay: 4, Sim: host.Sim}, "replay-format"},
		{"sim", BuildFingerprint{Protocol: 1, Replay: 3, Sim: 0x0123456789ABCDEF}, "sim-build"},
	}
	for _, tc := range cases {
		err := CheckBuildCompat(host, tc.joiner)
		if err == nil {
			t.Fatalf("%s mismatch was NOT refused", tc.name)
		}
		if !strings.Contains(err.Error(), tc.want) {
			t.Fatalf("%s refusal = %q, want it to name %q", tc.name, err, tc.want)
		}
		t.Logf("FSV refuse(%s): %v", tc.name, err)
	}

	// Precedence: when several fields differ, the first (protocol) is named —
	// stable, deterministic message.
	allDiff := BuildFingerprint{Protocol: 9, Replay: 9, Sim: 9}
	if err := CheckBuildCompat(host, allDiff); err == nil || !strings.Contains(err.Error(), "protocol") {
		t.Fatalf("all-fields-differ should name protocol first, got %v", err)
	}
}

func TestBuildFingerprintWireFSV(t *testing.T) {
	f := BuildFingerprint{Protocol: 0x0102, Replay: 0x0304, Sim: 0x05060708090A0B0C}

	raw := f.Encode()
	if len(raw) != buildFingerprintSize {
		t.Fatalf("encoded %d bytes, want %d", len(raw), buildFingerprintSize)
	}
	got, err := DecodeBuildFingerprint(raw)
	if err != nil {
		t.Fatalf("decode round-trip: %v", err)
	}
	if got != f {
		t.Fatalf("round-trip mismatch: got %+v, want %+v", got, f)
	}
	t.Logf("FSV wire round-trip: %+v -> % x -> %+v", f, raw, got)

	// Fail-closed: wrong length is refused, never zero-filled.
	for _, bad := range [][]byte{nil, {}, raw[:buildFingerprintSize-1], append(raw, 0x00)} {
		if _, err := DecodeBuildFingerprint(bad); err == nil {
			t.Fatalf("decode accepted %d-byte buffer (want refusal)", len(bad))
		}
	}
	t.Log("FSV wire fail-closed: short/long/empty buffers all refused")
}
