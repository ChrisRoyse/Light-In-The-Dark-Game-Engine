package luabind

// #265 regression: steady-state per-tick Lua coroutine resume must be ZERO-alloc
// (R-GC-1). A suspended Lua coroutine waking every tick is the engine's hot Lua
// path; before this patch each resume cost 2 allocs (the Resume return slice +
// the PolledWait yield value's LNumber→interface box). Both are now eliminated —
// the Resume return slice is a reused per-host buffer (LITD-PATCH #265 in the
// gopher-lua fork) and PolledWait hands its wait seconds to the host through a
// scheduler field instead of the Lua value stack.
//
// SoT = testing.AllocsPerRun over g.Advance(1) at steady state (warmed past the
// one-time frame-stack / handle-cache growth). Fails before the patch (2/op),
// passes after (0/op). The companion determinism golden (determinism_test.go)
// proves the pooling is semantically invisible — same state hash, fewer allocs.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func TestScriptResumeZeroAllocFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 32, Seed: 1234})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	// A coroutine that suspends every tick forever: PolledWait(0.05) == one 20 tps
	// tick. This is the pure scheduler resume/yield machinery, no API table churn.
	if err := L.DoString(`Run(function() while true do PolledWait(0.05) end end)`); err != nil {
		t.Fatalf("scenario: %v", err)
	}
	// Warm past one-time growth (coroutine callframe stack, scheduler slices).
	g.Advance(256)
	if w := PendingScriptWaits(L); w != 1 {
		t.Fatalf("expected 1 parked coroutine after warmup, got %d (scenario not resuming each tick?)", w)
	}

	allocs := testing.AllocsPerRun(2000, func() { g.Advance(1) })
	t.Logf("FSV #265: steady-state allocs per tick (1 coroutine resuming every tick) = %v (want 0)", allocs)
	if allocs != 0 {
		t.Fatalf("per-tick coroutine resume allocates %v/op — R-GC-1 steady-state violated", allocs)
	}
}
