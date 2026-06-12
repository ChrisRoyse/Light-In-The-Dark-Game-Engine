package sim

// Damage pipeline (#152, combat-and-orders.md §4): the backend of the
// `damage` effect primitive (ADR #294). Sources append DamagePackets
// to a deferred buffer during phase 5; ONE apply pass at combat-phase
// end walks the buffer in append order (ecs §6 deferred rule) so
// mutual kills and overkill stacking are well-defined regardless of
// entity iteration order.
//
// Mitigation order is fixed and documented: attack-vs-armor per-mille
// coefficient → armor-value reduction LUT → final clamp at zero.
// (Flat and multiplicative buff modifiers slot into this chain with
// #162.) The armor LUT is precomputed in pure fixed-point — no
// runtime division, no floats anywhere (R-SIM-1).

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
)

// EvUnitDamaged fires once per applied packet, before any same-tick
// death event. Src = damage source, Dst = victim, Arg = the
// post-mitigation amount in fixed.F64 bits.
const EvUnitDamaged uint16 = 2

// Armor LUT bounds (combat-and-orders.md §4: practical armor range).
// ArmorValue outside the range clamps to it.
const (
	ArmorLUTMin = -20
	ArmorLUTMax = 100
)

// armorMult[a-ArmorLUTMin] is the damage multiplier at armor value a,
// 32.32 fixed-point:
//
//	a >= 0: 1/(1 + a·k), k = 0.06      (reduction)
//	a <  0: 2 − 0.94^(−a)              (amplification)
//
// The spec's single-formula reading of negative armor diverges at
// a ≤ −17 (denominator 1+a·k crosses zero), so negative armor uses
// the WC3 piecewise amplification curve — recorded as a discovery.
// Built once at package init from integer ratios via fixed-point
// Mul/Div only: bit-identical on every platform.
var armorMult = buildArmorLUT()

func buildArmorLUT() [ArmorLUTMax - ArmorLUTMin + 1]fixed.F64 {
	var lut [ArmorLUTMax - ArmorLUTMin + 1]fixed.F64
	k := fixed.FromInt(6).Div(fixed.FromInt(100))    // 0.06
	p94 := fixed.FromInt(94).Div(fixed.FromInt(100)) // 0.94
	for a := ArmorLUTMin; a <= ArmorLUTMax; a++ {
		var m fixed.F64
		if a >= 0 {
			m = fixed.One.Div(fixed.One.Add(fixed.FromInt(int32(a)).Mul(k)))
		} else {
			pow := fixed.One
			for i := 0; i < -a; i++ {
				pow = pow.Mul(p94)
			}
			m = fixed.FromInt(2).Sub(pow)
		}
		lut[a-ArmorLUTMin] = m
	}
	return lut
}

// SetSeed reseeds the sim PRNG for a match (determinism.md: fixed
// seed + scripted commands define a run completely). Call before the
// first Step; the cursor folds into the state hash with #154.
func (w *World) SetSeed(seed uint64) { w.rng = prng.New(seed, 0) }

// BindDamageMatrix installs the loaded per-mille coefficient matrix.
// Fail-closed: a ragged or empty matrix is refused, and no damage
// applies before a successful bind (packets drop counted).
func (w *World) BindDamageMatrix(coeff [][]int32) error {
	if len(coeff) == 0 {
		return fmt.Errorf("sim: BindDamageMatrix: empty matrix")
	}
	cols := len(coeff[0])
	if cols == 0 {
		return fmt.Errorf("sim: BindDamageMatrix: empty armor-type row")
	}
	for i := range coeff {
		if len(coeff[i]) != cols {
			return fmt.Errorf("sim: BindDamageMatrix: ragged row %d (%d cols, want %d)", i, len(coeff[i]), cols)
		}
	}
	w.coeff = coeff
	return nil
}

