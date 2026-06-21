package sim

// Full State Verification for the ECA handler-identity registry (#455,
// ADR #451). SoT = the serialized registry bytes (a save round-trip)
// and the in-memory handlerReg.names/fns table, cross-checked against
// World.HashState(). Happy path + the four mandated edges, each
// printing system state before/after so the evidence is the bytes, not
// a return value.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// trueFn / falseFn are distinguishable named handlers — calling
// Resolve and invoking the returned func proves identity survived.
func trueFn(w *World, e Event) bool  { return true }
func falseFn(w *World, e Event) bool { return false }
func echoFn(w *World, e Event) bool  { return e.Arg != 0 }

// registerTriad registers the same three names in the same order on w
// and returns their refs. Re-running it on a fresh world is the
// "runtime swap re-registration" the save format depends on.
func registerTriad(w *World) (HandlerRef, HandlerRef, HandlerRef) {
	a := w.RegisterHandlerID("cond.alwaysTrue", trueFn)
	b := w.RegisterHandlerID("cond.alwaysFalse", falseFn)
	c := w.RegisterHandlerID("action.echoArg", echoFn)
	return a, b, c
}

// TestHandlerRegHappyPath — register 3 named handlers, then read the
// SoT (names/fns table) back and prove every ref resolves to the
// correct func and name.
func TestHandlerRegHappyPath(t *testing.T) {
	w := NewWorld(Caps{})
	a, b, c := registerTriad(w)

	// refs are 1-based in registration order.
	if a != 1 || b != 2 || c != 3 {
		t.Fatalf("refs not registration-ordered: got %d,%d,%d want 1,2,3", a, b, c)
	}
	t.Logf("name->ref table: %q=%d %q=%d %q=%d",
		w.HandlerNameOf(a), a, w.HandlerNameOf(b), b, w.HandlerNameOf(c), c)

	// NameOf / RefOf are inverse.
	for _, ref := range []HandlerRef{a, b, c} {
		name := w.HandlerNameOf(ref)
		if got, ok := w.HandlerRefOf(name); !ok || got != ref {
			t.Fatalf("RefOf(%q)=%d,%v want %d", name, got, ok, ref)
		}
	}

	// Resolve returns the actual func: invoke and check the known output.
	fnA, okA := w.ResolveHandlerRef(a)
	fnB, okB := w.ResolveHandlerRef(b)
	fnC, okC := w.ResolveHandlerRef(c)
	if !okA || !okB || !okC {
		t.Fatalf("resolve failed: %v %v %v", okA, okB, okC)
	}
	if fnA(w, Event{}) != true {
		t.Fatal("cond.alwaysTrue resolved to wrong func")
	}
	if fnB(w, Event{}) != false {
		t.Fatal("cond.alwaysFalse resolved to wrong func")
	}
	if fnC(w, Event{Arg: 7}) != true || fnC(w, Event{Arg: 0}) != false {
		t.Fatal("action.echoArg resolved to wrong func")
	}
	if w.HandlerCount() != 3 {
		t.Fatalf("HandlerCount=%d want 3", w.HandlerCount())
	}

	// Unknown refs fail closed.
	if _, ok := w.ResolveHandlerRef(NoHandler); ok {
		t.Fatal("NoHandler resolved")
	}
	if _, ok := w.ResolveHandlerRef(4); ok {
		t.Fatal("out-of-range ref resolved")
	}
	t.Logf("HandlerCount=%d; NoHandler and ref 4 both fail closed", w.HandlerCount())
}

// TestHandlerRegSaveLoadIdentity — the core serializability claim:
// register on src, save; a FRESH world re-registers the same names in
// the same order and loads successfully; refs and resolved funcs are
// identical, and HashState matches before save vs after load.
func TestHandlerRegSaveLoadIdentity(t *testing.T) {
	src := NewWorld(Caps{})
	registerTriad(src)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	t.Logf("BEFORE: src names=%v hash.handlers sub=%016x top=%016x save=%d bytes",
		src.handlerReg.names, subOf(reg, &before, "handlers"), before.Top, len(saved))

	dst := NewWorld(Caps{})
	registerTriad(dst) // fresh runtime re-registers the SAME identities
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatalf("load into re-registered world failed: %v", err)
	}

	var after statehash.Snapshot
	dst.HashState(reg, &after)
	t.Logf("AFTER:  dst names=%v hash.handlers sub=%016x top=%016x",
		dst.handlerReg.names, subOf(reg, &after, "handlers"), after.Top)

	if before.Top != after.Top {
		t.Fatalf("hash diverged across save/load: before=%016x after=%016x", before.Top, after.Top)
	}
	// ref(name) identical and Resolve still returns the working func.
	for i, name := range src.handlerReg.names {
		ref := HandlerRef(i + 1)
		if dst.HandlerNameOf(ref) != name {
			t.Fatalf("ref %d name %q != %q after load", ref, dst.HandlerNameOf(ref), name)
		}
	}
	fn, ok := dst.ResolveHandlerRef(3)
	if !ok || fn(dst, Event{Arg: 1}) != true {
		t.Fatal("resolved func wrong after load")
	}
	t.Logf("PASS: ref table + funcs identical across runtime swap; hash equal %016x", after.Top)
}

