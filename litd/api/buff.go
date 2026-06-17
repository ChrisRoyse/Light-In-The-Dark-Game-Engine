package litd

// Buff surface (#234, abilities-and-buffs.md; public-api-design.md §2 row 9).
// A buff/aura is a pooled sim instance carried by a unit — JASS hides these in
// ability + unit-state corners; LitD makes them a first-class noun. The sim
// BuffPool is the source of truth: apply/has/count/remove all read it back.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// BuffType names a bound buff/aura type. The zero value is the null type.
type BuffType struct {
	ref uint16 // typeIdx + 1; 0 = null
}

// IsZero reports the null buff type.
func (t BuffType) IsZero() bool { return t.ref == 0 }

// BuffType resolves a buff code (e.g. "slow") to its bound type, or the null
// BuffType if the code is unknown or no buff table is bound.
func (g *Game) BuffType(code string) BuffType {
	if g == nil || g.w == nil {
		return BuffType{}
	}
	if id, ok := g.w.BuffTypeID(code); ok {
		return BuffType{ref: id + 1}
	}
	return BuffType{}
}

// BuffOption configures Unit.ApplyBuff (R-API-3 functional option).
type BuffOption func(*buffConfig)

type buffConfig struct {
	source Unit
	stacks int
}

// FromSource records the unit that applied the buff (for source-scoped
// stacking and attribution). Default: no source.
func FromSource(u Unit) BuffOption { return func(c *buffConfig) { c.source = u } }

// WithStacks sets the initial stack count (default 1, clamped to [1,255]).
func WithStacks(n int) BuffOption { return func(c *buffConfig) { c.stacks = n } }

// ApplyBuff applies a buff of type typ to the unit, returning a handle to the
// instance (the zero Buff on failure — null type, dead unit, or pool
// exhaustion). The type's stacking rule resolves against any live instance.
// JASS: buff application is hidden behind dummy-caster abilities in WC3.
func (u Unit) ApplyBuff(typ BuffType, opts ...BuffOption) Buff {
	if !u.Valid() {
		if u.g != nil {
			u.g.reportInvalid("Unit.ApplyBuff")
		}
		return Buff{}
	}
	if typ.IsZero() {
		u.g.reportInvalid("Unit.ApplyBuff (null BuffType)")
		return Buff{}
	}
	cfg := buffConfig{stacks: 1}
	for _, o := range opts {
		o(&cfg)
	}
	if cfg.stacks < 1 {
		cfg.stacks = 1
	}
	if cfg.stacks > 255 {
		cfg.stacks = 255
	}
	var src sim.EntityID
	if cfg.source.Valid() {
		src = cfg.source.id
	}
	if !u.g.w.ApplyBuff(u.id, src, int(typ.ref-1), uint8(cfg.stacks)) {
		u.g.reportInvalid("Unit.ApplyBuff (pool exhausted or bad type)")
		return Buff{}
	}
	return Buff{owner: u.id, ref: uint32(typ.ref), g: u.g}
}

// HasBuff reports whether the unit carries a live instance of the buff type.
// JASS: UnitHasBuffBJ, UnitHasBuffsEx
func (u Unit) HasBuff(typ BuffType) bool {
	if !u.Valid() || typ.IsZero() {
		return false
	}
	return u.g.w.UnitHasBuff(u.id, typ.ref-1)
}

// BuffCount returns the number of live buff instances on the unit. JASS:
// UnitCountBuffsEx (the count form).
// JASS: UnitCountBuffsEx, UnitCountBuffsExBJ
func (u Unit) BuffCount() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.BuffCount")
		return 0
	}
	return u.g.w.UnitBuffCount(u.id)
}

// RemoveBuff removes every instance of the buff type from the unit, returning
// true if any were removed. JASS: UnitRemoveBuffBJ.
// JASS: UnitRemoveBuffBJ
func (u Unit) RemoveBuff(typ BuffType) bool {
	if !u.Valid() || typ.IsZero() {
		if u.g != nil {
			u.g.reportInvalid("Unit.RemoveBuff")
		}
		return false
	}
	return u.g.w.RemoveBuff(u.id, typ.ref-1) > 0
}

// RemoveAllBuffs strips every buff from the unit, returning the count removed.
// JASS: UnitRemoveBuffs, UnitRemoveBuffsBJ, UnitRemoveBuffsEx, UnitRemoveBuffsExBJ
func (u Unit) RemoveAllBuffs() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.RemoveAllBuffs")
		return 0
	}
	return u.g.w.RemoveAllBuffs(u.id)
}

// Type returns the buff instance's type handle.
func (b Buff) Type() BuffType {
	if !b.Valid() {
		return BuffType{}
	}
	return BuffType{ref: uint16(b.ref)}
}

// Stacks returns the buff's current stack count, or 0 if it is no longer
// present (expired/removed) or the handle is invalid.
func (b Buff) Stacks() int {
	if !b.Valid() {
		return 0
	}
	return int(b.g.w.BuffStacks(b.owner, uint16(b.ref-1)))
}

// Present reports whether the buff instance is still live on its owner (it may
// have expired or been dispelled even though the handle is structurally valid).
func (b Buff) Present() bool {
	return b.Valid() && b.g.w.UnitHasBuff(b.owner, uint16(b.ref-1))
}

// RemainingSeconds returns the buff's remaining duration in seconds, or 0 if it
// is no longer present. Quantized to the sim tick grid.
func (b Buff) RemainingSeconds() float64 {
	if !b.Valid() {
		return 0
	}
	return float64(b.g.w.BuffRemainingTicks(b.owner, uint16(b.ref-1))) / float64(data.TicksPerSecond)
}

// Remove dispels this buff (every instance of its type) from its owner. No-op
// on an invalid handle.
func (b Buff) Remove() {
	if !b.Valid() {
		return
	}
	b.g.w.RemoveBuff(b.owner, uint16(b.ref-1))
}
