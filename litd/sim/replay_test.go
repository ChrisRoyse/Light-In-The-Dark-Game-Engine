package sim

// Replay format tests (#198). SoT = encoded bytes / decode errors.

import (
	"bytes"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func sampleReplay() *Replay {
	subs := func(seed uint64) []uint64 {
		s := make([]uint64, len(HashSystems))
		for i := range s {
			s[i] = seed + uint64(i)
		}
		return s
	}
	return &Replay{
		Version: ReplayFormatVersion, Fingerprint: 0, MapHash: 0,
		Seed: 42, Roster: 64, Interval: 100, Ticks: 300,
		Commands: []ReplayCommand{
			{Tick: 5, Player: 0, Kind: 0, Unit: 3, X: 100 << 32, Y: 200 << 32},
			{Tick: 9, Player: 1, Kind: 0, Unit: 7, X: -5 << 32, Y: 1},
		},
		Checkpoints: []ReplayCheckpoint{
			{Tick: 100, Top: 111, Subs: subs(1000)},
			{Tick: 200, Top: 222, Subs: subs(2000)},
			{Tick: 300, Top: 333, Subs: subs(3000)},
		},
	}
}

// Round-trip: encode → decode reproduces every field.
func TestReplayRoundTrip(t *testing.T) {
	r := sampleReplay()
	var buf bytes.Buffer
	if err := r.Encode(&buf); err != nil {
		t.Fatal(err)
	}
	got, err := DecodeReplay(bytes.NewReader(buf.Bytes()))
	if err != nil {
		t.Fatal(err)
	}
	if got.Seed != 42 || got.Roster != 64 || got.Interval != 100 || got.Ticks != 300 ||
		len(got.Commands) != 2 || len(got.Checkpoints) != 3 {
		t.Fatalf("round-trip mismatch: %+v", got)
	}
	if got.Commands[1] != r.Commands[1] {
		t.Fatalf("command mismatch: %+v vs %+v", got.Commands[1], r.Commands[1])
	}
	if got.Checkpoints[2].Top != 333 || got.Checkpoints[2].Subs[5] != 3005 {
		t.Fatalf("checkpoint mismatch: %+v", got.Checkpoints[2])
	}
	t.Logf("round-trip OK, %d bytes", buf.Len())
}

// Fail-closed decode: every malformation is a NAMED error, no panic.
func TestReplayDecodeFailClosed(t *testing.T) {
	r := sampleReplay()
	var buf bytes.Buffer
	if err := r.Encode(&buf); err != nil {
		t.Fatal(err)
	}
	full := buf.Bytes()

	cases := []struct {
		name    string
		mutate  func([]byte) []byte
		wantErr string
	}{
		{"bad magic", func(b []byte) []byte { c := append([]byte{}, b...); c[0] = 'X'; return c }, "bad magic"},
		{"wrong version", func(b []byte) []byte { c := append([]byte{}, b...); c[8] = 99; return c }, "format version 99"},
		{"truncated header", func(b []byte) []byte { return b[:20] }, "truncated"},
		{"truncated commands", func(b []byte) []byte { return b[:60] }, "truncated"},
		{"truncated trace", func(b []byte) []byte { return b[:len(b)-4] }, "truncated"},
		{"trailing garbage", func(b []byte) []byte { return append(append([]byte{}, b...), 0xAA) }, "trailing bytes"},
		{"empty", func(b []byte) []byte { return nil }, "truncated"},
	}
	for _, c := range cases {
		_, err := DecodeReplay(bytes.NewReader(c.mutate(full)))
		if err == nil || !strings.Contains(err.Error(), c.wantErr) {
			t.Errorf("%s: err = %v, want containing %q", c.name, err, c.wantErr)
		} else {
			t.Logf("%s: %v", c.name, err)
		}
	}

	// out-of-order command ticks (offset: magic 8 + header 40 + count 4 = 52; cmd 1 at 52+30)
	c := append([]byte{}, full...)
	c[52] = 0xFF // first command tick 5 → huge, second (9) now earlier
	if _, err := DecodeReplay(bytes.NewReader(c)); err == nil || !strings.Contains(err.Error(), "out of order") {
		t.Errorf("out-of-order: err = %v", err)
	} else {
		t.Logf("out-of-order: %v", err)
	}
}

// CompareCheckpoint names the culprit system of the FIRST divergent
// sub-hash.
func TestReplayCompareCheckpointCulprit(t *testing.T) {
	r := sampleReplay()
	cp := &r.Checkpoints[0]
	snap := &statehash.Snapshot{Top: cp.Top, Subs: append([]uint64{}, cp.Subs...)}
	if culprit, match := CompareCheckpoint(cp, snap); !match || culprit != "" {
		t.Fatalf("identical snapshot reported %q/%v", culprit, match)
	}
	snap.Top++
	snap.Subs[4]++ // HashSystems[4] = "health"
	culprit, match := CompareCheckpoint(cp, snap)
	if match || culprit != HashSystems[4] {
		t.Fatalf("culprit = %q (match=%v), want %q", culprit, match, HashSystems[4])
	}
	t.Logf("culprit correctly named: %s", culprit)
}
