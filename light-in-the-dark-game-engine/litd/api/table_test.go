package litd

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestTable — generic Table[V] comma-ok semantics. SoT: the (value, ok)
// pair read back after writes, including the never-written-key miss.
func TestTable(t *testing.T) {
	tab := NewTable[int]()

	// edge (1): Get on a never-written key -> zero + false.
	v, ok := tab.Get(1, 2)
	t.Logf("FSV miss: Get(1,2)=(%d,%v) want (0,false)", v, ok)
	if v != 0 || ok {
		t.Fatalf("miss = (%d,%v), want (0,false)", v, ok)
	}

	tab.Set(1, 2, 42)
	tab.Set(1, 3, 7)
	tab.Set(9, 9, 100)
	v, ok = tab.Get(1, 2)
	t.Logf("FSV hit: Get(1,2)=(%d,%v) ; len=%d", v, ok, tab.Len())
	if v != 42 || !ok {
		t.Fatalf("hit = (%d,%v), want (42,true)", v, ok)
	}
	if tab.Len() != 3 {
		t.Fatalf("len = %d, want 3", tab.Len())
	}

	// overwrite, has, remove, remove-parent.
	tab.Set(1, 2, 43)
	if v, _ := tab.Get(1, 2); v != 43 {
		t.Fatalf("overwrite failed: %d", v)
	}
	tab.Remove(1, 2)
	if tab.Has(1, 2) {
		t.Fatalf("Remove left key present")
	}
	tab.RemoveParent(1) // drops (1,3) too
	if tab.Has(1, 3) || !tab.Has(9, 9) {
		t.Fatalf("RemoveParent wrong: has(1,3)=%v has(9,9)=%v", tab.Has(1, 3), tab.Has(9, 9))
	}

	// a string-typed table proves the matrix-to-generic collapse.
	st := NewTable[string]()
	st.Set(0, 0, "synthetic")
	if s, ok := st.Get(0, 0); s != "synthetic" || !ok {
		t.Fatalf("string table = (%q,%v)", s, ok)
	}
}

// TestAttachRecycleSafeFSV — edge (2): attach to a unit, kill it and let
// the slot recycle; the stale handle reads zero+false and the new
// occupant of the slot is clean. SoT: Get through both handles.
func TestAttachRecycleSafeFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 4})
	g := newGame(w)
	att := NewAttachment[int](g)

	u1, id1 := unitAt(t, w, g, 0, 100, 100)
	att.Set(u1, 555)
	v, ok := att.Get(u1)
	t.Logf("FSV attach: Get(u1)=(%d,%v) id1=%#x", v, ok, uint32(id1))
	if v != 555 || !ok {
		t.Fatalf("attach failed: (%d,%v)", v, ok)
	}

	u1.Kill()
	stepN(w, 2) // phase-7 removal frees the slot

	// recycle the slot.
	u2, id2 := unitAt(t, w, g, 0, 200, 200)
	t.Logf("FSV recycle: id1=%#x id2=%#x sameSlot=%v u1.valid=%v u2.valid=%v",
		uint32(id1), uint32(id2), id1.Index() == id2.Index(), u1.Valid(), u2.Valid())

	// stale handle: zero + false (never the old 555).
	sv, sok := att.Get(u1)
	t.Logf("FSV stale Get(u1)=(%d,%v) want (0,false)", sv, sok)
	if sok || sv != 0 {
		t.Fatalf("stale attach handle leaked: (%d,%v)", sv, sok)
	}
	// recycled unit: clean (no attachment).
	nv, nok := att.Get(u2)
	t.Logf("FSV recycled Get(u2)=(%d,%v) want (0,false)", nv, nok)
	if nok || nv != 0 {
		t.Fatalf("recycled unit inherited stale attachment: (%d,%v)", nv, nok)
	}
	// and it can hold its own.
	att.Set(u2, 999)
	if v, ok := att.Get(u2); v != 999 || !ok {
		t.Fatalf("recycled unit attach failed: (%d,%v)", v, ok)
	}
	// Unit.ID distinguishes the two handles.
	if u1.ID() == u2.ID() {
		t.Fatalf("recycled handle ID collided with stale: %#x", u1.ID())
	}
	if u1.ID() != 0 {
		t.Fatalf("stale handle ID should be 0, got %#x", u1.ID())
	}
}

// TestSaveDataRoundTripFSV — edge (4): Storage survives a Save→Load cycle
// (a process restart simulated through a byte buffer). SoT: each typed
// value before and after.
func TestSaveDataRoundTripFSV(t *testing.T) {
	g := newGame(sim.NewWorld(sim.Caps{}))
	s := g.Storage()
	s.SetInt("campaign", "gold", 2500)
	s.SetReal("campaign", "difficulty", 1.5)
	s.SetString("hero", "name", "synthetic_hero")
	s.SetBool("flags", "intro_done", true)

	var buf bytes.Buffer
	if err := s.Save(&buf); err != nil {
		t.Fatalf("save: %v", err)
	}
	// fresh game = "process restart".
	g2 := newGame(sim.NewWorld(sim.Caps{}))
	s2 := g2.Storage()
	if err := s2.Load(bytes.NewReader(buf.Bytes())); err != nil {
		t.Fatalf("load: %v", err)
	}
	gold, gok := s2.GetInt("campaign", "gold")
	diff, dok := s2.GetReal("campaign", "difficulty")
	name, nok := s2.GetString("hero", "name")
	flag, fok := s2.GetBool("flags", "intro_done")
	t.Logf("FSV restored: gold=(%d,%v) diff=(%.1f,%v) name=(%q,%v) flag=(%v,%v)", gold, gok, diff, dok, name, nok, flag, fok)
	if gold != 2500 || !gok || diff != 1.5 || !dok || name != "synthetic_hero" || !nok || flag != true || !fok {
		t.Fatalf("round-trip lost data")
	}
	// miss after load.
	if _, ok := s2.GetInt("campaign", "nope"); ok {
		t.Fatalf("phantom key after load")
	}

	// edge (3): identical write sequences serialize byte-identically.
	mk := func() []byte {
		gg := newGame(sim.NewWorld(sim.Caps{}))
		ss := gg.Storage()
		ss.SetInt("a", "x", 1)
		ss.SetInt("a", "y", 2)
		ss.SetString("b", "z", "hi")
		var b bytes.Buffer
		if err := ss.Save(&b); err != nil {
			t.Fatalf("save: %v", err)
		}
		return b.Bytes()
	}
	b1, b2 := mk(), mk()
	t.Logf("FSV determinism: len=%d identical=%v", len(b1), bytes.Equal(b1, b2))
	if !bytes.Equal(b1, b2) {
		t.Fatalf("serialization not deterministic across runs")
	}

	// bad magic fails closed.
	if err := g.Storage().Load(bytes.NewReader([]byte("NOPEXXXX\x01"))); err == nil {
		t.Fatalf("Load accepted bad magic")
	}
}
