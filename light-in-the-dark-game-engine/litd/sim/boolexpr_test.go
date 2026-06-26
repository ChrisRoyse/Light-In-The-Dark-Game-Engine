package sim

// Full State Verification for the boolexpr condition tree (#457, ADR
// #451). SoT = the expr arena + eval result vs a hand-computed truth
// table. Happy path (all 8 combos of A AND (B OR NOT C)) + the four
// mandated edges, printing combo→expected→actual.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// varFn returns a condition handler whose truth is read from a pointer —
// lets a test flip A/B/C between evaluations to sweep the truth table.
func varFn(p *bool) TriggerHandler {
	return func(w *World, e Event) bool { return *p }
}

// TestBoolExprTruthTable — build (A AND (B OR NOT C)); feed all 8
// combos; assert each equals the hand-computed value.
func TestBoolExprTruthTable(t *testing.T) {
	w := NewWorld(Caps{})
	var A, B, C bool
	refA := w.RegisterHandlerID("v.A", varFn(&A))
	refB := w.RegisterHandlerID("v.B", varFn(&B))
	refC := w.RegisterHandlerID("v.C", varFn(&C))

	// A AND (B OR (NOT C))
	root := w.And(w.Cond(refA), w.Or(w.Cond(refB), w.Not(w.Cond(refC))))
	t.Logf("arena (%d nodes): %v", len(w.exprArena), w.exprArena)

	for combo := 0; combo < 8; combo++ {
		A = combo&4 != 0
		B = combo&2 != 0
		C = combo&1 != 0
		want := A && (B || !C)
		got := w.EvalExpr(root, Event{})
		t.Logf("A=%v B=%v C=%v -> expected=%v actual=%v", A, B, C, want, got)
		if got != want {
			t.Fatalf("combo %d: A=%v B=%v C=%v got %v want %v", combo, A, B, C, got, want)
		}
	}
}

// TestBoolExprVacuousEmpty — edge 1: a NoExpr root passes (vacuous AND).
func TestBoolExprVacuousEmpty(t *testing.T) {
	w := NewWorld(Caps{})
	got := w.EvalExpr(NoExpr, Event{})
	t.Logf("EvalExpr(NoExpr) = %v (expected true, vacuous AND)", got)
	if !got {
		t.Fatal("empty condition did not pass vacuously")
	}
}

// TestBoolExprDeepZeroAlloc — edge 2: a 10-level nested expr evaluates
// correctly and allocates nothing.
func TestBoolExprDeepZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	tru := w.RegisterHandlerID("v.true", trueFn)
	// Nest NOT/AND 10 deep: AND(true, AND(true, ... Cond(true))).
	e := w.Cond(tru)
	for i := 0; i < 10; i++ {
		e = w.And(w.Cond(tru), e)
	}
	if !w.EvalExpr(e, Event{}) {
		t.Fatal("deep all-true expr evaluated false")
	}
	n := testing.AllocsPerRun(1000, func() { _ = w.EvalExpr(e, Event{}) })
	t.Logf("10-level expr (%d nodes): result=true allocs/op=%v", len(w.exprArena), n)
	if n != 0 {
		t.Fatalf("deep eval allocates %v/op, want 0", n)
	}
}

// TestBoolExprImpureLeafFlagged — edge 3: an impure leaf (returns a
// different value on re-eval) is flagged loudly by the debug hook.
func TestBoolExprImpureLeafFlagged(t *testing.T) {
	w := NewWorld(Caps{})
	calls := 0
	impure := func(wr *World, e Event) bool { calls++; return calls%2 == 1 } // alternates
	ref := w.RegisterHandlerID("v.impure", impure)
	root := w.Cond(ref)

	flagged := ExprRef(-99)
	w.DebugExprImpure = func(r ExprRef) { flagged = r }
	got := w.EvalExpr(root, Event{})
	t.Logf("impure leaf eval: result=%v, DebugExprImpure fired for ref=%d (root=%d)", got, flagged, root)
	if flagged != root {
		t.Fatalf("impure leaf not flagged: flagged=%d want %d", flagged, root)
	}
}

// TestBoolExprSaveLoadEvalIdentity — edge 4: save the arena, load into a
// fresh world that re-registered the same conditions, and the same
// inputs evaluate identically; arena hash matches.
func TestBoolExprSaveLoadEvalIdentity(t *testing.T) {
	build := func(w *World, a, b, c *bool) ExprRef {
		refA := w.RegisterHandlerID("v.A", varFn(a))
		refB := w.RegisterHandlerID("v.B", varFn(b))
		refC := w.RegisterHandlerID("v.C", varFn(c))
		return w.And(w.Cond(refA), w.Or(w.Cond(refB), w.Not(w.Cond(refC))))
	}

	var sA, sB, sC bool
	src := NewWorld(Caps{})
	root := build(src, &sA, &sB, &sC)
	// bind the expr onto a trigger so the cond-ref round-trips too.
	tr, _ := src.Triggers.New()
	src.Triggers.SetCondition(tr, root)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	t.Logf("BEFORE: arena=%v boolexpr-sub=%016x", src.exprArena, subOf(reg, &before, "boolexpr"))

	var dA, dB, dC bool
	dst := NewWorld(Caps{})
	dRoot := build(dst, &dA, &dB, &dC)
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatalf("load failed: %v", err)
	}
	var after statehash.Snapshot
	dst.HashState(reg, &after)
	t.Logf("AFTER:  arena=%v boolexpr-sub=%016x", dst.exprArena, subOf(reg, &after, "boolexpr"))

	if before.Top != after.Top {
		t.Fatalf("hash diverged across save/load: %016x != %016x", before.Top, after.Top)
	}
	// identical eval on all 8 combos between src(before-save inputs) and dst.
	for combo := 0; combo < 8; combo++ {
		sA, sB, sC = combo&4 != 0, combo&2 != 0, combo&1 != 0
		dA, dB, dC = sA, sB, sC
		want := sA && (sB || !sC)
		got := dst.EvalExpr(dRoot, Event{})
		if got != want {
			t.Fatalf("combo %d after load: got %v want %v", combo, got, want)
		}
	}
	t.Logf("PASS: arena hash identical; all 8 combos evaluate identically after load")
}

// TestBoolExprUnknownRefFailsClosed — an unregistered condition handler
// (ref past the registry) evaluates false, never panics.
func TestBoolExprUnknownRefFailsClosed(t *testing.T) {
	w := NewWorld(Caps{})
	root := w.Cond(HandlerRef(999)) // never registered
	got := w.EvalExpr(root, Event{})
	t.Logf("Cond(unregistered ref 999) = %v (expected false, fail-closed)", got)
	if got {
		t.Fatal("unknown condition ref did not fail closed")
	}
}
