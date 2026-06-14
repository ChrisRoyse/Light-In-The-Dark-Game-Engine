package litd

import (
	"fmt"
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

// TestUnitSetMaxLifeMaxManaFSV completes the D5 typed-accessor family: setting
// max life/mana. SoT = the Healths and Abilities stores read directly. WC3
// clamp semantics: lowering the max clamps the current value down; max life
// floors at 1, max mana floors at 0.
func TestUnitSetMaxLifeMaxManaFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100) // Life=100 MaxLife=100
	hr := w.Healths.Row(id)

	u.SetLife(80)
	t.Logf("start: Life=%.0f MaxLife=%.0f", toFloat(w.Healths.Life[hr]), toFloat(w.Healths.MaxLife[hr]))

	// Raise max: MaxLife up, current Life unchanged.
	u.SetMaxLife(200)
	if toFloat(w.Healths.MaxLife[hr]) != 200 || toFloat(w.Healths.Life[hr]) != 80 {
		t.Fatalf("raise max: MaxLife=%.0f Life=%.0f, want 200/80", toFloat(w.Healths.MaxLife[hr]), toFloat(w.Healths.Life[hr]))
	}
	// Lower max below current: current Life clamps down.
	u.SetMaxLife(50)
	t.Logf("after SetMaxLife(50): MaxLife=%.0f Life=%.0f", toFloat(w.Healths.MaxLife[hr]), toFloat(w.Healths.Life[hr]))
	if toFloat(w.Healths.MaxLife[hr]) != 50 || toFloat(w.Healths.Life[hr]) != 50 {
		t.Fatalf("lower max: MaxLife=%.0f Life=%.0f, want 50/50", toFloat(w.Healths.MaxLife[hr]), toFloat(w.Healths.Life[hr]))
	}
	// EDGE: max life floors at 1 (cannot be <=0).
	u.SetMaxLife(-10)
	t.Logf("after SetMaxLife(-10): MaxLife=%.0f Life=%.0f", toFloat(w.Healths.MaxLife[hr]), toFloat(w.Healths.Life[hr]))
	if toFloat(w.Healths.MaxLife[hr]) != 1 || toFloat(w.Healths.Life[hr]) != 1 {
		t.Fatalf("floor max life: MaxLife=%.0f Life=%.0f, want 1/1", toFloat(w.Healths.MaxLife[hr]), toFloat(w.Healths.Life[hr]))
	}

	// EDGE: SetMaxMana on a non-caster (no Ability component) is a no-op.
	u.SetMaxMana(300)
	if u.MaxMana() != 0 {
		t.Fatalf("SetMaxMana on non-caster wrote MaxMana=%v", u.MaxMana())
	}

	// Give a mana pool: 200 max / 150 current.
	if !w.Abilities.Add(w.Ents, id) {
		t.Fatal("Abilities.Add failed")
	}
	ar := w.Abilities.Row(id)
	w.Abilities.MaxMana[ar] = fromFloat(200)
	w.Abilities.Mana[ar] = fromFloat(150)

	// Raise: MaxMana up, current Mana unchanged.
	u.SetMaxMana(300)
	if toFloat(w.Abilities.MaxMana[ar]) != 300 || toFloat(w.Abilities.Mana[ar]) != 150 {
		t.Fatalf("raise max mana: MaxMana=%.0f Mana=%.0f, want 300/150", toFloat(w.Abilities.MaxMana[ar]), toFloat(w.Abilities.Mana[ar]))
	}
	// Lower below current: Mana clamps down.
	u.SetMaxMana(100)
	t.Logf("after SetMaxMana(100): MaxMana=%.0f Mana=%.0f", toFloat(w.Abilities.MaxMana[ar]), toFloat(w.Abilities.Mana[ar]))
	if toFloat(w.Abilities.MaxMana[ar]) != 100 || toFloat(w.Abilities.Mana[ar]) != 100 {
		t.Fatalf("lower max mana: MaxMana=%.0f Mana=%.0f, want 100/100", toFloat(w.Abilities.MaxMana[ar]), toFloat(w.Abilities.Mana[ar]))
	}
	// EDGE: max mana floors at 0 (non-caster pool is legal).
	u.SetMaxMana(-5)
	if toFloat(w.Abilities.MaxMana[ar]) != 0 || toFloat(w.Abilities.Mana[ar]) != 0 {
		t.Fatalf("floor max mana: MaxMana=%.0f Mana=%.0f, want 0/0", toFloat(w.Abilities.MaxMana[ar]), toFloat(w.Abilities.Mana[ar]))
	}

	// EDGE: zero-value handle is a safe no-op (no panic).
	(Unit{}).SetMaxLife(500)
	(Unit{}).SetMaxMana(500)
}

// TestUnitLifeManaPercentFSV: the D4 percent getters compute 100*cur/max
// off the sim stores, guarding divide-by-zero. SoT = Healths/Abilities stores.
func TestUnitLifeManaPercentFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100) // maxLife 100

	hr := w.Healths.Row(id)
	if hr < 0 {
		t.Fatal("health row missing")
	}

	// EDGE: non-caster has no mana pool -> ManaPercent guards to 0.
	t.Logf("FSV ManaPercent non-caster: maxMana store=0 -> %.2f (want 0)", u.ManaPercent())
	if u.ManaPercent() != 0 {
		t.Fatalf("ManaPercent non-caster = %v, want 0", u.ManaPercent())
	}

	// Life percent — known input/known output (X+X=Y): 25/100 -> 25%.
	cases := []struct {
		life float64
		want float64
	}{
		{100, 100}, // full
		{25, 25},   // quarter
		{1, 1},     // single point (off-by-one bait)
		{0, 0},     // empty (note: 0 life is lethal but the store read is pre-kill here)
	}
	for _, c := range cases {
		w.Healths.Life[hr] = fromFloat(c.life)
		got := u.LifePercent()
		t.Logf("FSV LifePercent: life store=%.0f max=100 -> %.2f (want %.2f)", c.life, got, c.want)
		if got != c.want {
			t.Fatalf("LifePercent(life=%v) = %v, want %v", c.life, got, c.want)
		}
	}

	// Mana percent with a pool: 50/200 -> 25%.
	if !w.Abilities.Add(w.Ents, id) {
		t.Fatal("Abilities.Add failed")
	}
	ar := w.Abilities.Row(id)
	w.Abilities.MaxMana[ar] = fromFloat(200)
	w.Abilities.Mana[ar] = fromFloat(50)
	got := u.ManaPercent()
	t.Logf("FSV ManaPercent: mana store=50 max=200 -> %.2f (want 25)", got)
	if got != 25 {
		t.Fatalf("ManaPercent(50/200) = %v, want 25", got)
	}

	// EDGE: invalid handle -> zero-value, no panic.
	w.DestroyUnit(id)
	if u.LifePercent() != 0 || u.ManaPercent() != 0 {
		t.Fatalf("dead unit percent = %v/%v, want 0/0", u.LifePercent(), u.ManaPercent())
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

// TestUnitPausedFSV checks the api surface for PauseUnit/IsUnitPaused
// (Unit.SetPaused/Paused). SoT = the sim Pauses presence set read directly.
// Replaces the former panic-bodied M5 stubs (manifest claimed mapped; code
// panicked — #217/#368-class contradiction).
func TestUnitPausedFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	if u.Paused() || w.IsUnitPaused(id) {
		t.Fatalf("fresh unit reports paused: api=%v sim=%v", u.Paused(), w.IsUnitPaused(id))
	}

	// Pause: api setter -> sim SoT flips, getter agrees.
	u.SetPaused(true)
	t.Logf("after SetPaused(true): api.Paused=%v sim.IsUnitPaused=%v count=%d", u.Paused(), w.IsUnitPaused(id), w.Pauses.Count())
	if !u.Paused() || !w.IsUnitPaused(id) || w.Pauses.Count() != 1 {
		t.Fatalf("SetPaused(true) not reflected: api=%v sim=%v count=%d", u.Paused(), w.IsUnitPaused(id), w.Pauses.Count())
	}

	// Resume: clears the row.
	u.SetPaused(false)
	t.Logf("after SetPaused(false): api.Paused=%v sim.IsUnitPaused=%v count=%d", u.Paused(), w.IsUnitPaused(id), w.Pauses.Count())
	if u.Paused() || w.IsUnitPaused(id) || w.Pauses.Count() != 0 {
		t.Fatalf("SetPaused(false) not reflected: api=%v sim=%v count=%d", u.Paused(), w.IsUnitPaused(id), w.Pauses.Count())
	}

	// EDGE: zero-value handle — getter returns false, setter is a no-op, no panic.
	if (Unit{}).Paused() {
		t.Error("zero Unit Paused() = true, want false")
	}
	(Unit{}).SetPaused(true) // must not panic
}

// orderDump renders the sim OrderStore head for a unit — the Source of Truth
// for Unit.Order. Reads Kind/Target/Point straight from the dense order row.
func orderDump(w *sim.World, id sim.EntityID) string {
	r := w.Orders.Row(id)
	if r < 0 {
		return "no-order-row"
	}
	p := w.Orders.Point[r]
	return fmt.Sprintf("kind=%d target=%d point=(%.1f,%.1f)",
		w.Orders.Kind[r], uint64(w.Orders.Target[r]), toFloat(p.X), toFloat(p.Y))
}

