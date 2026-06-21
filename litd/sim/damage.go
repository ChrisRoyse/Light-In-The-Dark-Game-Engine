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

// DamagePacket is the §3.4 deferred-damage value struct — the
// element of the phase-5 apply buffer and the rolled-at-launch
// payload of degenerate missiles (#158).
type DamagePacket struct {
	Source     EntityID
	Target     EntityID
	Amount     fixed.F64 // rolled at launch
	AttackType uint8
	Flags      uint8
}

// EvUnitDamaged fires once per applied packet, before any same-tick
// death event. Src = damage source, Dst = victim, Arg = the
// post-mitigation amount in fixed.F64 bits. (7, not 2: kinds share
// one dispatch namespace and 2/3 belong to movement — #332.)
const EvUnitDamaged uint16 = 7

// DamageFromWeapon tags a packet that originated from a weapon FIRE edge (#468).
// damageApplySystem emits EvAttackLanded for such a packet, immediately before
// its EvUnitDamaged, when it lands on a live target.
const DamageFromWeapon uint8 = 1 << 0

// Armor LUT bounds (combat-and-orders.md §4: practical armor range).
// ArmorValue outside the range clamps to it.
const (
	ArmorLUTMin = -20
	ArmorLUTMax = 100
)

// armorLUTSize is the number of armor values in [ArmorLUTMin, ArmorLUTMax].
const armorLUTSize = ArmorLUTMax - ArmorLUTMin + 1

// defaultArmorK is the shipped positive-branch reduction coefficient (0.06).
// SetArmorCoefficient (#474) replaces it per world; a world left at this value
// reproduces the historical LUT exactly (regression-pinned).
var defaultArmorK = fixed.FromInt(6).Div(fixed.FromInt(100)) // 0.06

// armorMult is the DEFAULT damage-multiplier LUT (k = 0.06). Each world copies
// it at NewWorld and may rebuild its own from a configured coefficient (#474);
// this global remains the canonical default + regression reference.
//
// armorMult[a-ArmorLUTMin] is the multiplier at armor value a, 32.32 fixed:
//
//	a >= 0: 1/(1 + a·k), k = 0.06      (reduction)
//	a <  0: 2 − 0.94^(−a)              (amplification)
//
// The spec's single-formula reading of negative armor diverges at a ≤ −17
// (denominator 1+a·k crosses zero), so negative armor uses the WC3 piecewise
// amplification curve (#330) — preserved regardless of the positive-branch
// coefficient. From integer ratios via fixed-point Mul/Div only:
// bit-identical on every platform.
var armorMult = buildArmorLUTk(defaultArmorK)

