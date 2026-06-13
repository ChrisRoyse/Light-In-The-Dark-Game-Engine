package sim

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func visGrid() *path.Grid {
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, path.Walkable|path.Buildable|path.Flyable)
		}
	}
	return g
}

func visWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{Units: 64})
	w.SetGrid(visGrid())
	defs := []data.Unit{
		{ID: "scout", Life: 100, SightDay: fixed.FromInt(360), SightNight: fixed.FromInt(160), CollisionSize: 16, Pathing: data.PathingGround},
		{ID: "air-eye", Life: 100, SightDay: fixed.FromInt(360), SightNight: fixed.FromInt(160), CollisionSize: 16, Pathing: data.PathingAir},
		{ID: "tower", Life: 500, Footprint: 4, BuildTicks: 1},
	}
	if !w.BindUnitDefs(defs) {
		t.Fatal("BindUnitDefs failed")
	}
	w.SetTimeOfDay(12 * fixed.One)
	w.SuspendTimeOfDay(true)
	return w
}

func spawnVisUnit(t *testing.T, w *World, player, team uint8, typeID uint16, cellX, cellY int32, pathFlags uint8) EntityID {
	t.Helper()
	id, ok := spawnVisUnitRaw(w, player, team, typeID, cellX, cellY, pathFlags)
	if !ok {
		t.Fatalf("spawn visibility unit failed id=%d", id)
	}
	return id
}

func spawnVisUnitRaw(w *World, player, team uint8, typeID uint16, cellX, cellY int32, pathFlags uint8) (EntityID, bool) {
	id, ok := w.CreateUnit(CellCenter(cellY*path.GridSize+cellX), 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, player, team, player) ||
		!w.UnitTypes.Add(w.Ents, id, typeID) ||
		!w.Collisions.Add(w.Ents, id, 1, pathFlags) ||
		!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) {
		return id, false
	}
	return id, true
}

func spawnVisBuilding(t *testing.T, w *World, player, team uint8, typeID uint16, cellX, cellY int32) EntityID {
	t.Helper()
	id := spawnVisUnit(t, w, player, team, typeID, cellX, cellY, PathBuild)
	if r := w.Build.add(w.Ents, id, cellX-2, cellY-2, 4); r == -1 {
		t.Fatal("Build.add failed")
	}
	return id
}

func visMove(t *testing.T, w *World, id EntityID, cellX, cellY int32) {
	t.Helper()
	r := w.Transforms.Row(id)
	if r == -1 {
		t.Fatalf("entity %d has no transform", id)
	}
	w.Transforms.Pos[r] = CellCenter(cellY*path.GridSize + cellX)
	w.bucketRemove(id)
	w.bucketInsert(id, w.Transforms.Pos[r])
}

func fogOf(cellX, cellY int32) (int32, int32) {
	return cellX / FogCellPathingSize, cellY / FogCellPathingSize
}

func visStateLine(w *World, player uint8, label string, cells ...[2]int32) string {
	out := label
	for _, c := range cells {
		fx, fy := fogOf(c[0], c[1])
		out += fmt.Sprintf(" cell(%d,%d)/fog(%d,%d)=%d", c[0], c[1], fx, fy, w.FogStateAt(player, fx, fy))
	}
	return out
}

func lastSeenLine(w *World, player uint8, id EntityID) string {
	rec, ok := w.Visibility.LastSeen(player, id)
	if !ok {
		return fmt.Sprintf("lastSeen[p%d,id=%d]=<none> count=%d", player, id, w.Visibility.lastSeenCount[player])
	}
	return fmt.Sprintf("lastSeen[p%d,id=%d]={type=%d owner=%d pos=(%d,%d)} count=%d",
		player, id, rec.TypeID, rec.Owner, rec.Pos.X.Floor(), rec.Pos.Y.Floor(), w.Visibility.lastSeenCount[player])
}

