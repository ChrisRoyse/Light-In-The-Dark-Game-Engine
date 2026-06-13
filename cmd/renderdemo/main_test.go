package main

import "testing"

func TestResolutionFlagSetFSV(t *testing.T) {
	var r resolutionFlag
	if err := r.Set("1920x1080"); err != nil {
		t.Fatalf("valid resolution rejected: %v", err)
	}
	t.Logf("FSV resolution valid BEFORE empty AFTER %+v", r)
	if r.W != 1920 || r.H != 1080 || !r.set {
		t.Fatalf("valid resolution parsed incorrectly: %+v", r)
	}

	before := r
	invalid := []string{
		"",
		"1920",
		"1920x",
		"x1080",
		"1920x1080extra",
		"1920x1080x1",
		"0x1080",
		"1920x-1",
		"1920X1080",
	}
	for _, input := range invalid {
		if err := r.Set(input); err == nil {
			t.Fatalf("invalid resolution %q accepted: %+v", input, r)
		}
		t.Logf("FSV resolution invalid input=%q BEFORE %+v AFTER %+v", input, before, r)
		if r != before {
			t.Fatalf("invalid resolution %q mutated state: got %+v want %+v", input, r, before)
		}
	}
}