// TestHandlerRegTwoRuntimesIdenticalHash — edge 4: two independent
// runtimes registering in the same order produce an identical ref
// table AND identical hash (run twice).
func TestHandlerRegTwoRuntimesIdenticalHash(t *testing.T) {
	reg := NewHashRegistry()
	hashOf := func() (uint64, []string) {
		w := NewWorld(Caps{})
		registerTriad(w)
		var s statehash.Snapshot
		w.HashState(reg, &s)
		return s.Top, append([]string(nil), w.handlerReg.names...)
	}
	h1, n1 := hashOf()
	h2, n2 := hashOf()
	t.Logf("run1 names=%v top=%016x", n1, h1)
	t.Logf("run2 names=%v top=%016x", n2, h2)
	if h1 != h2 {
		t.Fatalf("two runtimes diverged: %016x != %016x", h1, h2)
	}

	// A DIFFERENT registration must change the hash (the registry is not
	// hash-invisible — divergence is caught here, not only at load).
	w3 := NewWorld(Caps{})
	w3.RegisterHandlerID("cond.alwaysTrue", trueFn)
	w3.RegisterHandlerID("cond.alwaysFalse", falseFn)
	w3.RegisterHandlerID("action.different", echoFn) // different 3rd name
	var s3 statehash.Snapshot
	w3.HashState(reg, &s3)
	t.Logf("run3 names=%v top=%016x", w3.handlerReg.names, s3.Top)
	if s3.Top == h1 {
		t.Fatal("divergent registration produced the same hash — registry not bound into state hash")
	}
}

// TestHandlerRegLoadUnknownRefFailsClosed — edge 1: the loaded world
// re-registered a DIFFERENT name set, so a stored ref would resolve to
// the wrong/no func. Load must fail closed, loudly.
func TestHandlerRegLoadUnknownRefFailsClosed(t *testing.T) {
	src := NewWorld(Caps{})
	registerTriad(src)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}

	// Case A: name mismatch at a ref.
	dstA := NewWorld(Caps{})
	dstA.RegisterHandlerID("cond.alwaysTrue", trueFn)
	dstA.RegisterHandlerID("cond.WRONG", falseFn) // diverges at ref 2
	dstA.RegisterHandlerID("action.echoArg", echoFn)
	t.Logf("BEFORE load: save names=%v dstA names=%v", src.handlerReg.names, dstA.handlerReg.names)
	errA := dstA.LoadState(bytes.NewReader(buf.Bytes()), 0)
	if errA == nil {
		t.Fatal("load with mismatched handler name SUCCEEDED — must fail closed")
	}
	t.Logf("AFTER load (name mismatch): fail-closed error = %v", errA)

	// Case B: fewer registrations — a stored ref 3 has no func.
	dstB := NewWorld(Caps{})
	dstB.RegisterHandlerID("cond.alwaysTrue", trueFn)
	errB := dstB.LoadState(bytes.NewReader(buf.Bytes()), 0)
	if errB == nil {
		t.Fatal("load with too-few handlers SUCCEEDED — must fail closed")
	}
	t.Logf("AFTER load (count mismatch): fail-closed error = %v", errB)
}

// TestHandlerRegDuplicateNamePanics — edge 2.
func TestHandlerRegDuplicateNamePanics(t *testing.T) {
	w := NewWorld(Caps{})
	w.RegisterHandlerID("dup", trueFn)
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("duplicate name did not panic")
		}
		t.Logf("duplicate name rejected fail-closed: %v", r)
	}()
	w.RegisterHandlerID("dup", falseFn) // must panic
}

// TestHandlerRegDuringStepPanics — edge 3: registration is setup-only.
func TestHandlerRegDuringStepPanics(t *testing.T) {
	w := NewWorld(Caps{})
	w.inStep = true // simulate being mid-Step (internal test access)
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("registration during Step did not panic")
		}
		t.Logf("registration during Step rejected fail-closed: %v", r)
	}()
	w.RegisterHandlerID("late", trueFn) // must panic
}

// TestHandlerRegResolveZeroAlloc — R-GC-1: the hot-path lookup is a
// single slice index, no map, no allocation.
func TestHandlerRegResolveZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	registerTriad(w)
	var sink TriggerHandler
	n := testing.AllocsPerRun(1000, func() {
		if f, ok := w.ResolveHandlerRef(2); ok {
			sink = f
		}
	})
	_ = sink
	if n != 0 {
		t.Fatalf("ResolveHandlerRef allocates %v/op, want 0", n)
	}
	t.Logf("ResolveHandlerRef: %v allocs/op", n)
}

// subOf returns the named system's sub-hash from a snapshot.
func subOf(reg *statehash.Registry, s *statehash.Snapshot, name string) uint64 {
	for i, n := range reg.Names() {
		if n == name {
			return s.Subs[i]
		}
	}
	return 0
}
