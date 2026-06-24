package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// #563 — group query-fills. SoT = the filled group's Members span,
// dumped via members(s, g) after each fill.

func qSpawn(t *testing.T, w *World, player, team uint8, typeID uint16, cx, cy int32, fly bool) EntityID {
	t.Helper()
	flags := uint8(0)
	if fly {
		flags = PathAir
	}
	id, ok := w.CreateUnit(CellCenter(cy*path.GridSize+cx), 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, player, team, player) ||
		!w.UnitTypes.Add(w.Ents, id, typeID) ||
		!w.Collisions.Add(w.Ents, id, 1, flags) ||
		!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) {
		t.Fatalf("spawn failed id=%d", id)
	}
	return id
}

func groupHas(s *GroupStore, g GroupID, id EntityID) bool {
	for _, m := range members(s, g) {
		if m == id {
			return true
		}
	}
	return false
}

// queryWorld: type 1 = ground, 2 = air (flying), 3 = structure.
// u0 p0/t1 ground, u1 p1/t1 ground (enemy of p0), u2 p0/t2 flying — all
// near cell (1,1); u3 p0/t3 structure, far at cell (60,60).
func queryWorld(t *testing.T) (*World, [4]EntityID) {
	w := NewWorld(Caps{})
	defs := make([]data.Unit, 4)
	defs[1].Pathing = data.PathingGround
	defs[2].Pathing = data.PathingAir
	defs[3].Footprint = 2 // > 0 ⇒ structure
	if !w.BindUnitDefs(defs) {
		t.Fatal("BindUnitDefs failed")
	}
	var u [4]EntityID
	u[0] = qSpawn(t, w, 0, 0, 1, 1, 1, false)
	u[1] = qSpawn(t, w, 1, 1, 1, 1, 2, false)
	u[2] = qSpawn(t, w, 0, 0, 2, 2, 2, true)
	u[3] = qSpawn(t, w, 0, 0, 3, 60, 60, false)
	return w, u
}

func TestGroupFillRadius(t *testing.T) {
	w, u := queryWorld(t)
	center := w.Transforms.Pos[w.Transforms.Row(u[0])]
	g := w.Groups.CreateGroup()
	n := w.GroupFillRadius(g, center, 200*fixed.One, QueryMask{})
	if n != 3 {
		t.Fatalf("radius fill n=%d, want 3 (u0,u1,u2; u3 far)", n)
	}
	if groupHas(w.Groups, g, u[3]) {
		t.Fatal("far unit u3 in radius fill")
	}
	for _, id := range []EntityID{u[0], u[1], u[2]} {
		if !groupHas(w.Groups, g, id) {
			t.Fatalf("near unit %d missing from radius fill", id)
		}
	}
}

func TestGroupFillRadiusEnemyMask(t *testing.T) {
	w, u := queryWorld(t)
	center := w.Transforms.Pos[w.Transforms.Row(u[0])]
	g := w.Groups.CreateGroup()
	n := w.GroupFillRadius(g, center, 200*fixed.One, QueryMask{OfPlayer: 0, Enemy: true})
	if n != 1 || !groupHas(w.Groups, g, u[1]) {
		t.Fatalf("enemy mask n=%d, want only u1 (p1 enemy of p0)", n)
	}
}

func TestGroupFillRadiusExcludeFlying(t *testing.T) {
	w, u := queryWorld(t)
	center := w.Transforms.Pos[w.Transforms.Row(u[0])]
	g := w.Groups.CreateGroup()
	n := w.GroupFillRadius(g, center, 200*fixed.One, QueryMask{ExcludeFlying: true})
	if n != 2 || groupHas(w.Groups, g, u[2]) {
		t.Fatalf("exclude-flying n=%d, want 2 (u2 flying dropped)", n)
	}
}

func TestGroupFillRadiusMax(t *testing.T) {
	w, u := queryWorld(t)
	center := w.Transforms.Pos[w.Transforms.Row(u[0])]
	g := w.Groups.CreateGroup()
	n := w.GroupFillRadius(g, center, 200*fixed.One, QueryMask{Max: 2})
	if n != 2 {
		t.Fatalf("Max=2 fill n=%d, want 2", n)
	}
	// Deterministic truncation keeps the first two in ascending id order.
	got := members(w.Groups, g)
	if got[0] != u[0] || got[1] != u[1] {
		t.Fatalf("Max truncation = %v, want [u0 u1] (lowest ids)", got)
	}
}

func TestGroupFillOwner(t *testing.T) {
	w, u := queryWorld(t)
	g := w.Groups.CreateGroup()
	n := w.GroupFillOwner(g, 0, QueryMask{})
	if n != 3 {
		t.Fatalf("owner-0 fill n=%d, want 3 (u0,u2,u3)", n)
	}
	if groupHas(w.Groups, g, u[1]) {
		t.Fatal("p1 unit in owner-0 fill")
	}
	// StructuresOnly narrows the same owner enumeration to the building.
	g2 := w.Groups.CreateGroup()
	if n := w.GroupFillOwner(g2, 0, QueryMask{StructuresOnly: true}); n != 1 || !groupHas(w.Groups, g2, u[3]) {
		t.Fatalf("structures-only owner-0 fill n=%d, want only u3", n)
	}
}

func TestGroupFillType(t *testing.T) {
	w, u := queryWorld(t)
	g := w.Groups.CreateGroup()
	n := w.GroupFillType(g, 1, QueryMask{})
	if n != 2 || !groupHas(w.Groups, g, u[0]) || !groupHas(w.Groups, g, u[1]) {
		t.Fatalf("type-1 fill n=%d, want u0+u1", n)
	}
}

func TestGroupFillRectAndEmpty(t *testing.T) {
	w, _ := queryWorld(t)
	g := w.Groups.CreateGroup()
	// Rect far from everything → empty fill (and clears prior contents).
	w.Groups.GroupAdd(g, ent(777))
	n := w.GroupFillRect(g, 100000*fixed.One, 100000*fixed.One, 100100*fixed.One, 100100*fixed.One, QueryMask{})
	if n != 0 || w.Groups.GroupCount(g) != 0 {
		t.Fatalf("empty rect fill n=%d count=%d, want 0/0 (and prior member cleared)", n, w.Groups.GroupCount(g))
	}
}

func TestGroupFillStaleGroup(t *testing.T) {
	w, u := queryWorld(t)
	g := w.Groups.CreateGroup()
	w.Groups.DestroyGroup(g)
	center := w.Transforms.Pos[w.Transforms.Row(u[0])]
	if n := w.GroupFillRadius(g, center, 200*fixed.One, QueryMask{}); n != 0 {
		t.Fatalf("fill into stale group n=%d, want 0", n)
	}
}

func TestGroupFillZeroAlloc(t *testing.T) {
	w, u := queryWorld(t)
	center := w.Transforms.Pos[w.Transforms.Row(u[0])]
	g := w.Groups.CreateGroup()
	avg := testing.AllocsPerRun(200, func() {
		w.GroupFillRadius(g, center, 200*fixed.One, QueryMask{})
	})
	if avg != 0 {
		t.Fatalf("radius fill allocated %.2f objs/op, want 0", avg)
	}
}
