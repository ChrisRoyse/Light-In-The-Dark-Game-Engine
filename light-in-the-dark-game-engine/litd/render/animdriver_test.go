package render

import "testing"

// testClips: Idle/Walk loop at 1.0s; Attack plays once over 0.6s, impact at
// 0.3s; Death plays once over 0.8s.
func testClips() ClipSet {
	return ClipSet{
		ClipIdle:   {Duration: 1.0, Loop: true},
		ClipWalk:   {Duration: 1.0, Loop: true},
		ClipAttack: {Duration: 0.6, Loop: false, ImpactTime: 0.3},
		ClipDeath:  {Duration: 0.8, Loop: false},
	}
}

func vis(n int) []bool {
	v := make([]bool, n)
	for i := range v {
		v[i] = true
	}
	return v
}

// TestAnimStateToClipFSV — each sim state selects its contractual clip on the
// first observed frame.
func TestAnimStateToClipFSV(t *testing.T) {
	d := NewAnimDriver(4)
	states := []SimAnimState{StateIdle, StateMove, StateAttack, StateDead}
	want := []ClipID{ClipIdle, ClipWalk, ClipAttack, ClipDeath}
	d.Update(states, vis(4), 0, testClips(), 1)
	for i := range states {
		t.Logf("FSV state=%d -> clip=%d want=%d", states[i], d.Clip(i), want[i])
		if d.Clip(i) != want[i] {
			t.Fatalf("state %d -> clip %d want %d", states[i], d.Clip(i), want[i])
		}
	}
}

// TestAnimLoopWrapFSV — a looping clip wraps clipTime at its duration.
func TestAnimLoopWrapFSV(t *testing.T) {
	d := NewAnimDriver(1)
	clips := testClips()
	st := []SimAnimState{StateMove}
	d.Update(st, vis(1), 0, clips, 1) // init Walk at t=0
	d.Update(st, vis(1), 0.7, clips, 1)
	if d.Time(0) != 0.7 {
		t.Fatalf("walk t=%.3f want 0.7", d.Time(0))
	}
	d.Update(st, vis(1), 0.5, clips, 1) // 1.2 → wraps to 0.2
	t.Logf("FSV walk loop t after 0.7+0.5 over dur 1.0 = %.3f (want 0.2)", d.Time(0))
	if d.Time(0) < 0.199 || d.Time(0) > 0.201 {
		t.Fatalf("loop wrap t=%.4f want ~0.2", d.Time(0))
	}
}

// TestAnimDeathOnceHoldFadeFSV — Death plays once, clamps at the last frame,
// then the corpse-fade scalar ramps 1→0 and clamps at 0 (edge case 2).
func TestAnimDeathOnceHoldFadeFSV(t *testing.T) {
	d := NewAnimDriver(1)
	clips := testClips()
	st := []SimAnimState{StateDead}
	const fade = 0.5
	d.Update(st, vis(1), 0, clips, fade) // init Death
	if d.Fade(0) != 1 {
		t.Fatalf("fade at death start=%.3f want 1", d.Fade(0))
	}
	// Advance past the 0.8s clip; time clamps, fade not yet ramping.
	d.Update(st, vis(1), 0.8, clips, fade)
	t.Logf("FSV death t=%.3f fade=%.3f (clip just ended)", d.Time(0), d.Fade(0))
	if d.Time(0) != 0.8 {
		t.Fatalf("death clip time=%.3f want clamped 0.8", d.Time(0))
	}
	// Further dt clamps time and ramps fade by dt/fadeTime each step.
	d.Update(st, vis(1), 0.25, clips, fade) // fade -= 0.25/0.5 = 0.5 → 0.5
	t.Logf("FSV death hold t=%.3f fade=%.3f (want t=0.8 fade~0.5)", d.Time(0), d.Fade(0))
	if d.Time(0) != 0.8 || d.Fade(0) < 0.49 || d.Fade(0) > 0.51 {
		t.Fatalf("death hold t=%.3f fade=%.3f want 0.8/~0.5", d.Time(0), d.Fade(0))
	}
	d.Update(st, vis(1), 1.0, clips, fade) // fade -= 2 → clamps 0
	t.Logf("FSV death fade clamps to %.3f (want 0)", d.Fade(0))
	if d.Fade(0) != 0 {
		t.Fatalf("fade=%.3f want clamped 0", d.Fade(0))
	}
}

