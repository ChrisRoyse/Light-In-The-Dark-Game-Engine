package sim

// #331 linear/pierce skillshot tests. SoT = victim Healths.Life after
// the deferred-damage pass, the OnMissileImpact/Expire event log, and
// per-advance missile position; plus twin-hash + save v8 round-trip.

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func lmWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	return w
}

func life(w *World, id EntityID) int64 {
	hr := w.Healths.Row(id)
	if hr == -1 {
		return -1
	}
	return w.Healths.Life[hr].Floor()
}

func xy(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }

// Edge 1 (happy path + foe filter): a linear shot down the +X axis hits
// the enemy in its lane, skips the friendly standing in the same lane,
// and dies (pierce 1) at impact.
func TestLinearHitsFoeSkipsAlly(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	ally := atkUnit(t, w, 0, xy(1150, 1000), 0)    // same team — must NOT be hit
	foe := atkUnit(t, w, 1, xy(1300, 1000), 0)     // enemy in lane
	offlane := atkUnit(t, w, 1, xy(1300, 1400), 0) // enemy off lane — must NOT be hit
	var impacts []string
	w.OnMissileImpact = func(tick uint32, mid EntityID, at fixed.Vec2, tgt EntityID) {
		impacts = append(impacts, fmt.Sprintf("t%d at(%d,%d) tgt=%d", tick, at.X.Floor(), at.Y.Floor(), tgt))
	}
	id, ok := w.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Speed: 100 * fixed.One,
		Flags: MissileLinear, Dir: xy(1, 0), Range: 1000 * fixed.One, Pierce: 1,
		Packet: DamagePacket{Source: shooter, Amount: 30 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn linear")
	}
	for i := 0; i < 8 && w.Ents.Alive(id); i++ {
		w.Step()
		t.Logf("t%d missile@%s allyHP=%d foeHP=%d offHP=%d", w.Tick(), missilePos(w, id), life(w, ally), life(w, foe), life(w, offlane))
	}
	for _, l := range impacts {
		t.Logf("impact %s", l)
	}
	if life(w, foe) != 70 {
		t.Fatalf("foe life=%d, want 70 (took the 30 shot)", life(w, foe))
	}
	if life(w, ally) != 100 {
		t.Fatalf("ally life=%d, want 100 (friendly must not be hit)", life(w, ally))
	}
	if life(w, offlane) != 100 {
		t.Fatalf("off-lane foe life=%d, want 100 (outside collision radius)", life(w, offlane))
	}
	if len(impacts) != 1 {
		t.Fatalf("want exactly 1 impact, got %d", len(impacts))
	}
	if w.Ents.Alive(id) {
		t.Fatal("pierce-1 missile must die at its single hit")
	}
}

// Edge 2 (pierce + front-to-back decay): three enemies in one lane, all
// inside a single advance window (speed 500), pierce 3, decay 500‰. The
// FRONT enemy takes full damage, the next half, the next a quarter —
// proving both the pierce-continue and the deterministic front-to-back
// hit ordering (§3.2 tuple) drive the decay.
func TestLinearPierceDecayOrder(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	f1 := atkUnit(t, w, 1, xy(1200, 1000), 0)
	f2 := atkUnit(t, w, 1, xy(1400, 1000), 0)
	f3 := atkUnit(t, w, 1, xy(1600, 1000), 0)
	id, ok := w.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Speed: 500 * fixed.One,
		Flags: MissileLinear, Dir: xy(1, 0), Range: 2000 * fixed.One, Pierce: 3, Decay: 500,
		Packet: DamagePacket{Source: shooter, Amount: 40 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn")
	}
	for i := 0; i < 6 && w.Ents.Alive(id); i++ {
		w.Step()
	}
	t.Logf("front=%d mid=%d back=%d (want 60/80/90)", life(w, f1), life(w, f2), life(w, f3))
	if life(w, f1) != 60 || life(w, f2) != 80 || life(w, f3) != 90 {
		t.Fatalf("decay order wrong: f1=%d f2=%d f3=%d, want 60/80/90", life(w, f1), life(w, f2), life(w, f3))
	}
}

// Edge 3 (max-hits cap): pierce 2 against three foes all in one window.
// The two FRONT foes are consumed; the third is never touched and the
// missile dies with the pierce budget spent.
func TestLinearPierceCap(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	f1 := atkUnit(t, w, 1, xy(1200, 1000), 0)
	f2 := atkUnit(t, w, 1, xy(1400, 1000), 0)
	f3 := atkUnit(t, w, 1, xy(1600, 1000), 0)
	id, _ := w.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Speed: 800 * fixed.One,
		Flags: MissileLinear, Dir: xy(1, 0), Range: 2000 * fixed.One, Pierce: 2,
		Packet: DamagePacket{Source: shooter, Amount: 25 * fixed.One},
	})
	w.Step()
	t.Logf("after 1 advance: f1=%d f2=%d f3=%d alive=%v", life(w, f1), life(w, f2), life(w, f3), w.Ents.Alive(id))
	if life(w, f1) != 75 || life(w, f2) != 75 {
		t.Fatalf("front two not both hit: f1=%d f2=%d", life(w, f1), life(w, f2))
	}
	if life(w, f3) != 100 {
		t.Fatalf("third foe hit past the pierce cap: f3=%d", life(w, f3))
	}
	if w.Ents.Alive(id) {
		t.Fatal("missile must die once the pierce budget is spent")
	}
}

