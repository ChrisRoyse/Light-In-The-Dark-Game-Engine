package luabind_test

// #266 census: the sandbox's global environment is a CLOSED whitelist. This
// test reads the live _G of a freshly built sandbox (the Source of Truth) and
// asserts it equals the committed whitelist exactly — any new global smuggled
// in by an upstream lib bump, or any expected global gone missing, fails here.

import (
	"reflect"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

func TestGlobalCensus(t *testing.T) {
	i := luabind.NewSandbox(luabind.SandboxOptions{InstructionBudget: 100000})
	defer i.Close()

	got := i.GlobalNames()
	want := luabind.SandboxGlobalWhitelist

	t.Logf("FSV census: live _G = %v", got)
	t.Logf("FSV census: whitelist = %v", want)

	// Set-diff both directions so the failure message names the offender.
	wantSet := map[string]bool{}
	for _, n := range want {
		wantSet[n] = true
	}
	gotSet := map[string]bool{}
	for _, n := range got {
		gotSet[n] = true
	}
	for _, n := range got {
		if !wantSet[n] {
			t.Errorf("UNEXPECTED global in sandbox _G: %q (not on whitelist — a capability leaked in)", n)
		}
	}
	for _, n := range want {
		if !gotSet[n] {
			t.Errorf("MISSING whitelisted global: %q (sandbox lost an expected capability)", n)
		}
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("census mismatch:\n got=%v\nwant=%v", got, want)
	}
}