// buildArmorLUTk builds the armor multiplier LUT for positive-branch
// coefficient k. The negative-armor piecewise branch (#330) is independent of k
// and always preserved.
func buildArmorLUTk(k fixed.F64) [armorLUTSize]fixed.F64 {
	var lut [armorLUTSize]fixed.F64
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

// BindDamageTypes declares the named attack- and armor-type tables (#472).
// Order is significant: a name's index is its row/column in the matrix. The
// names are config, not state — they are not hashed or saved, and are restored
// by re-running setup on load (like the matrix itself). Fail-closed: empty or
// duplicate-named tables are refused. Binding the types makes BindDamageMatrix
// validate the matrix dims against the declared counts, so a matrix that is
// rectangular but the wrong size for the type tables is caught at the seam.
func (w *World) BindDamageTypes(attack, armor []string) error {
	if len(attack) == 0 || len(armor) == 0 {
		return fmt.Errorf("sim: BindDamageTypes: empty attack (%d) or armor (%d) table", len(attack), len(armor))
	}
	if err := uniqueNames("attack", attack); err != nil {
		return err
	}
	if err := uniqueNames("armor", armor); err != nil {
		return err
	}
	w.atkTypes = append(w.atkTypes[:0], attack...)
	w.armTypes = append(w.armTypes[:0], armor...)
	return nil
}

func uniqueNames(kind string, names []string) error {
	for i := range names {
		if names[i] == "" {
			return fmt.Errorf("sim: BindDamageTypes: empty %s-type name at index %d", kind, i)
		}
		for j := i + 1; j < len(names); j++ {
			if names[i] == names[j] {
				return fmt.Errorf("sim: BindDamageTypes: duplicate %s-type name %q (indices %d, %d)", kind, names[i], i, j)
			}
		}
	}
	return nil
}

// AttackTypeIndex resolves an attack-type name to its matrix row index.
// Setup-time lookup (linear scan over a small table — never in the damage hot
// path), fail-closed: ok=false on an unknown or unbound name.
func (w *World) AttackTypeIndex(name string) (uint8, bool) { return nameIndex(w.atkTypes, name) }

// ArmorTypeIndex resolves an armor-type name to its matrix column index.
func (w *World) ArmorTypeIndex(name string) (uint8, bool) { return nameIndex(w.armTypes, name) }

func nameIndex(names []string, name string) (uint8, bool) {
	for i := range names {
		if names[i] == name {
			return uint8(i), true
		}
	}
	return 0, false
}

// AttackTypeName / ArmorTypeName map an index back to its declared name for
// dumps and logging; "" if out of range or unbound.
func (w *World) AttackTypeName(i uint8) string { return nameAt(w.atkTypes, i) }
func (w *World) ArmorTypeName(i uint8) string  { return nameAt(w.armTypes, i) }

func nameAt(names []string, i uint8) string {
	if int(i) >= len(names) {
		return ""
	}
	return names[i]
}

// BindDamageMatrix installs the loaded per-mille coefficient matrix.
// Fail-closed: a ragged or empty matrix is refused, and no damage
// applies before a successful bind (packets drop counted). When the type
// tables are bound (BindDamageTypes), the matrix dims must equal
// len(attack)×len(armor) — a rectangular-but-mis-sized matrix is rejected.
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
	if len(w.atkTypes) > 0 && len(coeff) != len(w.atkTypes) {
		return fmt.Errorf("sim: BindDamageMatrix: %d rows but %d attack types declared", len(coeff), len(w.atkTypes))
	}
	if len(w.armTypes) > 0 && cols != len(w.armTypes) {
		return fmt.Errorf("sim: BindDamageMatrix: %d cols but %d armor types declared", cols, len(w.armTypes))
	}
	w.coeff = coeff
	return nil
}

