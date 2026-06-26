package main

import (
	"os"
	"testing"
)

// aiFixture exercises common.ai-shaped input: natives interleaved with an
// AI-script function definition (must be excluded from the native count) and a
// `takes nothing returns boolean` zero-param decl.
const aiFixture = `//==== header ====
native DebugS takes string str returns nothing
native DoAiScriptDebug takes nothing returns boolean

globals
    integer foo = 1
endglobals

function main takes nothing returns nothing
    call DebugS("hi")
endfunction

native GetUnitCount takes integer unitid returns integer
`

func TestParseCommonAIFixture(t *testing.T) {
	res := ParseDeclsFull(aiFixture)
	c := Tally(res.Decls)

	if c.PlainNatives != 3 {
		t.Errorf("plain natives = %d, want 3", c.PlainNatives)
	}
	if c.ConstantNatives != 0 || c.Types != 0 {
		t.Errorf("unexpected constants/types: %+v", c)
	}

	// Edge 3: function `main` excluded from natives, exclusion recorded.
	if len(res.ExcludedFuncs) != 1 || res.ExcludedFuncs[0] != "main" {
		t.Errorf("excluded funcs = %v, want [main]", res.ExcludedFuncs)
	}

	byName := map[string]Decl{}
	for _, d := range res.Decls {
		byName[d.Name] = d
	}
	// Edge 2: zero-param `takes nothing returns boolean`.
	if d := byName["DoAiScriptDebug"]; len(d.Params) != 0 || d.Returns != "boolean" {
		t.Errorf("DoAiScriptDebug params=%v ret=%q, want 0 params / boolean", d.Params, d.Returns)
	}
	if got := byName["DoAiScriptDebug"].Signature(); got != "native DoAiScriptDebug takes nothing returns boolean" {
		t.Errorf("DoAiScriptDebug sig = %q", got)
	}
}

// TestCommonAIDistinctOrigin proves the merge-stage requirement (edge 1): a
// native of the same name parsed from two origins is retained as two distinct
// entries tagged by origin (the parser never dedupes — that is #4's merge job).
func TestCommonAIDistinctOrigin(t *testing.T) {
	const shared = "native SharedName takes nothing returns nothing\n"
	jDecls := ParseDecls(shared)
	aiDecls := ParseDecls(shared)
	for i := range jDecls {
		jDecls[i].Origin = "common"
	}
	for i := range aiDecls {
		aiDecls[i].Origin = "commonai"
	}
	merged := append(append([]Decl{}, jDecls...), aiDecls...)
	if len(merged) != 2 {
		t.Fatalf("merged len = %d, want 2 (both retained)", len(merged))
	}
	if merged[0].Origin != "common" || merged[1].Origin != "commonai" {
		t.Errorf("origins = %q,%q want common,commonai", merged[0].Origin, merged[1].Origin)
	}
}

func TestOriginForFile(t *testing.T) {
	cases := map[string]string{
		"repoes/war3-types/scripts/common.j":  "common",
		"x/blizzard.j":                        "blizzard",
		"repoes/war3-types/scripts/common.ai": "commonai",
	}
	for path, want := range cases {
		if got := OriginForFile(path); got != want {
			t.Errorf("OriginForFile(%q) = %q, want %q", path, got, want)
		}
	}
}

// TestParseRealCommonAI asserts exact counts vs the vendored source.
func TestParseRealCommonAI(t *testing.T) {
	const path = "../../repoes/war3-types/scripts/common.ai"
	src, err := os.ReadFile(path)
	if err != nil {
		t.Skipf("vendored common.ai not present (%v)", err)
	}
	res := ParseDeclsFull(string(src))
	c := Tally(res.Decls)
	if c.PlainNatives != 123 {
		t.Errorf("plain natives = %d, want 123", c.PlainNatives)
	}
	if c.TotalNatives() != 123 {
		t.Errorf("total natives = %d, want 123", c.TotalNatives())
	}
	if len(res.ExcludedFuncs) != 120 {
		t.Errorf("excluded funcs = %d, want 120", len(res.ExcludedFuncs))
	}

	byName := map[string]Decl{}
	for _, d := range res.Decls {
		byName[d.Name] = d
	}
	want := map[string]string{
		"DebugS":          "native DebugS takes string str returns nothing",
		"GetUnitCount":    "native GetUnitCount takes integer unitid returns integer",
		"GetGoldOwned":    "native GetGoldOwned takes nothing returns integer",
		"TownHasMine":     "native TownHasMine takes integer townid returns boolean",
		"DoAiScriptDebug": "native DoAiScriptDebug takes nothing returns boolean",
	}
	for name, sig := range want {
		d, ok := byName[name]
		if !ok {
			t.Errorf("missing %s", name)
			continue
		}
		if d.Signature() != sig {
			t.Errorf("%s sig = %q, want %q", name, d.Signature(), sig)
		}
	}
}
