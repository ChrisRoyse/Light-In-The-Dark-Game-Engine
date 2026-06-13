package litd

import (
	"math"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// TestGameCreateUnitFSV: Game.CreateUnit resolves a UnitType from a code, spawns
// a fully-owned, typed unit at the given pose, and returns its handle. SoT =
// the sim Transform/Owner/UnitType/Health stores.
func TestGameCreateUnitFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)

	typ := g.UnitType("hfoo")
	if typ.IsZero() {
		t.Fatal(`UnitType("hfoo") resolved to null`)
	}
	owner := Player{idx: 2, g: g} // player slot 2 (constructed directly; Game.Player lands with #218)

	u := g.CreateUnit(owner, typ, Vec2{X: 300, Y: 400}, Deg(90))
	if !u.Valid() {
		t.Fatal("CreateUnit returned an invalid unit")
	}
	// SoT reads straight from the sim stores.
	pos, _ := rawPos(w, u.id)
	tr := w.Transforms.Row(u.id)
	faceDeg := angleFromBrad(w.Transforms.Facing[tr]).Degrees()
	or := w.Owners.Row(u.id)
	ownerSlot := w.Owners.Player[or]
	ut := w.UnitTypes.Row(u.id)
	typeID := w.UnitTypes.TypeID[ut]
	life, _ := rawLife(w, u.id)
	t.Logf("CreateUnit: pos=%+v facing=%.1f° owner=%d typeID=%d life=%.0f", pos, faceDeg, ownerSlot, typeID, life)
	if pos != (Vec2{X: 300, Y: 400}) {
		t.Errorf("pos=%+v, want {300 400}", pos)
	}
	if math.Abs(faceDeg-90) > 0.01 {
		t.Errorf("facing=%.4f°, want 90", faceDeg)
	}
	if ownerSlot != 2 {
		t.Errorf("owner slot=%d, want 2", ownerSlot)
	}
	if typeID != 0 {
		t.Errorf("typeID=%d, want 0 (hfoo is def index 0)", typeID)
	}
	if life != 100 {
		t.Errorf("life=%.0f, want 100", life)
	}

	// EDGE: unknown code -> null UnitType -> CreateUnit spawns nothing.
	before := w.UnitCount()
	if z := g.UnitType("zzzz"); !z.IsZero() {
		t.Error(`UnitType("zzzz") should be null`)
	}
	if bad := g.CreateUnit(owner, g.UnitType("zzzz"), Vec2{X: 1, Y: 1}, Deg(0)); bad.Valid() {
		t.Error("CreateUnit with a null UnitType returned a valid unit")
	}
	// EDGE: foreign/zero owner -> no spawn.
	if bad := g.CreateUnit(Player{}, typ, Vec2{X: 1, Y: 1}, Deg(0)); bad.Valid() {
		t.Error("CreateUnit with a foreign owner returned a valid unit")
	}
	if after := w.UnitCount(); after != before {
		t.Errorf("rejected CreateUnit calls changed unit count: %d -> %d", before, after)
	}
}

// rawPos reads the unit's position straight out of the sim Transform store —
// the Source of Truth behind Position().
func rawPos(w *sim.World, id sim.EntityID) (Vec2, bool) {
	r := w.Transforms.Row(id)
	if r < 0 {
		return Vec2{}, false
	}
	p := w.Transforms.Pos[r]
	return Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}, true
}

// TestUnitPositionFSV: the getter agrees with the Transform store before and
// after a teleport. Known input → known output (spawn at 64,64).
func TestUnitPositionFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	// BEFORE: liveUnit spawns at (64,64).
	got := u.Position()
	sot, _ := rawPos(w, id)
	t.Logf("spawn: Position()=%+v store=%+v", got, sot)
	if got != (Vec2{X: 64, Y: 64}) || got != sot {
		t.Fatalf("Position()=%+v store=%+v, want {64 64}", got, sot)
	}

	// ACTION: teleport to a known new position via the sim, re-read getter.
	if !w.TeleportUnit(id, vec(Vec2{X: 200, Y: -50})) {
		t.Fatal("TeleportUnit failed")
	}
	got = u.Position()
	sot, _ = rawPos(w, id)
	t.Logf("after teleport: Position()=%+v store=%+v", got, sot)
	if got != (Vec2{X: 200, Y: -50}) || got != sot {
		t.Fatalf("Position()=%+v store=%+v, want {200 -50}", got, sot)
	}

	// EDGE: zero-value handle is a safe no-op getter.
	if z := (Unit{}).Position(); z != (Vec2{}) {
		t.Fatalf("zero Unit Position()=%+v, want zero", z)
	}
}