// QueueDamage appends one packet to the deferred buffer. A full
// buffer is a counted drop — visible in DamageDropped(), never
// silent (the buffer is sized generously off World caps; hitting the
// cap means a runaway effect composition or too-small caps).
func (w *World) QueueDamage(p DamagePacket) bool {
	if len(w.dmgBuf) == cap(w.dmgBuf) {
		w.dmgDropped++
		return false
	}
	w.dmgBuf = append(w.dmgBuf, p)
	return true
}

// DamageDropped returns the cumulative count of packets dropped on a
// full buffer or a missing/mismatched damage matrix.
func (w *World) DamageDropped() uint32 { return w.dmgDropped }

// damageApplySystem is the single apply pass at combat-phase end.
// Buffer order is application order. Per packet against a living
// victim: mitigate, subtract life (floor 0), record LastAttacker /
// LastDamagedTick (threat memory, #148), emit EvUnitDamaged; lethal
// packets mark the deferred-kill buffer — removal stays phase 7's
// job, so same-tick mutual kills both resolve.
func (w *World) damageApplySystem() {
	for i := range w.dmgBuf {
		p := &w.dmgBuf[i]
		hr := w.Healths.Row(p.Target)
		if hr == -1 || !w.Ents.Alive(p.Target) {
			continue // victim already gone: deterministic no-op
		}
		// dead-source packets still land — the damage was already in
		// flight when the source died (WC3 semantics)
		if w.coeff == nil ||
			int(p.AttackType) >= len(w.coeff) ||
			int(w.Healths.ArmorType[hr]) >= len(w.coeff[p.AttackType]) {
			w.dmgDropped++
			continue // unbound matrix or type out of range: counted, never guessed
		}
		post := p.Amount.
			Mul(fixed.FromInt(w.coeff[p.AttackType][w.Healths.ArmorType[hr]])).
			Div(fixed.FromInt(1000))
		armor := int(w.Healths.ArmorValue[hr])
		if armor < ArmorLUTMin {
			armor = ArmorLUTMin
		} else if armor > ArmorLUTMax {
			armor = ArmorLUTMax
		}
		post = post.Mul(armorMult[armor-ArmorLUTMin])
		if post < 0 {
			post = 0 // final clamp: damage never heals
		}

		if cr := w.Combats.Row(p.Target); cr != -1 {
			w.Combats.LastAttacker[cr] = p.Source
			w.Combats.LastDamagedTick[cr] = w.tick
		}
		w.Emit(Event{Kind: EvUnitDamaged, Src: p.Source, Dst: p.Target, Arg: int64(post)})

		life := w.Healths.Life[hr].Sub(post)
		if life <= 0 {
			life = 0
			w.KillUnit(p.Target)
		}
		w.Healths.Life[hr] = life
	}
	w.dmgBuf = w.dmgBuf[:0]
}

// execDamage is the `damage` primitive backend: roll dice on the sim
// PRNG (one deterministic call site per invocation — R-SIM-2), queue
// the packet. Params in schema order: amount, dice, sides,
// attack-type.
func execDamage(w *World, ctx EffectCtx, e *data.CompiledEffect) {
	amt := fixed.FromInt(int32(e.Params[0]))
	if dice, sides := e.Params[1], e.Params[2]; dice > 0 && sides > 0 {
		for i := int64(0); i < dice; i++ {
			amt = amt.Add(fixed.FromInt(int32(w.rng.Uint32()%uint32(sides)) + 1))
		}
	}
	w.QueueDamage(DamagePacket{
		Source:     ctx.Source,
		Target:     ctx.Target,
		Amount:     amt,
		AttackType: uint8(e.Params[3]),
	})
}

// RegisterCoreEffectExecs registers the effect-primitive backends
// implemented so far. Engine init calls this once, then registers any
// game-specific execs, then FreezeEffectExecs. Grows as backends land
// (#158 spawn-missile, #160 casts, #162 apply-buff).
func RegisterCoreEffectExecs() {
	RegisterEffectExec(data.EPDamage, execDamage)
}
