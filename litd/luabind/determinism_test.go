package luabind

// #271 (G5.7) determinism gate — LOCAL portion. A Lua-scripted scenario
// (coroutine spawns, scheduler waits, ordered resumes, an OnEvent death handler,
// position math) runs 10,000 ticks headless and must produce a bit-identical
// 64-bit Game.StateHash every run. The golden hash is committed; any sim or
// bridge change that moves it requires a justified golden bump.
//
// SoT = the printed 64-bit state hash after 10k ticks, compared run-to-run and
// against the committed golden.
//
// Scope note: the FULL G5.7 gate also requires math.random routed to the sim
// PRNG (#263, blocked), LState pooling parity (#265), and the cross-OS/arch CI
// matrix (#284, CI billing blocked). Those surfaces are intentionally NOT
// exercised here yet; this locks the local, single-platform determinism of the
// coroutine/event/scheduler integration (#269) that exists today. The scenario
// avoids math.random for that reason.

import (
	"fmt"
	"runtime"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// goldenLuaDeterminism10k is the committed golden state hash for the scenario
// below after 10,000 ticks. Bump only with an explicit justification when a sim
// or bridge change legitimately alters simulation outcome (SimVersion discipline).
//
// Bumped 0xcb2b8f8681a2de23 → 0x1a07f91892d70515 (2026-06-20): the scenario now
// also exercises pairs() iteration order and number→string formatting (#271
// constraint), folding {k0..k5} order-sensitively and writing the result into
// u1's Y. That legitimately changes u1's final position, hence the hash. Not a
// regression — a coverage increase; run-to-run and GOMAXPROCS edges stay
// identical, the injected-divergence edge still has teeth.
//
// Bumped 0x1a07f91892d70515 → 0x2ca39aa855b38e89 (2026-06-20, #455): the ECA
// handler-identity registry adds a "handlers" system to HashSystems (ADR #451,
// R-SIM-6), so every World.HashState TopHash shifts by a constant — the registry
// is empty in this scenario, so its sub-hash is constant, but the system name is
// bound into the top hash regardless. Not a sim-outcome change: run-to-run and
// GOMAXPROCS edges stay identical (run1==run2==p1==p8), and the injected-
// divergence edge still has teeth.
//
// Bumped 0x2ca39aa855b38e89 → 0x9148c4e869dfed96 (2026-06-20, #456): the
// first-class ECA trigger slab adds a "triggers" system to HashSystems —
// another constant TopHash shift (empty slab here). run1==run2 unchanged.
//
// Bumped 0x9148c4e869dfed96 → 0xeeb7746e1f9808a2 (2026-06-20, #457): the
// boolexpr condition arena adds a "boolexpr" system to HashSystems — another
// constant TopHash shift (empty arena here). run1==run2 unchanged.
//
// Bumped 0xeeb7746e1f9808a2 → 0x7ea316e742921b02 (2026-06-20, #462): OnEvent
// is now sugar over a Trigger, so each Go-registered subscription in this
// scenario populates the (hashed) handler registry + trigger slab + boolexpr
// arena instead of the non-hashed legacy subs table. Behavior is identical
// (the api/luabind event-behavior suites stay green; dispatch order and fire
// counts unchanged); only the substrate carrying the subscription graph moved
// into the state hash. run1==run2==p1==p8 unchanged (deterministic).
// Updated for #555: adding the "timers" sub-hash to HashSystems shifts
// every state hash (still bit-deterministic; p1==p8 across GOMAXPROCS).
const goldenLuaDeterminism10k = uint64(0x1af1dc7850ac8003)

// runDeterminismScenario builds the fixed scenario, advances 10,000 ticks, and
// returns the resulting state hash. moveStep lets the divergence control change
// exactly one constant to prove the gate has teeth.
func runDeterminismScenario(t *testing.T, moveStep int) uint64 {
	t.Helper()
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
	const n = 6
	for i := 0; i < n; i++ {
		ud := L.NewUserData()
		ud.Value = g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: float64(i * 10), Y: 0}, api.Deg(0))
		L.SetGlobal(fmt.Sprintf("u%d", i), ud)
	}

	var src strings.Builder
	src.WriteString("deaths = 0\n")
	src.WriteString("OnEvent(1, function() deaths = deaths + 1 end)\n")
	// Each unit walks east in moveStep increments, 50 hops, 3 ticks apart.
	for i := 0; i < n; i++ {
		fmt.Fprintf(&src, "Run(function() for k=1,50 do local p = Unit_Position(u%d); Unit_SetPosition(u%d, {x = p.x + %d, y = p.y}); PolledWait(0.15) end end)\n", i, i, moveStep)
	}
	// One coroutine kills u0 at t=1s (20 ticks), firing exactly one death event.
	src.WriteString("Run(function() PolledWait(1.0); Unit_Kill(u0) end)\n")

	// Determinism surfaces pairs()-iteration order and number→string formatting
	// (both required by the #271 constraint and absent from the rest of the
	// scenario). A string-keyed table is folded ORDER-SENSITIVELY (acc = acc*31+v)
	// so the result depends on pairs() yielding a stable order — the fork guarantees
	// insertion order (verified in repoes/gopher-lua/table.go Next: string keys walk
	// the ordered skv/keys slices, never a Go map range). The fold is formatted via
	// string.format("%d"), parsed back, and written into u1's Y so both surfaces
	// enter the state hash; det_acc is asserted below as an observable cross-check.
	src.WriteString("det_acc = 0\ndet_u1y = -1\n")
	src.WriteString(`Run(function()
  local steps = {}
  for i = 0, 5 do steps["k" .. i] = i * 7 + 1 end
  local acc = 0
  for _, v in pairs(steps) do acc = acc * 31 + v end
  det_acc = acc
  local y = tonumber(string.format("%d", acc)) % 17
  PolledWait(0.5)
  local p = Unit_Position(u1)
  Unit_SetPosition(u1, {x = p.x, y = y})
  det_u1y = y
end)
`)
	if err := L.DoString(src.String()); err != nil {
		t.Fatalf("scenario script: %v", err)
	}

	g.Advance(10000)

	// Observable cross-check: exactly one death fired and was dispatched to Lua.
	if got := luaNum(t, L, "deaths"); got != 1 {
		t.Fatalf("scenario deaths=%v, want 1 (event dispatch nondeterministic?)", got)
	}
	// Observable cross-check on the pairs()/format surfaces: the order-sensitive
	// fold over {k0..k5}={1,8,15,22,29,36} in insertion order is
	// (((((1)*31+8)*31+15)*31+22)*31+29)*31+36 = 36486261. A different pairs() order
	// or a non-deterministic number format would yield a different acc — teeth.
	if got := luaNum(t, L, "det_acc"); got != 36486261 {
		t.Fatalf("pairs/format fold acc=%v, want 36486261 (pairs() order or number formatting nondeterministic?)", got)
	}
	if got := luaNum(t, L, "det_u1y"); got != 11 {
		t.Fatalf("formatted-result y=%v, want 11 (36486261 %% 17)", got)
	}
	return g.StateHash()
}