// TestUnitSetPositionFSV: the D3 SetPosition collapse. Default placement
// respects static pathing (nudges off an unpathable cell); Teleport() places
// raw. SoT = the sim Transform store + the grid walkability flags.
func TestUnitSetPositionFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)

	// Build a grid: an 11×11 walkable block of cells, with the single cell
	// (5,5) made unpathable. Cell = 32 world units; cell center = (x*32+16).
	grid := path.NewGrid()
	for y := int32(0); y <= 10; y++ {
		for x := int32(0); x <= 10; x++ {
			grid.SetFlags(x, y, path.Walkable)
		}
	}
	grid.SetFlags(5, 5, 0) // the blocked target cell
	w.SetGrid(grid)

	u, id := liveUnit(t, w, g, 0, 100)
	target := Vec2{X: 176, Y: 176} // center of the blocked cell (5,5)

	// --- default (pathed): must NOT land on the blocked cell ---
	u.SetPosition(target)
	got, _ := rawPos(w, id)
	gotCellX, gotCellY := int32(got.X)/32, int32(got.Y)/32
	cheb := func(a, b int32) int32 {
		if a < 0 {
			a = -a
		}
		if b < 0 {
			b = -b
		}
		if a > b {
			return a
		}
		return b
	}
	t.Logf("pathed SetPosition(%.0f,%.0f) -> %+v cell(%d,%d) walkable=%v",
		target.X, target.Y, got, gotCellX, gotCellY, grid.CellWalkable(gotCellX, gotCellY))
	if gotCellX == 5 && gotCellY == 5 {
		t.Fatalf("unit placed on the blocked cell (5,5): %+v", got)
	}
	if !grid.CellWalkable(gotCellX, gotCellY) {
		t.Fatalf("unit nudged onto a non-walkable cell (%d,%d)", gotCellX, gotCellY)
	}
	if d := cheb(gotCellX-5, gotCellY-5); d != 1 {
		t.Fatalf("nudge landed %d cells away, want nearest ring (1): %+v", d, got)
	}

	// --- Teleport(): raw placement, exact coords even on the blocked cell ---
	u.SetPosition(target, Teleport())
	got, _ = rawPos(w, id)
	t.Logf("teleport SetPosition(%.0f,%.0f) -> %+v (raw, ignores pathing)", target.X, target.Y, got)
	if got != target {
		t.Fatalf("Teleport() did not place raw: got %+v want %+v", got, target)
	}

	// --- already-pathable target keeps exact coords (no needless snap) ---
	clear := Vec2{X: 100, Y: 100} // cell (3,3), walkable
	u.SetPosition(clear)
	got, _ = rawPos(w, id)
	t.Logf("pathed SetPosition onto walkable (%.0f,%.0f) -> %+v (exact)", clear.X, clear.Y, got)
	if got != clear {
		t.Fatalf("pathable target was moved: got %+v want %+v", got, clear)
	}

	// EDGE: zero-value handle is a no-op.
	(Unit{}).SetPosition(target)
}

// TestUnitFacingFSV: SetFacing writes the Transform store; the getter reads it
// back. 90° round-trips exactly through the brad encoding (16384/65536).
func TestUnitFacingFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	const eps = 0.01 // brad quantization is ~0.0055°
	check := func(label string, setDeg, wantDeg float64) {
		u.SetFacing(Deg(setDeg))
		r := w.Transforms.Row(id)
		storeDeg := angleFromBrad(w.Transforms.Facing[r]).Degrees()
		getDeg := u.Facing().Degrees()
		t.Logf("%s: SetFacing(%.0f°) -> store=%.4f° getter=%.4f°", label, setDeg, storeDeg, getDeg)
		if math.Abs(storeDeg-wantDeg) > eps || math.Abs(getDeg-wantDeg) > eps {
			t.Fatalf("%s: store=%.4f getter=%.4f, want ~%.4f", label, storeDeg, getDeg, wantDeg)
		}
	}
	check("cardinal", 90, 90)
	// EDGE: 450° wraps to 90° in the unsigned brad encoding.
	check("wrap", 450, 90)
	// EDGE: zero-value handle SetFacing is a no-op (must not panic).
	(Unit{}).SetFacing(Deg(45))
}

