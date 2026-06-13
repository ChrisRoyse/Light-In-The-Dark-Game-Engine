package input

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestControlGroupRecallPrunesDeadFSV(t *testing.T) {
	g := NewControlGroups(DefaultGroupConfig())
	g.Assign(1, testSelection(gid(1, 0), gid(2, 0), gid(3, 0), gid(4, 0), gid(5, 0)))

	live := []GroupEntity{
		{ID: gid(1, 0), X: -240},
		{ID: gid(3, 0), X: 0},
		{ID: gid(5, 0), X: 240},
	}
	res := g.Recall(1, live, 1000)
	t.Logf("FSV group recall prune group=1 selection=%v pruned=%d commands=%d", idsOf(res.Selection), res.Pruned, res.CommandRecordsEmitted)

	want := []sim.EntityID{gid(1, 0), gid(3, 0), gid(5, 0)}
	if !sameIDs(res.Selection, want) || res.Pruned != 2 || res.CommandRecordsEmitted != 0 {
		t.Fatalf("recall after deaths = ids %v pruned=%d commands=%d, want %v pruned=2 commands=0",
			idsOf(res.Selection), res.Pruned, res.CommandRecordsEmitted, want)
	}
	if ids, _ := g.IDs(1); !sameIDs(testSelection(ids...), want) {
		t.Fatalf("group not lazily pruned: %v", ids)
	}
}

func TestControlGroupGenerationReuseFSV(t *testing.T) {
	g := NewControlGroups(DefaultGroupConfig())
	old := gid(7, 0)
	recycled := gid(7, 1)
	g.Assign(2, testSelection(old))

	res := g.Recall(2, []GroupEntity{{ID: recycled, X: 90, Z: 30}}, 1000)
	t.Logf("FSV group generation prune old=%#08x idx=%d gen=%d recycled=%#08x idx=%d gen=%d selection=%v pruned=%d",
		uint32(old), old.Index(), old.Generation(), uint32(recycled), recycled.Index(), recycled.Generation(), idsOf(res.Selection), res.Pruned)

	if res.Selection.Count != 0 || res.Pruned != 1 {
		t.Fatalf("recycled slot resurrected or was not pruned: selection=%v pruned=%d", idsOf(res.Selection), res.Pruned)
	}
}

func TestControlGroupDoubleTapThresholdFSV(t *testing.T) {
	g := NewControlGroups(DefaultGroupConfig())
	g.Assign(1, testSelection(gid(1, 0), gid(2, 0)))
	live := []GroupEntity{
		{ID: gid(1, 0), X: -10, Z: 4},
		{ID: gid(2, 0), X: 30, Z: 12},
	}

	first := g.Recall(1, live, 1000)
	at299 := g.Recall(1, live, 1299)
	reset := g.Recall(1, live, 2000)
	at350 := g.Recall(1, live, 2350)
	t.Logf("FSV group doubletap first.center=%v 299.center=%v anchor=(%.1f,%.1f) reset.center=%v 350.center=%v",
		first.CenterRequested, at299.CenterRequested, at299.CenterX, at299.CenterZ, reset.CenterRequested, at350.CenterRequested)

	if first.CenterRequested || !at299.CenterRequested || at299.CenterX != 10 || at299.CenterZ != 8 {
		t.Fatalf("299ms double-tap did not center on centroid: first=%+v at299=%+v", first, at299)
	}
	if reset.CenterRequested || at350.CenterRequested {
		t.Fatalf("350ms recall must not center: reset=%+v at350=%+v", reset, at350)
	}
}

func TestControlGroupAddAndReassignFSV(t *testing.T) {
	g := NewControlGroups(DefaultGroupConfig())
	g.Assign(1, testSelection(gid(1, 0), gid(2, 0)))
	add := g.Add(1, testSelection(gid(2, 0), gid(3, 0), gid(4, 0)))
	groupIDs, _ := g.IDs(1)
	t.Logf("FSV group shift-add group=1 groupIDs=%v selectionUnchanged=%v", groupIDs, idsOf(add.Selection))
	if !sameIDs(testSelection(groupIDs...), []sim.EntityID{gid(1, 0), gid(2, 0), gid(3, 0), gid(4, 0)}) {
		t.Fatalf("shift-add group ids = %v", groupIDs)
	}
	if !sameIDs(add.Selection, []sim.EntityID{gid(2, 0), gid(3, 0), gid(4, 0)}) {
		t.Fatalf("shift-add should not mutate current selection: %v", idsOf(add.Selection))
	}

	reassign := g.Assign(1, testSelection(gid(8, 0)))
	t.Logf("FSV group ctrl-reassign group=1 ids=%v", idsOf(reassign.Selection))
	if !sameIDs(reassign.Selection, []sim.EntityID{gid(8, 0)}) {
		t.Fatalf("ctrl reassign did not replace old group: %v", idsOf(reassign.Selection))
	}
}

func TestControlGroupRecallZeroAllocFSV(t *testing.T) {
	var live [16]GroupEntity
	var ids [16]sim.EntityID
	for i := range live {
		ids[i] = gid(uint32(i+1), 0)
		live[i] = GroupEntity{ID: ids[i], X: float32(i * 10), Z: float32(i)}
	}
	g := NewControlGroups(DefaultGroupConfig())
	g.Assign(1, testSelection(ids[:]...))
	allocs := testing.AllocsPerRun(1000, func() {
		_ = g.Recall(1, live[:], 1000)
	})
	t.Logf("FSV group recall allocs/op=%v count=%d", allocs, g.Count(1))
	if allocs != 0 {
		t.Fatalf("group recall allocated: %v", allocs)
	}
}

func gid(index uint32, gen uint8) sim.EntityID {
	return sim.EntityID(uint32(gen)<<24 | index&0x00FFFFFF)
}

func testSelection(ids ...sim.EntityID) Selection {
	var s Selection
	for _, id := range ids {
		if s.Count >= MaxSelection {
			break
		}
		s.IDs[s.Count] = id
		s.Count++
	}
	return s
}
