package main

// FSV for the #557 save-unsafe-timer lint. SoT = the findings scanDirs returns
// over planted fixtures (clean → none; violation → one per banned call) and the
// graceful skip of a missing directory.

import (
	"sort"
	"testing"
)

func TestTimerlintCleanFixtureNoFindings(t *testing.T) {
	got, err := scanDirs([]string{"fixtures/clean"})
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("clean fixture flagged %d call(s): %v", len(got), got)
	}
	t.Log("FSV: clean ability fixture → 0 findings")
}

func TestTimerlintViolationFixtureFlagsEach(t *testing.T) {
	got, err := scanDirs([]string{"fixtures/violation"})
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	want := []string{"After", "Every"}
	seen := make([]string, 0, len(got))
	for _, f := range got {
		seen = append(seen, f.sel)
	}
	sort.Strings(seen)
	if len(seen) != len(want) {
		t.Fatalf("violation fixture: got %d %v, want %d %v", len(seen), seen, len(want), want)
	}
	for i := range want {
		if seen[i] != want[i] {
			t.Fatalf("finding[%d]=%q want %q (full %v)", i, seen[i], want[i], seen)
		}
	}
	t.Logf("FSV: violation fixture → every banned call flagged: %v", seen)
}

func TestTimerlintMissingDirSkipped(t *testing.T) {
	got, err := scanDirs([]string{"fixtures/does-not-exist"})
	if err != nil {
		t.Fatalf("missing dir should be skipped, got err: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("missing dir produced findings: %v", got)
	}
	t.Log("FSV: missing directory skipped cleanly (gate safe before templates land)")
}
