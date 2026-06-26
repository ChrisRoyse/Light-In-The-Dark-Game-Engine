package sim

// Aura tests (#164). SoT: tick-stamped child-set dumps, instance
// tables, EvBuffExpired traces.

import (
	"fmt"
	"testing"
	"testing/fstest"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// auraTables: "command" aura (radius 200, child "cmd-child" ×1.5
// speed with refresh stacking, linger 1s = 20t) and "indep" aura
// (child "ind-child" with independent stacking — per-source children).
func auraTables(t *testing.T) *data.Tables {
	t.Helper()
	fsys := fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte("attack-types = [\"magic\"]\narmor-types = [\"none\"]\n[coefficients]\nmagic = [1000]\n")},
		"buffs/core.toml": &fstest.MapFile{Data: []byte(`
[[buff]]
id = "cmd-child"
duration = 1.0
stacking = "refresh"
[[buff.mod]]
stat = "move-speed"
permille = 1500

[[buff]]
id = "command"
duration = 100.0
stacking = "refresh"
[buff.aura]
radius = 200.0
child = "cmd-child"
linger = 1.0

[[buff]]
id = "ind-child"
duration = 1.0
stacking = "independent"
[[buff.mod]]
stat = "armor"
add = 2

[[buff]]
id = "indep"
duration = 100.0
stacking = "independent"
[buff.aura]
radius = 200.0
child = "ind-child"
linger = 1.0
`)},
	}
	tb, err := data.Load(fsys)
	if err != nil {
		t.Fatalf("aura tables must load: %v", err)
	}
	return tb
}

func auraWorld(t *testing.T) (*World, *data.Tables) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := auraTables(t)
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(tb.Coeff); err != nil {
		t.Fatal(err)
	}
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	if !w.BindBuffTypes(tb.BuffTypes) {
		t.Fatal("BindBuffTypes failed")
	}
	return w, tb
}

// childDump renders target's aura-child rows: the FSV child-set SoT.
func childDump(w *World, target EntityID) string {
	s := ""
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == target && p.rows[i].Flags&BuffInstAuraChild != 0 {
			r := &p.rows[i]
			s += fmt.Sprintf("{slot%d t%d src%d rem%d} ", i, r.BuffID, r.Source, r.RemainingTicks)
		}
	}
	if s == "" {
		return "(none)"
	}
	return s
}

func childCount(w *World, target EntityID) int {
	n := 0
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == target && p.rows[i].Flags&BuffInstAuraChild != 0 {
			n++
		}
	}
	return n
}

