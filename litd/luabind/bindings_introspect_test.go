package luabind_test

// #392 FSV: Valid(h)/IsZero(h) from Lua must agree with the Go-side predicates
// (the SoT), across live / zero / stale handles, and fail closed on a payload
// with no such predicate. This is what makes modder-contract rule 2 followable
// from a script.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

func setUD(L *lua.LState, name string, v any) {
	ud := L.NewUserData()
	ud.Value = v
	L.SetGlobal(name, ud)
}

func luaBool(t *testing.T, L *lua.LState, expr string) bool {
	t.Helper()
	if err := L.DoString("return " + expr); err != nil {
		t.Fatalf("%s: %v", expr, err)
	}
	v := L.Get(-1)
	L.Pop(1)
	return lua.LVAsBool(v)
}

// TestValidIsZeroAgreeWithGoFSV: the Lua predicates equal the Go predicates for
// a live handle and a zero handle. SoT = the Go Valid()/IsZero() values.
func TestValidIsZeroAgreeWithGoFSV(t *testing.T) {
	g := catalogGame(t, 1)
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 50, Y: 50}, api.Deg(0))
	var zero api.Unit

	L := lua.NewState()
	defer L.Close()
	if err := luabind.Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	setUD(L, "u", u)
	setUD(L, "z", zero)

	// Live handle.
	if got, want := luaBool(t, L, "Valid(u)"), u.Valid(); got != want {
		t.Errorf("Valid(live) lua=%v go=%v", got, want)
	}
	if got, want := luaBool(t, L, "IsZero(u)"), u.IsZero(); got != want {
		t.Errorf("IsZero(live) lua=%v go=%v", got, want)
	}
	// Zero handle.
	if got, want := luaBool(t, L, "Valid(z)"), zero.Valid(); got != want {
		t.Errorf("Valid(zero) lua=%v go=%v", got, want)
	}
	if got, want := luaBool(t, L, "IsZero(z)"), zero.IsZero(); got != want {
		t.Errorf("IsZero(zero) lua=%v go=%v", got, want)
	}
	t.Logf("FSV live/zero: Valid(u)=%v IsZero(u)=%v | Valid(z)=%v IsZero(z)=%v",
		luaBool(t, L, "Valid(u)"), luaBool(t, L, "IsZero(u)"),
		luaBool(t, L, "Valid(z)"), luaBool(t, L, "IsZero(z)"))
	if luaBool(t, L, "Valid(z)") {
		t.Error("zero handle must not be Valid")
	}
	if !luaBool(t, L, "IsZero(z)") {
		t.Error("zero handle must be IsZero")
	}
}

// TestValidStaleHandleAcrossTicksFSV is the modder-contract rule 2 scenario: a
// handle held across ticks goes invalid when its unit dies, and the script can
// observe that via Valid() — agreeing with the Go SoT at each step.
func TestValidStaleHandleAcrossTicksFSV(t *testing.T) {
	g := catalogGame(t, 1)
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 50, Y: 50}, api.Deg(0))

	L := lua.NewState()
	defer L.Close()
	if err := luabind.Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	setUD(L, "u", u)

	before := luaBool(t, L, "Valid(u)")
	if before != u.Valid() || !before {
		t.Fatalf("pre-kill Valid lua=%v go=%v, want both true", before, u.Valid())
	}
	u.Kill()
	g.Advance(5) // let death + cleanup run across ticks
	after := luaBool(t, L, "Valid(u)")
	t.Logf("FSV rule-2: Valid(u) before=%v after kill+Advance(5)=%v (go=%v)", before, after, u.Valid())
	if after != u.Valid() {
		t.Fatalf("post-kill Valid lua=%v != go=%v (binding diverged from SoT)", after, u.Valid())
	}
}

// TestValidFailsClosedOnNonPredicate: Valid on a payload with no Valid()
// predicate (a UnitType id-ref) is a loud error, not a silent answer.
func TestValidFailsClosedOnNonPredicate(t *testing.T) {
	g := catalogGame(t, 1)
	L := lua.NewState()
	defer L.Close()
	if err := luabind.Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	setUD(L, "ut", g.UnitType("hfoo")) // UnitType has IsZero but no Valid

	err := L.DoString("return Valid(ut)")
	if err == nil {
		t.Fatal("Valid on a non-Valid payload must error (fail-closed), got nil")
	}
	t.Logf("FSV fail-closed: Valid(unittype) -> %v", err)
	// IsZero IS implemented on UnitType, so it must work.
	if err := L.DoString("return IsZero(ut)"); err != nil {
		t.Errorf("IsZero(unittype) should work (UnitType has IsZero): %v", err)
	}
}