// TestUnitOrderCatalogMapsToSimKinds is the X+X=Y discipline for the catalog:
// each public Order's wire id MUST be its sim kind + 1 (so the zero Order is
// "unset", never aliasing OrderStop=0). Known input (catalog var) → known
// output (sim kind). Verified against the sim constants directly.
func TestUnitOrderCatalogMapsToSimKinds(t *testing.T) {
	cases := []struct {
		name string
		ord  Order
		kind uint8
	}{
		{"Stop", OrderStop, sim.OrderStop},
		{"Move", OrderMove, sim.OrderMove},
		{"Attack", OrderAttack, sim.OrderAttack},
		{"Smart", OrderSmart, sim.OrderSmart},
		{"Hold", OrderHold, sim.OrderHold},
		{"Patrol", OrderPatrol, sim.OrderPatrol},
		{"Follow", OrderFollow, sim.OrderFollow},
	}
	for _, c := range cases {
		if c.ord.IsZero() {
			t.Errorf("Order%s is the zero value — would be rejected as unset", c.name)
		}
		if got := uint8(c.ord.id - 1); got != c.kind {
			t.Errorf("Order%s maps to sim kind %d, want %d", c.name, got, c.kind)
		}
	}
	// The unset (zero) Order must be distinct from OrderStop.
	if (Order{}).id == OrderStop.id {
		t.Fatal("zero Order aliases OrderStop — IsZero() guard would never fire")
	}
}

// TestUnitOrderFSV verifies Unit.Order installs the right sim order head for
// each target shape, and fails closed on the documented edges. SoT = the sim
// OrderStore (w.Orders.Kind/Target/Point), read back via orderDump.
func TestUnitOrderFSV(t *testing.T) {
	w, g, _ := newDriverGame(t)
	u, id := apiOrderUnit(t, w, g, 0, Vec2{X: 64, Y: 64})
	victim, vid := apiOrderUnit(t, w, g, 1, Vec2{X: 256, Y: 256})

	// Fresh unit: order head defaults to OrderStop (kind 0), no target/point.
	t.Logf("BEFORE: %s", orderDump(w, id))
	if k := w.Orders.Kind[w.Orders.Row(id)]; k != sim.OrderStop {
		t.Fatalf("fresh order head kind=%d, want OrderStop=%d", k, sim.OrderStop)
	}

	// HAPPY 1 — point order (Move): Kind=OrderMove, Point=target, no entity target.
	if !u.Order(OrderMove, TargetPoint(Vec2{X: 500, Y: 600})) {
		t.Fatal("Order(Move, point) returned false")
	}
	r := w.Orders.Row(id)
	t.Logf("AFTER Move:   %s", orderDump(w, id))
	if w.Orders.Kind[r] != sim.OrderMove {
		t.Errorf("kind=%d, want OrderMove=%d", w.Orders.Kind[r], sim.OrderMove)
	}
	if p := w.Orders.Point[r]; p != vec(Vec2{X: 500, Y: 600}) {
		t.Errorf("point=(%.1f,%.1f), want (500,600)", toFloat(p.X), toFloat(p.Y))
	}
	if w.Orders.Target[r] != 0 {
		t.Errorf("point order left a stray entity target %d", uint64(w.Orders.Target[r]))
	}

	// HAPPY 2 — target order (Attack a unit): Kind=OrderAttack, Target=victim.
	// Unqueued, so it replaces the Move head (point must clear is not required;
	// the kind+target is what drives combat).
	if !u.Order(OrderAttack, TargetUnit(victim)) {
		t.Fatal("Order(Attack, unit) returned false")
	}
	t.Logf("AFTER Attack: %s", orderDump(w, id))
	if w.Orders.Kind[r] != sim.OrderAttack {
		t.Errorf("kind=%d, want OrderAttack=%d", w.Orders.Kind[r], sim.OrderAttack)
	}
	if w.Orders.Target[r] != vid {
		t.Errorf("target=%d, want victim %d", uint64(w.Orders.Target[r]), uint64(vid))
	}

	// HAPPY 3 — immediate order (Stop): Kind=OrderStop, target cleared.
	if !u.Order(OrderStop, TargetNone()) {
		t.Fatal("Order(Stop, none) returned false")
	}
	t.Logf("AFTER Stop:   %s", orderDump(w, id))
	if w.Orders.Kind[r] != sim.OrderStop {
		t.Errorf("kind=%d, want OrderStop=%d", w.Orders.Kind[r], sim.OrderStop)
	}
	if w.Orders.Target[r] != 0 {
		t.Errorf("immediate order left a stray target %d", uint64(w.Orders.Target[r]))
	}

	// EDGE 1 — unset (zero) order: rejected, SoT unchanged.
	before := orderDump(w, id)
	if u.Order(Order{}, TargetPoint(Vec2{X: 9, Y: 9})) {
		t.Error("Order(zero) returned true — must reject the unset order")
	}
	if after := orderDump(w, id); after != before {
		t.Errorf("unset order mutated SoT: %s -> %s", before, after)
	}

	// EDGE 2 — dead target unit: fail closed, no order issued, SoT unchanged.
	victim.Remove() // destroys the entity row immediately
	if victim.Valid() {
		t.Fatal("victim still valid after Remove()")
	}
	before = orderDump(w, id)
	if u.Order(OrderAttack, TargetUnit(victim)) {
		t.Error("Order(Attack, dead unit) returned true — must fail closed")
	}
	if after := orderDump(w, id); after != before {
		t.Errorf("order against dead target mutated SoT: %s -> %s", before, after)
	}

	// EDGE 3 — invalid ordering unit: no-op, returns false.
	u.Remove()
	if u.Order(OrderMove, TargetPoint(Vec2{X: 1, Y: 1})) {
		t.Error("Order on a removed unit returned true")
	}
	if w.Orders.Row(id) >= 0 {
		t.Error("removed unit still has an order row")
	}

	// EDGE 4 — zero-value handle: no panic, returns false.
	if (Unit{}).Order(OrderMove, TargetPoint(Vec2{})) {
		t.Error("Order on the zero Unit returned true")
	}
}

// TestUnitCollisionSizeFSV: Unit.CollisionSize reads the bound unit-type's
// collision radius. SoT = the data.Unit row resolved through the sim type store.
func TestUnitCollisionSizeFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 24},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}
	u := g.CreateUnit(owner, g.UnitType("hfoo"), Vec2{X: 100, Y: 100}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit invalid")
	}

	// SoT: the value bound into the type table (X+X=Y: bound 24 -> getter 24).
	t.Logf("bound CollisionSize=24; Unit.CollisionSize()=%v sim.UnitCollisionSize=%d", u.CollisionSize(), w.UnitCollisionSize(u.id))
	if u.CollisionSize() != 24 {
		t.Errorf("Unit.CollisionSize() = %v, want 24", u.CollisionSize())
	}

	// EDGE: untyped unit (no UnitTypes row) -> 0.
	bare, _ := liveUnit(t, w, g, 0, 50)
	if bare.CollisionSize() != 0 {
		t.Errorf("untyped unit CollisionSize() = %v, want 0", bare.CollisionSize())
	}
	// EDGE: zero-value handle -> 0, no panic.
	if (Unit{}).CollisionSize() != 0 {
		t.Errorf("zero Unit CollisionSize() = %v, want 0", (Unit{}).CollisionSize())
	}
}

// TestUnitTypeFSV verifies Unit.Type round-trips with the UnitType passed to
// CreateUnit. SoT = the sim UnitTypeStore.TypeID row.
func TestUnitTypeFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
		{ID: "hkni", Life: 200, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}

	typ := g.UnitType("hkni")
	if typ.IsZero() {
		t.Fatal(`UnitType("hkni") null`)
	}
	u := g.CreateUnit(owner, typ, Vec2{X: 100, Y: 100}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit invalid")
	}

	// SoT: the sim type row carries the bound type id; Unit.Type wraps it +1.
	r := w.UnitTypes.Row(u.id)
	if r < 0 {
		t.Fatal("created unit has no UnitTypes row")
	}
	rawTypeID := w.UnitTypes.TypeID[r]
	t.Logf("SoT UnitTypes.TypeID[row]=%d; UnitType.ref=%d", rawTypeID, typ.ref)

	got := u.Type()
	if got != typ {
		t.Errorf("Unit.Type() = %+v, want round-trip %+v", got, typ)
	}
	if got.ref != rawTypeID+1 {
		t.Errorf("Unit.Type().ref = %d, want sim TypeID+1 = %d", got.ref, rawTypeID+1)
	}
	// Distinguishes types: an hfoo unit must not report hkni's type.
	other := g.CreateUnit(owner, g.UnitType("hfoo"), Vec2{X: 200, Y: 200}, Deg(0))
	if other.Type() == got {
		t.Error("distinct unit types compared equal (hfoo == hkni)")
	}

	// EDGE: invalid/zero handle → null UnitType, no panic.
	if !(Unit{}).Type().IsZero() {
		t.Error("zero Unit.Type() is not null")
	}
	// EDGE: removed unit → null UnitType.
	u.Remove()
	if !u.Type().IsZero() {
		t.Error("removed Unit.Type() is not null")
	}
}

