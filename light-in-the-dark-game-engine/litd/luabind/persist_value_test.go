package luabind_test

// #264 persister, step 2: LValue data-graph serializer FSV. SoT = the contents
// and object-identity of the tables reconstructed into a FRESH LState, read
// back directly (RawGet / pointer comparison), proving sharing and cycles
// survive — not merely that Serialize returned bytes.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

// TestSerializeValueGraphRoundTrip — a graph with an array part, mixed scalar
// keys, a SHARED subtable, and a CYCLE round-trips into a fresh state with
// values intact and identity preserved.
func TestSerializeValueGraphRoundTrip(t *testing.T) {
	src := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer src.Close()

	root := src.NewTable()
	root.RawSetInt(1, lua.LNumber(10))
	root.RawSetInt(2, lua.LNumber(20))
	root.RawSetInt(3, lua.LNumber(30))
	root.RawSetString("x", lua.LString("hi"))
	root.RawSetString("flag", lua.LTrue)
	sub := src.NewTable()
	sub.RawSetString("a", lua.LNumber(1))
	root.RawSetString("nested", sub)
	root.RawSetString("alsoNested", sub) // SHARED — same object twice
	root.RawSetString("self", root)      // CYCLE

	blob, err := luabind.SerializeValue(root)
	if err != nil {
		t.Fatalf("serialize: %v", err)
	}
	t.Logf("FSV blob (%d bytes): %s", len(blob), string(blob))

	// Reconstruct in a DIFFERENT state — proving nothing leaks from src.
	dst := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer dst.Close()
	got, err := luabind.DeserializeValue(dst, blob)
	if err != nil {
		t.Fatalf("deserialize: %v", err)
	}
	d, ok := got.(*lua.LTable)
	if !ok {
		t.Fatalf("root decoded as %s, want table", got.Type())
	}

	// Scalar SoT.
	for i, want := range []float64{10, 20, 30} {
		if g := d.RawGetInt(i + 1); g != lua.LNumber(want) {
			t.Fatalf("root[%d] = %v, want %v", i+1, g, want)
		}
	}
	if g := d.RawGetString("x"); g != lua.LString("hi") {
		t.Fatalf("root.x = %v, want hi", g)
	}
	if g := d.RawGetString("flag"); g != lua.LTrue {
		t.Fatalf("root.flag = %v, want true", g)
	}
	t.Logf("FSV scalars: [1..3]=10,20,30 x=hi flag=true OK")

	// Sharing: nested and alsoNested must be the SAME reconstructed object.
	n1 := d.RawGetString("nested").(*lua.LTable)
	n2 := d.RawGetString("alsoNested").(*lua.LTable)
	if n1 != n2 {
		t.Fatalf("shared subtable duplicated on restore (nested != alsoNested)")
	}
	if a := n1.RawGetString("a"); a != lua.LNumber(1) {
		t.Fatalf("nested.a = %v, want 1", a)
	}
	t.Logf("FSV sharing: nested == alsoNested (same object), nested.a=1 OK")

	// Cycle: self must point back at the same root object.
	if self := d.RawGetString("self").(*lua.LTable); self != d {
		t.Fatalf("cycle not preserved: root.self != root")
	}
	t.Logf("FSV cycle: root.self == root OK")
}

// TestSerializeValueDeterministic — identical graphs produce byte-identical
// blobs (no map iteration in the wire form).
func TestSerializeValueDeterministic(t *testing.T) {
	build := func() lua.LValue {
		L := lua.NewState(lua.Options{SkipOpenLibs: true})
		defer L.Close()
		tb := L.NewTable()
		for i := 1; i <= 20; i++ {
			tb.RawSetInt(i, lua.LNumber(i*i))
		}
		tb.RawSetString("k", lua.LString("v"))
		return tb
	}
	b1, err1 := luabind.SerializeValue(build())
	b2, err2 := luabind.SerializeValue(build())
	if err1 != nil || err2 != nil {
		t.Fatalf("serialize: %v / %v", err1, err2)
	}
	t.Logf("FSV determinism: len=%d identical=%v", len(b1), bytes.Equal(b1, b2))
	if !bytes.Equal(b1, b2) {
		t.Fatalf("non-deterministic blob:\n %s\n %s", b1, b2)
	}
}

// TestSerializeValueMetatableRoundTrip — a table's metatable (itself a table)
// survives, including a metatable shared with the data graph.
func TestSerializeValueMetatableRoundTrip(t *testing.T) {
	src := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer src.Close()
	mt := src.NewTable()
	mt.RawSetString("kind", lua.LString("meta"))
	obj := src.NewTable()
	obj.RawSetString("v", lua.LNumber(7))
	obj.Metatable = mt

	blob, err := luabind.SerializeValue(obj)
	if err != nil {
		t.Fatalf("serialize: %v", err)
	}
	dst := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer dst.Close()
	got, err := luabind.DeserializeValue(dst, blob)
	if err != nil {
		t.Fatalf("deserialize: %v", err)
	}
	d := got.(*lua.LTable)
	gmt, ok := d.Metatable.(*lua.LTable)
	if !ok {
		t.Fatalf("metatable lost on restore: %v", d.Metatable)
	}
	t.Logf("FSV metatable: kind=%v v=%v", gmt.RawGetString("kind"), d.RawGetString("v"))
	if gmt.RawGetString("kind") != lua.LString("meta") {
		t.Fatalf("metatable contents lost")
	}
}

// TestSerializeValueRejectsNonData — edge: a function in the graph is a loud
// save-time error, never a silent drop (functions are a later step).
func TestSerializeValueRejectsNonData(t *testing.T) {
	src := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer src.Close()
	root := src.NewTable()
	root.RawSetString("fn", src.NewFunction(func(*lua.LState) int { return 0 }))
	_, err := luabind.SerializeValue(root)
	t.Logf("FSV function-in-graph -> err=%v", err)
	if err == nil {
		t.Fatalf("serializing a graph containing a function must fail loudly")
	}
}

// TestSerializeValueRejectsBadMetatable — edge: a non-table metatable cannot
// be serialized by the data layer; fail loud.
func TestSerializeValueRejectsBadMetatable(t *testing.T) {
	src := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer src.Close()
	obj := src.NewTable()
	obj.Metatable = src.NewFunction(func(*lua.LState) int { return 0 }) // not a table
	_, err := luabind.SerializeValue(obj)
	t.Logf("FSV non-table metatable -> err=%v", err)
	if err == nil {
		t.Fatalf("non-table metatable must fail loudly")
	}
}

// TestSerializeValueScalarRoot — a bare scalar (not a table) round-trips.
func TestSerializeValueScalarRoot(t *testing.T) {
	dst := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer dst.Close()
	for _, v := range []lua.LValue{lua.LNil, lua.LTrue, lua.LNumber(3.5), lua.LString("z")} {
		blob, err := luabind.SerializeValue(v)
		if err != nil {
			t.Fatalf("serialize %v: %v", v, err)
		}
		got, err := luabind.DeserializeValue(dst, blob)
		if err != nil {
			t.Fatalf("deserialize %v: %v", v, err)
		}
		if got.Type() != v.Type() || got.String() != v.String() {
			t.Fatalf("scalar round-trip: got %v(%s), want %v(%s)", got, got.Type(), v, v.Type())
		}
		t.Logf("FSV scalar root: %s(%s) OK", v.String(), v.Type())
	}
}
