package sim

import (
	"os"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// The wire-registry guard: data.OpcodeByName must agree with the
// sim's Op* constants — the two lists encode the same input.md §8
// contract and must never drift.
func TestSmartOrderRegistryAgreement(t *testing.T) {
	want := map[string]uint8{
		"move": OpMove, "attack": OpAttack, "stop": OpStop, "hold": OpHold,
		"patrol": OpPatrol, "cast-ability": OpCastAbility, "train": OpTrain,
		"build": OpBuild, "cancel": OpCancel, "rally": OpRally,
		"harvest": OpHarvest, "repair": OpRepair, "board": OpBoard, "unload": OpUnload,
		"get-item": OpGetItem, "follow": OpFollow, "resume-construction": OpResumeConstruction,
	}
	if len(want) != len(data.OpcodeByName) {
		t.Fatalf("registry size drift: sim %d vs data %d", len(want), len(data.OpcodeByName))
	}
	for name, op := range want {
		got, ok := data.OpcodeByName[name]
		t.Logf("%-14s sim=%d data=%d", name, op, got)
		if !ok || got != op {
			t.Fatalf("registry drift at %q: sim %d, data %d (ok=%v)", name, op, got, ok)
		}
	}
}

// smartWorld loads the SHIPPED table and binds typeID 0 → fighter,
// typeID 1 → worker.
func smartWorld(t *testing.T) (*World, *data.SmartTable) {
	t.Helper()
	tables, err := data.Load(os.DirFS("../../data"))
	if err != nil {
		t.Fatalf("starter data must load: %v", err)
	}
	if tables.Smart == nil {
		t.Fatal("data/orders/smart.toml must be present")
	}
	w := NewWorld(Caps{})
	if !w.BindSmartTable(tables.Smart, []uint8{0, 1}) {
		t.Fatal("BindSmartTable failed")
	}
	return w, tables.Smart
}

// smartUnit spawns a unit owned by (player, team) with a TypeID.
func smartUnit(t *testing.T, w *World, player, team uint8, typeID uint16) EntityID {
	t.Helper()
	id, ok := w.CreateUnit(fixed.Vec2{X: 100 * fixed.One, Y: 100 * fixed.One}, 0)
	if !ok || !w.Owners.Add(w.Ents, id, player, team, player) || !w.UnitTypes.Add(w.Ents, id, typeID) {
		t.Fatal("smart unit setup failed")
	}
	return id
}

// The full resolution matrix: every target class × both shipped unit
// classes, expected vs got (input.md §9.2 acceptance row).
func TestSmartOrderMatrix(t *testing.T) {
	w, table := smartWorld(t)
	fighter := smartUnit(t, w, 0, 0, 0) // class 0
	worker := smartUnit(t, w, 0, 0, 1)  // class 1
	units := []struct {
		name string
		id   EntityID
	}{{"fighter", fighter}, {"worker", worker}}

	// expected matrix — the §2.2 baseline within the opcode registry
	// (follow is not an opcode yet → move; items resolve to get-item
	// since #305; see table comment + the spec-gap discovery issue)
	want := [data.TargetClassCount][2]uint8{
		data.TCGroundPoint:  {OpMove, OpMove},
		data.TCEnemy:        {OpAttack, OpAttack},
		data.TCAlly:         {OpFollow, OpFollow},
		data.TCTransport:    {OpBoard, OpBoard},
		data.TCOwnBuilding:  {OpRally, OpRally},
		data.TCDamagedOwn:   {OpMove, OpRepair},
		data.TCConstruction: {OpMove, OpResumeConstruction},
		data.TCResource:     {OpMove, OpHarvest},
		data.TCItem:         {OpGetItem, OpGetItem},
	}
	pt := fixed.Vec2{X: 500 * fixed.One, Y: 500 * fixed.One}
	for tc := uint8(0); tc < data.TargetClassCount; tc++ {
		for ui, u := range units {
			cmds, ok := w.ResolveSmartClass(tc, []EntityID{u.id}, 0, pt)
			if !ok || len(cmds) != 1 {
				t.Fatalf("%s × %s: resolution failed (ok=%v n=%d)",
					data.TargetClassNames[tc], u.name, ok, len(cmds))
			}
			got := cmds[0].Opcode
			t.Logf("%-13s × %-7s → opcode %2d (want %2d) %s",
				data.TargetClassNames[tc], u.name, got, want[tc][ui],
				map[bool]string{true: "✓", false: "✗"}[got == want[tc][ui]])
			if got != want[tc][ui] {
				t.Fatalf("%s × %s: got opcode %d want %d",
					data.TargetClassNames[tc], u.name, got, want[tc][ui])
			}
		}
	}
	_ = table
}

// Edge 2: a mixed worker+fighter selection on a resource splits into
// one command per resolved opcode — fighters move, workers harvest —
// units in selection order, commands by first appearance.
func TestSmartOrderMixedSelection(t *testing.T) {
	w, _ := smartWorld(t)
	f1 := smartUnit(t, w, 0, 0, 0)
	wk := smartUnit(t, w, 0, 0, 1)
	f2 := smartUnit(t, w, 0, 0, 0)
	pt := fixed.Vec2{X: 800 * fixed.One, Y: 800 * fixed.One}
	cmds, ok := w.ResolveSmartClass(data.TCResource, []EntityID{f1, wk, f2}, 0, pt)
	if !ok {
		t.Fatal("mixed resolution failed")
	}
	for i, c := range cmds {
		t.Logf("command %d: opcode=%d units=%v", i, c.Opcode, c.Units)
	}
	if len(cmds) != 2 {
		t.Fatalf("want 2 split commands, got %d", len(cmds))
	}
	if cmds[0].Opcode != OpMove || len(cmds[0].Units) != 2 ||
		cmds[0].Units[0] != f1 || cmds[0].Units[1] != f2 {
		t.Fatalf("move command wrong: %+v", cmds[0])
	}
	if cmds[1].Opcode != OpHarvest || len(cmds[1].Units) != 1 || cmds[1].Units[0] != wk {
		t.Fatalf("harvest command wrong: %+v", cmds[1])
	}
	// per-unit verdicts (issue FSV edge 2)
	t.Logf("per-unit: fighter %d → move; worker %d → harvest; fighter %d → move", f1, wk, f2)
}

// Classification over live sim state: ground point, ally (same team),
// enemy (other team).
func TestSmartOrderClassify(t *testing.T) {
	w, _ := smartWorld(t)
	ally := smartUnit(t, w, 1, 0, 0)  // other player, same team
	enemy := smartUnit(t, w, 2, 1, 0) // other team
	cases := []struct {
		name   string
		target EntityID
		want   uint8
	}{
		{"no target → ground point", 0, data.TCGroundPoint},
		{"same team → ally", ally, data.TCAlly},
		{"other team → enemy", enemy, data.TCEnemy},
	}
	for _, c := range cases {
		got, ok := w.ClassifyTarget(0, c.target)
		t.Logf("%-26s → class %d (%s), ok=%v", c.name, got, data.TargetClassNames[got], ok)
		if !ok || got != c.want {
			t.Fatalf("%s: got %d ok=%v, want %d", c.name, got, ok, c.want)
		}
	}
}

// Edge 3: a target that died between click and ingest resolves to
// NOTHING — the deterministic no-op. State hash provably unchanged
// versus a control world that never saw the click.
func TestSmartOrderDeadTargetNoOp(t *testing.T) {
	build := func() (*World, EntityID, EntityID) {
		w, _ := smartWorld(t)
		me := smartUnit(t, w, 0, 0, 0)
		foe := smartUnit(t, w, 2, 1, 0)
		w.Orders.Add(w.Ents, me)
		return w, me, foe
	}
	w1, me1, foe1 := build()
	w2, _, foe2 := build()

	// the enemy dies before ingest on BOTH worlds
	w1.KillUnit(foe1)
	w2.KillUnit(foe2)
	w1.Step()
	w2.Step()

	// world 1 resolves the stale click; world 2 never saw it
	cmds, ok := w1.ResolveSmart(0, []EntityID{me1}, foe1, fixed.Vec2{})
	t.Logf("resolution against dead target %d: ok=%v cmds=%v (want ok=false, no commands)", foe1, ok, cmds)
	if ok || cmds != nil {
		t.Fatalf("dead target must resolve to the no-op")
	}
	for i := 0; i < 5; i++ {
		w1.Step()
		w2.Step()
	}
	h1, h2 := hashOrders(w1), hashOrders(w2)
	t.Logf("order-state hash after 5 ticks: resolved-world=%016x control=%016x (must be equal)", h1, h2)
	if h1 != h2 {
		t.Fatalf("dead-target resolution mutated state: %016x vs %016x", h1, h2)
	}
}

// Dead units WITHIN the selection are skipped; an all-dead selection
// resolves to nothing.
func TestSmartOrderDeadSelection(t *testing.T) {
	w, _ := smartWorld(t)
	a := smartUnit(t, w, 0, 0, 0)
	b := smartUnit(t, w, 0, 0, 1)
	w.KillUnit(a)
	w.Step()
	cmds, ok := w.ResolveSmartClass(data.TCResource, []EntityID{a, b}, 0, fixed.Vec2{X: fixed.One, Y: fixed.One})
	if !ok || len(cmds) != 1 || cmds[0].Opcode != OpHarvest || len(cmds[0].Units) != 1 || cmds[0].Units[0] != b {
		t.Fatalf("dead selection member must be skipped: ok=%v %+v", ok, cmds)
	}
	t.Logf("selection [dead %d, worker %d] → 1 command: harvest %v", a, b, cmds[0].Units)
	cmds, ok = w.ResolveSmartClass(data.TCResource, []EntityID{a}, 0, fixed.Vec2{})
	if ok || cmds != nil {
		t.Fatalf("all-dead selection must resolve to nothing")
	}
	t.Logf("all-dead selection → no-op ✓")
}
