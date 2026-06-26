package luabind

// #319 vector: "malicious serialized coroutine state (tampered save file → load
// must refuse with a structured error)". This became testable once #270 wired
// live suspended Lua coroutines into the save format (SaveScripts/LoadScripts);
// before that there was no live-coroutine blob to attack. Each tamper variant
// must produce a LOUD, structured LoadScripts error AND leave the scheduler
// table cleared — never a partial restore that smuggles a forged coroutine in.
//
// SoT = the LoadScripts error (quoted) + PendingScriptWaits after the failed
// load (must be 0: no coroutine parked from a rejected blob). Internal package so
// it reuses the save_test.go harness (newScriptGame / coScript / saveAt).

import (
	"bytes"
	"strings"
	"testing"
)

// loadTamperedScripts restores the matching sim blob into a fresh runtime, then
// feeds tampered into LoadScripts and returns the (expected) error + the parked
// count. registerChunk controls the "unresolvable proto" variant.
func loadTamperedScripts(t *testing.T, simBlob, tampered []byte, registerChunk bool) (error, int) {
	t.Helper()
	const fp = uint64(0xC0DEF00D)
	gB, LB, regB := newScriptGame(t)
	defer LB.Close()
	defer regB.Close()
	if registerChunk {
		if _, err := regB.Register("world", coScript); err != nil {
			t.Fatalf("re-register chunk: %v", err)
		}
	}
	if err := gB.LoadState(bytes.NewReader(simBlob), fp); err != nil {
		t.Fatalf("LoadState (the sim blob is untampered here): %v", err)
	}
	err := LoadScripts(LB, regB, bytes.NewReader(tampered))
	return err, PendingScriptWaits(LB)
}

func TestSandboxSaveLoadTamperRefusedFSV(t *testing.T) {
	// A valid baseline: one coroutine parked mid-PolledWait at tick 1.
	simBlob, validScr := saveAt(t, coScript, 1)
	if len(validScr) < 16 {
		t.Fatalf("baseline script blob implausibly small (%d B)", len(validScr))
	}
	t.Logf("FSV #319 baseline: valid script blob = %d B (1 parked coroutine)", len(validScr))

	// Sanity: the UNtampered blob loads cleanly and restores the 1 parked job —
	// so any failure below is attributable to the tamper, not a broken harness.
	if err, n := loadTamperedScripts(t, simBlob, validScr, true); err != nil || n != 1 {
		t.Fatalf("untampered control failed: err=%v parked=%d (want nil, 1)", err, n)
	}
	t.Logf("FSV #319 control: untampered blob loads → 1 coroutine parked")

	// Each tamper produces a fresh copy of the valid blob, corrupts it, and must
	// be refused loudly with the table left cleared (parked == 0).
	mid := len(validScr) / 2
	tampers := []struct {
		name        string
		mutate      func(b []byte) []byte
		registerCh  bool
		wantErrPart string
	}{
		{
			name:        "bad magic header",
			mutate:      func(b []byte) []byte { c := append([]byte{}, b...); copy(c[:8], "XXXXXXXX"); return c },
			registerCh:  true,
			wantErrPart: "bad magic",
		},
		{
			name:        "truncated mid-blob",
			mutate:      func(b []byte) []byte { return append([]byte{}, b[:mid]...) },
			registerCh:  true,
			wantErrPart: "LoadScripts",
		},
		{
			name:        "empty blob",
			mutate:      func(b []byte) []byte { return nil },
			registerCh:  true,
			wantErrPart: "LoadScripts",
		},
		{
			name: "bit-flipped coroutine image",
			// Flip a byte deep in the serialized coroutine body (past the header +
			// per-slot prefix) so the JSON image / thread restore rejects it.
			mutate: func(b []byte) []byte {
				c := append([]byte{}, b...)
				p := len(c) - 6 // inside the trailing coroutine blob / free list
				c[p] ^= 0xFF
				return c
			},
			registerCh:  true,
			wantErrPart: "LoadScripts",
		},
		{
			name:        "unresolvable proto (chunk not registered)",
			mutate:      func(b []byte) []byte { return append([]byte{}, b...) },
			registerCh:  false, // the coroutine's chunk is absent from the registry
			wantErrPart: "LoadScripts",
		},
	}

	for _, tc := range tampers {
		t.Run(tc.name, func(t *testing.T) {
			tampered := tc.mutate(validScr)
			err, parked := loadTamperedScripts(t, simBlob, tampered, tc.registerCh)
			if err == nil {
				t.Fatalf("TAMPER NOT REFUSED: %s — LoadScripts accepted a corrupted blob", tc.name)
			}
			if !strings.Contains(err.Error(), tc.wantErrPart) {
				t.Fatalf("%s: error %q does not contain %q (must be structured/located)", tc.name, err.Error(), tc.wantErrPart)
			}
			if parked != 0 {
				t.Fatalf("%s: %d coroutine(s) parked after a REFUSED load — partial restore smuggled state in", tc.name, parked)
			}
			t.Logf("REFUSED %-38s parked=0 -> %v", tc.name, oneLineErr(err))
		})
	}
}
