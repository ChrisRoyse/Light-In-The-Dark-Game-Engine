package litd

import "testing"

// #573 — public KV surface. SoT = values read back through the typed
// getters (which check the stored tag) across scopes.

func TestKVEntityTypedRoundTrip(t *testing.T) {
	w, g, _ := newDriverGame(t)
	u := grpUnit(t, w, g, 0)
	other := grpUnit(t, w, g, 0)

	u.SetInt("hp", 21+21)
	u.SetReal("ratio", 0.5)
	u.SetBool("boss", true)
	u.SetString("name", "warlord")
	u.SetUnit("target", other)
	u.SetPoint("home", Vec2{X: 12, Y: 34})

	if v, ok := u.GetInt("hp"); !ok || v != 42 {
		t.Fatalf("GetInt = %d,%v", v, ok)
	}
	if v, ok := u.GetReal("ratio"); !ok || v != 0.5 {
		t.Fatalf("GetReal = %v,%v", v, ok)
	}
	if v, ok := u.GetBool("boss"); !ok || !v {
		t.Fatalf("GetBool = %v,%v", v, ok)
	}
	if v, ok := u.GetString("name"); !ok || v != "warlord" {
		t.Fatalf("GetString = %q,%v", v, ok)
	}
	if v, ok := u.GetUnit("target"); !ok || v != other {
		t.Fatalf("GetUnit mismatch ok=%v", ok)
	}
	if v, ok := u.GetPoint("home"); !ok || v.X != 12 || v.Y != 34 {
		t.Fatalf("GetPoint = %v,%v", v, ok)
	}
}

func TestKVTypeMismatchAndAbsent(t *testing.T) {
	w, g, _ := newDriverGame(t)
	u := grpUnit(t, w, g, 0)
	u.SetInt("x", 5)
	// Reading the wrong type returns (zero,false), never reinterpreted bits.
	if _, ok := u.GetString("x"); ok {
		t.Fatal("GetString on an int key succeeded")
	}
	if _, ok := u.GetReal("x"); ok {
		t.Fatal("GetReal on an int key succeeded")
	}
	// Absent key: zero + false, and no key-table growth on read.
	if _, ok := u.GetInt("never"); ok {
		t.Fatal("absent key read ok")
	}
	if u.Has("never") {
		t.Fatal("Has on absent key true")
	}
}

func TestKVScopeIsolationAPI(t *testing.T) {
	w, g, _ := newDriverGame(t)
	u := grpUnit(t, w, g, 0)
	u.SetInt("score", 1)
	g.SetGlobalInt("score", 2)
	g.Player(1).SetInt("score", 3)
	if v, _ := u.GetInt("score"); v != 1 {
		t.Fatalf("entity score = %d", v)
	}
	if v, _ := g.GetGlobalInt("score"); v != 2 {
		t.Fatalf("global score = %d", v)
	}
	if v, _ := g.Player(1).GetInt("score"); v != 3 {
		t.Fatalf("player score = %d", v)
	}
}

func TestKVDeleteAndEach(t *testing.T) {
	w, g, _ := newDriverGame(t)
	u := grpUnit(t, w, g, 0)
	u.SetInt("a", 1)
	u.SetInt("b", 2)
	u.SetInt("c", 3)
	u.DeleteKey("b")
	if u.Has("b") {
		t.Fatal("key present after delete")
	}
	var keys []string
	u.EachKey(func(k string) { keys = append(keys, k) })
	if len(keys) != 2 {
		t.Fatalf("EachKey visited %v, want 2 keys", keys)
	}
}

func TestKVGroupRefRoundTrip(t *testing.T) {
	_, g, _ := newDriverGame(t)
	gr := g.NewGroup()
	u := g.NewGroup() // distinct handle as the holder owner needs a unit; use global instead
	_ = u
	g.SetGlobalInt("warm", 1) // ensure store initialized
	// store the group ref on a global key
	a := g.globalKV()
	a.SetGroupRef("camp", gr)
	if got, ok := a.GetGroupRef("camp"); !ok || got != gr {
		t.Fatalf("group ref round-trip ok=%v", ok)
	}
}
