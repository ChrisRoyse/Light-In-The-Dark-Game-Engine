package luabind

// The Dark escalating-spawns integration FSV (#172, dogfooding #267): loads
// worlds/the-dark — written purely against the bound surface (Game_Every,
// Game_RandomInt, Game_CreateUnit, Game_NeutralHostile, Storage) — and drives it
// headlessly under scripted beacon states. SoT = the wave log the world publishes
// to Storage + the spawned units' owner/positions via the Go api. Escalation must
// scale with dark-beacon count; spawns must land only at dark beacons; same seed
// must reproduce identical waves.

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func darkWorldGame(t *testing.T, seed int64, lit [3]int) *api.Game {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 1024, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "gloam_whelp", Life: 50, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 12},
		{ID: "gloam_stalker", Life: 120, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
		{ID: "gloam_horror", Life: 300, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 24},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	// Scripted beacon control (the input the real map's beacon mechanic publishes).
	for i, v := range lit {
		g.Storage().SetInt("beacon", "lit_"+string(rune('1'+i)), v)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "the-dark")); err != nil {
		t.Fatalf("LoadWorld(the-dark): %v", err)
	}
	return g
}

func darkInt(g *api.Game, key string) int { v, _ := g.Storage().GetInt("dark", key); return v }

// TestDarkSpawnNeverInLightFSV verifies #172's hard invariant geometrically: with
// a mix of lit and dark beacons, NO spawned Dark unit's REAL coordinate lies
// inside any lit beacon's safe radius. SoT = every neutral-hostile unit's actual
// position (not the world's own bookkeeping). The lit-radius re-roll guard in
// worlds/the-dark/main.lua is what makes this hold on any beacon spacing.
func TestDarkSpawnNeverInLightFSV(t *testing.T) {
	const lightRadius = 250.0
	centers := [3][2]float64{{500, 500}, {1500, 500}, {1000, 1500}}
	lit := [3]int{1, 0, 0} // beacon 1 lit; 2 & 3 dark

	g := darkWorldGame(t, 5, lit)
	g.Advance(240)

	nh := g.NeutralHostile().Slot()
	checked := 0
	for _, u := range g.AllUnits(nil) {
		if u.Owner().Slot() != nh {
			continue
		}
		p := u.Position()
		checked++
		for i, c := range centers {
			if lit[i] == 0 {
				continue
			}
			dx, dy := p.X-c[0], p.Y-c[1]
			if dx*dx+dy*dy <= lightRadius*lightRadius {
				t.Fatalf("Dark unit at (%.0f,%.0f) is INSIDE lit beacon %d (center %.0f,%.0f r=%.0f) — invariant violated",
					p.X, p.Y, i+1, c[0], c[1], lightRadius)
			}
		}
	}
	if checked == 0 {
		t.Fatal("no Dark units spawned from the 2 dark beacons — nothing to validate")
	}
	t.Logf("FSV #172 spawn-validity: %d Dark units checked, none inside the lit beacon's %.0f radius", checked, lightRadius)
}

