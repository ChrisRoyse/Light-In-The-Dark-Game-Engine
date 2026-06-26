package main

import "testing"

// #618 — eventlint flags closure-nested custom-kind registration and
// passes top-level setup registration; a missing dir is a safe skip.

func TestEventlintCleanPasses(t *testing.T) {
	f, err := scanDirs([]string{"fixtures/clean"})
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	if len(f) != 0 {
		t.Fatalf("clean fixture flagged %d (want 0): %+v", len(f), f)
	}
}

func TestEventlintDirtyFlags(t *testing.T) {
	f, err := scanDirs([]string{"fixtures/dirty"})
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	if len(f) != 1 || f[0].sel != "RegisterEvent" {
		t.Fatalf("dirty fixture: got %+v, want 1 RegisterEvent finding", f)
	}
}

func TestEventlintMissingDirSkips(t *testing.T) {
	f, err := scanDirs([]string{"definitely-not-here"})
	if err != nil || len(f) != 0 {
		t.Fatalf("missing dir: f=%+v err=%v, want clean skip", f, err)
	}
}
