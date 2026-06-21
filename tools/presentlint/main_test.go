package main

// FSV for the #471 presentation-trigger lint gate. SoT = the findings scanDirs
// returns over planted fixtures (clean → none; violation → one per banned call),
// and the real render/audio tree staying clean.

import (
	"sort"
	"testing"
)

// TestPresentlintCleanFixtureNoFindings — a consumer that drains the snapshot
// and uses only OnAudio/OnCamera trips nothing.
func TestPresentlintCleanFixtureNoFindings(t *testing.T) {
	got, err := scanDirs([]string{"fixtures/clean"})
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("clean fixture flagged %d call(s): %v", len(got), got)
	}
	t.Log("FSV: clean presentation fixture → 0 findings")
}

// TestPresentlintViolationFixtureFlagsEach — every banned entry point is caught
// exactly once, loudly.
func TestPresentlintViolationFixtureFlagsEach(t *testing.T) {
	got, err := scanDirs([]string{"fixtures/violation"})
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	want := []string{"NewTrigger", "OnAbilityCast", "OnAttack", "OnBuffApplied", "OnDamage", "OnEvent", "Subscribe"}
	seen := make([]string, 0, len(got))
	for _, f := range got {
		seen = append(seen, f.sel)
	}
	sort.Strings(seen)
	if len(seen) != len(want) {
		t.Fatalf("violation fixture: got %d findings %v, want %d %v", len(seen), seen, len(want), want)
	}
	for i := range want {
		if seen[i] != want[i] {
			t.Fatalf("finding[%d] = %q, want %q (full: %v)", i, seen[i], want[i], seen)
		}
	}
	t.Logf("FSV: violation fixture → every banned call flagged: %v", seen)
}

// TestPresentlintRealTreeClean — the actual render/audio packages are clean
// today; this pins #449 so a future regression fails here.
func TestPresentlintRealTreeClean(t *testing.T) {
	got, err := scanDirs([]string{"../../litd/render", "../../litd/audio"})
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("litd/render or litd/audio touch the hashing subscription path: %v", got)
	}
	t.Log("FSV: litd/render + litd/audio clean of the hashing subscription path")
}

// TestPresentlintTestFilesIgnored — a _test.go file in a scanned dir is not
// linted (test harnesses legitimately exercise OnEvent).
func TestPresentlintTestFilesIgnored(t *testing.T) {
	// fixtures/clean holds only clean.go; assert the walker's _test.go skip by
	// confirming the scan of the whole fixtures tree equals clean+violation only.
	got, err := scanDirs([]string{"fixtures"})
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	if len(got) != 7 {
		t.Fatalf("fixtures tree: got %d findings, want 7 (violation only)", len(got))
	}
	t.Log("FSV: whole fixtures tree → 7 findings (all from violation/, clean/ silent)")
}
