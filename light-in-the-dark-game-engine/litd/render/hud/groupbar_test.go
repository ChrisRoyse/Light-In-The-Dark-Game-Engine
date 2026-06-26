package hud

// #197 control-group bar + idle-worker button FSV. SoT = the widget's computed
// model (visible badges, rendered text, dirty flags, click targets) — the
// headless half of the same widget pattern resourcebar_test.go verifies; the
// canvas draw is presentation on top. Synthetic known counts => known badges.

import (
	"testing"
)

func counts(pairs ...[2]int) GroupBarState {
	var s GroupBarState
	for _, p := range pairs {
		s.Counts[p[0]] = uint8(p[1])
	}
	return s
}

func TestGroupBarVisiblePruneFSV(t *testing.T) {
	var text TextBuffer
	bar := NewGroupBar(&text)

	// Assign groups 1 (×3) and 2 (×5): two badges, ascending, "1:3  2:5".
	u := bar.Update(counts([2]int{1, 3}, [2]int{2, 5}))
	t.Logf("FSV assign g1,g2: %+v badges=%+v text=%q", u, bar.Badges(), text.String())
	if !u.Dirty || u.Visible != 2 || text.String() != "1:3  2:5" {
		t.Fatalf("two-group bar wrong: %+v text=%q", u, text.String())
	}

	// Steady (same counts): no repaint.
	u = bar.Update(counts([2]int{1, 3}, [2]int{2, 5}))
	t.Logf("FSV steady: %+v", u)
	if u.Dirty || u.Repaints != 0 {
		t.Fatalf("unchanged counts should not repaint: %+v", u)
	}

	// Half of group 2 dies → "1:3  2:2".
	u = bar.Update(counts([2]int{1, 3}, [2]int{2, 2}))
	t.Logf("FSV g2 losses: %+v text=%q", u, text.String())
	if !u.Dirty || text.String() != "1:3  2:2" {
		t.Fatalf("count prune wrong: %+v text=%q", u, text.String())
	}

	// Group 1 emptied by deaths → its badge disappears, only "2:2" remains.
	u = bar.Update(counts([2]int{2, 2}))
	t.Logf("FSV g1 emptied: %+v badges=%+v text=%q", u, bar.Badges(), text.String())
	if u.Visible != 1 || text.String() != "2:2" {
		t.Fatalf("emptied group not pruned off bar: %+v text=%q", u, text.String())
	}
	if bar.Badges()[0].Group != 2 || bar.Badges()[0].Count != 2 {
		t.Fatalf("surviving badge wrong: %+v", bar.Badges())
	}
}

func TestGroupBarGroupZeroAndOrderFSV(t *testing.T) {
	var text TextBuffer
	bar := NewGroupBar(&text)
	// Group 0 is a real group (key "0"); badges stay group-ascending.
	bar.Update(counts([2]int{0, 4}, [2]int{9, 1}, [2]int{5, 12}))
	t.Logf("FSV g0,g5,g9: text=%q badges=%+v", text.String(), bar.Badges())
	if text.String() != "0:4  5:12  9:1" {
		t.Fatalf("group-0/order wrong: %q", text.String())
	}
}

func TestGroupBarEmptyFSV(t *testing.T) {
	var text TextBuffer
	bar := NewGroupBar(&text)
	u := bar.Update(GroupBarState{}) // nothing assigned
	t.Logf("FSV empty: %+v text=%q", u, text.String())
	if u.Visible != 0 || text.String() != "" {
		t.Fatalf("empty bar should show nothing: %+v text=%q", u, text.String())
	}
}

func TestGroupBarSteadyZeroAllocFSV(t *testing.T) {
	var text TextBuffer
	bar := NewGroupBar(&text)
	st := counts([2]int{1, 3}, [2]int{4, 7})
	bar.Update(st) // warm
	got := testing.AllocsPerRun(1000, func() { bar.Update(st) })
	t.Logf("FSV groupbar steady Update allocs/op=%.2f", got)
	if got != 0 {
		t.Fatalf("steady GroupBar.Update allocated %.2f, want 0", got)
	}
}

func TestIdleWorkerVisibilityAndCycleFSV(t *testing.T) {
	var w IdleWorkerButton

	// None idle: hidden, click is a no-op.
	u := w.Update(IdleWorkerState{IdleCount: 0})
	c := w.Click()
	t.Logf("FSV no-idle: %+v click=%+v", u, c)
	if u.Visible || c.HasTarget {
		t.Fatalf("no idle workers: button hidden + click no-op, got %+v %+v", u, c)
	}

	// Three idle: visible; repeat clicks tour 0,1,2,0.
	u = w.Update(IdleWorkerState{IdleCount: 3})
	if !u.Visible || !u.Dirty || u.Count != 3 {
		t.Fatalf("3 idle should be visible+dirty: %+v", u)
	}
	var seq []int
	for i := 0; i < 4; i++ {
		seq = append(seq, w.Click().Index)
	}
	t.Logf("FSV cycle 3 workers: %v", seq)
	if seq[0] != 0 || seq[1] != 1 || seq[2] != 2 || seq[3] != 0 {
		t.Fatalf("idle cycle not round-robin: %v", seq)
	}

	// List shrinks to 1 → cursor resets, click points at 0.
	w.Update(IdleWorkerState{IdleCount: 1})
	if got := w.Click(); !got.HasTarget || got.Index != 0 {
		t.Fatalf("shrunk list: click should target 0, got %+v", got)
	}
}