// Edge 4 (range expiry, empty lane): a shot into empty space expires at
// max range with no payload and an OnMissileExpire signal.
func TestLinearRangeExpiry(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	expired := ""
	w.OnMissileExpire = func(tick uint32, mid EntityID, last fixed.Vec2) {
		expired = fmt.Sprintf("t%d last(%d,%d)", tick, last.X.Floor(), last.Y.Floor())
	}
	impacts := 0
	w.OnMissileImpact = func(uint32, EntityID, fixed.Vec2, EntityID) { impacts++ }
	id, _ := w.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Speed: 100 * fixed.One,
		Flags: MissileLinear, Dir: xy(1, 0), Range: 250 * fixed.One, Pierce: 1,
		Packet: DamagePacket{Source: shooter, Amount: 30 * fixed.One},
	})
	for i := 0; i < 6 && w.Ents.Alive(id); i++ {
		w.Step()
		t.Logf("t%d missile@%s", w.Tick(), missilePos(w, id))
	}
	t.Logf("expire=%q impacts=%d", expired, impacts)
	if impacts != 0 {
		t.Fatal("empty-lane shot delivered a payload")
	}
	if expired == "" {
		t.Fatal("range-spent missile never signalled expiry")
	}
	if w.Ents.Alive(id) {
		t.Fatal("range-spent missile not removed")
	}
}

// Edge 5 (fail-closed spawn): a linear spec with a degenerate direction,
// non-positive range, or zero pierce returns an invalid handle — never a
// silent fly-nowhere missile.
func TestLinearSpawnFailClosed(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	base := MissileSpec{Pos: xy(1000, 1000), Source: shooter, Speed: 100 * fixed.One, Flags: MissileLinear}
	cases := []struct {
		name string
		spec MissileSpec
	}{
		{"zero dir", func() MissileSpec { s := base; s.Dir = fixed.Vec2{}; s.Range = 500 * fixed.One; s.Pierce = 1; return s }()},
		{"zero range", func() MissileSpec { s := base; s.Dir = xy(1, 0); s.Range = 0; s.Pierce = 1; return s }()},
		{"neg range", func() MissileSpec { s := base; s.Dir = xy(1, 0); s.Range = -fixed.One; s.Pierce = 1; return s }()},
		{"zero pierce", func() MissileSpec { s := base; s.Dir = xy(1, 0); s.Range = 500 * fixed.One; s.Pierce = 0; return s }()},
	}
	before := w.Missiles.Count()
	for _, c := range cases {
		id, ok := w.SpawnMissile(c.spec)
		t.Logf("%-11s -> id=%d ok=%v", c.name, id.Index(), ok)
		if ok || id != 0 {
			t.Fatalf("%s: expected invalid handle, got id=%d ok=%v", c.name, id.Index(), ok)
		}
	}
	if w.Missiles.Count() != before {
		t.Fatalf("a failed linear spawn leaked a row: %d -> %d", before, w.Missiles.Count())
	}
}

// Determinism twin + save v8: a pierce missile mid-flight (one hit
// spent) twins and round-trips byte-identically, then resumes the same.
func TestLinearDeterminismAndSave(t *testing.T) {
	build := func() *World {
		w := lmWorld(t)
		shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
		atkUnit(t, w, 1, xy(1200, 1000), 0)
		atkUnit(t, w, 1, xy(1900, 1000), 0)
		w.SpawnMissile(MissileSpec{
			Pos: xy(1000, 1000), Source: shooter, Speed: 300 * fixed.One,
			Flags: MissileLinear, Dir: xy(1, 0), Range: 2000 * fixed.One, Pierce: 3, Decay: 700,
			Packet: DamagePacket{Source: shooter, Amount: 40 * fixed.One},
		})
		w.Step() // first foe consumed; missile mid-flight with pierce + decay state
		return w
	}
	a, b := build(), build()
	var sa, sb statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa)
	b.HashState(NewHashRegistry(), &sb)
	t.Logf("twin A=%016x B=%016x missileRows=%d", sa.Top, sb.Top, a.Missiles.Count())
	if sa.Top != sb.Top {
		t.Fatal("twin divergence")
	}
	if a.Missiles.Count() != 1 {
		t.Fatalf("expected the mid-flight missile to persist, rows=%d", a.Missiles.Count())
	}
	// flip a linear field → hash must move (it is real state)
	r := int32(0)
	a.Missiles.PierceLeft[r]++
	var sa2 statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("PierceLeft mutation invisible to the hash")
	}
	a.Missiles.PierceLeft[r]--

	var buf bytes.Buffer
	if err := a.SaveState(&buf, 5); err != nil {
		t.Fatal(err)
	}
	w2 := lmWorld(t)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 5); err != nil {
		t.Fatal(err)
	}
	var sl statehash.Snapshot
	w2.HashState(NewHashRegistry(), &sl)
	t.Logf("loaded=%016x (orig %016x)", sl.Top, sa.Top)
	if sl.Top != sa.Top {
		t.Fatal("save v8 load diverged from the original")
	}
	for i := 0; i < 8; i++ {
		a.Step()
		w2.Step()
	}
	a.HashState(NewHashRegistry(), &sa)
	w2.HashState(NewHashRegistry(), &sl)
	if sa.Top != sl.Top {
		t.Fatal("post-load resume diverged")
	}
}

func TestLinearAdvanceAllocs(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	for i := int32(0); i < 5; i++ {
		atkUnit(t, w, 1, xy(1200+i*60, 1000), 0)
	}
	w.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Speed: 80 * fixed.One,
		Flags: MissileLinear, Dir: xy(1, 0), Range: 4000 * fixed.One, Pierce: 99, Decay: 1000,
		Packet: DamagePacket{Source: shooter, Amount: 5 * fixed.One},
	})
	w.Step()
	allocs := testing.AllocsPerRun(50, func() { w.Step() })
	t.Logf("allocs/op advancing a linear pierce missile: %v", allocs)
	if allocs != 0 {
		t.Fatalf("linear advance allocates: %v", allocs)
	}
}
