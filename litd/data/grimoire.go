package data

// Grimoire research tracks (#155/#156; identity.md §4 — themed research tracks
// replacing flat upgrade lists: "choosing one forecloses another -> build
// variety"). A grimoire is an ORDERED track of tiers researched at a faction
// building; each tier grants units / abilities / upgrades by ID. The track and
// its grants are pure data — every grant resolves against the already-loaded
// unit / ability / upgrade tables, and the whole set folds into the
// determinism fingerprint. Foreclosure (committing to one track in a slot locks
// the alternative) is recorded here as the Slot label; enforcement is runtime.

import (
	"fmt"
	"io/fs"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// MaxGrimoireTiers bounds the per-track tier count (the v0.1 tracks are 4).
const MaxGrimoireTiers = 8

// GrimoireTier is one converted research step: its cost/time and the IDs it
// unlocks. Grant index slices are sorted; the tier sequence itself is NOT
// sorted — track order is the progression.
type GrimoireTier struct {
	ResearchTicks uint16
	Costs         []int64  // per resource index (nil = free)
	Units         []uint16 // granted unit indices, sorted
	Abilities     []uint16 // granted ability indices, sorted
	Upgrades      []uint16 // granted upgrade indices, sorted
}

// Grimoire is one converted research track.
type Grimoire struct {
	ID           string
	Name         string
	Slot         string // foreclosure slot; tracks sharing a slot are mutually exclusive (runtime-enforced)
	ResearchedAt uint16 // building unit index
	Tiers        []GrimoireTier
}

type rawGrimoireFile struct {
	Grimoire []rawGrimoire `toml:"grimoire" json:"grimoire"`
}

type rawGrimoire struct {
	ID           string            `toml:"id" json:"id"`
	Name         string            `toml:"name" json:"name"`
	Slot         string            `toml:"slot" json:"slot"`
	ResearchedAt string            `toml:"researched-at" json:"researched-at"`
	Tier         []rawGrimoireTier `toml:"tier" json:"tier"`
}

type rawGrimoireTier struct {
	ResearchSeconds float64          `toml:"research-seconds" json:"research-seconds"`
	Costs           map[string]int64 `toml:"costs" json:"costs"`
	GrantsUnits     []string         `toml:"grants-units" json:"grants-units"`
	GrantsAbilities []string         `toml:"grants-abilities" json:"grants-abilities"`
	GrantsUpgrades  []string         `toml:"grants-upgrades" json:"grants-upgrades"`
}

// loadGrimoires reads grimoires/*.toml|json (optional directory). Runs after
// units, abilities, and upgrades so every grant ID resolves.
func (t *Tables) loadGrimoires(fsys fs.FS) error {
	files, _ := listTables(fsys, "grimoires")
	if len(files) == 0 {
		return nil
	}
	for _, f := range files {
		blob, err := fs.ReadFile(fsys, f)
		if err != nil {
			return fmt.Errorf("data: %s: %w", f, err)
		}
		var rf rawGrimoireFile
		if err := decodeStrict(f, blob, &rf); err != nil {
			return err
		}
		for i := range rf.Grimoire {
			if rf.Grimoire[i].ID == "" {
				return fmt.Errorf("data: %s: grimoire with empty id", f)
			}
			g, err := t.convertGrimoire(f, &rf.Grimoire[i])
			if err != nil {
				return err
			}
			t.Grimoires = append(t.Grimoires, g)
		}
	}
	sort.Slice(t.Grimoires, func(i, j int) bool { return t.Grimoires[i].ID < t.Grimoires[j].ID })
	for i := 1; i < len(t.Grimoires); i++ {
		if t.Grimoires[i].ID == t.Grimoires[i-1].ID {
			return fmt.Errorf("data: duplicate grimoire id %q", t.Grimoires[i].ID)
		}
	}
	return nil
}

func (t *Tables) convertGrimoire(file string, r *rawGrimoire) (Grimoire, error) {
	fail := func(field string, e error) (Grimoire, error) {
		return Grimoire{}, fmt.Errorf("data: %s: grimoire %q: %s: %w", file, r.ID, field, e)
	}
	if r.Name == "" {
		return fail("name", fmt.Errorf("empty name"))
	}
	if r.Slot == "" {
		return fail("slot", fmt.Errorf("empty foreclosure slot"))
	}
	bi := t.unitIndex(r.ResearchedAt)
	if bi < 0 {
		return fail("researched-at", fmt.Errorf("unit %q is not defined", r.ResearchedAt))
	}
	if t.Units[bi].Footprint == 0 {
		return fail("researched-at", fmt.Errorf("%q is not a building (footprint 0) — a grimoire is researched at a building", r.ResearchedAt))
	}
	g := Grimoire{ID: r.ID, Name: r.Name, Slot: r.Slot, ResearchedAt: uint16(bi)}
	if len(r.Tier) == 0 || len(r.Tier) > MaxGrimoireTiers {
		return fail("tier", fmt.Errorf("%d tiers out of range [1, %d]", len(r.Tier), MaxGrimoireTiers))
	}
	for ti := range r.Tier {
		rt := &r.Tier[ti]
		tierFail := func(e error) (Grimoire, error) {
			return Grimoire{}, fmt.Errorf("data: %s: grimoire %q: tier[%d]: %w", file, r.ID, ti, e)
		}
		ticks, e := SecondsToTicks(rt.ResearchSeconds)
		if e != nil || ticks == 0 {
			return tierFail(fmt.Errorf("research-seconds %v invalid (must be >= one tick)", rt.ResearchSeconds))
		}
		gt := GrimoireTier{ResearchTicks: ticks}
		if len(rt.Costs) > 0 {
			c, err := t.costMap(fmt.Sprintf("%s: grimoire %q tier[%d] costs", file, r.ID, ti), rt.Costs)
			if err != nil {
				return Grimoire{}, err
			}
			gt.Costs = c
		}
		for _, id := range rt.GrantsUnits {
			ui := t.unitIndex(id)
			if ui < 0 {
				return tierFail(fmt.Errorf("grants-units: unit %q is not defined", id))
			}
			gt.Units = append(gt.Units, uint16(ui))
		}
		for _, id := range rt.GrantsAbilities {
			ai := t.abilityIndex(id)
			if ai < 0 {
				return tierFail(fmt.Errorf("grants-abilities: ability %q is not defined", id))
			}
			gt.Abilities = append(gt.Abilities, uint16(ai))
		}
		for _, id := range rt.GrantsUpgrades {
			gi := t.upgradeIndex(id)
			if gi < 0 {
				return tierFail(fmt.Errorf("grants-upgrades: upgrade %q is not defined", id))
			}
			gt.Upgrades = append(gt.Upgrades, uint16(gi))
		}
		if len(gt.Units)+len(gt.Abilities)+len(gt.Upgrades) == 0 {
			return tierFail(fmt.Errorf("grants nothing — each tier must grant >= 1 unit/ability/upgrade"))
		}
		sortU16(gt.Units)
		sortU16(gt.Abilities)
		sortU16(gt.Upgrades)
		g.Tiers = append(g.Tiers, gt)
	}
	return g, nil
}

func sortU16(s []uint16) { sort.Slice(s, func(i, j int) bool { return s[i] < s[j] }) }

// hashGrimoires folds the converted research tracks into the fingerprint.
func (t *Tables) hashGrimoires(h *statehash.Hasher) {
	h.WriteU32(uint32(len(t.Grimoires)))
	for i := range t.Grimoires {
		g := &t.Grimoires[i]
		writeString(h, g.ID)
		writeString(h, g.Name)
		writeString(h, g.Slot)
		h.WriteU16(g.ResearchedAt)
		h.WriteU32(uint32(len(g.Tiers)))
		for ti := range g.Tiers {
			gt := &g.Tiers[ti]
			h.WriteU16(gt.ResearchTicks)
			h.WriteU32(uint32(len(gt.Costs)))
			for _, c := range gt.Costs {
				h.WriteI64(c)
			}
			for _, grp := range [][]uint16{gt.Units, gt.Abilities, gt.Upgrades} {
				h.WriteU32(uint32(len(grp)))
				for _, x := range grp {
					h.WriteU16(x)
				}
			}
		}
	}
}
