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
const goldenLuaDeterminism10k = uint64(0xcb2b8f8681a2de23)

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
	if err := L.DoString(src.String()); err != nil {
		t.Fatalf("scenario script: %v", err)
	}

	g.Advance(10000)

	// Observable cross-check: exactly one death fired and was dispatched to Lua.
	if got := luaNum(t, L, "deaths"); got != 1 {
		t.Fatalf("scenario deaths=%v, want 1 (event dispatch nondeterministic?)", got)
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