// TestAnimTransitionResetsFSV — a state change mid-clip switches clip and
// restarts at frame 0 (no T-pose; edge case 1).
func TestAnimTransitionResetsFSV(t *testing.T) {
	d := NewAnimDriver(1)
	clips := testClips()
	d.Update([]SimAnimState{StateMove}, vis(1), 0.5, clips, 1) // Walk at 0.5
	if d.Clip(0) != ClipWalk || d.Time(0) != 0.5 {
		t.Fatalf("pre-switch clip=%d t=%.3f", d.Clip(0), d.Time(0))
	}
	// Move canceled → Idle: clip switches, time resets to 0 (valid frame).
	d.Update([]SimAnimState{StateIdle}, vis(1), 0.016, clips, 1)
	t.Logf("FSV transition Walk@0.5 -> Idle clip=%d t=%.3f (want Idle, advanced from 0)", d.Clip(0), d.Time(0))
	if d.Clip(0) != ClipIdle {
		t.Fatalf("clip=%d want Idle after switch", d.Clip(0))
	}
	// Time restarted at 0 then advanced by this frame's dt (0.016) — not 0.516.
	if d.Time(0) < 0.015 || d.Time(0) > 0.017 {
		t.Fatalf("post-switch t=%.4f want ~0.016 (restarted, no carry-over)", d.Time(0))
	}
}

// TestAnimCullSkipResumeFSV — a culled unit freezes its phase and resumes there
// on re-entry, never a T-pose (edge case 3).
func TestAnimCullSkipResumeFSV(t *testing.T) {
	d := NewAnimDriver(1)
	clips := testClips()
	st := []SimAnimState{StateMove}
	d.Update(st, []bool{true}, 0.4, clips, 1) // Walk at 0.4
	frozen := d.Time(0)
	// Culled for several frames: time must not advance.
	for i := 0; i < 5; i++ {
		d.Update(st, []bool{false}, 0.1, clips, 1)
	}
	t.Logf("FSV culled walk t=%.3f (frozen at %.3f)", d.Time(0), frozen)
	if d.Time(0) != frozen {
		t.Fatalf("culled time advanced: %.3f != %.3f", d.Time(0), frozen)
	}
	// Re-enter: resumes from the frozen phase (valid, not 0/T-pose).
	d.Update(st, []bool{true}, 0.1, clips, 1)
	t.Logf("FSV re-enter walk t=%.3f (want %.3f+0.1)", d.Time(0), frozen)
	if d.Time(0) < frozen+0.099 || d.Time(0) > frozen+0.101 {
		t.Fatalf("resume t=%.4f want %.4f", d.Time(0), frozen+0.1)
	}
	if d.Clip(0) != ClipWalk {
		t.Fatalf("resumed clip=%d want Walk (no T-pose)", d.Clip(0))
	}
}

// TestAnimAttackImpactFSV — the impact fires exactly once, the frame clip time
// crosses ImpactTime.
func TestAnimAttackImpactFSV(t *testing.T) {
	d := NewAnimDriver(1)
	clips := testClips() // Attack impact at 0.3
	st := []SimAnimState{StateAttack}
	d.Update(st, vis(1), 0, clips, 1) // init at 0
	impacts := 0
	// Step in 0.1s frames to 0.6s; impact must fire once, between 0.2 and 0.3 cross.
	for i := 0; i < 6; i++ {
		d.Update(st, vis(1), 0.1, clips, 1)
		if d.Impacted(0) {
			impacts++
			t.Logf("FSV attack impact at clipTime=%.3f", d.Time(0))
		}
	}
	if impacts != 1 {
		t.Fatalf("attack impact fired %d times, want exactly 1", impacts)
	}
}

func TestAnimZeroAllocFSV(t *testing.T) {
	const n = 500
	d := NewAnimDriver(n)
	clips := testClips()
	states := make([]SimAnimState, n)
	visible := make([]bool, n)
	for i := 0; i < n; i++ {
		states[i] = SimAnimState(i % 4)
		visible[i] = i%3 != 0 // a third culled
	}
	d.Update(states, visible, 0.016, clips, 1) // warm
	allocs := testing.AllocsPerRun(200, func() {
		d.Update(states, visible, 0.016, clips, 1)
	})
	t.Logf("FSV 500-unit anim Update allocs/op=%v", allocs)
	if allocs != 0 {
		t.Fatalf("anim driver allocates %v/op for 500 units, want 0", allocs)
	}
}