// TestUnitSetLifeClampAndDeath is the D5 SetLife contract (issue edge case 1):
// values clamp to [0, MaxLife], and a lethal set (≤0) kills the unit, firing
// the death event in the step. SoT = the Health store + the event ring.
func TestUnitSetLifeClampAndDeath(t *testing.T) {
	// --- clamp high: 150 on a 100-max unit pins to 100, no death ---
	t.Run("clamp-high", func(t *testing.T) {
		w := sim.NewWorld(sim.Caps{Units: 16})
		g := newGame(w)
		u, id := liveUnit(t, w, g, 0, 100)
		deaths := 0
		g.OnEvent(EventUnitDeath, func(Event) { deaths++ })
		before, _ := rawLife(w, id)
		u.SetLife(150)
		after, _ := rawLife(w, id)
		w.Step()
		t.Logf("SetLife(150): store %.0f -> %.0f, Valid=%v deaths=%d", before, after, u.Valid(), deaths)
		if after != 100 || !u.Valid() || deaths != 0 {
			t.Fatalf("clamp-high: life=%.0f Valid=%v deaths=%d, want 100/true/0", after, u.Valid(), deaths)
		}
	})

	// --- lethal: SetLife(-10) clamps to 0 AND kills, firing one death ---
	t.Run("lethal-set", func(t *testing.T) {
		w := sim.NewWorld(sim.Caps{Units: 16})
		g := newGame(w)
		u, id := liveUnit(t, w, g, 0, 100)
		deaths := 0
		var deadID sim.EntityID
		g.OnEvent(EventUnitDeath, func(e Event) { deaths++; deadID = e.Unit().id })

		before, _ := rawLife(w, id)
		u.SetLife(-10)
		afterLife, _ := rawLife(w, id) // store written to 0; unit still present pre-step
		t.Logf("BEFORE step: SetLife(-10) store %.0f -> %.0f, Valid=%v", before, afterLife, u.Valid())
		if afterLife != 0 {
			t.Fatalf("lethal set did not clamp to 0: %.0f", afterLife)
		}
		w.Step()
		_, present := rawLife(w, id)
		t.Logf("AFTER step: deaths=%d deadID==unit=%v Valid=%v storePresent=%v",
			deaths, deadID == id, u.Valid(), present)
		if deaths != 1 || deadID != id || u.Valid() || present {
			t.Fatalf("lethal: deaths=%d deadID==unit=%v Valid=%v present=%v, want 1/true/false/false",
				deaths, deadID == id, u.Valid(), present)
		}
	})
}

// TestUnitManaFSV: the D5 mana accessors clamp to [0, MaxMana] over the sim
// AbilityStore. SoT = Abilities.Mana store.
func TestUnitManaFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	// EDGE: no mana pool (non-caster) -> Mana()=0, SetMana no-op.
	if u.Mana() != 0 || u.MaxMana() != 0 {
		t.Fatalf("non-caster mana=%v/%v, want 0/0", u.Mana(), u.MaxMana())
	}
	u.SetMana(50) // must not panic, must not write
	if u.Mana() != 0 {
		t.Fatalf("SetMana on non-caster wrote mana=%v", u.Mana())
	}

	// Give it a mana pool (200 max, 50 current) via the sim store.
	if !w.Abilities.Add(w.Ents, id) {
		t.Fatal("Abilities.Add failed")
	}
	ar := w.Abilities.Row(id)
	w.Abilities.MaxMana[ar] = fromFloat(200)
	w.Abilities.Mana[ar] = fromFloat(50)

	if u.Mana() != 50 || u.MaxMana() != 200 {
		t.Fatalf("mana=%v/%v, want 50/200", u.Mana(), u.MaxMana())
	}
	cases := []struct{ set, want float64 }{
		{150, 150}, // in range
		{999, 200}, // clamp high
		{-5, 0},    // clamp low
	}
	for _, c := range cases {
		u.SetMana(c.set)
		store := toFloat(w.Abilities.Mana[ar])
		t.Logf("SetMana(%.0f) -> getter=%.0f store=%.0f", c.set, u.Mana(), store)
		if u.Mana() != c.want || store != c.want {
			t.Errorf("SetMana(%.0f): getter=%.0f store=%.0f, want %.0f", c.set, u.Mana(), store, c.want)
		}
	}
}

