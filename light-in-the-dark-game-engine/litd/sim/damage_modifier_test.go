package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// fxUnits renders a fixed.F64 as whole game units for logs/asserts.
func fxUnits(v fixed.F64) int64 { return int64(v) / int64(fixed.One) }

// TestDamageModifierFSV — the #219 writable-damage hook. With armor-type 0
// / armor 0 the mitigation is exactly 1.0, so a 40 raw packet would apply
// 40. A halving modifier must make the victim lose exactly 20. SoT: the
// victim's Healths.Life before and after the tick, plus the EvUnitDamaged
// arg the post-hoc observer sees (must be the modified value).
func TestDamageModifierFSV(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)

	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	t.Logf("FSV before: victim life=%d units", fxUnits(before))

	// observer captures the EvUnitDamaged arg (the applied amount).
	var observed fixed.F64
	w.RegisterHandler(900, func(_ *World, e Event) {
		if e.Kind == EvUnitDamaged {
			observed = fixed.F64(e.Arg)
		}
	})
	w.Subscribe(EvUnitDamaged, 900)

	// halving modifier.
	w.SetDamageModifier(func(src, dst EntityID, amount fixed.F64) fixed.F64 {
		t.Logf("FSV modifier sees src=%d dst=%d amount=%d units -> %d units", src, dst, fxUnits(amount), fxUnits(amount)/2)
		return amount.Div(fixed.FromInt(2))
	})

	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 0})

	after := w.Healths.Life[hr]
	t.Logf("FSV after: victim life=%d units ; observed applied=%d units", fxUnits(after), fxUnits(observed))

	// 40 raw, halved => 20 applied => 100 - 20 = 80.
	if after != 80*fixed.One {
		t.Fatalf("victim life = %d units, want 80 (40 raw halved to 20)", fxUnits(after))
	}
	if observed != 20*fixed.One {
		t.Fatalf("EvUnitDamaged arg = %d units, want the modified 20", fxUnits(observed))
	}
}

// TestDamageModifierNilUnchangedFSV — no modifier installed ⇒ behavior is
// byte-identical to the legacy path (the golden-trace safety claim). SoT:
// life delta equals the full unmitigated packet.
func TestDamageModifierNilUnchangedFSV(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)
	hr := w.Healths.Row(victim)
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 0})
	t.Logf("FSV nil-hook: life=%d units (want 60)", fxUnits(w.Healths.Life[hr]))
	if w.Healths.Life[hr] != 60*fixed.One {
		t.Fatalf("nil-hook life = %d units, want 60 (full 40 applied)", fxUnits(w.Healths.Life[hr]))
	}
}

// TestDamageModifierClampFSV — a modifier that returns a negative amount
// is clamped to 0 (damage never heals). Edge case.
func TestDamageModifierClampFSV(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)
	hr := w.Healths.Row(victim)
	w.SetDamageModifier(func(_, _ EntityID, _ fixed.F64) fixed.F64 { return -100 * fixed.One })
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 0})
	t.Logf("FSV negative-modifier: life=%d units (want 100, clamped to 0 damage)", fxUnits(w.Healths.Life[hr]))
	if w.Healths.Life[hr] != 100*fixed.One {
		t.Fatalf("negative modifier not clamped: life=%d units, want 100", fxUnits(w.Healths.Life[hr]))
	}
}