// TestUnitSetOwnerFSV verifies Unit.SetOwner migrates ownership AND the derived
// per-player food ledger and color, not just the Owners store row. SoT = the
// sim Owners store + World.FoodUsed/FoodCap ledgers (the #362 trap: a raw store
// poke would leave the ledger desynced).
func TestUnitSetOwnerFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16, FoodCost: 3, FoodProvided: 2},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	const A, B = uint8(2), uint8(5)
	pa := Player{idx: int32(A), g: g}
	pb := Player{idx: int32(B), g: g}
	typ := g.UnitType("hfoo")

	u := g.CreateUnit(pa, typ, Vec2{X: 100, Y: 100}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit invalid")
	}
	or := w.Owners.Row(u.id)

	dump := func(tag string) {
		t.Logf("%s: owner=%d team=%d color=%d | foodUsed[A]=%d cap[A]=%d foodUsed[B]=%d cap[B]=%d",
			tag, w.Owners.Player[or], w.Owners.Team[or], w.Owners.Color[or],
			w.FoodUsed(A), w.FoodCap(A), w.FoodUsed(B), w.FoodCap(B))
	}

	// BEFORE: unit charged to A (cost 3, provided 2); B is empty; color = A's slot.
	dump("BEFORE")
	if w.Owners.Player[or] != A || w.FoodUsed(A) != 3 || w.FoodCap(A) != 2 {
		t.Fatalf("precondition: owner=%d foodUsed[A]=%d cap[A]=%d, want A=%d,3,2", w.Owners.Player[or], w.FoodUsed(A), w.FoodCap(A), A)
	}
	if w.FoodUsed(B) != 0 || w.FoodCap(B) != 0 {
		t.Fatalf("precondition: B ledger not empty: used=%d cap=%d", w.FoodUsed(B), w.FoodCap(B))
	}
	totalUsedBefore := w.FoodUsed(A) + w.FoodUsed(B)

	// ACTION: hand the unit to B, changing color.
	u.SetOwner(pb, true)
	dump("AFTER SetOwner(B, changeColor=true)")

	// Owner row migrated.
	if w.Owners.Player[or] != B || w.Owners.Team[or] != B || w.Owners.Color[or] != B {
		t.Errorf("owner row = (player %d, team %d, color %d), want all %d", w.Owners.Player[or], w.Owners.Team[or], w.Owners.Color[or], B)
	}
	// Food ledger migrated: A shed it, B took it on.
	if w.FoodUsed(A) != 0 || w.FoodCap(A) != 0 {
		t.Errorf("A ledger not shed: used=%d cap=%d, want 0,0", w.FoodUsed(A), w.FoodCap(A))
	}
	if w.FoodUsed(B) != 3 || w.FoodCap(B) != 2 {
		t.Errorf("B ledger not charged: used=%d cap=%d, want 3,2", w.FoodUsed(B), w.FoodCap(B))
	}
	// Conservation invariant: total food across players unchanged.
	if got := w.FoodUsed(A) + w.FoodUsed(B); got != totalUsedBefore {
		t.Errorf("food not conserved: total used %d -> %d", totalUsedBefore, got)
	}
	// Public getter agrees.
	if w.Owners.Player[w.Owners.Row(u.id)] != B || u.Owner().idx != int32(B) {
		t.Errorf("Owner() = %d, want %d", u.Owner().idx, B)
	}

	// EDGE 1 — changeColor=false keeps the old color while moving the owner.
	u2 := g.CreateUnit(pa, typ, Vec2{X: 150, Y: 150}, Deg(0))
	r2 := w.Owners.Row(u2.id)
	u2.SetOwner(pb, false)
	t.Logf("EDGE changeColor=false: owner=%d color=%d (want owner=%d, color=%d kept)", w.Owners.Player[r2], w.Owners.Color[r2], B, A)
	if w.Owners.Player[r2] != B {
		t.Errorf("owner not changed: %d, want %d", w.Owners.Player[r2], B)
	}
	if w.Owners.Color[r2] != A {
		t.Errorf("color changed despite changeColor=false: %d, want %d", w.Owners.Color[r2], A)
	}

	// EDGE 2 — foreign player (different game): no-op, owner unchanged.
	otherG := newGame(sim.NewWorld(sim.Caps{Units: 4}))
	foreign := Player{idx: 1, g: otherG}
	beforeOwner := w.Owners.Player[or]
	u.SetOwner(foreign, true)
	if w.Owners.Player[or] != beforeOwner {
		t.Errorf("foreign-player SetOwner changed owner: %d -> %d", beforeOwner, w.Owners.Player[or])
	}

	// EDGE 3 — removed unit: no-op, no panic.
	u.Remove()
	u.SetOwner(pa, true)
	// EDGE 4 — zero handle: no panic.
	(Unit{}).SetOwner(pa, true)
}

// TestUnitLevelFSV: the non-hero Level accessor returns the type's
// configured design level. SoT = the bound data.Unit.Level via sim
// World.UnitLevel. (Hero precedence is covered in the sim layer where
// the hero machinery lives — see sim.TestUnitLevelHeroPrecedenceFSV.)
func TestUnitLevelFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hlvl", Life: 100, Level: 2 + 3}, // 2+3=5
		{ID: "hzro", Life: 100},               // Level omitted -> 0
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}

	u := g.CreateUnit(owner, g.UnitType("hlvl"), Vec2{X: 64, Y: 64}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit hlvl invalid")
	}
	t.Logf("FSV Level: hlvl api=%d simSoT=%d (want 5)", u.Level(), w.UnitLevel(u.id))
	if got, sot := u.Level(), int(w.UnitLevel(u.id)); got != 5 || sot != 5 {
		t.Errorf("hlvl Level: api=%d sim=%d, want 5", got, sot)
	}

	// EDGE: type with no level -> 0.
	z := g.CreateUnit(owner, g.UnitType("hzro"), Vec2{X: 96, Y: 96}, Deg(0))
	if z.Level() != 0 {
		t.Errorf("hzro Level = %d, want 0", z.Level())
	}

	// EDGE: untyped unit -> 0.
	bare, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(8), Y: fixed.FromInt(8)}, 0)
	if !ok {
		t.Fatal("bare CreateUnit failed")
	}
	if lvl := (Unit{id: bare, g: g}).Level(); lvl != 0 {
		t.Errorf("untyped Level = %d, want 0", lvl)
	}

	// EDGE: invalid handle -> 0, no panic.
	if (Unit{}).Level() != 0 {
		t.Error("zero-handle Level != 0")
	}
}

// TestUnitRaceFSV: Race/IsRace read the type's configured race off the
// sim. SoT = bound data.Unit.Race via World.UnitRace.
func TestUnitRaceFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "horc", Life: 100, Race: data.RaceOrc},
		{ID: "hnil", Life: 100}, // race omitted -> RaceNone
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}

	u := g.CreateUnit(owner, g.UnitType("horc"), Vec2{X: 64, Y: 64}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit horc invalid")
	}
	t.Logf("FSV Race: horc api=%d simSoT=%d (want %d=orc)", u.Race(), w.UnitRace(u.id), RaceOrc)
	if u.Race() != RaceOrc || uint8(u.Race()) != w.UnitRace(u.id) {
		t.Errorf("horc Race: api=%d sim=%d, want %d", u.Race(), w.UnitRace(u.id), RaceOrc)
	}
	// IsRace: true only for the matching race.
	if !u.IsRace(RaceOrc) {
		t.Error("horc IsRace(Orc) = false")
	}
	if u.IsRace(RaceHuman) || u.IsRace(RaceNone) {
		t.Errorf("horc IsRace cross-match: human=%v none=%v, want false", u.IsRace(RaceHuman), u.IsRace(RaceNone))
	}

	// EDGE: type with no race -> RaceNone, matches only IsRace(RaceNone).
	z := g.CreateUnit(owner, g.UnitType("hnil"), Vec2{X: 96, Y: 96}, Deg(0))
	if z.Race() != RaceNone || !z.IsRace(RaceNone) || z.IsRace(RaceOrc) {
		t.Errorf("hnil Race=%d IsRace(None)=%v IsRace(Orc)=%v, want None/true/false", z.Race(), z.IsRace(RaceNone), z.IsRace(RaceOrc))
	}

	// EDGE: invalid handle -> RaceNone, IsRace false (even for RaceNone).
	if (Unit{}).Race() != RaceNone {
		t.Error("zero-handle Race != RaceNone")
	}
	if (Unit{}).IsRace(RaceNone) {
		t.Error("zero-handle IsRace(None) = true, want false (invalid guard)")
	}
}

