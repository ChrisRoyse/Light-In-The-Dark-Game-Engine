package sim

// #207: save-load-resume hash-identical fixture (milestones.md M3
// exit 6). An unbroken run records a hash trace; runs resumed from
// saves taken at parameterized ticks must reproduce that trace
// row-for-row from the save tick to the end — not just the final
// hash. SoT = the side-by-side trace tables this test prints.
//
// The scenario carries every save-sensitive machine the engine has
// today: real combat with deaths (entity free-list churn), missiles
// in flight, periodic buffs, queued pooled orders, and a
// self-re-arming scheduler continuation (a sleep AND a pending timer
// at every possible save point). A parked path search is impossible
// until #333 wires pathfinding into the World.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	resumeTicks                 = 5000
	resumeTraceEvr              = 50
	resumeCont     sched.ContID = 13
	resumePeriod   uint32       = 37
)

// resumeWorld builds the #207 scenario. The returned fired slice
// records every continuation wake tick; the continuation re-arms
// itself and applies a periodic poison to a live unit chosen by
// rotating its state counter — pure sim state, deterministic.
func resumeWorld(t *testing.T, tb *data.Tables) (*World, *[]uint32) {
	t.Helper()
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
	fired := &[]uint32{}
	poison := buffTypeIdx(t, tb, "poison")
	w.Sched.Register(resumeCont, func(s *sched.Scheduler, st sched.State) {
		*fired = append(*fired, s.Now())
		if n := w.Transforms.Count(); n > 0 {
			row := int32(st[0]) % n
			w.ApplyBuff(w.Transforms.Entity[row], w.Transforms.Entity[row], poison, 1)
		}
		st[0]++
		s.After(resumePeriod, resumeCont, st)
	})
	w.SetSeed(0x5A7E)

	ranged := data.Attack{
		AttackType: 0, Range: 60 * fixed.One,
		DamageBase: 2, Dice: 1, Sides: 3,
		CooldownTicks: 22, DamagePointTicks: 5, BackswingTicks: 5,
		ProjectileSpeedPerTick: fixed.One,
	}
	pt := func(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }
	for i := 0; i < 40; i++ {
		team := uint8(i % 2)
		pos := pt(1000+50*int32(team), 1000+6*int32(i/2))
		id := atkUnit(t, w, team, pos, fixed.One*2)
		hr := w.Healths.Row(id)
		w.Healths.MaxLife[hr] = 200 * fixed.One
		w.Healths.Life[hr] = 200 * fixed.One
		if i%4 < 2 {
			if !w.SetWeapon(id, 0, &ranged, 0, data.EffectList{}) {
				t.Fatal("SetWeapon failed")
			}
		} else {
			arm(t, w, id, 0, 0)
		}
		w.Combats.AcquisitionRange[w.Combats.Row(id)] = 150 * fixed.One
		if i == 0 { // queued pooled orders on one unit
			w.IssueOrder(id, Order{Kind: OrderMove, Point: pt(1100, 1100)}, false)
			w.IssueOrder(id, Order{Kind: OrderMove, Point: pt(900, 950)}, true)
		}
	}
	w.Sched.After(resumePeriod, resumeCont, sched.State{})
	return w, fired
}

// runResumeTrace steps to resumeTicks, hashing every resumeTraceEvr
// ticks, and snapshots the save bytes at each requested save tick.
func runResumeTrace(t *testing.T, w *World, fingerprint uint64, savePoints []uint32) (trace map[uint32]uint64, saves map[uint32][]byte) {
	t.Helper()
	reg := NewHashRegistry()
	var snap statehash.Snapshot
	trace = map[uint32]uint64{}
	saves = map[uint32][]byte{}
	want := map[uint32]bool{}
	for _, sp := range savePoints {
		want[sp] = true
	}
	for w.Tick() < resumeTicks {
		w.Step()
		tk := w.Tick()
		if tk%resumeTraceEvr == 0 || want[tk] {
			w.HashState(reg, &snap)
			trace[tk] = snap.Top
		}
		if want[tk] {
			var buf bytes.Buffer
			if err := w.SaveState(&buf, fingerprint); err != nil {
				t.Fatalf("save at tick %d: %v", tk, err)
			}
			saves[tk] = append([]byte(nil), buf.Bytes()...)
		}
	}
	return trace, saves
}

