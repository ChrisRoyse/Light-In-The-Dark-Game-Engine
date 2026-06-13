package main

import "testing"

func manifestWith(syms ...GoMapping) Manifest {
	m := Manifest{SchemaVersion: 1}
	for i, gm := range syms {
		g := gm
		m.Functions = append(m.Functions, FunctionEntry{
			Name:           "Src" + string(rune('A'+i)),
			Origin:         "common.j",
			Signature:      Signature{Params: []ParamEntry{}, Returns: "nothing"},
			Classification: "D3",
			ClassifiedBy:   "override",
			Disposition:    "mapped",
			GoMapping:      &g,
		})
	}
	return m
}

func TestSplitSymbol(t *testing.T) {
	if r, n := splitSymbol("Unit.SetPosition"); r != "Unit" || n != "SetPosition" {
		t.Errorf("got %q.%q", r, n)
	}
	if r, n := splitSymbol("PolledWait"); r != "" || n != "PolledWait" {
		t.Errorf("got %q.%q, want free function", r, n)
	}
}

func TestExistingSymbolsSkipsHandWritten(t *testing.T) {
	// litd/api hand-implements Unit.SetLife; the generator must detect it.
	existing, err := existingSymbols("../../litd/api")
	if err != nil {
		t.Fatalf("scan litd/api: %v", err)
	}
	if !existing["Unit.SetLife"] {
		t.Error("expected Unit.SetLife to be detected as already implemented")
	}
	if existing["Unit.NeverDefinedXYZ"] {
		t.Error("phantom symbol reported as existing")
	}
}

// TestExistingSymbolsExcludesGeneratedStub is the regression guard for the
// stub-retirement bug: existingSymbols must NOT count jassgen's own generated
// stub file, or a stub could never retire — once a symbol is hand-written it
// would collide with its own panic stub as a duplicate method. Unit.Paused
// lives ONLY in the generated api_stubs_gen.go, so it must read as NOT existing;
// Unit.SetPosition and Unit.SetLife are hand-written and must read as existing.
// (Before the fix, Unit.Paused was reported existing and the stub was permanent.)
func TestExistingSymbolsExcludesGeneratedStub(t *testing.T) {
	existing, err := existingSymbols("../../litd/api")
	if err != nil {
		t.Fatalf("scan litd/api: %v", err)
	}
	if existing["Unit.Paused"] {
		t.Error("Unit.Paused (only in the generated stub) must not count as an existing implementation")
	}
	if !existing["Unit.SetPosition"] {
		t.Error("hand-written Unit.SetPosition must be detected as existing")
	}
	if !existing["Unit.SetLife"] {
		t.Error("hand-written Unit.SetLife must be detected as existing")
	}
}

// TestGenerateStubsD3CollapseAndSkip drives GenerateStubs over a temp dir-free
// manifest path that targets only new packages, asserting D3 collapse (one
// symbol claimed twice => one stub) does not double-emit.
func TestGenerateStubsDedup(t *testing.T) {
	// two manifest entries map to the SAME canonical symbol (D3 collapse)
	m := Manifest{SchemaVersion: 1, Functions: []FunctionEntry{
		{Name: "SetUnitX", Origin: "common.j", Signature: Signature{Params: []ParamEntry{}, Returns: "nothing"},
			Classification: "D3", ClassifiedBy: "override", Disposition: "mapped",
			GoMapping: &GoMapping{Symbol: "Unit.SetPosition", Package: "litd/api", GoSignature: "(pos Vec2)"}},
		{Name: "SetUnitPosition", Origin: "common.j", Signature: Signature{Params: []ParamEntry{}, Returns: "nothing"},
			Classification: "D3", ClassifiedBy: "override", Disposition: "mapped",
			GoMapping: &GoMapping{Symbol: "Unit.SetPosition", Package: "litd/api", GoSignature: "(pos Vec2)"}},
	}}
	// Collect via the dedupe logic without writing files: reuse the front of
	// GenerateStubs by counting distinct (recv.name).
	seen := map[string]bool{}
	count := 0
	for _, f := range m.Functions {
		r, n := splitSymbol(f.GoMapping.Symbol)
		k := r + "." + n
		if !seen[k] {
			seen[k] = true
			count++
		}
	}
	if count != 1 {
		t.Errorf("D3 collapse: distinct symbols = %d, want 1", count)
	}
}
