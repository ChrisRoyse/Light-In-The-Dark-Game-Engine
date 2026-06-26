package luabind

// Handle-return pooling (#407). The generated dispatch and the hand-written
// catalog/event marshalers push value handles through pushHandle, which caches
// the userdata per live handle so a per-tick re-marshal allocates ZERO after the
// first sight (R-GC-1). Root cause this fixes: the prior handleToLua took the
// handle as `any`, boxing it (1 alloc/op, measured) — pushHandle's typed param +
// typed cache remove that box. SoT = measured allocs/op + userdata identity.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestPushHandlePoolingFSV(t *testing.T) {
	g, u := confGame(t, 21)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}

	// Steady-state success metric: re-marshaling a cached handle is zero-alloc.
	_ = pushHandle(L, u) // warm the cache
	allocs := testing.AllocsPerRun(1000, func() { _ = pushHandle(L, u) })
	t.Logf("FSV #407 pushHandle steady-state allocs/op = %v (want 0; handleToLua any-box was 1)", allocs)
	if allocs != 0 {
		t.Fatalf("pushHandle still allocates %v/op — the any-box was not eliminated", allocs)
	}

	// Identity: the same live handle marshals to the SAME userdata.
	if pushHandle(L, u) != pushHandle(L, u) {
		t.Fatal("same handle produced distinct userdata")
	}
	// Distinct handles -> distinct userdata (no aliasing across the cache).
	u2 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 5, Y: 5}, api.Deg(0))
	if !u2.Valid() {
		t.Fatal("u2 invalid")
	}
	if pushHandle(L, u) == pushHandle(L, u2) {
		t.Fatal("distinct handles shared one userdata")
	}
	// Different handle TYPES are isolated by the per-type sub-cache (a Player and
	// a Unit that happen to share an id must not collide).
	if got, ok := pushHandle(L, g.Player(1)).Value.(api.Player); !ok || got != g.Player(1) {
		t.Fatal("Player marshal collided with or lost its value")
	}

	// No-scheduler fallback (g==nil path): a bare LState still marshals, no crash.
	L2 := lua.NewState()
	defer L2.Close()
	if got := pushHandle(L2, u); got == nil {
		t.Fatal("no-scheduler fallback returned nil")
	} else if v, ok := got.Value.(api.Unit); !ok || v != u {
		t.Fatalf("no-scheduler fallback lost the handle: %#v", got.Value)
	}
}