func TestVisibilityTransitionsHashAndSave(t *testing.T) {
	w := visWorld(t)
	scout := spawnVisUnit(t, w, 0, 0, 0, 40, 40, PathGround)
	origin := [2]int32{40, 40}
	far := [2]int32{80, 80}

	before := visStateLine(w, 0, "BEFORE", origin, far)
	w.RecomputeVisibility()
	afterSee := visStateLine(w, 0, "AFTER see", origin, far)
	visMove(t, w, scout, far[0], far[1])
	w.RecomputeVisibility()
	afterLeave := visStateLine(w, 0, "AFTER leave", origin, far)
	t.Logf("FSV visibility transition: %s", before)
	t.Logf("FSV visibility transition: %s", afterSee)
	t.Logf("FSV visibility transition: %s", afterLeave)
	ox, oy := fogOf(origin[0], origin[1])
	fx, fy := fogOf(far[0], far[1])
	if w.FogStateAt(0, ox, oy) != FogExplored || w.FogStateAt(0, fx, fy) != FogVisible {
		t.Fatalf("transition failed: %s", afterLeave)
	}

	reg := NewHashRegistry()
	var beforeHash, afterHash statehash.Snapshot
	w.HashState(reg, &beforeHash)
	idx := hashSystemIndex(t, "visibility")
	w.Visibility.setStateCell(0, fogCellIndex(ox, oy), FogHidden)
	w.HashState(reg, &afterHash)
	t.Logf("FSV visibility hash BEFORE top=%016x visibility=%016x", beforeHash.Top, beforeHash.Subs[idx])
	t.Logf("FSV visibility hash AFTER mutate top=%016x visibility=%016x", afterHash.Top, afterHash.Subs[idx])
	if beforeHash.Subs[idx] == afterHash.Subs[idx] {
		t.Fatal("visibility mutation invisible to state hash")
	}
	w.Visibility.setStateCell(0, fogCellIndex(ox, oy), FogExplored)

	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	dst := visWorld(t)
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatal(err)
	}
	var loaded statehash.Snapshot
	dst.HashState(reg, &loaded)
	t.Logf("FSV visibility save bytes=%d BEFORE top=%016x visibility=%016x", buf.Len(), beforeHash.Top, beforeHash.Subs[idx])
	t.Logf("FSV visibility load AFTER top=%016x visibility=%016x %s", loaded.Top, loaded.Subs[idx], visStateLine(dst, 0, "loaded", origin, far))
	if loaded.Top != beforeHash.Top {
		t.Fatalf("visibility save/load hash mismatch: %v", snapDiff(t, &beforeHash, &loaded))
	}
}

func TestVisibilityCliffLOS(t *testing.T) {
	w := visWorld(t)
	low := [2]int32{40, 40}
	high := [2]int32{48, 40}
	hfx, hfy := fogOf(high[0], high[1])
	for y := hfy * FogCellPathingSize; y < (hfy+1)*FogCellPathingSize; y++ {
		for x := hfx * FogCellPathingSize; x < (hfx+1)*FogCellPathingSize; x++ {
			w.Grid.SetCliffLevel(x, y, 1)
		}
	}
	scout := spawnVisUnit(t, w, 0, 0, 0, low[0], low[1], PathGround)
	w.RecomputeVisibility()
	lowToHigh := visStateLine(w, 0, "ground low", low, high)
	visMove(t, w, scout, high[0], high[1])
	w.RecomputeVisibility()
	highToLow := visStateLine(w, 0, "ground high", low, high)
	air := spawnVisUnit(t, w, 1, 1, 1, low[0], low[1], PathAir)
	_ = air
	w.RecomputeVisibility()
	airToHigh := visStateLine(w, 1, "air low", low, high)
	t.Logf("FSV cliff LOS: %s", lowToHigh)
	t.Logf("FSV cliff LOS: %s", highToLow)
	t.Logf("FSV cliff LOS: %s", airToHigh)
	if w.FogStateAt(0, hfx, hfy) != FogVisible {
		t.Fatalf("high-ground source should see its own high cell: %s", highToLow)
	}
	visMove(t, w, scout, low[0], low[1])
	w.RecomputeVisibility()
	if w.FogStateAt(0, hfx, hfy) == FogVisible {
		t.Fatalf("low-ground source saw up-cliff cell: %s", visStateLine(w, 0, "ground low retry", low, high))
	}
	if w.FogStateAt(1, hfx, hfy) != FogVisible {
		t.Fatalf("air source should see over cliff: %s", airToHigh)
	}
}

func TestVisibilityInvisibleRequiresTrueSight(t *testing.T) {
	w := visWorld(t)
	scout := spawnVisUnit(t, w, 0, 0, 0, 40, 40, PathGround)
	target := spawnVisUnit(t, w, 1, 1, 0, 42, 40, PathGround)
	if !w.SetVisibilityFlags(target, VisibilityInvisible) {
		t.Fatal("SetVisibilityFlags invisible failed")
	}
	w.RecomputeVisibility()
	noTrueSight := w.CanSeeEntity(0, target)
	if !w.SetVisibilityFlags(scout, VisibilityTrueSight) {
		t.Fatal("SetVisibilityFlags true sight failed")
	}
	withTrueSight := w.CanSeeEntity(0, target)
	t.Logf("FSV invisibility: cell=%s targetFlags=%02x scoutFlags=%02x noTrueSight=%v withTrueSight=%v",
		visStateLine(w, 0, "visible cell", [2]int32{42, 40}), w.VisibilityFlags(target), w.VisibilityFlags(scout), noTrueSight, withTrueSight)
	if noTrueSight || !withTrueSight {
		t.Fatalf("invisibility/true-sight detectability wrong: no=%v yes=%v", noTrueSight, withTrueSight)
	}
}

