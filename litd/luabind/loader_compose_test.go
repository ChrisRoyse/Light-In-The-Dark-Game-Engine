package luabind

// #412 FSV: multi-chunk world composition via the host require shim. SoT = Lua
// globals/return values after LoadWorldFS over an in-memory world (fstest.MapFS).
// A sibling chunk beside main.lua is now reachable (was compiled-then-dead), runs
// at most once, cycles are caught, and unknown modules fail loudly.

import (
	"strings"
	"testing"
	"testing/fstest"

	lua "github.com/yuin/gopher-lua"
)

func loadComposed(t *testing.T, fsys fstest.MapFS) (*lua.LState, error) {
	t.Helper()
	L := lua.NewState()
	t.Cleanup(L.Close)
	reg := NewChunkRegistry()
	t.Cleanup(reg.Close)
	_, err := LoadWorldFS(L, reg, fsys, "compose-test")
	return L, err
}

// TestRequireComposesSiblingChunk — main.lua pulls in scripts/lib.lua, which was
// previously compiled then ignored.
func TestRequireComposesSiblingChunk(t *testing.T) {
	fsys := fstest.MapFS{
		"main.lua":        {Data: []byte(`local lib = require("scripts/lib"); _G.result = lib.add(2, 3)`)},
		"scripts/lib.lua": {Data: []byte(`local M = {}; function M.add(a,b) return a+b end; _G.libran = (_G.libran or 0) + 1; return M`)},
	}
	L, err := loadComposed(t, fsys)
	if err != nil {
		t.Fatalf("LoadWorldFS: %v", err)
	}
	if got := int(lua.LVAsNumber(L.GetGlobal("result"))); got != 5 {
		t.Fatalf("result = %d, want 5 (sibling chunk's function was not reachable)", got)
	}
	if ran := int(lua.LVAsNumber(L.GetGlobal("libran"))); ran != 1 {
		t.Fatalf("libran = %d, want 1", ran)
	}
	t.Log("FSV compose: main.lua require('scripts/lib') ran the sibling chunk; add(2,3)=5")
}

// TestRequireRunsChunkOnce — a module required from multiple places executes once.
func TestRequireRunsChunkOnce(t *testing.T) {
	fsys := fstest.MapFS{
		"main.lua":         {Data: []byte(`require("a"); local lib = require("scripts/lib"); require("scripts/lib"); _G.same = (require("scripts/lib") == lib)`)},
		"a.lua":            {Data: []byte(`require("scripts/lib"); return true`)},
		"scripts/lib.lua":  {Data: []byte(`_G.libran = (_G.libran or 0) + 1; return {}`)},
	}
	L, err := loadComposed(t, fsys)
	if err != nil {
		t.Fatalf("LoadWorldFS: %v", err)
	}
	if ran := int(lua.LVAsNumber(L.GetGlobal("libran"))); ran != 1 {
		t.Fatalf("libran = %d, want 1 (required from main twice + a.lua once → must run once)", ran)
	}
	if L.GetGlobal("same") != lua.LTrue {
		t.Fatal("repeated require returned a different value (cache miss)")
	}
	t.Log("FSV run-once: scripts/lib required 4× across 2 chunks → executed once, same cached value")
}

// TestRequireNameForms — exact rel, no-extension, and dotted forms all resolve to
// the same cached module.
func TestRequireNameForms(t *testing.T) {
	fsys := fstest.MapFS{
		"main.lua": {Data: []byte(`
			local a = require("scripts/lib.lua")
			local b = require("scripts/lib")
			local c = require("scripts.lib")
			_G.allsame = (a == b) and (b == c)
		`)},
		"scripts/lib.lua": {Data: []byte(`_G.libran = (_G.libran or 0) + 1; return {}`)},
	}
	L, err := loadComposed(t, fsys)
	if err != nil {
		t.Fatalf("LoadWorldFS: %v", err)
	}
	if L.GetGlobal("allsame") != lua.LTrue {
		t.Fatal("name forms resolved to different modules")
	}
	if ran := int(lua.LVAsNumber(L.GetGlobal("libran"))); ran != 1 {
		t.Fatalf("libran = %d, want 1 (3 name forms → one module)", ran)
	}
	t.Log("FSV name forms: scripts/lib.lua, scripts/lib, scripts.lib → one cached module")
}

// TestRequireCycleRefused — a → b → a is caught at load, loudly.
func TestRequireCycleRefused(t *testing.T) {
	fsys := fstest.MapFS{
		"main.lua": {Data: []byte(`require("a")`)},
		"a.lua":    {Data: []byte(`require("b"); return {}`)},
		"b.lua":    {Data: []byte(`require("a"); return {}`)},
	}
	_, err := loadComposed(t, fsys)
	if err == nil {
		t.Fatal("cyclic require was accepted")
	}
	if !strings.Contains(err.Error(), "cyclic") {
		t.Fatalf("error %q does not name the cycle", err)
	}
	t.Logf("FSV cycle: a→b→a refused at load: %v", err)
}

// TestRequireUnknownRefused — requiring a non-existent module fails loudly.
func TestRequireUnknownRefused(t *testing.T) {
	fsys := fstest.MapFS{
		"main.lua": {Data: []byte(`require("does/not/exist")`)},
	}
	_, err := loadComposed(t, fsys)
	if err == nil {
		t.Fatal("require of unknown module was accepted")
	}
	if !strings.Contains(err.Error(), "no module") {
		t.Fatalf("error %q does not explain the missing module", err)
	}
	t.Logf("FSV unknown: require('does/not/exist') refused: %v", err)
}
