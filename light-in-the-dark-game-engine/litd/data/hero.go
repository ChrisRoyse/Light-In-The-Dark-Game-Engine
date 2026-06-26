package data

// Hero tables (#304, combat-and-orders.md §5.3, D4): the XP curve
// (cumulative, NO formula in code), kill bounties by unit type, XP
// share/split/death rules, per-hero rows (base attributes +
// fixed-point growth, skill tree), the attribute→stat coefficient
// table, and revive cost/time. Lives in heroes/heroes.toml; absence
// is a nil Hero table, never a silent default. All decode is
// fail-closed; everything folds into the fingerprint. Loads after
// units and abilities (references resolve against the sorted
// tables).

import (
	"fmt"
	"io/fs"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// MaxHeroSkills bounds the per-hero skill tree (command-card scale).
const MaxHeroSkills = 5

// MaxHeroLevel bounds the XP curve length.
const MaxHeroLevel = 32

// XP split rules.
const (
	SplitEqual uint8 = 0 // bounty / heroes-in-range (integer division)
	SplitFull  uint8 = 1 // every hero in range gets the full bounty
)

var splitNames = []string{"equal", "full"}

// HeroSkill is one learnable skill-tree entry.
type HeroSkill struct {
	Ability      uint16  // index into Tables.Abilities
	MinHeroLevel []uint8 // per skill level: minimum hero level (len = max skill level)
}

// HeroDef is one converted hero row.
type HeroDef struct {
	Unit             uint16 // index into Tables.Units
	Str, Agi, Int    fixed.F64
	StrG, AgiG, IntG fixed.F64 // growth per level
	Skills           []HeroSkill
}

// AttrCoeffs is the attribute→stat coefficient table (LitD balance).
type AttrCoeffs struct {
	StrHP               fixed.F64 // hit points per strength point
	StrRegen            fixed.F64 // life per TICK per strength point
	AgiArmor            fixed.F64 // armor points per agility point (floored at fold)
	AgiCooldownPermille int32     // attack-cooldown permille per FULL agility point (1000 = none)
	IntMana             fixed.F64 // mana per intelligence point
	IntManaRegen        fixed.F64 // mana per TICK per intelligence point
}

// ReviveSpec is the revive cost/time table.
type ReviveSpec struct {
	BaseTicks     uint16
	TicksPerLevel uint16
	CostsBase     []int64 // per resource index
	CostsPerLevel []int64 // added per hero level
}

// HeroTables is the loaded hero rule set (Tables.Hero; nil = absent).
type HeroTables struct {
	Curve         []int64 // Curve[L-1] = cumulative XP to BE level L; Curve[0] = 0
	ShareRadius   fixed.F64
	Split         uint8
	DeathPenalty  int32 // permille of XP lost on death
	StartSkillPts uint8
	Bounty        []int64 // per unit type index (0 = no bounty)
	Heroes        []HeroDef
	Attr          AttrCoeffs
	Revive        ReviveSpec
}

type rawHeroFile struct {
	XPCurve []int64     `toml:"xp-curve" json:"xp-curve"`
	XP      *rawXPCfg   `toml:"xp" json:"xp"`
	Bounty  []rawBounty `toml:"bounty" json:"bounty"`
	Hero    []rawHero   `toml:"hero" json:"hero"`
	Attr    *rawAttr    `toml:"attributes" json:"attributes"`
	Revive  *rawRevive  `toml:"revive" json:"revive"`
}

type rawXPCfg struct {
	ShareRadius          float64 `toml:"share-radius" json:"share-radius"`
	Split                string  `toml:"split" json:"split"`
	DeathPenaltyPermille int64   `toml:"death-penalty-permille" json:"death-penalty-permille"`
	StartSkillPoints     int64   `toml:"start-skill-points" json:"start-skill-points"`
}

type rawBounty struct {
	Unit string `toml:"unit" json:"unit"`
	XP   int64  `toml:"xp" json:"xp"`
}

type rawHero struct {
	Unit        string         `toml:"unit" json:"unit"`
	Str         float64        `toml:"str" json:"str"`
	Agi         float64        `toml:"agi" json:"agi"`
	Int         float64        `toml:"int" json:"int"`
	StrPerLevel float64        `toml:"str-per-level" json:"str-per-level"`
	AgiPerLevel float64        `toml:"agi-per-level" json:"agi-per-level"`
	IntPerLevel float64        `toml:"int-per-level" json:"int-per-level"`
	Skill       []rawHeroSkill `toml:"skill" json:"skill"`
}

type rawHeroSkill struct {
	Ability      string  `toml:"ability" json:"ability"`
	MinHeroLevel []int64 `toml:"min-hero-level" json:"min-hero-level"`
}

type rawAttr struct {
	StrHP               float64 `toml:"str-hp" json:"str-hp"`
	StrRegen            float64 `toml:"str-regen" json:"str-regen"` // per second
	AgiArmor            float64 `toml:"agi-armor" json:"agi-armor"`
	AgiCooldownPermille int64   `toml:"agi-cooldown-permille" json:"agi-cooldown-permille"`
	IntMana             float64 `toml:"int-mana" json:"int-mana"`
	IntManaRegen        float64 `toml:"int-mana-regen" json:"int-mana-regen"` // per second
}

type rawRevive struct {
	BaseSeconds     float64          `toml:"base-seconds" json:"base-seconds"`
	SecondsPerLevel float64          `toml:"seconds-per-level" json:"seconds-per-level"`
	CostsBase       map[string]int64 `toml:"costs-base" json:"costs-base"`
	CostsPerLevel   map[string]int64 `toml:"costs-per-level" json:"costs-per-level"`
}

func (t *Tables) abilityIndex(id string) int {
	i := sort.Search(len(t.Abilities), func(k int) bool { return t.Abilities[k].ID >= id })
	if i == len(t.Abilities) || t.Abilities[i].ID != id {
		return -1
	}
	return i
}

// costMap converts a name-keyed cost table to a per-resource slice.
func (t *Tables) costMap(where string, m map[string]int64) ([]int64, error) {
	if len(m) == 0 {
		return nil, nil
	}
	out := make([]int64, len(t.ResourceTypes))
	for name, v := range m {
		res := indexOf(t.ResourceTypes, name)
		if res < 0 {
			return nil, fmt.Errorf("data: %s: resource %q is not in resource-types %v", where, name, t.ResourceTypes)
		}
		if v < 0 || v > 1_000_000 {
			return nil, fmt.Errorf("data: %s: %s = %d out of range [0, 1e6]", where, name, v)
		}
		out[res] = v
	}
	return out, nil
}

// loadHeroes reads heroes/heroes.toml (optional).
func (t *Tables) loadHeroes(fsys fs.FS) error {
	files, err := listTables(fsys, "heroes")
	if err != nil || len(files) == 0 {
		return nil
	}
	file, blob, err := readOne(fsys, "heroes", "heroes")
	if err != nil {
		return err
	}
	var raw rawHeroFile
	if err := decodeStrict(file, blob, &raw); err != nil {
		return err
	}
	h := &HeroTables{StartSkillPts: 1, Split: SplitEqual}

	// curve: cumulative, strictly increasing, starts at 0 (D4)
	if len(raw.XPCurve) < 2 || len(raw.XPCurve) > MaxHeroLevel {
		return fmt.Errorf("data: %s: xp-curve has %d entries, want [2, %d]", file, len(raw.XPCurve), MaxHeroLevel)
	}
	if raw.XPCurve[0] != 0 {
		return fmt.Errorf("data: %s: xp-curve[0] = %d, level 1 must cost 0", file, raw.XPCurve[0])
	}
	for i := 1; i < len(raw.XPCurve); i++ {
		if raw.XPCurve[i] <= raw.XPCurve[i-1] {
			return fmt.Errorf("data: %s: xp-curve not strictly increasing at index %d (%d after %d)", file, i, raw.XPCurve[i], raw.XPCurve[i-1])
		}
	}
	h.Curve = raw.XPCurve

	if raw.XP != nil {
		r, e := worldUnits(raw.XP.ShareRadius)
		if e != nil || r < 0 {
			return fmt.Errorf("data: %s: xp.share-radius %v invalid", file, raw.XP.ShareRadius)
		}
		h.ShareRadius = r
		if raw.XP.Split != "" {
			si := indexOf(splitNames, raw.XP.Split)
			if si < 0 {
				return fmt.Errorf("data: %s: xp.split %q is not equal|full", file, raw.XP.Split)
			}
			h.Split = uint8(si)
		}
		if raw.XP.DeathPenaltyPermille < 0 || raw.XP.DeathPenaltyPermille > 1000 {
			return fmt.Errorf("data: %s: xp.death-penalty-permille %d out of range [0, 1000]", file, raw.XP.DeathPenaltyPermille)
		}
		h.DeathPenalty = int32(raw.XP.DeathPenaltyPermille)
		if raw.XP.StartSkillPoints != 0 {
			if raw.XP.StartSkillPoints < 0 || raw.XP.StartSkillPoints > 10 {
				return fmt.Errorf("data: %s: xp.start-skill-points %d out of range [0, 10]", file, raw.XP.StartSkillPoints)
			}
			h.StartSkillPts = uint8(raw.XP.StartSkillPoints)
		}
	}

	h.Bounty = make([]int64, len(t.Units))
	for i := range raw.Bounty {
		b := &raw.Bounty[i]
		ui := t.unitIndex(b.Unit)
		if ui < 0 {
			return fmt.Errorf("data: %s: bounty[%d]: unit %q is not defined", file, i, b.Unit)
		}
		if b.XP <= 0 || b.XP > 1_000_000 {
			return fmt.Errorf("data: %s: bounty[%d]: xp %d out of range [1, 1e6]", file, i, b.XP)
		}
		if h.Bounty[ui] != 0 {
			return fmt.Errorf("data: %s: duplicate bounty for unit %q", file, b.Unit)
		}
		h.Bounty[ui] = b.XP
	}

	for i := range raw.Hero {
		r := &raw.Hero[i]
		fail := func(field string, e error) error {
			return fmt.Errorf("data: %s: hero %q: %s: %w", file, r.Unit, field, e)
		}
		ui := t.unitIndex(r.Unit)
		if ui < 0 {
			return fail("unit", fmt.Errorf("unit %q is not defined", r.Unit))
		}
		hd := HeroDef{Unit: uint16(ui)}
		for _, f := range []struct {
			name string
			in   float64
			out  *fixed.F64
		}{
			{"str", r.Str, &hd.Str}, {"agi", r.Agi, &hd.Agi}, {"int", r.Int, &hd.Int},
			{"str-per-level", r.StrPerLevel, &hd.StrG},
			{"agi-per-level", r.AgiPerLevel, &hd.AgiG},
			{"int-per-level", r.IntPerLevel, &hd.IntG},
		} {
			v, e := worldUnits(f.in)
			if e != nil || v < 0 || v > fixed.FromInt(1000) {
				return fail(f.name, fmt.Errorf("%v out of range [0, 1000]", f.in))
			}
			*f.out = v
		}
		if len(r.Skill) == 0 || len(r.Skill) > MaxHeroSkills {
			return fail("skill", fmt.Errorf("%d skills out of range [1, %d]", len(r.Skill), MaxHeroSkills))
		}
		for si := range r.Skill {
			rs := &r.Skill[si]
			ai := t.abilityIndex(rs.Ability)
			if ai < 0 {
				return fail(fmt.Sprintf("skill[%d].ability", si), fmt.Errorf("%q is not a defined ability", rs.Ability))
			}
			if len(rs.MinHeroLevel) == 0 || len(rs.MinHeroLevel) > len(h.Curve) {
				return fail(fmt.Sprintf("skill[%d].min-hero-level", si), fmt.Errorf("%d levels out of range [1, %d]", len(rs.MinHeroLevel), len(h.Curve)))
			}
			sk := HeroSkill{Ability: uint16(ai)}
			prev := int64(0)
			for li, ml := range rs.MinHeroLevel {
				if ml < 1 || ml > int64(len(h.Curve)) || ml < prev {
					return fail(fmt.Sprintf("skill[%d].min-hero-level[%d]", si, li), fmt.Errorf("%d invalid (1..%d, non-decreasing)", ml, len(h.Curve)))
				}
				prev = ml
				sk.MinHeroLevel = append(sk.MinHeroLevel, uint8(ml))
			}
			hd.Skills = append(hd.Skills, sk)
		}
		h.Heroes = append(h.Heroes, hd)
	}
	sort.Slice(h.Heroes, func(i, j int) bool { return h.Heroes[i].Unit < h.Heroes[j].Unit })
	for i := 1; i < len(h.Heroes); i++ {
		if h.Heroes[i].Unit == h.Heroes[i-1].Unit {
			return fmt.Errorf("data: %s: duplicate hero for unit %q", file, t.Units[h.Heroes[i].Unit].ID)
		}
	}

	if raw.Attr != nil {
		a := raw.Attr
		conv := func(name string, v float64, out *fixed.F64) error {
			f, e := worldUnits(v)
			if e != nil || f < 0 {
				return fmt.Errorf("data: %s: attributes.%s %v invalid", file, name, v)
			}
			*out = f
			return nil
		}
		if err := conv("str-hp", a.StrHP, &h.Attr.StrHP); err != nil {
			return err
		}
		sr, e := perSecondToPerTick(a.StrRegen)
		if e != nil {
			return fmt.Errorf("data: %s: attributes.str-regen: %w", file, e)
		}
		h.Attr.StrRegen = sr
		if err := conv("agi-armor", a.AgiArmor, &h.Attr.AgiArmor); err != nil {
			return err
		}
		if a.AgiCooldownPermille != 0 {
			if a.AgiCooldownPermille < 1 || a.AgiCooldownPermille > 10000 {
				return fmt.Errorf("data: %s: attributes.agi-cooldown-permille %d out of range [1, 10000]", file, a.AgiCooldownPermille)
			}
			h.Attr.AgiCooldownPermille = int32(a.AgiCooldownPermille)
		} else {
			h.Attr.AgiCooldownPermille = 1000
		}
		if err := conv("int-mana", a.IntMana, &h.Attr.IntMana); err != nil {
			return err
		}
		mr, e := perSecondToPerTick(a.IntManaRegen)
		if e != nil {
			return fmt.Errorf("data: %s: attributes.int-mana-regen: %w", file, e)
		}
		h.Attr.IntManaRegen = mr
	} else {
		h.Attr.AgiCooldownPermille = 1000
	}

	if raw.Revive != nil {
		bt, e := SecondsToTicks(raw.Revive.BaseSeconds)
		if e != nil || bt == 0 {
			return fmt.Errorf("data: %s: revive.base-seconds %v invalid (must be ≥ one tick)", file, raw.Revive.BaseSeconds)
		}
		h.Revive.BaseTicks = bt
		if raw.Revive.SecondsPerLevel != 0 {
			pt, e := SecondsToTicks(raw.Revive.SecondsPerLevel)
			if e != nil {
				return fmt.Errorf("data: %s: revive.seconds-per-level: %w", file, e)
			}
			h.Revive.TicksPerLevel = pt
		}
		if h.Revive.CostsBase, err = t.costMap(file+": revive.costs-base", raw.Revive.CostsBase); err != nil {
			return err
		}
		if h.Revive.CostsPerLevel, err = t.costMap(file+": revive.costs-per-level", raw.Revive.CostsPerLevel); err != nil {
			return err
		}
	} else if len(h.Heroes) > 0 {
		return fmt.Errorf("data: %s: hero rows without a [revive] table", file)
	}

	t.Hero = h
	return nil
}

// hashHero folds the hero rule set into the fingerprint.
func (t *Tables) hashHero(h *statehash.Hasher) {
	if t.Hero == nil {
		h.WriteU8(0)
		return
	}
	h.WriteU8(1)
	ht := t.Hero
	h.WriteU32(uint32(len(ht.Curve)))
	for _, c := range ht.Curve {
		h.WriteI64(c)
	}
	h.WriteI64(int64(ht.ShareRadius))
	h.WriteU8(ht.Split)
	h.WriteU32(uint32(ht.DeathPenalty))
	h.WriteU8(ht.StartSkillPts)
	h.WriteU32(uint32(len(ht.Bounty)))
	for _, b := range ht.Bounty {
		h.WriteI64(b)
	}
	h.WriteU32(uint32(len(ht.Heroes)))
	for i := range ht.Heroes {
		hd := &ht.Heroes[i]
		h.WriteU16(hd.Unit)
		for _, v := range []fixed.F64{hd.Str, hd.Agi, hd.Int, hd.StrG, hd.AgiG, hd.IntG} {
			h.WriteI64(int64(v))
		}
		h.WriteU32(uint32(len(hd.Skills)))
		for si := range hd.Skills {
			h.WriteU16(hd.Skills[si].Ability)
			h.WriteU32(uint32(len(hd.Skills[si].MinHeroLevel)))
			for _, ml := range hd.Skills[si].MinHeroLevel {
				h.WriteU8(ml)
			}
		}
	}
	for _, v := range []fixed.F64{ht.Attr.StrHP, ht.Attr.StrRegen, ht.Attr.AgiArmor, ht.Attr.IntMana, ht.Attr.IntManaRegen} {
		h.WriteI64(int64(v))
	}
	h.WriteU32(uint32(ht.Attr.AgiCooldownPermille))
	h.WriteU16(ht.Revive.BaseTicks)
	h.WriteU16(ht.Revive.TicksPerLevel)
	h.WriteU32(uint32(len(ht.Revive.CostsBase)))
	for _, c := range ht.Revive.CostsBase {
		h.WriteI64(c)
	}
	h.WriteU32(uint32(len(ht.Revive.CostsPerLevel)))
	for _, c := range ht.Revive.CostsPerLevel {
		h.WriteI64(c)
	}
}