// TestSaveLoadResume is the M3 exit-6 fixture: for each save point,
// the resumed run's hash trace equals the unbroken run's from the
// save tick to tick 5,000.
func TestSaveLoadResume(t *testing.T) {
	if testing.Short() {
		t.Skip("5,000-tick resume fixture skipped in -short")
	}
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := buffTables(t)

	// the unbroken run; save point 3 lands EXACTLY on a continuation
	// wake tick (the cont first arms before tick 1, wakes every 37)
	wakeSave := uint32(resumePeriod * 67) // 2479: a wake tick mid-battle
	savePoints := []uint32{1, 2500, wakeSave}
	src, firedSrc := resumeWorld(t, tb)
	traceA, saves := runResumeTrace(t, src, tb.Fingerprint, savePoints)
	if len(*firedSrc) == 0 {
		t.Fatal("degenerate: continuation never fired in the unbroken run")
	}
	wakeSeen := false
	for _, f := range *firedSrc {
		if f == wakeSave {
			wakeSeen = true
		}
	}
	if !wakeSeen {
		t.Fatalf("save point %d is not a wake tick (fires: %v...)", wakeSave, (*firedSrc)[:5])
	}
	t.Logf("unbroken run: %d trace rows, %d cont fires, final hash %016x, units left %d",
		len(traceA), len(*firedSrc), traceA[resumeTicks], src.UnitCount())

	reg := NewHashRegistry()
	var snap statehash.Snapshot
	for _, sp := range savePoints {
		resetEffectExecs()
		RegisterCoreEffectExecs()
		dst, firedDst := resumeWorld(t, tb)
		if err := dst.LoadState(bytes.NewReader(saves[sp]), tb.Fingerprint); err != nil {
			t.Fatalf("load save@%d: %v", sp, err)
		}
		dst.HashState(reg, &snap)
		if snap.Top != traceA[sp] {
			t.Fatalf("save@%d: restored hash %016x != unbroken %016x", sp, snap.Top, traceA[sp])
		}
		mismatches := 0
		rows := 0
		for dst.Tick() < resumeTicks {
			dst.Step()
			tk := dst.Tick()
			if tk%resumeTraceEvr == 0 {
				dst.HashState(reg, &snap)
				rows++
				if a, ok := traceA[tk]; ok && a != snap.Top {
					if mismatches == 0 {
						t.Errorf("save@%d: trace diverged at tick %d: unbroken=%016x resumed=%016x", sp, tk, a, snap.Top)
					}
					mismatches++
				}
			}
		}
		// wake fires from the save tick onward must match exactly
		// (and fire ONCE each — a double-fire would duplicate a tick)
		var tailA []uint32
		for _, f := range *firedSrc {
			if f > sp {
				tailA = append(tailA, f)
			}
		}
		if !equalU32(tailA, *firedDst) {
			t.Errorf("save@%d: wake ticks differ after resume:\n unbroken tail: %v\n resumed:       %v", sp, head(tailA, 8), head(*firedDst, 8))
		}
		t.Logf("save@%-5d (%d bytes): restored=%016x, %d trace rows compared, %d mismatches, %d wakes replayed (first %v)",
			sp, len(saves[sp]), traceA[sp], rows, mismatches, len(*firedDst), head(*firedDst, 4))
	}

	// trace excerpt around the mid-battle save for the FSV record
	for tk := uint32(2400); tk <= 2600; tk += resumeTraceEvr {
		t.Logf("trace t%-5d unbroken=%016x", tk, traceA[tk])
	}
}

// Edge 4: save → load → save again; both files restore to the same
// hash and are byte-identical.
func TestSaveLoadSaveAgain(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := buffTables(t)
	src, _ := resumeWorld(t, tb)
	for i := 0; i < 300; i++ {
		src.Step()
	}
	var s1 bytes.Buffer
	if err := src.SaveState(&s1, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	resetEffectExecs()
	RegisterCoreEffectExecs()
	mid, _ := resumeWorld(t, tb)
	if err := mid.LoadState(bytes.NewReader(s1.Bytes()), tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	var s2 bytes.Buffer
	if err := mid.SaveState(&s2, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(s1.Bytes(), s2.Bytes()) {
		t.Fatal("second-generation save is not byte-identical to the first")
	}
	resetEffectExecs()
	RegisterCoreEffectExecs()
	end, _ := resumeWorld(t, tb)
	if err := end.LoadState(bytes.NewReader(s2.Bytes()), tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	reg := NewHashRegistry()
	var h1, h2 statehash.Snapshot
	mid.HashState(reg, &h1)
	end.HashState(reg, &h2)
	if h1.Top != h2.Top {
		t.Fatalf("generation hashes differ: %016x vs %016x", h1.Top, h2.Top)
	}
	t.Logf("save→load→save: %d bytes byte-identical across generations, restored hash %016x both", s1.Len(), h1.Top)
}

func head(v []uint32, n int) []uint32 {
	if len(v) > n {
		return v[:n]
	}
	return v
}