func TestLuaDeterminism10k(t *testing.T) {
	if testing.Short() {
		t.Skip("10k-tick determinism scenario is slow; run without -short")
	}
	h1 := runDeterminismScenario(t, 1)
	h2 := runDeterminismScenario(t, 1)
	t.Logf("10k determinism: run1=%#x run2=%#x golden=%#x", h1, h2, goldenLuaDeterminism10k)
	if h1 != h2 {
		t.Fatalf("run-to-run divergence: %#x != %#x", h1, h2)
	}
	if h1 != goldenLuaDeterminism10k {
		t.Fatalf("golden mismatch: got %#x, want %#x (intentional? bump golden with justification)", h1, goldenLuaDeterminism10k)
	}
}

func TestLuaDeterminism10kGOMAXPROCS(t *testing.T) {
	if testing.Short() {
		t.Skip("slow")
	}
	// GOMAXPROCS must not affect the result — one script runs at a time on the
	// sim domain regardless of how many OS threads Go may use.
	defer runtime.GOMAXPROCS(runtime.GOMAXPROCS(0))
	runtime.GOMAXPROCS(1)
	h1 := runDeterminismScenario(t, 1)
	runtime.GOMAXPROCS(8)
	h8 := runDeterminismScenario(t, 1)
	t.Logf("GOMAXPROCS edge: p1=%#x p8=%#x", h1, h8)
	if h1 != h8 || h1 != goldenLuaDeterminism10k {
		t.Fatalf("GOMAXPROCS affected the hash: p1=%#x p8=%#x golden=%#x", h1, h8, goldenLuaDeterminism10k)
	}
}

func TestLuaDeterminism10kInjectedDivergence(t *testing.T) {
	if testing.Short() {
		t.Skip("slow")
	}
	// Negative control: change exactly one scenario constant (move step 1 -> 2)
	// and the hash MUST diverge from the golden — proving the gate detects a real
	// simulation-outcome change, i.e. it is not a test that cannot fail.
	h := runDeterminismScenario(t, 2)
	t.Logf("injected divergence: variant=%#x golden=%#x", h, goldenLuaDeterminism10k)
	if h == goldenLuaDeterminism10k {
		t.Fatal("changing the move step did not change the hash — gate is blind")
	}
}