// SetDamageModifier installs (or clears, with nil) the synchronous
// pre-apply damage hook (#219). fn receives the source, target, and the
// final post-mitigation amount and returns the amount to actually apply.
// It must be pure and deterministic (no waits, no PRNG of its own) — it
// runs inside the combat phase. Replacing it is script wiring, not hashed
// sim state.
func (w *World) SetDamageModifier(fn func(src, dst EntityID, amount fixed.F64) fixed.F64) {
	w.damageMod = fn
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
		if w.Healths.Invulnerable[hr] {
			continue // invulnerable: the packet lands on nothing — no life
			// change, no kill, no XP, no EvUnitDamaged (#365). Defense at the
			// single damage chokepoint.
		}
		// dead-source packets still land — the damage was already in
		// flight when the source died (WC3 semantics)
		if w.coeff == nil ||
			int(p.AttackType) >= len(w.coeff) ||
			int(w.Healths.ArmorType[hr]) >= len(w.coeff[p.AttackType]) {
			w.dmgDropped++
			continue // unbound matrix or type out of range: counted, never guessed
		}
		// #473 staged damage-formula pipeline: coeff-lookup → armor-reduction
		// → handicap → script-modifier (#219) → clamp, by default. A world may
		// replace the whole formula or any stage; the value left in the reused
		// context is the applied damage. Fixed-point + reused ctx ⇒ zero-alloc;
		// a base (unmodified) formula is byte-identical to the old inline path,
		// so the golden trace is stable.
		post := w.runDamageFormula(
			p.Source, p.Target, p.AttackType, w.Healths.ArmorType[hr],
			int(w.Healths.ArmorValue[hr]), p.Amount,
		)

		if cr := w.Combats.Row(p.Target); cr != -1 {
			w.Combats.LastAttacker[cr] = p.Source
			w.Combats.LastDamagedTick[cr] = w.tick
		}
		// A weapon-sourced packet landed on a live target: emit EvAttackLanded
		// first, so a handler sees Landed→Damaged for the same hit (#468).
		if p.Flags&DamageFromWeapon != 0 {
			w.Emit(Event{Kind: EvAttackLanded, Src: p.Source, Dst: p.Target, Arg: int64(post)})
		}
		w.Emit(Event{Kind: EvUnitDamaged, Src: p.Source, Dst: p.Target, Arg: int64(post)})

		life := w.Healths.Life[hr].Sub(post)
		if life <= 0 {
			if w.Healths.Life[hr] > 0 { // lethal CROSSING: pay XP once (#304)
				w.grantKillXP(p.Source, p.Target)
			}
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

// execArea is the `area` combinator backend: every valid target in
// radius, nearest-first under the §3.2 total order (distSq128, then
// entity index), capped at max-targets; the payload runs once per
// target. Candidates are enemies of ctx.Source with a Health row —
// the same filter as acquisition. Params in schema order: radius,
// max-targets.
func execArea(w *World, ctx EffectCtx, e *data.CompiledEffect) {
	center := ctx.Point
	if ctx.Target != 0 {
		if tr := w.Transforms.Row(ctx.Target); tr != -1 {
			center = w.Transforms.Pos[tr]
		}
	}
	sor := w.Owners.Row(ctx.Source)
	if sor == -1 {
		return // unowned sources cannot classify enemies: no targets
	}
	team := w.Owners.Team[sor]
	radius := fixed.F64(e.Params[0])
	maxTargets := int(e.Params[1])
	rHi, rLo := fixed.RadiusSq(radius)

	// nearest-first selection into the reusable scratch (insertion
	// sort under the total order; the schema caps max-targets at 64)
	sel := w.areaScratch[:0]
	selHi := w.areaDistHi[:0]
	selLo := w.areaDistLo[:0]
	x0, x1 := bucketCoord(center.X.Sub(radius)), bucketCoord(center.X.Add(radius))
	y0, y1 := bucketCoord(center.Y.Sub(radius)), bucketCoord(center.Y.Add(radius))
	for by := y0; by <= y1; by++ {
		for bx := x0; bx <= x1; bx++ {
			for be := w.bucketHead[by*BucketGridSize+bx]; be != -1; be = w.bucketNext[be] {
				cid := w.bucketID[be]
				if cid == ctx.Source || !w.Ents.Alive(cid) {
					continue
				}
				cor := w.Owners.Row(cid)
				if cor == -1 || w.Owners.Team[cor] == team || w.Healths.Row(cid) == -1 {
					continue
				}
				ctr := w.Transforms.Row(cid)
				if ctr == -1 {
					continue
				}
				dHi, dLo := fixed.DistSq(center, w.Transforms.Pos[ctr])
				if dHi > rHi || (dHi == rHi && dLo > rLo) {
					continue
				}
				// insert position under (distSq, index) ascending
				pos := len(sel)
				for pos > 0 {
					p := pos - 1
					if selHi[p] < dHi || (selHi[p] == dHi && (selLo[p] < dLo ||
						(selLo[p] == dLo && sel[p].Index() < cid.Index()))) {
						break
					}
					pos--
				}
				if pos >= maxTargets {
					continue // farther than every kept candidate
				}
				if len(sel) < maxTargets {
					sel = append(sel, 0)
					selHi = append(selHi, 0)
					selLo = append(selLo, 0)
				}
				copy(sel[pos+1:], sel[pos:])
				copy(selHi[pos+1:], selHi[pos:])
				copy(selLo[pos+1:], selLo[pos:])
				sel[pos], selHi[pos], selLo[pos] = cid, dHi, dLo
			}
		}
	}
	for _, cid := range sel {
		child := ctx
		child.Target = cid
		if tr := w.Transforms.Row(cid); tr != -1 {
			child.Point = w.Transforms.Pos[tr]
		}
		w.RunEffectChildren(e, child)
	}
	w.areaScratch = sel[:0]
}

// RegisterCoreEffectExecs registers the effect-primitive backends
// implemented so far. Engine init calls this once, then registers any
// game-specific execs, then FreezeEffectExecs. Grows as backends land
// (#160 casts, #162 apply-buff, spawn-missile once missile-type data
// rows exist).
func RegisterCoreEffectExecs() {
	RegisterEffectExec(data.EPDamage, execDamage)
	RegisterEffectExec(data.EPArea, execArea)
	RegisterEffectExec(data.EPApplyBuff, execApplyBuff)
}