func TestVisibilityGatesAcquisition(t *testing.T) {
	w := visWorld(t)
	scout := spawnVisUnit(t, w, 0, 0, 0, 40, 40, PathGround)
	target := spawnVisUnit(t, w, 1, 1, 0, 42, 40, PathGround)
	if !w.Combats.Add(w.Ents, scout) {
		t.Fatal("scanner combat add failed")
	}
	cr := w.Combats.Row(scout)
	w.Combats.AcquisitionRange[cr] = 500 * fixed.One
	beforeGrid := visStateLine(w, 0, "hidden-before", [2]int32{42, 40})
	hidden := w.acquireScan(cr, scout)
	w.RecomputeVisibility()
	afterGrid := visStateLine(w, 0, "visible-after", [2]int32{42, 40})
	visible := w.acquireScan(cr, scout)
	if !w.SetVisibilityFlags(target, VisibilityInvisible) {
		t.Fatal("target invisible flag failed")
	}
	invisible := w.acquireScan(cr, scout)
	if !w.SetVisibilityFlags(scout, VisibilityTrueSight) {
		t.Fatal("scanner true-sight flag failed")
	}
	detected := w.acquireScan(cr, scout)
	t.Logf("FSV acquisition gate: BEFORE hidden target=%d grid=%s", hidden, beforeGrid)
	t.Logf("FSV acquisition gate: AFTER visible target=%d invisible=%d detected=%d targetFlags=%02x scoutFlags=%02x",
		visible, invisible, detected, w.VisibilityFlags(target), w.VisibilityFlags(scout))
	t.Logf("FSV acquisition gate: AFTER grid=%s", afterGrid)
	if hidden != 0 || visible != target || invisible != 0 || detected != target {
		t.Fatalf("visibility acquisition gate failed: hidden=%d visible=%d invisible=%d detected=%d target=%d",
			hidden, visible, invisible, detected, target)
	}
}

func TestVisibilityLastSeenBuildingGhost(t *testing.T) {
	w := visWorld(t)
	scout := spawnVisUnit(t, w, 0, 0, 0, 40, 40, PathGround)
	building := spawnVisBuilding(t, w, 1, 1, 2, 42, 40)
	w.RecomputeVisibility()
	seen := lastSeenLine(w, 0, building)
	visMove(t, w, scout, 90, 90)
	w.RecomputeVisibility()
	left := lastSeenLine(w, 0, building)
	if !w.DestroyUnit(building) {
		t.Fatal("DestroyUnit building failed")
	}
	w.RecomputeVisibility()
	destroyedAway := lastSeenLine(w, 0, building)
	visMove(t, w, scout, 42, 40)
	w.RecomputeVisibility()
	rescout := lastSeenLine(w, 0, building)
	t.Logf("FSV building ghost SEE:       %s %s", visStateLine(w, 0, "grid", [2]int32{42, 40}), seen)
	t.Logf("FSV building ghost LEAVE:     %s", left)
	t.Logf("FSV building ghost DESTROYED: %s", destroyedAway)
	t.Logf("FSV building ghost RESCOUT:   %s", rescout)
	if _, ok := w.Visibility.LastSeen(0, building); ok {
		t.Fatalf("destroyed building ghost did not clear on re-scout: %s", rescout)
	}
}

func TestVisibilityUpdateAllocFree(t *testing.T) {
	w := visWorld(t)
	for i := int32(0); i < 24; i++ {
		spawnVisUnit(t, w, uint8(i%4), uint8(i%4), 0, 20+i, 20+(i%6), PathGround)
	}
	w.RecomputeVisibility()
	allocs := testing.AllocsPerRun(100, func() {
		w.tick++
		w.visibilitySystem()
	})
	t.Logf("FSV visibility update allocs/op=%v interval=%d stateBytes=%d cycleBytes=%d", allocs, w.Visibility.Interval(), len(w.Visibility.state), len(w.Visibility.cycle))
	if allocs != 0 {
		t.Fatalf("visibility update allocates %v/op", allocs)
	}
}

func BenchmarkVisibilityUpdate(b *testing.B) {
	w := NewWorld(Caps{Units: 1000})
	w.SetGrid(visGrid())
	w.BindUnitDefs([]data.Unit{{ID: "scout", Life: 100, SightDay: fixed.FromInt(360), SightNight: fixed.FromInt(160)}})
	for i := int32(0); i < 1000; i++ {
		x := 8 + (i % 100)
		y := 8 + ((i / 100) * 8)
		if _, ok := spawnVisUnitRaw(w, uint8(i%MaxPlayers), uint8(i%MaxPlayers), 0, x, y, PathGround); !ok {
			b.Fatal("spawnVisUnitRaw failed")
		}
	}
	w.RecomputeVisibility()
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		w.tick++
		w.visibilitySystem()
	}
}