// TestUnitOwnedByFSV verifies Unit.OwnedBy against the sim Owners.Player
// slot: true only for the owning player, false for everyone else and on
// every invalid combination.
func TestUnitOwnedByFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	const A, B = int32(2), int32(5)
	pa := Player{idx: A, g: g}
	pb := Player{idx: B, g: g}

	u, id := liveUnit(t, w, g, uint8(A), 100) // spawned owned by player A
	or := w.Owners.Row(id)
	t.Logf("FSV OwnedBy BEFORE: store owner=%d (A=%d B=%d)", w.Owners.Player[or], A, B)

	// Happy: owner matches, non-owner does not.
	if !u.OwnedBy(pa) {
		t.Fatalf("OwnedBy(A) = false, want true; store owner=%d", w.Owners.Player[or])
	}
	if u.OwnedBy(pb) {
		t.Fatalf("OwnedBy(B) = true, want false; store owner=%d", w.Owners.Player[or])
	}

	// After reassigning to B, the verdict flips with the store.
	u.SetOwner(pb, true)
	t.Logf("FSV OwnedBy AFTER SetOwner(B): store owner=%d", w.Owners.Player[or])
	if u.OwnedBy(pa) || !u.OwnedBy(pb) {
		t.Fatalf("post-reassign OwnedBy A=%v B=%v, want false/true; store owner=%d",
			u.OwnedBy(pa), u.OwnedBy(pb), w.Owners.Player[or])
	}

	// EDGE: invalid player handle -> false, no panic.
	if u.OwnedBy(Player{}) {
		t.Fatal("OwnedBy(zero player) = true, want false")
	}
	// EDGE: player from a different game -> false.
	other := newGame(sim.NewWorld(sim.Caps{Units: 4}))
	if u.OwnedBy(Player{idx: B, g: other}) {
		t.Fatal("OwnedBy(foreign-game player) = true, want false")
	}
	// EDGE: invalid (removed) unit -> false, no panic.
	w.DestroyUnit(id)
	if u.OwnedBy(pb) {
		t.Fatal("OwnedBy on dead unit = true, want false")
	}
	if (Unit{}).OwnedBy(pb) {
		t.Fatal("zero-handle OwnedBy = true, want false")
	}
}

// TestUnitRallyFSV: RallyPoint/RallyUnit read the produce store's rally
// state. SoT = w.Produce.RallyKind/RallyPoint/RallyEnt.
func TestUnitRallyFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hall", Life: 500, CollisionSize: 32},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 0, g: g}

	b := g.CreateUnit(owner, g.UnitType("hall"), Vec2{X: 200, Y: 200}, Deg(0))
	if !b.Valid() {
		t.Fatal("CreateUnit hall invalid")
	}

	// EDGE: no produce row -> zero rally point, zero rally unit.
	t.Logf("FSV rally pre-producer: point=%v unit.valid=%v", b.RallyPoint(), b.RallyUnit().Valid())
	if !b.RallyPoint().IsZero() || b.RallyUnit().Valid() {
		t.Fatalf("no-producer rally: point=%v unitValid=%v, want zero/false", b.RallyPoint(), b.RallyUnit().Valid())
	}

	if !w.AddProducer(b.id) {
		t.Fatal("AddProducer failed")
	}

	// Point rally: X+X=Y, set (250+250, 300+300)=(500,600).
	if !w.SetRallyPoint(b.id, fixed.Vec2{X: fixed.FromInt(500), Y: fixed.FromInt(600)}) {
		t.Fatal("SetRallyPoint failed")
	}
	pt := b.RallyPoint()
	t.Logf("FSV point rally: api=%v simKind=set (want {500,600}); RallyUnit.valid=%v (want false)", pt, b.RallyUnit().Valid())
	if pt.X != 500 || pt.Y != 600 {
		t.Fatalf("RallyPoint = %v, want {500,600}", pt)
	}
	if b.RallyUnit().Valid() {
		t.Fatal("point rally reported a RallyUnit")
	}

	// Entity rally: rally to another unit -> RallyUnit resolves, RallyPoint
	// follows that unit's position.
	target := g.CreateUnit(owner, g.UnitType("hall"), Vec2{X: 700, Y: 800}, Deg(0))
	if !w.SetRallyTarget(b.id, target.id) {
		t.Fatal("SetRallyTarget failed")
	}
	ru := b.RallyUnit()
	rp := b.RallyPoint()
	t.Logf("FSV entity rally: RallyUnit.id match=%v RallyPoint=%v (want target pos {700,800})", ru.id == target.id, rp)
	if !ru.Valid() || ru.id != target.id {
		t.Fatalf("RallyUnit = valid:%v, want target %#x", ru.Valid(), uint32(target.id))
	}
	if rp.X != 700 || rp.Y != 800 {
		t.Fatalf("entity-rally RallyPoint = %v, want target pos {700,800}", rp)
	}

	// EDGE: invalid handle -> zero values, no panic.
	if !(Unit{}).RallyPoint().IsZero() || (Unit{}).RallyUnit().Valid() {
		t.Fatal("zero-handle rally not zero")
	}
}

// TestUnitVisibleToFSV verifies Unit.VisibleTo against the sim visibility
// system: a player always sees its own units, sees foreign units only
// when one of its units has them in active sight, and never panics on
// invalid handles. SoT = w.CanSeeEntity / the fog grid.
func TestUnitVisibleToFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 64})
	g := newGame(w)
	grid := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			grid.SetFlags(x, y, path.Walkable|path.Buildable|path.Flyable)
		}
	}
	w.SetGrid(grid)
	if !w.BindUnitDefs([]data.Unit{
		{ID: "scout", Life: 100, SightDay: fixed.FromInt(360), SightNight: fixed.FromInt(160), CollisionSize: 16, Pathing: data.PathingGround},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	w.SetTimeOfDay(12 * fixed.One)
	w.SuspendTimeOfDay(true)

	spawn := func(player, team uint8, cellX, cellY int32) sim.EntityID {
		id, ok := w.CreateUnit(sim.CellCenter(cellY*path.GridSize+cellX), 0)
		if !ok || !w.Owners.Add(w.Ents, id, player, team, player) ||
			!w.UnitTypes.Add(w.Ents, id, 0) ||
			!w.Collisions.Add(w.Ents, id, 1, sim.PathGround) ||
			!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) {
			t.Fatalf("spawn failed id=%d ok=%v", id, ok)
		}
		return id
	}

	p0 := Player{idx: 0, g: g}
	p1 := Player{idx: 1, g: g}
	scout := spawn(0, 0, 40, 40)  // player 0
	target := spawn(1, 1, 42, 40) // player 1, adjacent to scout
	_ = scout
	w.RecomputeVisibility()

	tgt := Unit{id: target, g: g}
	// Owner always sees its own unit.
	t.Logf("FSV VisibleTo near: ownerSees=%v p0Sees=%v simSoT(p0)=%v",
		tgt.VisibleTo(p1), tgt.VisibleTo(p0), w.CanSeeEntity(0, target))
	if !tgt.VisibleTo(p1) {
		t.Fatal("owner cannot see own unit")
	}
	// Player 0's scout has the target in sight -> visible to p0.
	if !tgt.VisibleTo(p0) {
		t.Fatalf("adjacent enemy not visible to p0; sim CanSee=%v", w.CanSeeEntity(0, target))
	}

	// Move the target far out of p0's vision (via the public setter so the
	// bucket grid reconciles); owner still sees it, p0 does not.
	far := sim.CellCenter(5*path.GridSize + 5)
	tgt.SetPosition(Vec2{X: toFloat(far.X), Y: toFloat(far.Y)})
	w.RecomputeVisibility()
	t.Logf("FSV VisibleTo far: ownerSees=%v p0Sees=%v simSoT(p0)=%v",
		tgt.VisibleTo(p1), tgt.VisibleTo(p0), w.CanSeeEntity(0, target))
	if !tgt.VisibleTo(p1) {
		t.Fatal("owner lost sight of own unit after move")
	}
	if tgt.VisibleTo(p0) {
		t.Fatalf("distant enemy still visible to p0; sim CanSee=%v", w.CanSeeEntity(0, target))
	}

	// EDGE: invalid/foreign player -> false.
	if tgt.VisibleTo(Player{}) {
		t.Fatal("VisibleTo(zero player) = true")
	}
	other := newGame(sim.NewWorld(sim.Caps{Units: 4}))
	if tgt.VisibleTo(Player{idx: 1, g: other}) {
		t.Fatal("VisibleTo(foreign-game player) = true")
	}
	// EDGE: dead unit / zero handle -> false, no panic.
	w.DestroyUnit(target)
	if tgt.VisibleTo(p1) {
		t.Fatal("dead unit VisibleTo = true")
	}
	if (Unit{}).VisibleTo(p1) {
		t.Fatal("zero-handle VisibleTo = true")
	}
}

// TestUnitAliveFSV: Alive tracks the life store (WC3 Life>0), false for corpses
// and invalid handles. SoT = the sim Health store life value.
func TestUnitAliveFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)
	lf := func() float64 { v, _ := rawLife(w, id); return v }

	// Alive at full life.
	t.Logf("spawn: life=%.0f alive=%v", lf(), u.Alive())
	if !u.Alive() {
		t.Fatal("freshly spawned unit not Alive")
	}
	// Wound but not kill — still alive.
	u.SetLife(1)
	if !u.Alive() {
		t.Errorf("unit at life 1 not Alive (life=%.0f)", lf())
	}
	// Lethal SetLife(0): life store hits 0 -> not alive (corpse).
	u.SetLife(0)
	t.Logf("after SetLife(0): life=%.0f alive=%v", lf(), u.Alive())
	if u.Alive() {
		t.Errorf("unit at life 0 reports Alive (life=%.0f)", lf())
	}
	// EDGE: invalid/removed handle -> not alive.
	u.Remove()
	if u.Alive() {
		t.Error("removed unit reports Alive")
	}
	if (Unit{}).Alive() {
		t.Error("zero Unit reports Alive")
	}
}

