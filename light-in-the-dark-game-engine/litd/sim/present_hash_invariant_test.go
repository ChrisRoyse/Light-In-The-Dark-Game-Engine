package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// FSV for the #449/#471 presentation-trigger invariant via the RENDER-EVENT
// staging path. litd/api/present_invariant_test.go covers the OnAudio/OnCamera
// sink path; this covers the *other* presentation mechanism — EmitRenderEvent /
// publishSnapshot, the snapshot cue seam that attack.go, missile.go, produce.go,
// and store_destructable.go feed during normal simulation.
//
// The invariant (CLAUDE.md architecture rule; R-SIM-6): a rendered / audio-on
// game stages render cues and drains them into the snapshot every frame, and may
// overflow the fixed staging ring (fail-closed, counted in renderEvDropped). A
// headless determinism / netplay peer may stage and never drain. NONE of that may
// touch Game.StateHash — otherwise an audio-on game desyncs from an audio-off one.
//
// SoT = w.HashState(...).Top, read BEFORE and AFTER each presentation action. This
// is a same-package test specifically so it can reach the unexported staging
// buffer (renderEvStaging), the drop counter (renderEvDropped), and the drain
// (publishSnapshot) directly — the exact state whose non-hashing we must prove.
func TestRenderEventStagingDoesNotHashFSV(t *testing.T) {
	w := NewWorld(Caps{PendingEvents: 16})
	id, ok := w.CreateUnit(fixed.Vec2{X: 5 * fixed.One, Y: 5 * fixed.One}, 0)
	if !ok {
		t.Fatal("CreateUnit id")
	}
	id2, ok := w.CreateUnit(fixed.Vec2{X: 9 * fixed.One, Y: 2 * fixed.One}, 0)
	if !ok {
		t.Fatal("CreateUnit id2")
	}
	for i := 0; i < 5; i++ { // settle to a stable state
		w.Step()
	}

	reg := NewHashRegistry()
	hash := func() uint64 {
		var s statehash.Snapshot
		return w.HashState(reg, &s).Top
	}
	h0 := hash()

	// --- trigger 1: stage render cues, no other mutation. SoT must not move. ---
	w.EmitRenderEvent(RenderUnitAttack, id, 7)
	w.EmitRenderEventAt(RenderMissileImpact, id2, 3, fixed.Vec2{X: 4 * fixed.One, Y: 4 * fixed.One})
	if len(w.renderEvStaging) != 2 { // non-vacuity: events really staged
		t.Fatalf("expected 2 staged events, got %d — trigger vacuous", len(w.renderEvStaging))
	}
	h1 := hash()
	t.Logf("FSV: h0=%#016x  staged 2 cues → h1=%#016x (staging len=%d)", h0, h1, len(w.renderEvStaging))
	if h1 != h0 {
		t.Fatalf("HASH DIVERGENCE: staging render cues moved StateHash %#016x → %#016x (#449 render-event path)", h0, h1)
	}

	// --- trigger 2: drain into the snapshot (the per-frame render consume). ---
	w.publishSnapshot()
	drained := len(w.Snaps.Curr().Events)
	if drained == 0 { // non-vacuity: cues really reached the snapshot
		t.Fatal("publishSnapshot drained 0 events — drain trigger vacuous")
	}
	if len(w.renderEvStaging) != 0 {
		t.Fatalf("staging not reset after drain: len=%d", len(w.renderEvStaging))
	}
	h2 := hash()
	t.Logf("FSV: drained %d cues to snapshot → h2=%#016x", drained, h2)
	if h2 != h0 {
		t.Fatalf("HASH DIVERGENCE: draining render cues moved StateHash %#016x → %#016x — a rendered game would desync from headless (#449)", h0, h2)
	}

	// --- edge: overflow the staging ring → fail-closed renderEvDropped > 0 → still no hash change. ---
	target := cap(w.renderEvStaging)
	for i := 0; i < target+5; i++ {
		w.EmitRenderEvent(RenderUnitReady, id, uint16(i))
	}
	if w.renderEvDropped == 0 { // non-vacuity: overflow really happened
		t.Fatalf("staging never overflowed (cap=%d) — overflow edge vacuous", target)
	}
	if len(w.renderEvStaging) != target {
		t.Fatalf("staging should be full at cap=%d, got %d", target, len(w.renderEvStaging))
	}
	h3 := hash()
	t.Logf("FSV edge: overflowed staging, renderEvDropped=%d (fail-closed drop) → h3=%#016x", w.renderEvDropped, h3)
	if h3 != h0 {
		t.Fatalf("HASH DIVERGENCE: fail-closed renderEvDropped counter leaked into StateHash %#016x → %#016x", h0, h3)
	}

	t.Log("FSV #449/#471 (render-event path): staging, draining, and fail-closed overflow all leave StateHash identical")
}
