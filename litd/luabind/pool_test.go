package luabind

// Cross-world LState pooling parity (#265, patch 4). pool.go's contract is that
// a recycled interpreter drives the sim BIT-IDENTICALLY to a never-pooled one:
// Pool.Put resets a used interpreter to the exact state NewSandbox produces, so
// whatever a prior world wrote (globals, reassigned builtins, registered
// handlers, parked coroutine threads) cannot leak into the next world's sim
// outcome. This test is the proof pool.go's doc comment promises.
//
// SoT = Game.StateHash() after a fixed scenario. The same scenario over a
// recycled interpreter must produce the same 64-bit hash as over a fresh one.
// The negative control makes the reset load-bearing rather than vacuous: an
// interpreter that is dirtied but NOT reset leaks a global the scenario reads,
// and that leak shifts the StateHash — proving (a) the leak is sim-relevant and
// (b) Put's reset is what prevents it.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// poolScenarioTicks keeps the scenario long enough to fold many sim steps into
// the hash but short enough to stay a fast unit test.
const poolScenarioTicks = 2000

// newPoolScenarioGame builds the fixed sim the pool parity scenario drives: one
// mover unit owned by player 1. Identical for the fresh and recycled paths so
// any hash divergence is attributable to interpreter state, not sim setup.
func newPoolScenarioGame(t *testing.T) *api.Game {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 8, Seed: 1234})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	return g
}

// poolScenarioSrc is the world-B script run on both the fresh and the recycled
// interpreter. It reads global `extra` with an `extra or 0` default: a fresh
// interpreter has no `extra` (→ 0), a properly reset one had it cleared (→ 0),
// but a dirty interpreter that leaked `extra` from a prior world would override
// it — moving the unit a different distance and so shifting the StateHash. That
// is what wires the leak to the SoT for the negative control.
const poolScenarioSrc = `
extra = extra or 0
Run(function()
  for k = 1, 50 do
    local p = Unit_Position(u0)
    Unit_SetPosition(u0, {x = p.x + 1 + extra, y = p.y})
    PolledWait(0.15)
  end
end)
`

// runPoolScenario registers g onto i, spawns u0, runs the scenario script, and
// advances poolScenarioTicks. Returns g.StateHash() — the SoT.
func runPoolScenario(t *testing.T, i *Interp, g *api.Game) uint64 {
	t.Helper()
	if err := Register(i.L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	ud := i.L.NewUserData()
	ud.Value = g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	i.L.SetGlobal("u0", ud)
	if err := i.L.DoString(poolScenarioSrc); err != nil {
		t.Fatalf("scenario script: %v", err)
	}
	g.Advance(poolScenarioTicks)
	return g.StateHash()
}

// dirtyInterpreter simulates a prior world's churn on i: it leaks a global the
// scenario reads, reassigns a builtin, registers a handler, and parks a
// coroutine thread mid-wait. A correct reset must orphan ALL of this.
func dirtyInterpreter(t *testing.T, i *Interp) {
	t.Helper()
	g := newPoolScenarioGame(t)
	if err := Register(i.L, g); err != nil {
		t.Fatalf("dirty Register: %v", err)
	}
	const junk = `
extra = 1000                       -- the sim-relevant leak
type = function() return "poisoned" end
OnEvent(1, function() leaked_handler = true end)
Run(function() PolledWait(99.0) end) -- parks a coroutine thread across the boundary
`
	if err := i.L.DoString(junk); err != nil {
		t.Fatalf("dirty script: %v", err)
	}
	// Take a couple of ticks so the parked coroutine is genuinely suspended.
	g.Advance(2)
}

// TestPoolRecycledHashParityFSV proves a recycled interpreter drives the sim to
// a BIT-IDENTICAL StateHash as a freshly built one, and that the reset is
// load-bearing (a leaked global, left un-reset, shifts the hash).
func TestPoolRecycledHashParityFSV(t *testing.T) {
	opts := SandboxOptions{InstructionBudget: 1_000_000}

	// --- Path 1: FRESH interpreter (the baseline / golden for this run). ---
	fresh := NewSandbox(opts)
	defer fresh.Close()
	freshHash := runPoolScenario(t, fresh, newPoolScenarioGame(t))
	t.Logf("FSV fresh:    StateHash = 0x%016x (extra defaulted to 0)", freshHash)

	// --- Path 2: RECYCLED interpreter via the pool. ---
	// Get a fresh one, dirty it as a prior world would, Put (which resets),
	// then Get it back — it must now be pristine and drive the sim identically.
	pool := NewSandboxPool(opts)
	defer pool.Close()
	dirty := pool.Get()
	dirtyInterpreter(t, dirty)
	pool.Put(dirty) // <-- resetSandbox runs here

	recycled := pool.Get()
	if recycled != dirty {
		t.Fatalf("pool did not recycle the interpreter (got a fresh build); pooling not exercised")
	}
	recycledHash := runPoolScenario(t, recycled, newPoolScenarioGame(t))
	t.Logf("FSV recycled: StateHash = 0x%016x (reset cleared leaked extra=1000)", recycledHash)

	if recycledHash != freshHash {
		t.Fatalf("recycled hash 0x%016x != fresh hash 0x%016x — pool reset leaks state into the next world's sim",
			recycledHash, freshHash)
	}

	// --- Negative control: prove the reset is load-bearing, not vacuous. ---
	// Dirty a fresh interpreter the SAME way but do NOT reset it; run the same
	// scenario. The leaked extra=1000 must move u0 farther and shift the hash.
	// If this hash equalled the fresh one, the "leak" would be sim-irrelevant
	// and the parity assertion above would prove nothing.
	leaky := NewSandbox(opts)
	defer leaky.Close()
	dirtyInterpreter(t, leaky) // sets extra=1000, no reset
	// dirtyInterpreter already Registered a throwaway game on leaky; the leaked
	// `extra` global persists across that into the next Register's _G (Register
	// does not clear globals — only resetSandbox does). Run the scenario on a
	// NEW game over the same un-reset interpreter:
	leakyHash := runPoolScenario(t, leaky, newPoolScenarioGame(t))
	t.Logf("FSV leaky:    StateHash = 0x%016x (un-reset extra=1000 leaked into sim)", leakyHash)

	if leakyHash == freshHash {
		t.Fatalf("negative control failed: un-reset leak did NOT change the hash (0x%016x) — "+
			"the leak is sim-irrelevant so the parity test has no teeth", leakyHash)
	}
	t.Logf("FSV PASS: recycled==fresh (0x%016x), leaky!=fresh (0x%016x != 0x%016x) — reset is load-bearing",
		recycledHash, leakyHash, freshHash)
}
