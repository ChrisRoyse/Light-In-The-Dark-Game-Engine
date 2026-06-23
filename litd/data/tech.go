package data

// Tech tables (#303, abilities-and-buffs.md): upgrade rows
// (per-level costs/research time, stat modifiers on the buff-stat
// vocabulary, unit filter) and the requirement table gating
// train/research admission. Lives in tech/upgrades.toml; absence is
// a visible empty set, never a silent default. All decode is
// fail-closed; everything folds into the fingerprint. References
// resolve against the SORTED unit table, so loadTech runs after the
// unit sort in Load.

import (
	"fmt"
	"io/fs"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// MaxUpgradeLevel bounds per-upgrade level counts (uint8 level space
// with headroom for the WC3-style 1..3 ranges).
const MaxUpgradeLevel = 16

// UpgradeLevel is one converted research step.
type UpgradeLevel struct {
	Costs         []int64 // per resource index (nil = free)
	ResearchTicks uint16
}

// UpgradeMod is one PER-LEVEL stat modifier: at player level L the
// contribution is Add×L added and Permille applied L times.
type UpgradeMod struct {
	Stat     uint8
	Add      int64 // sim units (convertStatAdd)
	Permille int32 // 1000 = unchanged
}

// Upgrade is one converted upgrade row. MaxLevel = len(Levels).
type Upgrade struct {
	ID        string
	Levels    []UpgradeLevel
	Mods      []UpgradeMod
	AppliesTo []uint16 // sorted unit indices; nil = every owned unit
}

// ReqTerm is one minimum-upgrade-level requirement term.
type ReqTerm struct {
	Upgrade uint16
	Level   uint8
}

// Require gates ONE admission target: training a unit type
// (IsUpgrade=false, Target = unit index) or researching an upgrade
// (IsUpgrade=true, Target = upgrade index). All terms must hold.
type Require struct {
	IsUpgrade bool
	Target    uint16
	Upgrades  []ReqTerm // sorted by upgrade index
	Alive     []uint16  // ≥1 own alive unit of each type, sorted
}

type rawTechFile struct {
	Upgrade []rawUpgrade `toml:"upgrade" json:"upgrade"`
	Require []rawRequire `toml:"require" json:"require"`
}

// rawUpgradeShard is one per-faction upgrade table under upgrades/*.toml
// (validation-and-data.md §3.3). Shards carry ONLY upgrade rows — a stray
// [[require]] (or any other key) is rejected by decodeStrict, so the tech
// tree stays authoritative in tech/upgrades.toml.
type rawUpgradeShard struct {
	Upgrade []rawUpgrade `toml:"upgrade" json:"upgrade"`
}

type rawUpgrade struct {
	ID        string            `toml:"id" json:"id"`
	AppliesTo []string          `toml:"applies-to" json:"applies-to"`
	Level     []rawUpgradeLevel `toml:"level" json:"level"`
	Mod       []rawStatMod      `toml:"mod" json:"mod"`
}

type rawUpgradeLevel struct {
	ResearchSeconds float64          `toml:"research-seconds" json:"research-seconds"`
	Costs           map[string]int64 `toml:"costs" json:"costs"`
}

type rawRequire struct {
	Unit     string           `toml:"unit" json:"unit"`
	Upgrade  string           `toml:"upgrade" json:"upgrade"`
	Upgrades map[string]int64 `toml:"upgrades" json:"upgrades"`
	Alive    []string         `toml:"alive" json:"alive"`
}

// unitIndex resolves a unit ID against the sorted unit table.
func (t *Tables) unitIndex(id string) int {
	i := sort.Search(len(t.Units), func(k int) bool { return t.Units[k].ID >= id })
	if i == len(t.Units) || t.Units[i].ID != id {
		return -1
	}
	return i
}

func (t *Tables) upgradeIndex(id string) int {
	i := sort.Search(len(t.Upgrades), func(k int) bool { return t.Upgrades[k].ID >= id })
	if i == len(t.Upgrades) || t.Upgrades[i].ID != id {
		return -1
	}
	return i
}

// loadTech reads tech/upgrades.toml (optional — absence is an empty
// tech tree) and resolves the units' researches lists (collected
// during unit decode, resolvable only once upgrades exist).
func (t *Tables) loadTech(fsys fs.FS, pendingResearches map[string][]string) error {
	techFiles, _ := listTables(fsys, "tech")
	upgFiles, _ := listTables(fsys, "upgrades")
	if len(techFiles) == 0 && len(upgFiles) == 0 {
		if len(pendingResearches) > 0 {
			return fmt.Errorf("data: units declare researches but no tech/upgrade table exists")
		}
		return nil
	}

	// addUpgrade converts one raw upgrade row from any source (the tech file or
	// an upgrades/*.toml shard) and appends it. Sort + cross-source dedup run
	// once after every source is read, so the fingerprint is independent of
	// which file a row came from (rows fold in sorted-by-ID order).
	addUpgrade := func(file string, r *rawUpgrade) error {
		fail := func(field string, e error) error {
			return fmt.Errorf("data: %s: upgrade %q: %s: %w", file, r.ID, field, e)
		}
		if r.ID == "" {
			return fmt.Errorf("data: %s: upgrade with empty id", file)
		}
		if len(r.Level) == 0 || len(r.Level) > MaxUpgradeLevel {
			return fail("level", fmt.Errorf("%d levels out of range [1, %d]", len(r.Level), MaxUpgradeLevel))
		}
		u := Upgrade{ID: r.ID}
		for li := range r.Level {
			rl := &r.Level[li]
			ticks, e := SecondsToTicks(rl.ResearchSeconds)
			if e != nil || ticks == 0 {
				return fail(fmt.Sprintf("level[%d].research-seconds", li), fmt.Errorf("%v (must be ≥ one tick)", rl.ResearchSeconds))
			}
			lv := UpgradeLevel{ResearchTicks: ticks}
			if len(rl.Costs) > 0 {
				lv.Costs = make([]int64, len(t.ResourceTypes))
				for name, v := range rl.Costs {
					res := indexOf(t.ResourceTypes, name)
					if res < 0 {
						return fail(fmt.Sprintf("level[%d].costs", li), fmt.Errorf("resource %q is not in resource-types %v", name, t.ResourceTypes))
					}
					if v < 0 || v > 1_000_000 {
						return fail(fmt.Sprintf("level[%d].costs", li), fmt.Errorf("%s = %d out of range [0, 1e6]", name, v))
					}
					lv.Costs[res] = v
				}
			}
			u.Levels = append(u.Levels, lv)
		}
		for mi := range r.Mod {
			m := &r.Mod[mi]
			si := indexOf(buffStatNames[:], m.Stat)
			if si < 0 {
				return fail("mod.stat", fmt.Errorf("%q is not move-speed|armor|attack-cooldown|attack-damage", m.Stat))
			}
			um := UpgradeMod{Stat: uint8(si), Permille: 1000}
			if m.Permille != 0 {
				if m.Permille < 0 || m.Permille > 10000 {
					return fail("mod.permille", fmt.Errorf("value %d out of range [0, 10000]", m.Permille))
				}
				um.Permille = int32(m.Permille)
			}
			add, e := convertStatAdd(um.Stat, m.Add)
			if e != nil {
				return fail("mod.add", e)
			}
			um.Add = add
			u.Mods = append(u.Mods, um)
		}
		for _, name := range r.AppliesTo {
			ui := t.unitIndex(name)
			if ui < 0 {
				return fail("applies-to", fmt.Errorf("unit %q is not defined", name))
			}
			u.AppliesTo = append(u.AppliesTo, uint16(ui))
		}
		sort.Slice(u.AppliesTo, func(a, b int) bool { return u.AppliesTo[a] < u.AppliesTo[b] })
		t.Upgrades = append(t.Upgrades, u)
		return nil
	}

	// tech/upgrades.toml carries the requirement rows (tech tree) and MAY also
	// carry upgrade rows; per-faction upgrade shards live in upgrades/*.toml.
	var raw rawTechFile
	techFile := ""
	if len(techFiles) > 0 {
		f, blob, err := readOne(fsys, "tech", "upgrades")
		if err != nil {
			return err
		}
		techFile = f
		if err := decodeStrict(f, blob, &raw); err != nil {
			return err
		}
		for i := range raw.Upgrade {
			if err := addUpgrade(f, &raw.Upgrade[i]); err != nil {
				return err
			}
		}
	}
	for _, sf := range upgFiles {
		blob, err := fs.ReadFile(fsys, sf)
		if err != nil {
			return fmt.Errorf("data: %s: %w", sf, err)
		}
		var shard rawUpgradeShard
		if err := decodeStrict(sf, blob, &shard); err != nil {
			return err
		}
		for i := range shard.Upgrade {
			if err := addUpgrade(sf, &shard.Upgrade[i]); err != nil {
				return err
			}
		}
	}
	sort.Slice(t.Upgrades, func(i, j int) bool { return t.Upgrades[i].ID < t.Upgrades[j].ID })
	for i := 1; i < len(t.Upgrades); i++ {
		if t.Upgrades[i].ID == t.Upgrades[i-1].ID {
			return fmt.Errorf("data: duplicate upgrade id %q (across tech/upgrades.toml + upgrades/*.toml)", t.Upgrades[i].ID)
		}
	}

	// requirement rows (resolve after the upgrade sort)
	for i := range raw.Require {
		r := &raw.Require[i]
		fail := func(e error) error { return fmt.Errorf("data: %s: require[%d]: %w", techFile, i, e) }
		var req Require
		switch {
		case r.Unit != "" && r.Upgrade != "":
			return fail(fmt.Errorf("names both unit %q and upgrade %q — exactly one target", r.Unit, r.Upgrade))
		case r.Unit != "":
			ui := t.unitIndex(r.Unit)
			if ui < 0 {
				return fail(fmt.Errorf("unit %q is not defined", r.Unit))
			}
			req = Require{Target: uint16(ui)}
		case r.Upgrade != "":
			gi := t.upgradeIndex(r.Upgrade)
			if gi < 0 {
				return fail(fmt.Errorf("upgrade %q is not defined", r.Upgrade))
			}
			req = Require{IsUpgrade: true, Target: uint16(gi)}
		default:
			return fail(fmt.Errorf("no target — name a unit or an upgrade"))
		}
		if len(r.Upgrades) == 0 && len(r.Alive) == 0 {
			return fail(fmt.Errorf("no terms — an empty requirement row is a load error, not an allow"))
		}
		for name, lvl := range r.Upgrades {
			gi := t.upgradeIndex(name)
			if gi < 0 {
				return fail(fmt.Errorf("term upgrade %q is not defined", name))
			}
			max := len(t.Upgrades[gi].Levels)
			if lvl < 1 || int(lvl) > max {
				return fail(fmt.Errorf("term %q level %d out of range [1, %d]", name, lvl, max))
			}
			req.Upgrades = append(req.Upgrades, ReqTerm{Upgrade: uint16(gi), Level: uint8(lvl)})
		}
		sort.Slice(req.Upgrades, func(a, b int) bool { return req.Upgrades[a].Upgrade < req.Upgrades[b].Upgrade })
		for _, name := range r.Alive {
			ui := t.unitIndex(name)
			if ui < 0 {
				return fail(fmt.Errorf("alive unit %q is not defined", name))
			}
			req.Alive = append(req.Alive, uint16(ui))
		}
		sort.Slice(req.Alive, func(a, b int) bool { return req.Alive[a] < req.Alive[b] })
		t.Requires = append(t.Requires, req)
	}
	sort.Slice(t.Requires, func(i, j int) bool {
		a, b := &t.Requires[i], &t.Requires[j]
		if a.IsUpgrade != b.IsUpgrade {
			return !a.IsUpgrade
		}
		return a.Target < b.Target
	})
	for i := 1; i < len(t.Requires); i++ {
		if t.Requires[i].IsUpgrade == t.Requires[i-1].IsUpgrade && t.Requires[i].Target == t.Requires[i-1].Target {
			return fmt.Errorf("data: %s: duplicate requirement target (isUpgrade=%v index=%d)", techFile, t.Requires[i].IsUpgrade, t.Requires[i].Target)
		}
	}

	// building research lists (collected during unit decode)
	for unitID, refs := range pendingResearches {
		ui := t.unitIndex(unitID)
		for _, ref := range refs {
			gi := t.upgradeIndex(ref)
			if gi < 0 {
				return fmt.Errorf("data: unit %q: researches reference to undefined upgrade %q", unitID, ref)
			}
			t.Units[ui].Researches = append(t.Units[ui].Researches, uint16(gi))
		}
		sort.Slice(t.Units[ui].Researches, func(a, b int) bool {
			return t.Units[ui].Researches[a] < t.Units[ui].Researches[b]
		})
	}
	return nil
}

// hashTech folds upgrades and requirement rows into the fingerprint.
func (t *Tables) hashTech(h *statehash.Hasher) {
	h.WriteU32(uint32(len(t.Upgrades)))
	for i := range t.Upgrades {
		u := &t.Upgrades[i]
		writeString(h, u.ID)
		h.WriteU32(uint32(len(u.Levels)))
		for li := range u.Levels {
			lv := &u.Levels[li]
			h.WriteU16(lv.ResearchTicks)
			h.WriteU32(uint32(len(lv.Costs)))
			for _, c := range lv.Costs {
				h.WriteI64(c)
			}
		}
		h.WriteU32(uint32(len(u.Mods)))
		for mi := range u.Mods {
			m := &u.Mods[mi]
			h.WriteU8(m.Stat)
			h.WriteI64(m.Add)
			h.WriteU32(uint32(m.Permille))
		}
		h.WriteU32(uint32(len(u.AppliesTo)))
		for _, a := range u.AppliesTo {
			h.WriteU16(a)
		}
	}
	h.WriteU32(uint32(len(t.Requires)))
	for i := range t.Requires {
		r := &t.Requires[i]
		h.WriteBool(r.IsUpgrade)
		h.WriteU16(r.Target)
		h.WriteU32(uint32(len(r.Upgrades)))
		for _, term := range r.Upgrades {
			h.WriteU16(term.Upgrade)
			h.WriteU8(term.Level)
		}
		h.WriteU32(uint32(len(r.Alive)))
		for _, a := range r.Alive {
			h.WriteU16(a)
		}
	}
}
