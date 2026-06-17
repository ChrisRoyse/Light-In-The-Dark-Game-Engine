package render

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// fakeGhostRec is one synthetic last-seen record; cellX/cellY double as both
// the fog cell and (via FromInt) the encoded world position, so FogCellOf is a
// trivial floor. The verdict is always the produced ghost list.
type fakeGhostRec struct {
	typeID       uint16
	owner        uint8
	cellX, cellY int32
	inBounds     bool
}

type fakeGhostSrc struct {
	player uint8
	recs   []fakeGhostRec
}

func (s *fakeGhostSrc) LastSeenCount(player uint8) int {
	if player != s.player {
		return 0
	}
	return len(s.recs)
}

func (s *fakeGhostSrc) LastSeenAt(player uint8, i int) (uint16, uint8, fixed.Vec2, bool) {
	if player != s.player || i < 0 || i >= len(s.recs) {
		return 0, 0, fixed.Vec2{}, false
	}
	r := s.recs[i]
	return r.typeID, r.owner, fixed.Vec2{X: fixed.FromInt(r.cellX), Y: fixed.FromInt(r.cellY)}, true
}

func (s *fakeGhostSrc) FogCellOf(pos fixed.Vec2) (int32, int32, bool) {
	x := int32(pos.X.Floor())
	y := int32(pos.Y.Floor())
	for _, r := range s.recs {
		if r.cellX == x && r.cellY == y {
			return x, y, r.inBounds
		}
	}
	return x, y, true
}

// TestGhostExploredVsVisibleFSV — a record whose cell is explored becomes a
// ghost; a record whose cell is currently visible is skipped (live drawn).
func TestGhostExploredVsVisibleFSV(t *testing.T) {
	const player uint8 = 0
	src := &fakeGhostSrc{player: player, recs: []fakeGhostRec{
		{typeID: 7, owner: 1, cellX: 10, cellY: 10, inBounds: true}, // explored → ghost
		{typeID: 9, owner: 2, cellX: 20, cellY: 20, inBounds: true}, // visible  → skip
	}}
	fog := newFakeFogGrid()
	fog.set(player, 10, 10, fogStateExplored)
	fog.set(player, 20, 20, fogStateVisible)

	var gs GhostSet
	out := gs.Rebuild(src, fog, player)
	t.Logf("FSV ghosts emitted=%d", len(out))
	for _, g := range out {
		t.Logf("FSV ghost type=%d owner=%d pos=(%d,%d)", g.TypeID, g.Owner, g.Pos.X.Floor(), g.Pos.Y.Floor())
	}
	if len(out) != 1 {
		t.Fatalf("want 1 ghost (explored only), got %d", len(out))
	}
	g := out[0]
	if g.TypeID != 7 || g.Owner != 1 || g.Pos.X.Floor() != 10 || g.Pos.Y.Floor() != 10 {
		t.Fatalf("ghost mismatch: type=%d owner=%d pos=(%d,%d)", g.TypeID, g.Owner, g.Pos.X.Floor(), g.Pos.Y.Floor())
	}
}

// TestGhostHiddenStillDrawnFSV — a record in a hidden (not-visible) cell is
// still drawn as a ghost (only re-scouting, i.e. visible, suppresses it).
func TestGhostHiddenStillDrawnFSV(t *testing.T) {
	const player uint8 = 0
	src := &fakeGhostSrc{player: player, recs: []fakeGhostRec{
		{typeID: 3, owner: 1, cellX: 5, cellY: 5, inBounds: true},
	}}
	fog := newFakeFogGrid() // cell (5,5) stays hidden (0)
	var gs GhostSet
	out := gs.Rebuild(src, fog, player)
	t.Logf("FSV hidden-cell ghosts=%d", len(out))
	if len(out) != 1 {
		t.Fatalf("hidden cell should still ghost, got %d", len(out))
	}
}

// TestGhostOutOfBoundsSkippedFSV — a record whose position maps outside the
// playable bounds is skipped, not drawn at a bogus cell.
func TestGhostOutOfBoundsSkippedFSV(t *testing.T) {
	const player uint8 = 0
	src := &fakeGhostSrc{player: player, recs: []fakeGhostRec{
		{typeID: 1, owner: 1, cellX: 12, cellY: 12, inBounds: false}, // OOB
		{typeID: 2, owner: 1, cellX: 13, cellY: 13, inBounds: true},  // explored
	}}
	fog := newFakeFogGrid()
	fog.set(player, 13, 13, fogStateExplored)
	var gs GhostSet
	out := gs.Rebuild(src, fog, player)
	t.Logf("FSV oob-skip ghosts=%d (want 1)", len(out))
	if len(out) != 1 || out[0].TypeID != 2 {
		t.Fatalf("OOB record not skipped: got %d ghosts", len(out))
	}
}

// TestGhostEmptyAndWrongPlayerFSV — no records, or querying a player with none,
// yields an empty list.
func TestGhostEmptyAndWrongPlayerFSV(t *testing.T) {
	src := &fakeGhostSrc{player: 0, recs: nil}
	fog := newFakeFogGrid()
	var gs GhostSet
	if out := gs.Rebuild(src, fog, 0); len(out) != 0 {
		t.Fatalf("empty store should produce 0 ghosts, got %d", len(out))
	}
	src2 := &fakeGhostSrc{player: 0, recs: []fakeGhostRec{{typeID: 1, cellX: 1, cellY: 1, inBounds: true}}}
	if out := gs.Rebuild(src2, fog, 3); len(out) != 0 {
		t.Fatalf("wrong player should produce 0 ghosts, got %d", len(out))
	}
	t.Logf("FSV empty + wrong-player both yield 0 ghosts")
}

// TestGhostZeroAllocFSV — Rebuild over a stable record set allocates nothing
// once the backing slice is reserved.
func TestGhostZeroAllocFSV(t *testing.T) {
	const player uint8 = 0
	const n = 64
	recs := make([]fakeGhostRec, n)
	fog := newFakeFogGrid()
	for i := 0; i < n; i++ {
		recs[i] = fakeGhostRec{typeID: uint16(i), owner: 1, cellX: int32(i), cellY: int32(i), inBounds: true}
		fog.set(player, int32(i), int32(i), fogStateExplored)
	}
	src := &fakeGhostSrc{player: player, recs: recs}
	var gs GhostSet
	gs.Reserve(n)
	gs.Rebuild(src, fog, player) // warm
	allocs := testing.AllocsPerRun(200, func() { gs.Rebuild(src, fog, player) })
	out := gs.Rebuild(src, fog, player)
	t.Logf("FSV ghost Rebuild allocs/op=%v ghosts=%d", allocs, len(out))
	if allocs != 0 {
		t.Fatalf("Rebuild allocates %v/op, want 0", allocs)
	}
	if len(out) != n {
		t.Fatalf("want %d ghosts, got %d", n, len(out))
	}
}