// Happy path + edge 1: an ally in radius gains the child on the first
// evaluation; walking out, the child persists EXACTLY linger ticks
// past its last refresh, then expires with EvBuffExpired. An ally
// beyond radius never gains one.
func TestAuraWalkOutLinger(t *testing.T) {
	w, tb := auraWorld(t)
	cmd := buffTypeIdx(t, tb, "command")
	src := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	ally := atkUnit(t, w, 0, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0)  // in radius (100)
	far := atkUnit(t, w, 0, fixed.Vec2{X: 9000 * fixed.One, Y: 9000 * fixed.One}, 0)   // out (≫200)
	enemy := atkUnit(t, w, 1, fixed.Vec2{X: 1050 * fixed.One, Y: 1000 * fixed.One}, 0) // hostile in radius
	w.ApplyBuff(src, src, cmd, 1)

	var expiredAt []uint32
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvBuffExpired && e.Src == ally {
			expiredAt = append(expiredAt, w.Tick())
		}
	})
	w.Subscribe(EvBuffExpired, 1)

	gained := uint32(0)
	var lastRefresh uint32
	tr := w.Transforms.Row(ally)
	for w.Tick() < 60 {
		w.Step()
		if gained == 0 && childCount(w, ally) == 1 {
			gained = w.Tick()
			t.Logf("t%d: ally gained child: %s", w.Tick(), childDump(w, ally))
		}
		// walk out by teleport once established (bucket reconciles next phase 4)
		if gained != 0 && w.Tick() == gained+7 {
			w.Transforms.Pos[tr] = fixed.Vec2{X: 8000 * fixed.One, Y: 8000 * fixed.One}
			t.Logf("t%d: ally teleported out; child: %s", w.Tick(), childDump(w, ally))
		}
		// track the last refresh (RemainingTicks back at linger 20 means
		// an in-range evaluation hit this tick; decrement runs phase 7)
		p := w.Buffs
		for i := int32(0); int(i) < p.Cap(); i++ {
			if p.live[i] && p.rows[i].Target == ally && p.rows[i].Flags&BuffInstAuraChild != 0 &&
				p.rows[i].RemainingTicks == 19 { // post-sweep value of a this-tick refresh
				lastRefresh = w.Tick()
			}
		}
	}
	if gained == 0 {
		t.Fatal("ally never gained the child")
	}
	if childCount(w, far) != 0 {
		t.Errorf("out-of-radius ally has a child: %s", childDump(w, far))
	}
	if childCount(w, enemy) != 0 {
		t.Errorf("ENEMY in radius has a child: %s", childDump(w, enemy))
	}
	if len(expiredAt) != 1 {
		t.Fatalf("EvBuffExpired for ally fired %d times (%v), want 1", len(expiredAt), expiredAt)
	}
	// expiry sweep tick = lastRefresh + linger(20) − 1, dispatched +1
	wantDispatch := lastRefresh + 20
	t.Logf("last refresh t%d + linger 20t → expiry dispatch t%d (got t%d)", lastRefresh, wantDispatch, expiredAt[0])
	if expiredAt[0] != wantDispatch {
		t.Errorf("expiry dispatched t%d, want t%d", expiredAt[0], wantDispatch)
	}
}

// Edge 2: re-entering during the linger refreshes the SAME instance —
// count stays 1 throughout, no expiry fires.
func TestAuraReenterDuringLinger(t *testing.T) {
	w, tb := auraWorld(t)
	cmd := buffTypeIdx(t, tb, "command")
	src := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	ally := atkUnit(t, w, 0, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0)
	w.ApplyBuff(src, src, cmd, 1)
	expired := 0
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvBuffExpired {
			expired++
		}
	})
	w.Subscribe(EvBuffExpired, 1)

	tr := w.Transforms.Row(ally)
	out, back := uint32(0), uint32(0)
	maxCount := 0
	for w.Tick() < 60 {
		w.Step()
		if out == 0 && childCount(w, ally) == 1 {
			out = w.Tick() + 2
			back = out + 10 // re-enter 10 ticks into the 20-tick linger
		}
		if w.Tick() == out {
			w.Transforms.Pos[tr] = fixed.Vec2{X: 8000 * fixed.One, Y: 8000 * fixed.One}
			t.Logf("t%d: out;  %s", w.Tick(), childDump(w, ally))
		}
		if w.Tick() == back {
			w.Transforms.Pos[tr] = fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}
			t.Logf("t%d: back; %s", w.Tick(), childDump(w, ally))
		}
		if c := childCount(w, ally); c > maxCount {
			maxCount = c
		}
	}
	t.Logf("end: count=%d max=%d expired=%d, %s", childCount(w, ally), maxCount, expired, childDump(w, ally))
	if maxCount != 1 {
		t.Errorf("child duplicated: max instance count %d", maxCount)
	}
	if expired != 0 {
		t.Errorf("child expired during linger re-entry: %d events", expired)
	}
	if childCount(w, ally) != 1 {
		t.Errorf("child missing at end")
	}
}