// TestUnitCurrentOrderFSV: CurrentOrder reports the order head verb. SoT = the
// sim OrderStore Kind, set via Unit.Order.
func TestUnitCurrentOrderFSV(t *testing.T) {
	w, g, _ := newDriverGame(t)
	u, id := apiOrderUnit(t, w, g, 0, Vec2{X: 64, Y: 64})

	// Fresh unit: order head is OrderStop.
	t.Logf("fresh: simKind=%d CurrentOrder==OrderStop:%v", w.Orders.Kind[w.Orders.Row(id)], u.CurrentOrder() == OrderStop)
	if u.CurrentOrder() != OrderStop {
		t.Errorf("fresh CurrentOrder = %+v, want OrderStop", u.CurrentOrder())
	}
	// Issue Move → CurrentOrder reports OrderMove, agreeing with the store.
	u.Order(OrderMove, TargetPoint(Vec2{X: 200, Y: 200}))
	t.Logf("after Move: simKind=%d (OrderMove=%d) CurrentOrder==OrderMove:%v", w.Orders.Kind[w.Orders.Row(id)], sim.OrderMove, u.CurrentOrder() == OrderMove)
	if u.CurrentOrder() != OrderMove {
		t.Errorf("CurrentOrder = %+v, want OrderMove", u.CurrentOrder())
	}
	if got := uint8(u.CurrentOrder().id - 1); got != w.Orders.Kind[w.Orders.Row(id)] {
		t.Errorf("CurrentOrder kind %d != store kind %d", got, w.Orders.Kind[w.Orders.Row(id)])
	}
	// EDGE: invalid/zero handle → zero Order.
	if !(Unit{}).CurrentOrder().IsZero() {
		t.Error("zero Unit CurrentOrder not zero")
	}
	u.Remove()
	if !u.CurrentOrder().IsZero() {
		t.Error("removed unit CurrentOrder not zero")
	}
}

// TestUnitArmorFSV: Armor reflects the base ArmorValue (plus buffs; none here)
// from the Health store. SoT = w.Healths.ArmorValue + BuffedArmor.
func TestUnitArmorFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)
	r := w.Healths.Row(id)

	// Set a known base armor; with no buffs, BuffedArmor == base.
	w.Healths.ArmorValue[r] = 5
	want := float64(w.BuffedArmor(id, 5))
	t.Logf("base ArmorValue=5 BuffedArmor=%v Unit.Armor()=%v", want, u.Armor())
	if u.Armor() != want {
		t.Errorf("Armor() = %v, want %v", u.Armor(), want)
	}
	if u.Armor() != 5 {
		t.Errorf("unbuffed Armor() = %v, want base 5", u.Armor())
	}

	// Negative base armor is representable (a debuffed light unit).
	w.Healths.ArmorValue[r] = -2
	if got, want := u.Armor(), float64(w.BuffedArmor(id, -2)); got != want {
		t.Errorf("negative base: Armor() = %v, want %v", got, want)
	}

	// EDGE: invalid / zero handle → 0.
	if (Unit{}).Armor() != 0 {
		t.Error("zero Unit Armor() != 0")
	}
	u.Remove()
	if u.Armor() != 0 {
		t.Error("removed unit Armor() != 0")
	}
}

// TestUnitInvulnerableFSV: SetInvulnerable/Invulnerable round-trip through the
// Health store flag. SoT = w.Healths.Invulnerable.
func TestUnitInvulnerableFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)
	r := w.Healths.Row(id)

	// Default vulnerable.
	if u.Invulnerable() || w.Healths.Invulnerable[r] {
		t.Fatal("unit spawned invulnerable")
	}
	// Toggle on → store + getter agree.
	u.SetInvulnerable(true)
	t.Logf("after SetInvulnerable(true): store=%v getter=%v", w.Healths.Invulnerable[r], u.Invulnerable())
	if !w.Healths.Invulnerable[r] || !u.Invulnerable() {
		t.Errorf("set true: store=%v getter=%v", w.Healths.Invulnerable[r], u.Invulnerable())
	}
	// Toggle off.
	u.SetInvulnerable(false)
	if w.Healths.Invulnerable[r] || u.Invulnerable() {
		t.Errorf("set false: store=%v getter=%v", w.Healths.Invulnerable[r], u.Invulnerable())
	}
	// EDGE: zero / removed handle → false, no panic.
	if (Unit{}).Invulnerable() {
		t.Error("zero Unit Invulnerable() true")
	}
	u.Remove()
	u.SetInvulnerable(true) // no-op, no panic
	if u.Invulnerable() {
		t.Error("removed unit Invulnerable() true")
	}
}

// TestUnitTurnSpeedFSV: SetTurnSpeed/TurnSpeed round-trip through the Movement
// store TurnRate (per-second radians ↔ per-tick brad). SoT = Movements.TurnRate.
func TestUnitTurnSpeedFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)
	if !w.Movements.Add(w.Ents, w.Transforms, id, fixed.FromInt(8), 0) {
		t.Fatal("Movements.Add failed")
	}
	r := w.Movements.Row(id)

	// Known input: π rad/s. Store must equal the same per-tick quantization.
	u.SetTurnSpeed(math.Pi)
	wantBrad := angleToBrad(Rad(math.Pi / float64(data.TicksPerSecond)))
	t.Logf("SetTurnSpeed(π): store=%d wantBrad=%d getter=%.4f rad/s", w.Movements.TurnRate[r], wantBrad, u.TurnSpeed())
	if w.Movements.TurnRate[r] != wantBrad {
		t.Errorf("store TurnRate=%d, want %d", w.Movements.TurnRate[r], wantBrad)
	}
	if math.Abs(u.TurnSpeed()-math.Pi) > 0.01 {
		t.Errorf("TurnSpeed()=%.4f, want ~%.4f", u.TurnSpeed(), math.Pi)
	}

	// EDGE: negative clamps to 0.
	u.SetTurnSpeed(-1)
	if w.Movements.TurnRate[r] != 0 || u.TurnSpeed() != 0 {
		t.Errorf("negative not clamped: store=%d getter=%.4f", w.Movements.TurnRate[r], u.TurnSpeed())
	}
	// EDGE: zero / removed handle → 0, no panic.
	if (Unit{}).TurnSpeed() != 0 {
		t.Error("zero Unit TurnSpeed() != 0")
	}
	u.Remove()
	u.SetTurnSpeed(1) // no-op
	if u.TurnSpeed() != 0 {
		t.Error("removed unit TurnSpeed() != 0")
	}
}

// TestUnitAcquireRangeFSV: SetAcquireRange/AcquireRange round-trip through the
// Combat store. SoT = Combats.AcquisitionRange.
func TestUnitAcquireRangeFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	// No combat row → getter 0, setter no-op.
	if u.AcquireRange() != 0 {
		t.Fatalf("no-combat AcquireRange=%v, want 0", u.AcquireRange())
	}
	u.SetAcquireRange(500) // no-op
	if u.AcquireRange() != 0 {
		t.Fatalf("SetAcquireRange on no-combat unit took effect: %v", u.AcquireRange())
	}

	if !w.Combats.Add(w.Ents, id) {
		t.Fatal("Combats.Add failed")
	}
	r := w.Combats.Row(id)

	// Known input 600 → store holds it, getter reads it back.
	u.SetAcquireRange(600)
	t.Logf("SetAcquireRange(600): store=%v getter=%.0f", w.Combats.AcquisitionRange[r], u.AcquireRange())
	if w.Combats.AcquisitionRange[r] != fromFloat(600) {
		t.Errorf("store=%v, want %v", w.Combats.AcquisitionRange[r], fromFloat(600))
	}
	if u.AcquireRange() != 600 {
		t.Errorf("AcquireRange()=%.0f, want 600", u.AcquireRange())
	}
	// EDGE: negative clamps to 0.
	u.SetAcquireRange(-50)
	if w.Combats.AcquisitionRange[r] != 0 || u.AcquireRange() != 0 {
		t.Errorf("negative not clamped: store=%v getter=%.0f", w.Combats.AcquisitionRange[r], u.AcquireRange())
	}
	// EDGE: zero / removed handle → 0, no panic.
	if (Unit{}).AcquireRange() != 0 {
		t.Error("zero Unit AcquireRange() != 0")
	}
	u.Remove()
	if u.AcquireRange() != 0 {
		t.Error("removed unit AcquireRange() != 0")
	}
}

