package sim

// #199: the permanent full-sim determinism fixture. 10,000 ticks of
// the REAL World — movement, acquisition, combat, deaths, orders,
// PRNG — from a fixed seed plus a seed-derived command stream, hash
// trace every 100 ticks (100 entries, top + per-system sub-hashes).
// Two independent runs must be bit-identical; a divergence names its
// first 100-tick window and culprit system. CI-matrix activation
// (cross-OS/arch cells) queues behind #284; this test is the cell
// body and runs in plain `go test` in seconds.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	det10kSeed  = 0xC0FFEE
	det10kUnits = 256
	det10kTicks = 10_000
	det10kEvery = 100
)

// battleWorld builds the full-combatant layout (the cmd/headless
// twin): alternating teams, health, movement, melee weapons,
// acquisition — real fights, deaths, and orders.
func battleWorld(tb testing.TB, seed uint64, n int) (*World, []EntityID) {
	tb.Helper()
	w := NewWorld(Caps{})
	w.SetSeed(seed)
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		tb.Fatal(err)
	}
	weapon := data.Attack{
		AttackType: 0, Range: fixed.FromInt(8), DamageBase: 5, Dice: 1, Sides: 4,
		CooldownTicks: 27, DamagePointTicks: 10, BackswingTicks: 10,
	}
	rng := prng.New(seed, 0)
	ids := make([]EntityID, 0, n)
	for i := 0; i < n; i++ {
		pos := fixed.Vec2{
			X: fixed.FromInt(int32(rng.Uint32() % 512)),
			Y: fixed.FromInt(int32(rng.Uint32() % 512)),
		}
		id, ok := w.CreateUnit(pos, fixed.Angle(rng.Uint32()%65536))
		if !ok {
			tb.Fatal("unit cap")
		}
		team := uint8(i % 2)
		if !w.Owners.Add(w.Ents, id, team, team, team) ||
			!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) ||
			!w.Combats.Add(w.Ents, id) ||
			!w.Orders.Add(w.Ents, id) ||
			!w.Movements.Add(w.Ents, w.Transforms, id, fixed.One*7/2, 2048) {
			tb.Fatal("component add failed")
		}
		if !w.SetWeapon(id, 0, &weapon, 0, data.EffectList{}) {
			tb.Fatal("weapon set failed")
		}
		w.Combats.AcquisitionRange[w.Combats.Row(id)] = fixed.FromInt(24)
		ids = append(ids, id)
	}
	return w, ids
}

// det10kTrace runs the scenario once and returns the 100-entry trace.
// Commands derive from the seed's sub-stream 99 (fixed input, never
// wall-clock anything): a move order to a random board point every
// 13–49 ticks for a random roster unit.
func det10kTrace(tb testing.TB) []statehash.Snapshot {
	tb.Helper()
	w, ids := battleWorld(tb, det10kSeed, det10kUnits)
	cs := prng.Split(det10kSeed, 99)
	nextCmd := cs.Uint32()%37 + 13
	reg := NewHashRegistry()
	trace := make([]statehash.Snapshot, 0, det10kTicks/det10kEvery)
	for t := uint32(1); t <= det10kTicks; t++ {
		for nextCmd == t {
			u := ids[cs.Uint32()%uint32(len(ids))]
			pt := fixed.Vec2{
				X: fixed.FromInt(int32(cs.Uint32() % 512)),
				Y: fixed.FromInt(int32(cs.Uint32() % 512)),
			}
			if w.Ents.Alive(u) {
				w.IssueOrder(u, Order{Kind: OrderMove, Point: pt}, false)
			}
			nextCmd += cs.Uint32()%37 + 13
		}
		w.Step()
		if t%det10kEvery == 0 {
			var s statehash.Snapshot
			w.HashState(reg, &s)
			s.Subs = append([]uint64(nil), s.Subs...)
			trace = append(trace, s)
		}
	}
	return trace
}

// TestDeterminism10k: two independent full runs produce bit-identical
// 100-entry traces. A divergence is reported as its first 100-tick
// window plus the culprit per-system sub-hash.
func TestDeterminism10k(t *testing.T) {
	if testing.Short() {
		t.Skip("10k-tick fixture skipped in -short")
	}
	a := det10kTrace(t)
	b := det10kTrace(t)
	if len(a) != det10kTicks/det10kEvery || len(b) != len(a) {
		t.Fatalf("trace lengths %d/%d, want %d", len(a), len(b), det10kTicks/det10kEvery)
	}
	for i := range a {
		if a[i].Top != b[i].Top {
			culprit := "top"
			for j := range a[i].Subs {
				if a[i].Subs[j] != b[i].Subs[j] {
					culprit = HashSystems[j]
					break
				}
			}
			t.Fatalf("DIVERGED in window ticks %d–%d: %016x vs %016x, culprit system %q",
				(i)*det10kEvery+1, (i+1)*det10kEvery, a[i].Top, b[i].Top, culprit)
		}
	}
	t.Logf("traces bit-identical: %d entries", len(a))
	t.Logf("head: t100=%016x t200=%016x t300=%016x", a[0].Top, a[1].Top, a[2].Top)
	t.Logf("tail: t9800=%016x t9900=%016x t10000=%016x", a[97].Top, a[98].Top, a[99].Top)
}