// Edge 3: the aura source dies — its aura instance frees on the dead
// carrier, refreshes stop, and every child expires after the linger.
func TestAuraSourceDeath(t *testing.T) {
	w, tb := auraWorld(t)
	cmd := buffTypeIdx(t, tb, "command")
	src := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	a1 := atkUnit(t, w, 0, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0)
	a2 := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1120 * fixed.One}, 0)
	w.ApplyBuff(src, src, cmd, 1)
	var expiredAt []uint32
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvBuffExpired {
			expiredAt = append(expiredAt, w.Tick())
		}
	})
	w.Subscribe(EvBuffExpired, 1)

	killAt := uint32(0)
	for w.Tick() < 80 {
		w.Step()
		if killAt == 0 && childCount(w, a1) == 1 && childCount(w, a2) == 1 {
			killAt = w.Tick() + 1
			t.Logf("t%d: both allies buffed: a1=%s a2=%s", w.Tick(), childDump(w, a1), childDump(w, a2))
		}
		if w.Tick() == killAt {
			w.KillUnit(src)
			t.Logf("t%d: source killed", w.Tick())
		}
	}
	t.Logf("end: a1=%s a2=%s expiry dispatches=%v pool live=%d",
		childDump(w, a1), childDump(w, a2), expiredAt, w.Buffs.Live())
	if childCount(w, a1) != 0 || childCount(w, a2) != 0 {
		t.Fatalf("children survived source death")
	}
	if w.Buffs.Live() != 0 {
		t.Errorf("pool live = %d, want 0 (aura + children all gone)", w.Buffs.Live())
	}
	if len(expiredAt) != 2 {
		t.Errorf("expiry events = %d, want 2 (one per child; the aura frees silently with its dead carrier)", len(expiredAt))
	}
}

// Edge 4: two identical auras overlapping one unit — the CHILD type's
// stacking rule decides: refresh-stacking child stays a single shared
// instance; independent-stacking child keeps one instance per source.
func TestAuraOverlapStacking(t *testing.T) {
	w, tb := auraWorld(t)
	cmd := buffTypeIdx(t, tb, "command")
	ind := buffTypeIdx(t, tb, "indep")
	s1 := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	s2 := atkUnit(t, w, 0, fixed.Vec2{X: 1200 * fixed.One, Y: 1000 * fixed.One}, 0)
	mid := atkUnit(t, w, 0, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0)
	w.ApplyBuff(s1, s1, cmd, 1)
	w.ApplyBuff(s2, s2, cmd, 1)
	w.ApplyBuff(s1, s1, ind, 1)
	w.ApplyBuff(s2, s2, ind, 1)
	for w.Tick() < 12 {
		w.Step()
	}
	cmdChild := buffTypeIdx(t, tb, "cmd-child")
	indChild := buffTypeIdx(t, tb, "ind-child")
	nCmd, nInd := 0, 0
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] || p.rows[i].Target != mid || p.rows[i].Flags&BuffInstAuraChild == 0 {
			continue
		}
		switch int(p.rows[i].BuffID) {
		case cmdChild:
			nCmd++
		case indChild:
			nInd++
		}
	}
	t.Logf("mid instance table: %s", childDump(w, mid))
	if nCmd != 1 {
		t.Errorf("refresh-rule child instances = %d, want 1 (shared)", nCmd)
	}
	if nInd != 2 {
		t.Errorf("independent-rule child instances = %d, want 2 (per source)", nInd)
	}
	// the two armor +2 children stack through the cache: armor 0 → 4
	if got := w.BuffedArmor(mid, 0); got != 4 {
		t.Errorf("stacked armor = %d, want 4 (2 sources × +2)", got)
	}
}

// R-GC-1: aura maintenance allocates nothing per tick.
func TestAuraTickAllocs(t *testing.T) {
	w, tb := auraWorld(t)
	cmd := buffTypeIdx(t, tb, "command")
	src := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	for i := 0; i < 8; i++ {
		atkUnit(t, w, 0, fixed.Vec2{X: fixed.FromInt(int32(1020 + i*15)), Y: 1000 * fixed.One}, 0)
	}
	w.ApplyBuff(src, src, cmd, 1)
	for i := 0; i < 10; i++ {
		w.Step() // settle: children established
	}
	avg := testing.AllocsPerRun(200, func() { w.Step() })
	if avg != 0 {
		t.Fatalf("allocs/tick = %v, want 0 (R-GC-1)", avg)
	}
	t.Logf("allocs/tick = %v, pool live=%d", avg, w.Buffs.Live())
}