// TestUnitUserDataFSV: SetUserData/UserData round-trip through the sparse
// UserDataStore. SoT = the store row (w.UserDatas.Row/Value), read directly —
// never via the getter return alone. Proves the lazy-allocation invariant:
// no row exists until a value is set; an unset unit reads 0.
func TestUnitUserDataFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	// BEFORE: never set → sparse store has NO row, getter reads default 0.
	if r := w.UserDatas.Row(id); r != -1 {
		t.Fatalf("unset unit already has a userdata row %d (not sparse)", r)
	}
	if got := u.UserData(); got != 0 {
		t.Fatalf("unset UserData()=%d, want 0", got)
	}
	t.Logf("BEFORE: row=%d getter=%d count=%d", w.UserDatas.Row(id), u.UserData(), w.UserDatas.Count())

	// X+X=Y: set 21+21=42, then read the byte in the store, not the return.
	u.SetUserData(21 + 21)
	r := w.UserDatas.Row(id)
	if r == -1 {
		t.Fatal("after SetUserData: still no store row (lazy-alloc failed)")
	}
	if w.UserDatas.Value[r] != 42 {
		t.Errorf("store Value[%d]=%d, want 42", r, w.UserDatas.Value[r])
	}
	if u.UserData() != 42 {
		t.Errorf("getter UserData()=%d, want 42", u.UserData())
	}
	t.Logf("AFTER set 42: row=%d store=%d getter=%d count=%d", r, w.UserDatas.Value[r], u.UserData(), w.UserDatas.Count())

	// Overwrite reuses the SAME row (no leak): set again, count must not grow.
	beforeCount := w.UserDatas.Count()
	u.SetUserData(-7)
	if w.UserDatas.Count() != beforeCount {
		t.Errorf("overwrite grew count %d→%d (leaked a row)", beforeCount, w.UserDatas.Count())
	}
	if w.UserDatas.Value[w.UserDatas.Row(id)] != -7 || u.UserData() != -7 {
		t.Errorf("overwrite: store=%d getter=%d, want -7", w.UserDatas.Value[w.UserDatas.Row(id)], u.UserData())
	}

	// EDGE: int32 extremes survive the int↔int32 boundary unchanged.
	for _, v := range []int{math.MaxInt32, math.MinInt32, 0} {
		u.SetUserData(v)
		if got := u.UserData(); got != v {
			t.Errorf("extreme %d: round-trip got %d", v, got)
		}
		if w.UserDatas.Value[w.UserDatas.Row(id)] != int32(v) {
			t.Errorf("extreme %d: store=%d", v, w.UserDatas.Value[w.UserDatas.Row(id)])
		}
	}

	// EDGE: zero / removed handle → getter 0, setter no-op, no panic, and the
	// store row is reclaimed on removal (DestroyUnit path).
	if (Unit{}).UserData() != 0 {
		t.Error("zero Unit UserData() != 0")
	}
	u.SetUserData(99)
	u.Remove()
	if w.UserDatas.Row(id) != -1 {
		t.Errorf("removed unit still has userdata row %d (DestroyUnit leak)", w.UserDatas.Row(id))
	}
	u.SetUserData(5) // no-op on dead handle
	if u.UserData() != 0 {
		t.Errorf("removed unit UserData()=%d, want 0", u.UserData())
	}
}

// TestUnitPointValueFSV: Unit.PointValue surfaces the unit type's static
// point value. SoT = the bound data.Unit table (known input, X+X=Y) read back
// through the sim accessor w.UnitPointValue, independent of the getter return.
func TestUnitPointValueFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hbnt", Life: 100, PointValue: 12 + 13}, // 12+13=25
		{ID: "hzro", Life: 100},                      // PointValue omitted -> 0
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}

	// Happy: typed unit returns its type's value, via both api and sim SoT.
	u := g.CreateUnit(owner, g.UnitType("hbnt"), Vec2{X: 64, Y: 64}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit hbnt invalid")
	}
	t.Logf("hbnt: api=%d simSoT=%d", u.PointValue(), w.UnitPointValue(u.id))
	if got, sot := u.PointValue(), w.UnitPointValue(u.id); got != 25 || sot != 25 {
		t.Errorf("hbnt PointValue: api=%d sim=%d, want 25", got, sot)
	}

	// EDGE: type with no point value -> 0.
	z := g.CreateUnit(owner, g.UnitType("hzro"), Vec2{X: 96, Y: 96}, Deg(0))
	if z.PointValue() != 0 || w.UnitPointValue(z.id) != 0 {
		t.Errorf("hzro PointValue: api=%d sim=%d, want 0", z.PointValue(), w.UnitPointValue(z.id))
	}

	// EDGE: untyped unit (no UnitTypes row) -> 0.
	bare, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(8), Y: fixed.FromInt(8)}, 0)
	if !ok {
		t.Fatal("bare CreateUnit failed")
	}
	if w.UnitPointValue(bare) != 0 {
		t.Errorf("untyped unit PointValue=%d, want 0", w.UnitPointValue(bare))
	}

	// EDGE: zero / removed handle -> 0, no panic.
	if (Unit{}).PointValue() != 0 {
		t.Error("zero Unit PointValue() != 0")
	}
	u.Remove()
	if u.PointValue() != 0 || w.UnitPointValue(u.id) != 0 {
		t.Errorf("removed unit PointValue: api=%d sim=%d, want 0", u.PointValue(), w.UnitPointValue(u.id))
	}
}

// TestUnitDefaultStatsFSV: the Default{MoveSpeed,AcquireRange,TurnSpeed} getters
// read straight from the unit type's data row. SoT = the bound data.Unit fields
// (known inputs), verified both through the sim accessor and the api method, and
// against an exact hand-computed expectation (X+X=Y).
func TestUnitDefaultStatsFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	const turnPerTick = fixed.Angle(16384) // 1/4 turn per tick => π/2 rad/tick
	if !w.BindUnitDefs([]data.Unit{{
		ID:               "hspd",
		Life:             100,
		MoveSpeedPerTick: 8 * fixed.One,      // 8 u/tick => 160 u/s
		AcquisitionRange: fixed.FromInt(600), // 600 world units
		TurnRatePerTick:  turnPerTick,
	}}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}
	u := g.CreateUnit(owner, g.UnitType("hspd"), Vec2{X: 64, Y: 64}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit hspd invalid")
	}

	wantTurn := angleFromBrad(turnPerTick).Radians() * float64(data.TicksPerSecond)
	t.Logf("DefaultMoveSpeed api=%.4f (want 160)  DefaultAcquireRange api=%.4f (want 600)  DefaultTurnSpeed api=%.4f (want %.4f)",
		u.DefaultMoveSpeed(), u.DefaultAcquireRange(), u.DefaultTurnSpeed(), wantTurn)

	if got := u.DefaultMoveSpeed(); math.Abs(got-160) > 1e-6 {
		t.Errorf("DefaultMoveSpeed=%.6f, want 160", got)
	}
	if got := u.DefaultAcquireRange(); math.Abs(got-600) > 1e-6 {
		t.Errorf("DefaultAcquireRange=%.6f, want 600", got)
	}
	if got := u.DefaultTurnSpeed(); math.Abs(got-wantTurn) > 1e-6 {
		t.Errorf("DefaultTurnSpeed=%.6f, want %.6f", got, wantTurn)
	}
	// sim accessor agrees with the raw def fields (independent SoT path).
	if w.UnitDefaultMoveSpeed(u.id) != 8*fixed.One || w.UnitDefaultAcquireRange(u.id) != fixed.FromInt(600) || w.UnitDefaultTurnSpeed(u.id) != turnPerTick {
		t.Errorf("sim accessors disagree with def: ms=%d acq=%d turn=%d",
			w.UnitDefaultMoveSpeed(u.id), w.UnitDefaultAcquireRange(u.id), w.UnitDefaultTurnSpeed(u.id))
	}

	// Defaults are independent of instance mutation: change instance speed,
	// the default must NOT move.
	if w.Movements.Add(w.Ents, w.Transforms, u.id, fixed.FromInt(8), 0) {
		u.SetMoveSpeed(50)
		t.Logf("after SetMoveSpeed(50): instance=%.1f default=%.1f", u.MoveSpeed(), u.DefaultMoveSpeed())
		if math.Abs(u.DefaultMoveSpeed()-160) > 1e-6 {
			t.Errorf("default moved with instance: DefaultMoveSpeed=%.4f, want 160", u.DefaultMoveSpeed())
		}
	}

	// EDGE: untyped unit -> all zero.
	bare, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(8), Y: fixed.FromInt(8)}, 0)
	if !ok {
		t.Fatal("bare CreateUnit failed")
	}
	if w.UnitDefaultMoveSpeed(bare) != 0 || w.UnitDefaultAcquireRange(bare) != 0 || w.UnitDefaultTurnSpeed(bare) != 0 {
		t.Error("untyped unit default stats not all zero")
	}

	// EDGE: zero / removed handle -> 0.
	if (Unit{}).DefaultMoveSpeed() != 0 || (Unit{}).DefaultAcquireRange() != 0 || (Unit{}).DefaultTurnSpeed() != 0 {
		t.Error("zero Unit default stats != 0")
	}
	u.Remove()
	if u.DefaultMoveSpeed() != 0 || u.DefaultAcquireRange() != 0 || u.DefaultTurnSpeed() != 0 {
		t.Error("removed unit default stats != 0")
	}
}