func TestDarkEscalatingSpawnsFSV(t *testing.T) {
	// --- Edge 1: all beacons lit → zero pressure over a long span. ---
	gLit := darkWorldGame(t, 5, [3]int{1, 1, 1})
	gLit.Advance(150)
	if w, tot := darkInt(gLit, "waves"), darkInt(gLit, "total"); w != 0 || tot != 0 {
		t.Fatalf("all-lit: waves=%d total=%d, want 0/0 (no pressure)", w, tot)
	}
	t.Logf("FSV #172 all-lit: 0 waves / 0 spawns over 150 ticks (turtling unpunished only when map fully held)")

	// --- One dark beacon: tier 1, base interval, spawns only at that beacon. ---
	g1 := darkWorldGame(t, 5, [3]int{0, 1, 1}) // beacon 1 (500,500) dark; 2 & 3 lit
	g1.Advance(150)
	iv1, tier1 := darkInt(g1, "interval"), darkInt(g1, "tier")
	if iv1 != 60 || tier1 != 1 {
		t.Fatalf("1-dark: interval=%d tier=%d, want 60/1", iv1, tier1)
	}
	if darkInt(g1, "waves") == 0 || darkInt(g1, "total") == 0 {
		t.Fatalf("1-dark produced no waves/spawns: waves=%d total=%d", darkInt(g1, "waves"), darkInt(g1, "total"))
	}
	// Spawn-point validity: every wave spawned at the dark beacon (x≈500), never
	// at the lit beacons (x=1500 or x=1000). SoT = published last spawn location.
	if bx := darkInt(g1, "lastbeaconx"); bx != 500 {
		t.Fatalf("1-dark spawned at beacon x=%d, want 500 (the only dark beacon — never a lit one)", bx)
	}
	if lx := darkInt(g1, "lastx"); lx < 500-40 || lx > 500+40 {
		t.Fatalf("1-dark spawn x=%d outside jitter of dark beacon 500±40", lx)
	}
	// And actual units exist owned by the neutral-hostile Dark player near (500,500).
	near := g1.UnitsInRange(api.Vec2{X: 500, Y: 500}, 60, nil)
	if len(near) == 0 {
		t.Fatal("no Dark units spawned near the dark beacon")
	}
	if owner := near[0].Owner(); owner != g1.NeutralHostile() {
		t.Fatalf("spawned unit owner != NeutralHostile (got slot %d)", owner.Slot())
	}
	t.Logf("FSV #172 one-dark: interval=60 tier=1, %d waves / %d spawns, all at dark beacon x=500 owned by NeutralHostile", darkInt(g1, "waves"), darkInt(g1, "total"))

	// --- Edge 2 (escalation): all three dark → shorter interval AND higher tier. ---
	g3 := darkWorldGame(t, 5, [3]int{0, 0, 0})
	g3.Advance(150)
	iv3, tier3 := darkInt(g3, "interval"), darkInt(g3, "tier")
	if iv3 != 30 || tier3 != 3 { // 60 - 2*15 = 30
		t.Fatalf("3-dark: interval=%d tier=%d, want 30/3", iv3, tier3)
	}
	if !(iv3 < iv1 && tier3 > tier1) {
		t.Fatalf("escalation broken: 1-dark(iv=%d,tier=%d) vs 3-dark(iv=%d,tier=%d) — expect shorter+higher", iv1, tier1, iv3, tier3)
	}
	if darkInt(g3, "total") <= darkInt(g1, "total") {
		t.Fatalf("3-dark total spawns (%d) not greater than 1-dark (%d)", darkInt(g3, "total"), darkInt(g1, "total"))
	}
	t.Logf("FSV #172 escalation: 1-dark(iv=60,tier=1,%d spawns) → 3-dark(iv=30,tier=3,%d spawns)", darkInt(g1, "total"), darkInt(g3, "total"))

	// --- Edge 2b (de-escalation): recapture all beacons mid-run → pressure stops. ---
	wavesBefore := darkInt(g3, "waves")
	for i := 0; i < 3; i++ {
		g3.Storage().SetInt("beacon", "lit_"+string(rune('1'+i)), 1)
	}
	g3.Advance(150)
	if darkInt(g3, "waves") != wavesBefore {
		t.Fatalf("de-escalation: waves kept climbing after full recapture (%d → %d)", wavesBefore, darkInt(g3, "waves"))
	}
	if dc := darkInt(g3, "darkcount"); dc != 0 {
		t.Fatalf("de-escalation: darkcount=%d after recapture, want 0", dc)
	}
	t.Logf("FSV #172 de-escalation: recaptured all beacons → waves frozen at %d, darkcount=0 (map control rewarded)", wavesBefore)

	// --- Edge 3 (determinism): same seed → identical waves/spawns/hash. ---
	da := darkWorldGame(t, 9, [3]int{0, 0, 0})
	db := darkWorldGame(t, 9, [3]int{0, 0, 0})
	da.Advance(150)
	db.Advance(150)
	if darkInt(da, "total") != darkInt(db, "total") || darkInt(da, "lastx") != darkInt(db, "lastx") || da.StateHash() != db.StateHash() {
		t.Fatalf("non-deterministic: a(total=%d lastx=%d hash=%#x) b(total=%d lastx=%d hash=%#x)",
			darkInt(da, "total"), darkInt(da, "lastx"), da.StateHash(),
			darkInt(db, "total"), darkInt(db, "lastx"), db.StateHash())
	}
	// Different seed → different jitter (same envelope), proving PRNG genuinely feeds it.
	dc := darkWorldGame(t, 99, [3]int{0, 0, 0})
	dc.Advance(150)
	if dc.StateHash() == da.StateHash() {
		t.Fatal("different seed produced identical hash — spawn jitter not seed-dependent")
	}
	t.Logf("FSV #172 determinism: seed 9 runs identical (total=%d hash=%#x); seed 99 differs (PRNG-fed jitter)", darkInt(da, "total"), da.StateHash())
}

// TestDarkReDarkenStartsFreshIntervalFSV (#172 edge): losing a beacon after a
// period of peace must start a FRESH interval countdown — the Dark takes the full
// interval to manifest, not an instant wave. Bug: while all beacons were lit
// (n==0) the world returned without advancing `lastWave`, so it went stale; when
// a beacon was then lost, `t - lastWave` already exceeded the interval and a wave
// fired on the very first dark tick — and the first-wave timing wrongly depended
// on how long peace had lasted. SoT = the wave/spawn counters in Storage.
func TestDarkReDarkenStartsFreshIntervalFSV(t *testing.T) {
	g := darkWorldGame(t, 5, [3]int{1, 1, 1}) // all beacons lit
	g.Advance(500)                            // long peace — zero pressure
	if w := darkInt(g, "waves"); w != 0 {
		t.Fatalf("precondition: %d waves during all-lit peace, want 0", w)
	}

	// Lose beacon 1 → 1 dark beacon, interval 60 ticks.
	g.Storage().SetInt("beacon", "lit_1", 0)
	g.Advance(5) // 5 ticks << interval(60): the Dark must NOT have manifested yet
	if w, tot := darkInt(g, "waves"), darkInt(g, "total"); w != 0 || tot != 0 {
		t.Fatalf("INSTANT-PUNISH BUG: a wave fired %d ticks after losing a beacon (waves=%d total=%d) — stale lastWave from the peace made the first wave instant; the Dark must take the full interval to manifest", 5, w, tot)
	}

	// After the full interval the Dark legitimately manifests.
	g.Advance(60)
	if w := darkInt(g, "waves"); w == 0 {
		t.Fatalf("no wave after a full interval post-darkening: waves=%d (should manifest by now)", w)
	}
	t.Logf("FSV #172 re-darken: losing a beacon after peace starts a fresh interval — no instant punish, first wave ~60 ticks later")
}
