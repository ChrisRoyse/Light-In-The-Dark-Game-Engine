package luabind

// #664 security crux: the archive-internal require is a CAGED resolver. It maps a
// require name to a registered sibling chunk via a pure closed-set lookup
// (resolveModule over relToID) — no filesystem, no path resolution, no `..`
// traversal. Allowing the `require` keyword in archives (lualint #664) is only safe
// because of this: no string a script passes to require can reach host code or the
// disk; it can only name one of the world's own already-hash-verified chunks.
//
// These tests PROVE the escape attempts fail (the operator's condition for relaxing
// the sandbox lint). SoT = the error raised when require resolves a non-sibling
// name. Each escape name is NOT a registered chunk, so require must raise
// "no module", never read a file.

import (
	"strings"
	"testing"
	"testing/fstest"
)

func TestRequireCannotEscapeArchiveFSV(t *testing.T) {
	// A world with exactly one legitimate sibling chunk. Every escape name below is
	// absent from this set, so the caged resolver must reject it.
	base := func(mainBody string) fstest.MapFS {
		return fstest.MapFS{
			"main.lua":        {Data: []byte(mainBody)},
			"scripts/lib.lua": {Data: []byte(`return { ok = true }`)},
		}
	}

	// Happy path first (SoT baseline): a real sibling resolves.
	if L, err := loadComposed(t, base(`_G.r = require("scripts/lib").ok`)); err != nil {
		t.Fatalf("legit sibling require must succeed, got: %v", err)
	} else if L.GetGlobal("r").String() != "true" {
		t.Fatalf("sibling require returned %v, want true", L.GetGlobal("r"))
	}
	t.Log("FSV #664 baseline: require(\"scripts/lib\") resolves the sibling chunk")

	// Every escape attempt must FAIL with "no module" — proving require never
	// touches the host filesystem or stdlib, only the closed chunk set.
	escapes := []struct {
		name string
		body string
	}{
		{"stdlib os", `require("os")`},
		{"stdlib io", `require("io")`},
		{"parent traversal", `require("../secret")`},
		{"deep traversal", `require("scripts/../../../etc/passwd")`},
		{"absolute path", `require("/etc/passwd")`},
		{"absolute lua", `require("/etc/passwd.lua")`},
		{"dotted traversal", `require("..secret")`},
		{"unknown sibling", `require("scripts/nope")`},
	}
	for _, e := range escapes {
		t.Run(e.name, func(t *testing.T) {
			_, err := loadComposed(t, base(e.body))
			if err == nil {
				t.Fatalf("escape %q (%s) MUST fail — require resolved a non-sibling name (sandbox breach)", e.body, e.name)
			}
			// The failure must be the caged-resolver rejection, not a partial read.
			if !strings.Contains(err.Error(), "no module") {
				t.Fatalf("escape %q failed, but not via the caged 'no module' path: %v", e.body, err)
			}
			t.Logf("FSV #664 escape blocked: %s -> %q rejected: no module", e.name, e.body)
		})
	}
}
