package data

// Buff-type table (#162, combat-and-orders.md §5.2): buffs are DATA —
// duration, stacking rule, periodic effect composition (#296), stat
// modifiers, flags. Loaded from buffs/*.{toml|json} BEFORE abilities
// so apply-buff params resolve buff names at compile time. Fail-closed
// like every other table.

import (
	"fmt"
	"io/fs"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Stacking rules.
const (
	StackRefresh       uint8 = iota // reapply resets the duration
	StackCount                      // reapply increments stacks (≤ MaxStacks)
	StackIndependent                // every application is its own instance
	StackStrongestWins              // larger stack-count instance survives
	stackRuleCount
)

var stackRuleNames = [stackRuleCount]string{
	"refresh", "stack-count", "independent", "strongest-wins",
}

// Buff-modifiable stats. The sim's derived-stat cache is indexed by
// these — appending is safe, reordering is a sim-version change.
const (
	StatMoveSpeed      uint8 = iota // Add in world-units/second → per-tick fixed
	StatArmor                       // Add in integer armor points
	StatAttackCooldown              // Add in seconds → ticks (signed)
	BuffStatCount
)

var buffStatNames = [BuffStatCount]string{"move-speed", "armor", "attack-cooldown"}

// Buff flags.
const (
	BuffDispellable uint8 = 1 << 0
)

// StatMod is one converted stat modifier: derived = (base + Add) ×
// Permille / 1000, folded in (BuffID, instance index) order.
type StatMod struct {
	Stat     uint8
	Add      int64 // sim units (per-tick fixed bits / armor points / ticks)
	Permille int32 // 1000 = unchanged
}

// BuffType is one converted buff row.
type BuffType struct {
	ID            string
	Name          string
	DurationTicks uint16
	Stacking      uint8
	MaxStacks     uint8 // StackCount rule cap; ≥1
	PeriodTicks   uint16
	Periodic      EffectList // runs every period against the carrier
	Mods          []StatMod
	Flags         uint8
}

type rawBuffFile struct {
	Buff []rawBuff `toml:"buff" json:"buff"`
}

type rawBuff struct {
	ID          string           `toml:"id" json:"id"`
	Name        string           `toml:"name" json:"name"`
	Duration    float64          `toml:"duration" json:"duration"` // seconds
	Stacking    string           `toml:"stacking" json:"stacking"`
	MaxStacks   int64            `toml:"max-stacks" json:"max-stacks"`
	Period      float64          `toml:"period" json:"period"` // seconds; 0 = no periodic
	Dispellable bool             `toml:"dispellable" json:"dispellable"`
	Mods        []rawStatMod     `toml:"mod" json:"mod"`
	Effects     []map[string]any `toml:"effects" json:"effects"` // periodic composition
}

type rawStatMod struct {
	Stat     string  `toml:"stat" json:"stat"`
	Add      float64 `toml:"add" json:"add"`
	Permille int64   `toml:"permille" json:"permille"`
}

// convertBuff converts one raw buff row; the caller compiles Effects.
func convertBuff(file string, r *rawBuff) (BuffType, error) {
	fail := func(field string, err error) (BuffType, error) {
		return BuffType{}, fmt.Errorf("data: %s: buff %q: %s: %w", file, r.ID, field, err)
	}
	if r.ID == "" {
		return BuffType{}, fmt.Errorf("data: %s: buff with empty id", file)
	}
	b := BuffType{ID: r.ID, Name: r.Name, MaxStacks: 1}
	dur, err := SecondsToTicks(r.Duration)
	if err != nil {
		return fail("duration", err)
	}
	if dur == 0 {
		return fail("duration", fmt.Errorf("must be positive"))
	}
	b.DurationTicks = dur
	si := indexOf(stackRuleNames[:], r.Stacking)
	if si < 0 {
		return fail("stacking", fmt.Errorf("%q is not refresh|stack-count|independent|strongest-wins", r.Stacking))
	}
	b.Stacking = uint8(si)
	if r.MaxStacks != 0 {
		if r.MaxStacks < 1 || r.MaxStacks > 255 {
			return fail("max-stacks", fmt.Errorf("value %d out of range [1, 255]", r.MaxStacks))
		}
		b.MaxStacks = uint8(r.MaxStacks)
	}
	per, err := SecondsToTicks(r.Period)
	if err != nil {
		return fail("period", err)
	}
	b.PeriodTicks = per
	if r.Dispellable {
		b.Flags |= BuffDispellable
	}
	for i := range r.Mods {
		m := &r.Mods[i]
		mi := indexOf(buffStatNames[:], m.Stat)
		if mi < 0 {
			return fail("mod.stat", fmt.Errorf("%q is not move-speed|armor|attack-cooldown", m.Stat))
		}
		sm := StatMod{Stat: uint8(mi), Permille: 1000}
		if m.Permille != 0 {
			if m.Permille < 0 || m.Permille > 10000 {
				return fail("mod.permille", fmt.Errorf("value %d out of range [0, 10000]", m.Permille))
			}
			sm.Permille = int32(m.Permille)
		}
		switch sm.Stat {
		case StatMoveSpeed:
			v, err := perSecondToPerTick(m.Add)
			if err != nil {
				return fail("mod.add", err)
			}
			sm.Add = int64(v)
		case StatArmor:
			if m.Add != float64(int64(m.Add)) {
				return fail("mod.add", fmt.Errorf("armor add must be an integer"))
			}
			sm.Add = int64(m.Add)
		case StatAttackCooldown:
			if m.Add < 0 {
				neg, err := SecondsToTicks(-m.Add)
				if err != nil {
					return fail("mod.add", err)
				}
				sm.Add = -int64(neg)
			} else {
				pos, err := SecondsToTicks(m.Add)
				if err != nil {
					return fail("mod.add", err)
				}
				sm.Add = int64(pos)
			}
		}
		b.Mods = append(b.Mods, sm)
	}
	return b, nil
}

// loadBuffs reads buffs/ into sorted, compiled BuffType rows.
func (t *Tables) loadBuffs(fsys fs.FS, comp *effectCompiler) error {
	files, err := listTables(fsys, "buffs")
	if err != nil {
		return err
	}
	type pendingBuff struct {
		file string
		raw  rawBuff
	}
	var pending []pendingBuff
	for _, f := range files {
		blob, err := fs.ReadFile(fsys, f)
		if err != nil {
			return fmt.Errorf("data: %s: %w", f, err)
		}
		var raw rawBuffFile
		if err := decodeStrict(f, blob, &raw, "buff.effects"); err != nil {
			return err
		}
		for i := range raw.Buff {
			pending = append(pending, pendingBuff{file: f, raw: raw.Buff[i]})
		}
	}
	sort.Slice(pending, func(i, j int) bool { return pending[i].raw.ID < pending[j].raw.ID })
	for i := 1; i < len(pending); i++ {
		if pending[i].raw.ID == pending[i-1].raw.ID {
			return fmt.Errorf("data: duplicate buff id %q (%s, %s)",
				pending[i].raw.ID, pending[i-1].file, pending[i].file)
		}
	}
	// names first: periodic compositions may apply-buff by name,
	// including self-references
	comp.buffTypes = comp.buffTypes[:0]
	for i := range pending {
		comp.buffTypes = append(comp.buffTypes, pending[i].raw.ID)
	}
	for i := range pending {
		p := &pending[i]
		b, err := convertBuff(p.file, &p.raw)
		if err != nil {
			return err
		}
		if p.raw.Effects != nil {
			comp.file = p.file
			where := fmt.Sprintf("buff %q effects", p.raw.ID)
			lst, inv, err := comp.compile(where, p.raw.Effects, 1)
			if err != nil {
				return err
			}
			if inv > MaxEffectInvocations {
				return fmt.Errorf("data: %s: %s: worst-case invocation count %d exceeds ceiling %d",
					p.file, where, inv, MaxEffectInvocations)
			}
			b.Periodic = lst
		}
		t.BuffTypes = append(t.BuffTypes, b)
	}
	return nil
}

// hashBuffs folds the buff table into the fingerprint.
func (t *Tables) hashBuffs(h *statehash.Hasher) {
	h.WriteU32(uint32(len(t.BuffTypes)))
	for i := range t.BuffTypes {
		b := &t.BuffTypes[i]
		writeString(h, b.ID)
		writeString(h, b.Name)
		h.WriteU16(b.DurationTicks)
		h.WriteU8(b.Stacking)
		h.WriteU8(b.MaxStacks)
		h.WriteU16(b.PeriodTicks)
		h.WriteU16(b.Periodic.Off)
		h.WriteU16(b.Periodic.Len)
		h.WriteU8(b.Flags)
		h.WriteU32(uint32(len(b.Mods)))
		for _, m := range b.Mods {
			h.WriteU8(m.Stat)
			h.WriteI64(m.Add)
			h.WriteU32(uint32(m.Permille))
		}
	}
}
