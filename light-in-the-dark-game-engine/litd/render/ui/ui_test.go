package ui

// #315 tooltips + minimap pings + screen-edge alerts FSV. SoT = each widget's
// computed model (tooltip visibility/lines, ping pool, latest alert, camera
// targets). Synthetic known hovers/events => known overlay state.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func tooltipLabels() TooltipStrings { return TooltipStrings{Gold: "G", Wood: "W", Food: "F"} }

func TestTooltipHoverDelayFSV(t *testing.T) {
	tip := NewTooltip(500, tooltipLabels())
	content := TooltipContent{Title: "Footman", Gold: 135, Food: 2}

	// Hover starts at t=0: not yet visible before the delay elapses.
	if u := tip.Update(7, content, 0); u.Visible {
		t.Fatalf("tooltip visible immediately: %+v", u)
	}
	if u := tip.Update(7, content, 300); u.Visible {
		t.Fatalf("tooltip visible before delay (300<500): %+v", u)
	}
	// Delay elapsed on a stable hover: appears + paints once.
	u := tip.Update(7, content, 500)
	t.Logf("FSV tooltip shown: %+v title=%q cost=%q", u, tip.Title.String(), tip.Cost.String())
	if !u.Visible || !u.Shown {
		t.Fatalf("tooltip should appear at delay: %+v", u)
	}
	if tip.Title.String() != "Footman" || tip.Cost.String() != "G 135  F 2" { // wood 0 skipped
		t.Fatalf("tooltip lines wrong: title=%q cost=%q", tip.Title.String(), tip.Cost.String())
	}
	// Still hovering: visible, no longer "shown" (steady).
	if u := tip.Update(7, content, 800); !u.Visible || u.Shown {
		t.Fatalf("steady tooltip wrong: %+v", u)
	}
}

func TestTooltipRetargetAndHideFSV(t *testing.T) {
	tip := NewTooltip(200, tooltipLabels())
	c := TooltipContent{Title: "A", Gold: 50}
	tip.Update(1, c, 0)
	tip.Update(1, c, 200) // visible now
	// Move onto a different target: timer restarts, hidden again.
	if u := tip.Update(2, TooltipContent{Title: "B"}, 250); u.Visible {
		t.Fatalf("retarget should hide until new delay: %+v", u)
	}
	if u := tip.Update(2, TooltipContent{Title: "B"}, 450); !u.Visible {
		t.Fatalf("new target should appear after its own delay: %+v", u)
	}
	// Hover nothing (key 0): hidden.
	if u := tip.Update(0, TooltipContent{}, 500); u.Visible {
		t.Fatalf("key 0 should hide: %+v", u)
	}
}

func TestTooltipGatedRequirementFSV(t *testing.T) {
	tip := NewTooltip(0, tooltipLabels()) // zero delay -> appears immediately
	u := tip.Update(9, TooltipContent{Title: "Rifle", Gold: 200, Wood: 25, Gated: true, Requirement: "needs Blacksmith"}, 0)
	t.Logf("FSV gated tooltip: %+v cost=%q req=%q", u, tip.Cost.String(), tip.Req.String())
	if !u.Visible || !u.Gated {
		t.Fatalf("gated tooltip should be visible+gated: %+v", u)
	}
	if tip.Cost.String() != "G 200  W 25" || tip.Req.String() != "needs Blacksmith" {
		t.Fatalf("gated lines wrong: cost=%q req=%q", tip.Cost.String(), tip.Req.String())
	}
}

func TestTooltipSteadyZeroAllocFSV(t *testing.T) {
	tip := NewTooltip(100, tooltipLabels())
	c := TooltipContent{Title: "Footman", Gold: 135, Food: 2}
	tip.Update(3, c, 0)
	tip.Update(3, c, 100) // warm: visible
	got := testing.AllocsPerRun(1000, func() { tip.Update(3, c, 200) })
	t.Logf("FSV tooltip steady Update allocs/op=%.2f", got)
	if got != 0 {
		t.Fatalf("steady Tooltip.Update allocated %.2f, want 0", got)
	}
}

