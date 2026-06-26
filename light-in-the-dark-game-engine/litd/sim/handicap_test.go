package sim

// #373 difficulty-handicap FSV. Each handicap is wired into exactly one
// pipeline chokepoint; this proves the multiplier reaches the Source of
// Truth — the victim's Healths.Life (damage), the hero's Heroes.XP (kill
// XP), and Produce.Done (revive ticks) — with known-input/known-output
// (X+X=Y) cases, edges, and the default-1.0 no-op.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// handicapDmgWorld builds a matrix-bound world with an owned victim
// (player 1) and an owned attacker (player 0), each at 100 life, armor 0 /
// type 0 so mitigation is exactly 1.0 and a raw N packet applies N before
// handicaps. Owners are required: the handicap lookup keys off the owner row.
func handicapDmgWorld(t *testing.T) (w *World, victim, attacker EntityID) {
	t.Helper()
	w = NewWorld(Caps{})
	if err := w.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatalf("bind matrix: %v", err)
	}
	mk := func(x int32, player, team uint8) EntityID {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(100)}, 0)
		if !ok ||
			!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) ||
			!w.Combats.Add(w.Ents, id) ||
			!w.Owners.Add(w.Ents, id, player, team, player) {
			t.Fatal("spawn failed")
		}
		return id
	}
	victim = mk(100, 1, 1)
	attacker = mk(200, 0, 0)
	return w, victim, attacker
}

// TestHandicapDamageFSV — source handicapDamage and target handicap both
// scale the applied amount. SoT: victim Healths.Life before/after a 40-raw
// packet. armor 0 / type 0 → 1.0 mitigation, so the only scaling is the
// handicaps.
func TestHandicapDamageFSV(t *testing.T) {
	type tc struct {
		name        string
		dealMul     float64 // attacker(player 0) handicapDamage
		takeMul     float64 // victim(player 1) handicap
		wantApplied int64   // game units lost
	}
	mul := func(f float64) fixed.F64 { return fixed.F64(int64(f * float64(fixed.One))) }
	cases := []tc{
		{"default 1.0 (no-op)", 1, 1, 40},   // X+X=Y baseline: full 40 applies
		{"deal 0.5", 0.5, 1, 20},            // 40×0.5
		{"take 0.5", 1, 0.5, 20},            // 40×0.5
		{"both 0.5 (compounds)", 0.5, 0.5, 10}, // 40×0.5×0.5
		{"deal 2.0 (amplify)", 2, 1, 80},    // 40×2
	}
	for _, c := range cases {
		w, victim, attacker := handicapDmgWorld(t)
		w.SetHandicapDamage(0, mul(c.dealMul))
		w.SetHandicap(1, mul(c.takeMul))
		hr := w.Healths.Row(victim)
		before := w.Healths.Life[hr]
		stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 0})
		after := w.Healths.Life[hr]
		applied := fxUnits(before) - fxUnits(after)
		t.Logf("FSV %-22s deal=%.2f take=%.2f : life %d -> %d (applied %d, want %d)",
			c.name, c.dealMul, c.takeMul, fxUnits(before), fxUnits(after), applied, c.wantApplied)
		if applied != c.wantApplied {
			t.Fatalf("%s: applied %d units, want %d", c.name, applied, c.wantApplied)
		}
	}
}

// TestHandicapClampFSV — edge: a negative handicap clamps to 0 (read-back
// proves it) and zero-damage means the victim's life is untouched. SoT: the
// stored handicap and the victim's life.
func TestHandicapClampFSV(t *testing.T) {
	w, victim, attacker := handicapDmgWorld(t)
	w.SetHandicap(1, -5*fixed.One)
	got := w.Handicap(1)
	t.Logf("FSV clamp: SetHandicap(-5) -> stored %d (want 0)", int64(got))
	if got != 0 {
		t.Fatalf("negative handicap not clamped: %d", int64(got))
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 0})
	after := w.Healths.Life[hr]
	t.Logf("FSV clamp: life %d -> %d (0× damage, want unchanged)", fxUnits(before), fxUnits(after))
	if after != before {
		t.Fatalf("0-handicap victim took damage: %d -> %d", fxUnits(before), fxUnits(after))
	}
}

// TestHandicapDefaultsFSV — a fresh world reads 1.0 for every handicap on
// every slot, and an out-of-range slot also reads 1.0. SoT: the getters.
func TestHandicapDefaultsFSV(t *testing.T) {
	w := NewWorld(Caps{})
	for p := uint8(0); p < MaxPlayers; p++ {
		if w.Handicap(p) != fixed.One || w.HandicapDamage(p) != fixed.One ||
			w.HandicapXP(p) != fixed.One || w.HandicapReviveTime(p) != fixed.One {
			t.Fatalf("slot %d not defaulted to 1.0", p)
		}
	}
	if w.Handicap(200) != fixed.One {
		t.Fatalf("out-of-range slot getter = %d, want 1.0", int64(w.Handicap(200)))
	}
	t.Logf("FSV defaults: all %d slots + out-of-range read 1.0 (%d)", MaxPlayers, int64(fixed.One))
}