// TestUnitMoveSpeedFSV: MoveSpeed is per-second public, per-tick in the sim
// (×20). SoT = Movements.Speed store.
func TestUnitMoveSpeedFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	// EDGE: no movement component -> MoveSpeed()=0, SetMoveSpeed no-op.
	if u.MoveSpeed() != 0 {
		t.Fatalf("immobile unit MoveSpeed=%v, want 0", u.MoveSpeed())
	}
	u.SetMoveSpeed(300) // no-op, must not panic
	if u.MoveSpeed() != 0 {
		t.Fatalf("SetMoveSpeed on immobile unit took effect: %v", u.MoveSpeed())
	}

	// Add movement at 8 world units/tick = 160 units/second.
	if !w.Movements.Add(w.Ents, w.Transforms, id, fixed.FromInt(8), 65535) {
		t.Fatal("Movements.Add failed")
	}
	mr := w.Movements.Row(id)
	t.Logf("8 wu/tick -> MoveSpeed()=%.0f (want 160)", u.MoveSpeed())
	if u.MoveSpeed() != 160 {
		t.Fatalf("MoveSpeed=%.0f, want 160 (8/tick × 20)", u.MoveSpeed())
	}

	// SetMoveSpeed(300 u/s) -> 15 u/tick in the store; round-trips to 300.
	u.SetMoveSpeed(300)
	storePerTick := toFloat(w.Movements.Speed[mr])
	t.Logf("SetMoveSpeed(300) -> store=%.1f u/tick, getter=%.0f u/s", storePerTick, u.MoveSpeed())
	if storePerTick != 15 || u.MoveSpeed() != 300 {
		t.Fatalf("SetMoveSpeed(300): store=%.1f getter=%.0f, want 15/300", storePerTick, u.MoveSpeed())
	}

	// EDGE: negative clamps to 0.
	u.SetMoveSpeed(-50)
	if u.MoveSpeed() != 0 {
		t.Fatalf("SetMoveSpeed(-50) -> %v, want 0", u.MoveSpeed())
	}
}

// TestUnitKillFiresDeathThenDestroys: Kill marks the unit; after one Step the
// death event fires exactly once and the unit is gone from the store.
func TestUnitKillFiresDeathThenDestroys(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	deaths := 0
	var deadID sim.EntityID
	g.OnEvent(EventUnitDeath, func(e Event) {
		deaths++
		deadID = e.Unit().id
	})

	t.Logf("BEFORE Kill: Valid=%v storeRow=%d", u.Valid(), w.Transforms.Row(id))
	u.Kill()
	// Kill is deferred: still alive until the step resolves it.
	if !u.Valid() {
		t.Fatal("unit must remain valid until Step resolves the kill")
	}
	w.Step()

	_, present := rawPos(w, id)
	t.Logf("AFTER Kill+Step: deaths=%d deadID==unit=%v Valid=%v storePresent=%v",
		deaths, deadID == id, u.Valid(), present)
	if deaths != 1 {
		t.Fatalf("death event fired %d times, want 1", deaths)
	}
	if deadID != id {
		t.Fatalf("death event carried entity %v, want %v", deadID, id)
	}
	if u.Valid() || present {
		t.Fatalf("killed unit still present: Valid=%v storePresent=%v", u.Valid(), present)
	}

	// EDGE: Kill on the now-dead handle is a silent no-op, no second death.
	u.Kill()
	w.Step()
	if deaths != 1 {
		t.Fatalf("Kill on dead unit fired another death event (deaths=%d)", deaths)
	}
}

// TestUnitRemoveIsSilentAndImmediate: Remove deletes the unit immediately with
// NO death event — the Kill/Remove distinction. SoT: store absence + no event.
func TestUnitRemoveIsSilentAndImmediate(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	deaths := 0
	g.OnEvent(EventUnitDeath, func(Event) { deaths++ })

	_, before := rawPos(w, id)
	u.Remove()
	_, after := rawPos(w, id)
	t.Logf("Remove: storePresent %v -> %v, Valid=%v, deaths(immediate)=%d", before, after, u.Valid(), deaths)
	if !before {
		t.Fatal("unit not present before Remove")
	}
	if after || u.Valid() {
		t.Fatalf("unit still present after Remove: storePresent=%v Valid=%v", after, u.Valid())
	}

	// Step to prove no deferred death event was queued by Remove.
	w.Step()
	if deaths != 0 {
		t.Fatalf("Remove fired %d death event(s), want 0 (Remove is silent)", deaths)
	}

	// EDGE: Remove on an invalid handle is a no-op.
	u.Remove()
	(Unit{}).Remove()
}