func TestAlertFeedEventMappingFSV(t *testing.T) {
	a := NewAlertFeed(1000)

	// Player alt-click ping.
	a.PlayerPing(120, 80)
	// Under-attack event -> ping; latest alert tracks it.
	if !a.FromEvent(sim.RenderEvent{Kind: sim.RenderUnderAttack, Ent: 5}, 300, 200) {
		t.Fatal("under-attack event should produce a ping")
	}
	// Unit-ready event -> ping.
	if !a.FromEvent(sim.RenderEvent{Kind: sim.RenderUnitReady, Ent: 8}, 50, 60) {
		t.Fatal("unit-ready event should produce a ping")
	}
	// A non-alert event (death) is ignored.
	if a.FromEvent(sim.RenderEvent{Kind: sim.RenderUnitDeath, Ent: 9}, 0, 0) {
		t.Fatal("death event should NOT produce a ping")
	}
	t.Logf("FSV alert pings: %+v", a.Pings())
	if len(a.Pings()) != 3 {
		t.Fatalf("expected 3 pings (player + 2 alerts), got %d", len(a.Pings()))
	}
	if a.Pings()[1].Kind != PingUnderAttack || a.Pings()[2].Kind != PingUnitReady {
		t.Fatalf("ping kinds wrong: %+v", a.Pings())
	}
	// Latest alert = the unit-ready ping; click jumps the camera there.
	x, z, ok := a.LatestAlert()
	t.Logf("FSV latest alert: (%g,%g) ok=%v", x, z, ok)
	if !ok || x != 50 || z != 60 {
		t.Fatalf("latest alert wrong: (%g,%g) ok=%v", x, z, ok)
	}
	if cx, cz, ok := a.ClickPing(1); !ok || cx != 300 || cz != 200 {
		t.Fatalf("click ping camera target wrong: (%g,%g) ok=%v", cx, cz, ok)
	}
}

func TestAlertFeedTTLExpiryFSV(t *testing.T) {
	a := NewAlertFeed(500)
	a.FromEvent(sim.RenderEvent{Kind: sim.RenderUnderAttack}, 1, 1) // TTL 500
	a.PlayerPing(2, 2)                                              // TTL 500
	a.Tick(200)                                                     // -> 300 each
	if len(a.Pings()) != 2 {
		t.Fatalf("no ping should expire at 200ms: %d", len(a.Pings()))
	}
	a.Tick(300) // -> 0, both expire
	t.Logf("FSV after expiry: pings=%d latest_ok=%v", len(a.Pings()), func() bool { _, _, ok := a.LatestAlert(); return ok }())
	if len(a.Pings()) != 0 {
		t.Fatalf("both pings should expire: %d", len(a.Pings()))
	}
	if _, _, ok := a.LatestAlert(); ok {
		t.Fatal("no latest alert after all expired")
	}
}

func TestAlertFeedPoolEvictOldestFSV(t *testing.T) {
	a := NewAlertFeed(10000)
	for i := 0; i < AlertPoolCap+3; i++ { // overflow by 3
		a.PlayerPing(float32(i), 0)
	}
	t.Logf("FSV pool overflow: count=%d evicted=%d firstX=%g", len(a.Pings()), a.Evicted(), a.Pings()[0].X)
	if len(a.Pings()) != AlertPoolCap {
		t.Fatalf("pool should cap at %d, got %d", AlertPoolCap, len(a.Pings()))
	}
	if a.Evicted() != 3 {
		t.Fatalf("3 oldest should be evicted, got %d", a.Evicted())
	}
	// Oldest three (X=0,1,2) gone; the pool now starts at X=3.
	if a.Pings()[0].X != 3 {
		t.Fatalf("oldest not evicted: front X=%g, want 3", a.Pings()[0].X)
	}
}