// TestUnitShowHideFSV: Show/IsHidden round-trip through the sparse HiddenStore.
// SoT = the store row (w.Hiddens.Row / Count), read directly. Proves presence-
// is-the-signal: a row exists iff hidden, hide is idempotent, and Remove on the
// unit reclaims the row.
func TestUnitShowHideFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	// BEFORE: visible -> no store row.
	if w.Hiddens.Row(id) != -1 || u.IsHidden() {
		t.Fatalf("spawned hidden: row=%d getter=%v", w.Hiddens.Row(id), u.IsHidden())
	}
	t.Logf("BEFORE: row=%d getter=%v count=%d", w.Hiddens.Row(id), u.IsHidden(), w.Hiddens.Count())

	// Hide -> store row appears, getter true.
	u.Show(false)
	t.Logf("after Show(false): row=%d getter=%v count=%d", w.Hiddens.Row(id), u.IsHidden(), w.Hiddens.Count())
	if w.Hiddens.Row(id) == -1 || !u.IsHidden() {
		t.Errorf("hide: row=%d getter=%v, want present+true", w.Hiddens.Row(id), u.IsHidden())
	}

	// Hiding again is idempotent (no second row).
	cnt := w.Hiddens.Count()
	u.Show(false)
	if w.Hiddens.Count() != cnt {
		t.Errorf("double-hide grew count %d->%d", cnt, w.Hiddens.Count())
	}

	// Reveal -> row gone, getter false.
	u.Show(true)
	if w.Hiddens.Row(id) != -1 || u.IsHidden() {
		t.Errorf("reveal: row=%d getter=%v, want absent+false", w.Hiddens.Row(id), u.IsHidden())
	}
	// Revealing an already-visible unit is a harmless no-op.
	u.Show(true)
	if u.IsHidden() {
		t.Error("double-reveal made unit hidden")
	}

	// EDGE: zero handle -> false, no panic.
	if (Unit{}).IsHidden() {
		t.Error("zero Unit IsHidden() true")
	}
	(Unit{}).Show(false) // no panic

	// EDGE: Remove reclaims the hidden row (DestroyUnit path); dead handle no-op.
	u.Show(false)
	u.Remove()
	if w.Hiddens.Row(id) != -1 {
		t.Errorf("removed unit kept hidden row %d (DestroyUnit leak)", w.Hiddens.Row(id))
	}
	u.Show(false) // no-op on dead handle
	if u.IsHidden() {
		t.Error("removed unit IsHidden() true")
	}
}

// TestUnitIsTypeFSV: Unit.IsType classifies units by the structural UNIT_TYPE_*
// subset. SoT = the unit type's pathing/footprint (data) and life (Healths).
func TestUnitIsTypeFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hgnd", Life: 100, Pathing: data.PathingGround},
		{ID: "hair", Life: 100, Pathing: data.PathingAir},
		{ID: "hbld", Life: 100, Pathing: data.PathingGround, Footprint: 3},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}
	gnd := g.CreateUnit(owner, g.UnitType("hgnd"), Vec2{X: 64, Y: 64}, Deg(0))
	air := g.CreateUnit(owner, g.UnitType("hair"), Vec2{X: 96, Y: 96}, Deg(0))
	bld := g.CreateUnit(owner, g.UnitType("hbld"), Vec2{X: 200, Y: 200}, Deg(0))

	check := func(label string, u Unit, class UnitClass, want bool) {
		if got := u.IsType(class); got != want {
			t.Errorf("%s IsType(%d)=%v, want %v", label, class, got, want)
		}
	}
	// Ground unit: ground yes, flying/structure/hero no.
	check("gnd", gnd, ClassGround, true)
	check("gnd", gnd, ClassFlying, false)
	check("gnd", gnd, ClassStructure, false)
	check("gnd", gnd, ClassHero, false)
	// Flying unit: flying yes, ground no.
	check("air", air, ClassFlying, true)
	check("air", air, ClassGround, false)
	// Building: structure yes, and still ground-pathing.
	check("bld", bld, ClassStructure, true)
	check("bld", bld, ClassGround, true)
	check("bld", bld, ClassFlying, false)
	t.Logf("gnd: ground=%v flying=%v | air: flying=%v | bld: structure=%v ground=%v",
		gnd.IsType(ClassGround), gnd.IsType(ClassFlying), air.IsType(ClassFlying), bld.IsType(ClassStructure), bld.IsType(ClassGround))

	// Dead: a living unit is not Dead; zeroing its life flips it (SoT = Healths).
	check("gnd-alive", gnd, ClassDead, false)
	hr := w.Healths.Row(gnd.id)
	w.Healths.Life[hr] = 0
	check("gnd-life0", gnd, ClassDead, true)
	t.Logf("after life=0: Dead=%v Alive=%v", gnd.IsType(ClassDead), gnd.Alive())

	// EDGE: unrecognized class -> false; zero handle -> false.
	if gnd.IsType(UnitClass(200)) {
		t.Error("unknown class returned true")
	}
	if (Unit{}).IsType(ClassGround) {
		t.Error("zero Unit IsType true")
	}
}

// TestUnitIsTypeCombatFSV: the weapon-derived classes (melee/ranged/attacks-
// ground/attacks-flying). SoT = the type's Attacks (delivery + targets-allowed).
func TestUnitIsTypeCombatFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatalf("bind matrix: %v", err) // attacking types need a matrix to spawn
	}
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hmel", Life: 100, Attacks: []data.Attack{
			{Delivery: data.DeliveryInstant, TargetsAllowed: data.TargetGround,
				DamageBase: 10, CooldownTicks: 20}, // CooldownTicks>0 required by SetWeapon
		}},
		{ID: "harc", Life: 100, Attacks: []data.Attack{
			{Delivery: data.DeliveryProjectile, TargetsAllowed: data.TargetGround | data.TargetAir,
				DamageBase: 8, CooldownTicks: 25, ProjectileSpeedPerTick: fixed.One},
		}},
		{ID: "hunarmed", Life: 100}, // no attacks
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}
	mel := g.CreateUnit(owner, g.UnitType("hmel"), Vec2{X: 64, Y: 64}, Deg(0))
	if !mel.Valid() {
		t.Fatal("mel failed to spawn")
	}
	arc := g.CreateUnit(owner, g.UnitType("harc"), Vec2{X: 96, Y: 96}, Deg(0))
	un := g.CreateUnit(owner, g.UnitType("hunarmed"), Vec2{X: 128, Y: 128}, Deg(0))

	check := func(label string, u Unit, class UnitClass, want bool) {
		if got := u.IsType(class); got != want {
			t.Errorf("%s IsType(%d)=%v, want %v", label, class, got, want)
		}
	}
	// Melee, ground-only.
	check("mel", mel, ClassMelee, true)
	check("mel", mel, ClassRanged, false)
	check("mel", mel, ClassAttacksGround, true)
	check("mel", mel, ClassAttacksFlying, false)
	// Ranged, hits ground + air.
	check("arc", arc, ClassRanged, true)
	check("arc", arc, ClassMelee, false)
	check("arc", arc, ClassAttacksGround, true)
	check("arc", arc, ClassAttacksFlying, true)
	// Unarmed: none.
	check("unarmed", un, ClassMelee, false)
	check("unarmed", un, ClassRanged, false)
	check("unarmed", un, ClassAttacksGround, false)
	check("unarmed", un, ClassAttacksFlying, false)
	t.Logf("mel melee=%v atkAir=%v | arc ranged=%v atkAir=%v | unarmed melee=%v",
		mel.IsType(ClassMelee), mel.IsType(ClassAttacksFlying),
		arc.IsType(ClassRanged), arc.IsType(ClassAttacksFlying), un.IsType(ClassMelee))
}

// TestUnitInRangeFSV: InRange/InRangeOf test center-to-center distance with an
// inclusive boundary. SoT = the Transform positions (known 100-unit gap).
func TestUnitInRangeFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u1, _ := liveUnit(t, w, g, 0, 100) // spawns at (64,64)
	id2, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(164), Y: fixed.FromInt(64)}, 0)
	if !ok {
		t.Fatal("second unit spawn failed")
	}
	u2 := Unit{id: id2, g: g} // (164,64): exactly 100 from u1

	// Inclusive boundary at the exact distance.
	t.Logf("gap=100: InRange(100)=%v InRange(99)=%v InRange(101)=%v",
		u1.InRange(u2, 100), u1.InRange(u2, 99), u1.InRange(u2, 101))
	if !u1.InRange(u2, 100) {
		t.Error("InRange(100) false at exactly 100 (want inclusive true)")
	}
	if u1.InRange(u2, 99) {
		t.Error("InRange(99) true (gap is 100)")
	}
	if !u1.InRange(u2, 101) {
		t.Error("InRange(101) false")
	}
	// Symmetric.
	if u2.InRange(u1, 100) != u1.InRange(u2, 100) {
		t.Error("InRange not symmetric")
	}

	// InRangeOf a point: (64,164) is 100 from u1 (dy=100).
	if !u1.InRangeOf(Vec2{X: 64, Y: 164}, 100) || u1.InRangeOf(Vec2{X: 64, Y: 164}, 99) {
		t.Errorf("InRangeOf boundary wrong: 100=%v 99=%v", u1.InRangeOf(Vec2{X: 64, Y: 164}, 100), u1.InRangeOf(Vec2{X: 64, Y: 164}, 99))
	}

	// EDGE: negative distance -> false; invalid other -> false; zero handle.
	if u1.InRange(u2, -5) {
		t.Error("negative distance true")
	}
	if u1.InRange(Unit{}, 1000) {
		t.Error("InRange against invalid other true")
	}
	if (Unit{}).InRange(u1, 1000) || (Unit{}).InRangeOf(Vec2{}, 1000) {
		t.Error("zero handle InRange true")
	}
}

