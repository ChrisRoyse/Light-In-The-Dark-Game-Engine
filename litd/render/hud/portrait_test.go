package hud

// #193 portrait-panel logic FSV. The panel is the decision layer for the portrait
// row: which mode, whether the offscreen RTT pass runs, the name/level labels. The
// render-to-texture primitive itself (litd/render.PortraitTarget) is verified
// separately; here the SoT is the PortraitUpdate the renderer consumes.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func newTestPortrait() (PortraitPanel, *TextBuffer) {
	tb := &TextBuffer{}
	return NewPortraitPanel(tb, PortraitStrings{Level: "Lv"}), tb
}

// Animated: a selected unit WITH a Portrait clip plays it in the offscreen pass.
func TestPortraitAnimatedRunsRTTPassFSV(t *testing.T) {
	p, tb := newTestPortrait()
	u := p.Update(PortraitState{SelectionVersion: 1, Entity: 42, Name: "Warden", Level: 3, HasPortraitClip: true, Alive: true})
	t.Logf("FSV animated: %+v labels=%q", u, tb.String())
	if u.Mode != PortraitAnimated || !u.RTTPass || !u.ClipPlaying || u.Fallback || !u.Visible {
		t.Fatalf("animated state wrong: %+v", u)
	}
	if !u.Dirty || u.Repaints != 1 || u.Entity != 42 {
		t.Fatalf("first fold should repaint subject 42: %+v", u)
	}
	if tb.String() != "Warden  Lv 3" {
		t.Fatalf("labels = %q, want %q", tb.String(), "Warden  Lv 3")
	}
}

// Fallback: a model with NO Portrait clip shows a static icon and runs NO offscreen
// pass — the constraint that forbids a T-pose render of an un-animated head.
func TestPortraitFallbackNoRTTPassFSV(t *testing.T) {
	p, _ := newTestPortrait()
	u := p.Update(PortraitState{SelectionVersion: 1, Entity: 7, Name: "Grunt", HasPortraitClip: false, Alive: true})
	t.Logf("FSV fallback: %+v", u)
	if u.Mode != PortraitFallback || !u.Fallback || u.RTTPass || u.ClipPlaying {
		t.Fatalf("fallback must show a static icon with no offscreen pass (no T-pose): %+v", u)
	}
	if !u.Visible {
		t.Fatalf("fallback row should still be visible (the icon): %+v", u)
	}
}

// Empty: no selection → blank row, RTT pass SKIPPED (0 portrait calls in the dump).
func TestPortraitEmptySkipsRTTPassFSV(t *testing.T) {
	p, tb := newTestPortrait()
	u := p.Update(PortraitState{SelectionVersion: 1, Entity: 0})
	t.Logf("FSV empty: %+v labels=%q", u, tb.String())
	if u.Mode != PortraitEmpty || u.Visible || u.RTTPass || u.Entity != 0 {
		t.Fatalf("empty selection must skip the offscreen pass: %+v", u)
	}
	if tb.String() != "" {
		t.Fatalf("empty selection should clear labels, got %q", tb.String())
	}
}

// Edge (1): selection change → portrait swaps the SAME frame (no stale face).
func TestPortraitSwapsOnSelectionChangeFSV(t *testing.T) {
	p, tb := newTestPortrait()
	p.Update(PortraitState{SelectionVersion: 1, Entity: 42, Name: "Warden", Level: 3, HasPortraitClip: true, Alive: true})
	u := p.Update(PortraitState{SelectionVersion: 2, Entity: 99, Name: "Archer", Level: 1, HasPortraitClip: true, Alive: true})
	t.Logf("FSV swap: %+v labels=%q", u, tb.String())
	if !u.Dirty || u.Entity != 99 || tb.String() != "Archer  Lv 1" {
		t.Fatalf("selection change must swap subject + labels same frame: %+v labels=%q", u, tb.String())
	}
}

// Edge (3): the selected unit dies and no successor is fed (Alive=false) → empty,
// offscreen pass stops. (Sub-advance is a plain Entity change, covered by swap.)
func TestPortraitClearsOnDeathFSV(t *testing.T) {
	p, _ := newTestPortrait()
	p.Update(PortraitState{SelectionVersion: 1, Entity: 42, Name: "Warden", HasPortraitClip: true, Alive: true})
	u := p.Update(PortraitState{SelectionVersion: 2, Entity: 42, Name: "Warden", HasPortraitClip: true, Alive: false})
	t.Logf("FSV death: %+v", u)
	if u.Mode != PortraitEmpty || u.RTTPass || u.Entity != 0 {
		t.Fatalf("dead subject with no successor must clear + stop the pass: %+v", u)
	}
}

// A steady selection (version unchanged) is a no-op repaint-wise, but the offscreen
// pass MUST keep running so the clip animates frame to frame.
func TestPortraitSteadyKeepsAnimatingFSV(t *testing.T) {
	p, _ := newTestPortrait()
	s := PortraitState{SelectionVersion: 1, Entity: 42, Name: "Warden", Level: 3, HasPortraitClip: true, Alive: true}
	p.Update(s)
	u := p.Update(s)
	t.Logf("FSV steady: %+v", u)
	if u.Dirty || u.Repaints != 0 {
		t.Fatalf("unchanged selection should not repaint: %+v", u)
	}
	if !u.RTTPass || !u.ClipPlaying {
		t.Fatalf("steady selection must keep the offscreen clip animating: %+v", u)
	}
}

// Zero-alloc steady state (R-GC-1 posture for HUD widgets): a folded Update must not
// allocate, including the label rebuild on a dirty swap.
func TestPortraitUpdateZeroAllocFSV(t *testing.T) {
	p, _ := newTestPortrait()
	steady := PortraitState{SelectionVersion: 1, Entity: 42, Name: "Warden", Level: 3, HasPortraitClip: true, Alive: true}
	p.Update(steady)
	if n := testing.AllocsPerRun(200, func() { p.Update(steady) }); n != 0 {
		t.Fatalf("steady Update allocates %v/run, want 0", n)
	}
	// A dirty swap rebuilds the label buffer in place — also zero alloc.
	toggle := uint32(1)
	swapA := PortraitState{Entity: 42, Name: "Warden", Level: 3, HasPortraitClip: true, Alive: true}
	swapB := PortraitState{Entity: 99, Name: "Archer", Level: 1, HasPortraitClip: true, Alive: true}
	if n := testing.AllocsPerRun(200, func() {
		toggle++
		if toggle&1 == 0 {
			swapA.SelectionVersion = toggle
			p.Update(swapA)
		} else {
			swapB.SelectionVersion = toggle
			p.Update(swapB)
		}
	}); n != 0 {
		t.Fatalf("dirty-swap Update allocates %v/run, want 0", n)
	}
}

var _ = sim.EntityID(0)