// TestHandicapXPFSV — handicapXP scales a hero's kill-XP share. SoT: the
// killer hero's Heroes.XP after a lethal packet on a bounty-25 worker. One
// hero in range, so the full bounty (25) is the unscaled share.
func TestHandicapXPFSV(t *testing.T) {
	type tc struct {
		name   string
		xpMul  float64
		wantXP int64
	}
	mul := func(f float64) fixed.F64 { return fixed.F64(int64(f * float64(fixed.One))) }
	cases := []tc{
		{"default 1.0", 1, 25},  // X+X=Y: bounty 25 unscaled
		{"double 2.0", 2, 50},   // 25×2
		{"half 0.5 (trunc)", 0.5, 12}, // 25×0.5 = 12.5 → trunc 12
		{"zero", 0, 0},          // no XP
	}
	for _, c := range cases {
		w := heroWorld(t)
		hero, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
		w.SetHandicapXP(0, mul(c.xpMul))
		victim, _ := w.SpawnFromTable(tWorker, 1, 1, pt2(120, 100))
		r := w.Heroes.Row(hero)
		before := w.Heroes.XP[r]
		stepWithPackets(w, DamagePacket{Source: hero, Target: victim, Amount: fixed.FromInt(10000)})
		after := w.Heroes.XP[r]
		t.Logf("FSV %-16s xpMul=%.2f : XP %d -> %d (want %d) victimAlive=%v",
			c.name, c.xpMul, before, after, c.wantXP, w.Ents.Alive(victim))
		if after != c.wantXP {
			t.Fatalf("%s: hero XP = %d, want %d", c.name, after, c.wantXP)
		}
	}
}

// TestHandicapReviveTimeFSV — handicapReviveTime scales the queued revive
// duration. SoT: Produce.Done minus the current tick at admission. A level-1
// record costs BaseTicks=10; the handicap scales it.
func TestHandicapReviveTimeFSV(t *testing.T) {
	type tc struct {
		name      string
		revMul    float64
		wantTicks uint32
	}
	mul := func(f float64) fixed.F64 { return fixed.F64(int64(f * float64(fixed.One))) }
	cases := []tc{
		{"default 1.0", 1, 10}, // X+X=Y: base 10 ticks for level 1
		{"double 2.0", 2, 20},  // 10×2
		{"half 0.5", 0.5, 5},   // 10×0.5
	}
	for _, c := range cases {
		w := heroWorld(t)
		w.resources[0][0] = 1000
		altar, ok := w.SpawnFromTable(tAltar, 0, 0, pt2(200, 200))
		if !ok {
			t.Fatal("altar spawn failed")
		}
		hero, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100)) // level 1
		w.KillUnit(hero)
		w.Step() // phase-7 capture into dead pool
		if !w.DeadHero(0, 0).Used {
			t.Fatal("dead-hero pool slot not filled")
		}
		w.SetHandicapReviveTime(0, mul(c.revMul))
		atTick := w.Tick()
		if got := w.ReviveHero(altar, 0); got != TrainOK {
			t.Fatalf("%s: ReviveHero = %d", c.name, got)
		}
		pr := w.Produce.Row(altar)
		dur := w.Produce.Done[pr] - atTick
		t.Logf("FSV %-12s revMul=%.2f : Done=%d atTick=%d -> %d ticks (want %d)",
			c.name, c.revMul, w.Produce.Done[pr], atTick, dur, c.wantTicks)
		if dur != c.wantTicks {
			t.Fatalf("%s: revive duration %d ticks, want %d", c.name, dur, c.wantTicks)
		}
	}
}

// TestHandicapSaveRoundTripFSV — the four handicaps survive a save→load
// cycle (v21) and the post-load hash matches. SoT: the handicap getters on
// the reloaded world plus the full-World state hash.
func TestHandicapSaveRoundTripFSV(t *testing.T) {
	mul := func(f float64) fixed.F64 { return fixed.F64(int64(f * float64(fixed.One))) }
	w := NewWorld(Caps{})
	w.SetHandicap(0, mul(0.6))
	w.SetHandicapDamage(0, mul(0.5))
	w.SetHandicapXP(2, mul(1.5))
	w.SetHandicapReviveTime(3, mul(0.5))
	reg := NewHashRegistry()
	var before statehash.Snapshot
	w.HashState(reg, &before)

	var buf bytes.Buffer
	const fp = 0xABCDEF
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := NewWorld(Caps{})
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	t.Logf("FSV reload: p0 handicap=%d dmg=%d ; p2 xp=%d ; p3 revive=%d",
		int64(w2.Handicap(0)), int64(w2.HandicapDamage(0)), int64(w2.HandicapXP(2)), int64(w2.HandicapReviveTime(3)))
	if w2.Handicap(0) != mul(0.6) || w2.HandicapDamage(0) != mul(0.5) ||
		w2.HandicapXP(2) != mul(1.5) || w2.HandicapReviveTime(3) != mul(0.5) {
		t.Fatal("handicaps not restored")
	}
	// untouched slots stay at the 1.0 default after load.
	if w2.HandicapXP(0) != fixed.One {
		t.Fatalf("untouched slot drifted: %d", int64(w2.HandicapXP(0)))
	}
	var after statehash.Snapshot
	w2.HashState(reg, &after)
	t.Logf("FSV hash: orig=%016x reload=%016x", before.Top, after.Top)
	if before.Top != after.Top {
		t.Fatalf("post-load hash mismatch: %016x vs %016x", before.Top, after.Top)
	}
}