// TestUnitNameFSV: Unit.Name surfaces the unit type's proper name. SoT = the
// bound data.Unit.Name, via api + sim accessor.
func TestUnitNameFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hfoo", Life: 100, Name: "Footman"},
		{ID: "hnon", Life: 100}, // no name
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}

	foo := g.CreateUnit(owner, g.UnitType("hfoo"), Vec2{X: 64, Y: 64}, Deg(0))
	non := g.CreateUnit(owner, g.UnitType("hnon"), Vec2{X: 96, Y: 96}, Deg(0))
	t.Logf("hfoo name api=%q sim=%q ; hnon name api=%q", foo.Name(), w.UnitName(foo.id), non.Name())
	if foo.Name() != "Footman" || w.UnitName(foo.id) != "Footman" {
		t.Errorf("hfoo Name=%q, want Footman", foo.Name())
	}
	if non.Name() != "" {
		t.Errorf("hnon Name=%q, want empty", non.Name())
	}

	// Per-instance override (BlzSetUnitName) shadows the type name.
	foo.SetName("Sir Reginald")
	t.Logf("after SetName: api=%q sim=%q override-row=%d", foo.Name(), w.UnitName(foo.id), w.UnitNames.Row(foo.id))
	if foo.Name() != "Sir Reginald" || w.UnitName(foo.id) != "Sir Reginald" {
		t.Errorf("override Name=%q, want %q", foo.Name(), "Sir Reginald")
	}
	// An unnamed type can still be given an instance name.
	non.SetName("Nameless No More")
	if non.Name() != "Nameless No More" {
		t.Errorf("override on unnamed type=%q", non.Name())
	}

	// EDGE: untyped unit -> "".
	bare, _ := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(8), Y: fixed.FromInt(8)}, 0)
	if w.UnitName(bare) != "" {
		t.Errorf("untyped Name=%q, want empty", w.UnitName(bare))
	}
	// EDGE: zero / removed handle -> "".
	if (Unit{}).Name() != "" {
		t.Error("zero Unit Name() != empty")
	}
	foo.Remove()
	if foo.Name() != "" {
		t.Errorf("removed unit Name=%q, want empty", foo.Name())
	}
}

// TestUnitFoodFSV: FoodUsed/FoodMade surface the unit type's economy fields.
// SoT = the bound data.Unit FoodCost/FoodProvided, verified via api + sim
// accessor. A consumer (footman: uses 3, makes 0) and a provider (farm: uses 0,
// makes 10) prove the two fields don't cross-wire.
func TestUnitFoodFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	if !w.BindUnitDefs([]data.Unit{
		{ID: "hfoo", Life: 100, FoodCost: 3, FoodProvided: 0},
		{ID: "hfrm", Life: 100, FoodCost: 0, FoodProvided: 10},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	owner := Player{idx: 1, g: g}

	foo := g.CreateUnit(owner, g.UnitType("hfoo"), Vec2{X: 64, Y: 64}, Deg(0))
	frm := g.CreateUnit(owner, g.UnitType("hfrm"), Vec2{X: 96, Y: 96}, Deg(0))
	if !foo.Valid() || !frm.Valid() {
		t.Fatal("CreateUnit invalid")
	}
	t.Logf("footman: used api=%d sim=%d made api=%d sim=%d", foo.FoodUsed(), w.UnitFoodUsed(foo.id), foo.FoodMade(), w.UnitFoodMade(foo.id))
	t.Logf("farm:    used api=%d sim=%d made api=%d sim=%d", frm.FoodUsed(), w.UnitFoodUsed(frm.id), frm.FoodMade(), w.UnitFoodMade(frm.id))

	if foo.FoodUsed() != 3 || foo.FoodMade() != 0 || w.UnitFoodUsed(foo.id) != 3 || w.UnitFoodMade(foo.id) != 0 {
		t.Errorf("footman food: used=%d made=%d, want 3/0", foo.FoodUsed(), foo.FoodMade())
	}
	if frm.FoodUsed() != 0 || frm.FoodMade() != 10 || w.UnitFoodUsed(frm.id) != 0 || w.UnitFoodMade(frm.id) != 10 {
		t.Errorf("farm food: used=%d made=%d, want 0/10", frm.FoodUsed(), frm.FoodMade())
	}

	// EDGE: untyped unit -> 0/0.
	bare, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(8), Y: fixed.FromInt(8)}, 0)
	if !ok {
		t.Fatal("bare CreateUnit failed")
	}
	if w.UnitFoodUsed(bare) != 0 || w.UnitFoodMade(bare) != 0 {
		t.Errorf("untyped food: used=%d made=%d, want 0/0", w.UnitFoodUsed(bare), w.UnitFoodMade(bare))
	}

	// EDGE: zero / removed handle -> 0.
	if (Unit{}).FoodUsed() != 0 || (Unit{}).FoodMade() != 0 {
		t.Error("zero Unit food != 0")
	}
	foo.Remove()
	if foo.FoodUsed() != 0 || foo.FoodMade() != 0 {
		t.Errorf("removed unit food: used=%d made=%d, want 0/0", foo.FoodUsed(), foo.FoodMade())
	}
}

// TestUnitInventorySizeFSV: InventorySize reflects whether the unit has an
// inventory row. SoT = the Invents store presence (Row != -1). The engine
// models inventory all-or-nothing, so the size is 6 with a row, 0 without.
func TestUnitInventorySizeFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	// BEFORE: no inventory row -> size 0.
	t.Logf("BEFORE: invRow=%d size=%d", w.Invents.Row(id), u.InventorySize())
	if w.Invents.Row(id) != -1 || u.InventorySize() != 0 {
		t.Fatalf("no-inventory size=%d (row=%d), want 0", u.InventorySize(), w.Invents.Row(id))
	}

	// Grant an inventory -> size becomes the full six slots.
	if !w.Invents.Add(w.Ents, id) {
		t.Fatal("Invents.Add failed")
	}
	t.Logf("AFTER Add: invRow=%d size=%d (InventorySlots=%d)", w.Invents.Row(id), u.InventorySize(), sim.InventorySlots)
	if u.InventorySize() != sim.InventorySlots {
		t.Errorf("with-inventory size=%d, want %d", u.InventorySize(), sim.InventorySlots)
	}

	// EDGE: zero / removed handle -> 0.
	if (Unit{}).InventorySize() != 0 {
		t.Error("zero Unit InventorySize() != 0")
	}
	u.Remove()
	if w.Invents.Row(id) != -1 || u.InventorySize() != 0 {
		t.Errorf("removed unit InventorySize=%d (row=%d), want 0", u.InventorySize(), w.Invents.Row(id))
	}
}

// TestUnitHeroStatsContract: the api hero getters delegate to the sim accessors
// and honor the handle contract. The hero-present SoT is proven in the sim
// package (TestHeroStatAccessorsFSV, distinct attrs catch cross-wiring); here we
// verify a plain (non-hero) unit and invalid/removed handles all read 0/false.
func TestUnitHeroStatsContract(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, _ := liveUnit(t, w, g, 0, 100)

	t.Logf("non-hero: isHero=%v level=%d xp=%d str=%d agi=%d int=%d",
		u.IsHero(), u.HeroLevel(), u.HeroXP(), u.Strength(), u.Agility(), u.Intelligence())
	if u.IsHero() || u.HeroLevel() != 0 || u.HeroXP() != 0 || u.Strength() != 0 || u.Agility() != 0 || u.Intelligence() != 0 {
		t.Error("non-hero unit reported hero state")
	}

	// EDGE: zero handle -> all zero/false, no panic.
	z := Unit{}
	if z.IsHero() || z.HeroLevel() != 0 || z.HeroXP() != 0 || z.Strength() != 0 || z.Agility() != 0 || z.Intelligence() != 0 {
		t.Error("zero Unit reported hero state")
	}

	// Setters on a non-hero are safe no-ops (no panic; nothing to change).
	u.SetStrength(50)
	u.SetAgility(50)
	u.SetIntelligence(50)
	if u.Strength() != 0 || u.Agility() != 0 || u.Intelligence() != 0 {
		t.Error("hero setters mutated a non-hero")
	}

	// Skill points / XP on a non-hero: 0 / false / no-op.
	u.AddExperience(500)
	if u.SkillPoints() != 0 || u.ModifySkillPoints(3) {
		t.Error("non-hero accepted skill-point / XP ops")
	}

	// XP suspension on a non-hero: false getter, setter no-op.
	u.SuspendExperience(true)
	if u.ExperienceSuspended() {
		t.Error("non-hero accepted XP suspension")
	}

	// XP/level setters on a non-hero: no-op (level/xp stay 0).
	u.SetHeroXP(500)
	u.SetHeroLevel(3)
	if u.HeroXP() != 0 || u.HeroLevel() != 0 {
		t.Error("hero XP/level setters mutated a non-hero")
	}

	// EDGE: removed unit -> all zero/false, setters no-op.
	u.Remove()
	z.SetStrength(1) // zero handle, no panic
	if u.IsHero() || u.HeroLevel() != 0 || u.Strength() != 0 {
		t.Error("removed unit reported hero state")
	}
	u.SetStrength(1) // removed handle, no panic
}
