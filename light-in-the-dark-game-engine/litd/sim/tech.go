package sim

// Upgrade research + tech-tree gating (#303,
// abilities-and-buffs.md): per-player upgrade levels, requirement
// admission for training and research, and stat application through
// the derived-stat cache.
//
// Research rides the #302 production queue as a TrainFlagResearch
// row — cost/cancel/refund semantics identical, slot value = upgrade
// index, no food. Requirements check at ADMISSION only (WC3: a
// requirement building destroyed after enqueue does not cancel the
// queued entry; the next enqueue refuses). Completion bumps the
// player's level, emits EvResearchFinished in flush order, and
// re-derives every affected owned unit's stat cache — upgrades fold
// FIRST (upgrade index ascending, level-many permille applications),
// then buffs in their #162 (BuffID, instance) order, so the cache is
// always upgrades⊕buffs over the table base value.
//
// BindTech installs the requirement check into the #302 tech-gate
// hook. SetTechAllowed caps a player's reachable level below the
// table maximum (SetPlayerTechMaxAllowed analogue).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// EvResearchFinished: Src = the researching building,
// Arg = upgrade<<8 | new level.
const EvResearchFinished uint16 = 13

// Research-specific refusal reasons (continuing the Train* space).
const (
	TrainMaxLevel     uint8 = 8 // at the table/SetTechAllowed cap
	TrainResearchBusy uint8 = 9 // same upgrade already queued for this player
)

// BindTech installs the loaded upgrade and requirement tables and
// wires the requirement check into the #302 admission hook. Requires
// BindUnitDefs and BindEconomy first (references resolve against
// them); rebinding a different upgrade count is refused (the level
// arrays are state).
func (w *World) BindTech(upgrades []data.Upgrade, requires []data.Require) bool {
	if w.unitDefs == nil || len(upgrades) == 0 || len(upgrades) > 1<<16 {
		return false
	}
	if w.upgradeDefs != nil && len(w.upgradeDefs) != len(upgrades) {
		return false
	}
	for i := range upgrades {
		for _, a := range upgrades[i].AppliesTo {
			if int(a) >= len(w.unitDefs) {
				return false
			}
		}
		for li := range upgrades[i].Levels {
			if c := upgrades[i].Levels[li].Costs; c != nil && len(c) != w.resourceCount {
				return false
			}
		}
	}
	for i := range requires {
		r := &requires[i]
		if r.IsUpgrade && int(r.Target) >= len(upgrades) {
			return false
		}
		if !r.IsUpgrade && int(r.Target) >= len(w.unitDefs) {
			return false
		}
		for _, term := range r.Upgrades {
			if int(term.Upgrade) >= len(upgrades) || int(term.Level) > len(upgrades[term.Upgrade].Levels) {
				return false
			}
		}
		for _, a := range r.Alive {
			if int(a) >= len(w.unitDefs) {
				return false
			}
		}
	}
	w.upgradeDefs = upgrades
	w.techReqs = requires
	w.reqOfUnit = make([]int32, len(w.unitDefs))
	for i := range w.reqOfUnit {
		w.reqOfUnit[i] = -1
	}
	w.reqOfUpgrade = make([]int32, len(upgrades))
	for i := range w.reqOfUpgrade {
		w.reqOfUpgrade[i] = -1
	}
	for i := range requires {
		if requires[i].IsUpgrade {
			w.reqOfUpgrade[requires[i].Target] = int32(i)
		} else {
			w.reqOfUnit[requires[i].Target] = int32(i)
		}
	}
	for p := 0; p < MaxPlayers; p++ {
		w.upgradeLevel[p] = make([]uint8, len(upgrades))
		w.techMax[p] = make([]uint8, len(upgrades))
		for u := range upgrades {
			w.techMax[p][u] = uint8(len(upgrades[u].Levels))
		}
	}
	w.SetTechGate(func(player uint8, typeID uint16) bool {
		return w.UnitRequirementsMet(player, typeID)
	})
	return true
}

// UpgradeLevel reads a player's researched level of one upgrade.
func (w *World) UpgradeLevel(player uint8, upgrade uint16) uint8 {
	if player >= MaxPlayers || w.upgradeDefs == nil || int(upgrade) >= len(w.upgradeDefs) {
		return 0
	}
	return w.upgradeLevel[player][upgrade]
}

// SetTechAllowed caps a player's reachable level of one upgrade
// (clamped to the table maximum). The SetPlayerTechMaxAllowed
// analogue; lowering below the researched level only blocks FURTHER
// research, never revokes.
func (w *World) SetTechAllowed(player uint8, upgrade uint16, maxLevel uint8) bool {
	if player >= MaxPlayers || w.upgradeDefs == nil || int(upgrade) >= len(w.upgradeDefs) {
		return false
	}
	if m := uint8(len(w.upgradeDefs[upgrade].Levels)); maxLevel > m {
		maxLevel = m
	}
	w.techMax[player][upgrade] = maxLevel
	return true
}

// UnitRequirementsMet is the train/build admission term: true when
// the player satisfies the unit type's requirement row (no row =
// allowed, the table's explicit vocabulary).
func (w *World) UnitRequirementsMet(player uint8, typeID uint16) bool {
	if w.reqOfUnit == nil || int(typeID) >= len(w.reqOfUnit) {
		return true
	}
	ri := w.reqOfUnit[typeID]
	if ri == -1 {
		return true
	}
	return w.requirementMet(player, &w.techReqs[ri])
}

func (w *World) requirementMet(player uint8, r *data.Require) bool {
	for _, term := range r.Upgrades {
		if w.upgradeLevel[player][term.Upgrade] < term.Level {
			return false
		}
	}
	for _, ut := range r.Alive {
		if !w.ownAliveOfType(player, ut) {
			return false
		}
	}
	return true
}

// ownAliveOfType scans the dense unit-type rows for one own alive
// unit of the type (bounded by live units; admission-time only).
func (w *World) ownAliveOfType(player uint8, typeID uint16) bool {
	ut := w.UnitTypes
	for r := int32(0); r < ut.Count(); r++ {
		if ut.TypeID[r] != typeID {
			continue
		}
		id := ut.Entity[r]
		if !w.Ents.Alive(id) {
			continue
		}
		or := w.Owners.Row(id)
		if or != -1 && w.Owners.Player[or] == player {
			return true
		}
	}
	return false
}

// ResearchUpgrade admits one research request on a producing
// building. Rides the production queue as a flagged row; refusals
// reuse the Train* vocabulary + EvTrainRefused (Arg low bits carry
// the upgrade index).
func (w *World) ResearchUpgrade(building EntityID, upgrade uint16) uint8 {
	refuse := func(reason uint8) uint8 {
		w.Emit(Event{Kind: EvTrainRefused, Src: building, Arg: int64(reason)<<16 | int64(upgrade)})
		return reason
	}
	r := w.Produce.Row(building)
	or := w.Owners.Row(building)
	if r == -1 || or == -1 || !w.Ents.Alive(building) || w.upgradeDefs == nil {
		return refuse(TrainNoProducer)
	}
	if int(upgrade) >= len(w.upgradeDefs) {
		return refuse(TrainUnknownType)
	}
	if !w.buildingResearches(building, upgrade) {
		return refuse(TrainNotTrainable)
	}
	if w.Produce.QCount[r] >= ProduceQueueCap {
		return refuse(TrainQueueFull)
	}
	p := w.Owners.Player[or]
	if p >= MaxPlayers {
		return refuse(TrainNoProducer)
	}
	def := &w.upgradeDefs[upgrade]
	cur := w.upgradeLevel[p][upgrade]
	allowed := w.techMax[p][upgrade]
	if uint8(len(def.Levels)) < allowed {
		allowed = uint8(len(def.Levels))
	}
	if cur >= allowed {
		return refuse(TrainMaxLevel)
	}
	if w.researchPending(p, upgrade) {
		return refuse(TrainResearchBusy)
	}
	if ri := w.reqOfUpgrade[upgrade]; ri != -1 && !w.requirementMet(p, &w.techReqs[ri]) {
		return refuse(TrainTechLocked)
	}
	costs := def.Levels[cur].Costs
	for i := 0; i < len(costs) && i < w.resourceCount; i++ {
		if w.resources[p][i] < costs[i] {
			return refuse(TrainNoResources)
		}
	}
	for i := 0; i < len(costs) && i < w.resourceCount; i++ {
		w.resources[p][i] -= costs[i]
	}
	q := w.Produce.QCount[r]
	w.Produce.Queue[r][q] = upgrade
	w.Produce.QFlags[r][q] = TrainFlagResearch
	w.Produce.QCount[r] = q + 1
	if q == 0 {
		w.Produce.Done[r] = w.tick + uint32(def.Levels[cur].ResearchTicks)
	}
	return TrainOK
}

// buildingResearches reports whether the upgrade is in the
// building's data-table researches list.
func (w *World) buildingResearches(building EntityID, upgrade uint16) bool {
	ut := w.UnitTypes.Row(building)
	if ut == -1 {
		return false
	}
	btid := w.UnitTypes.TypeID[ut]
	if int(btid) >= len(w.unitDefs) {
		return false
	}
	for _, rs := range w.unitDefs[btid].Researches {
		if rs == upgrade {
			return true
		}
	}
	return false
}

// researchPending scans every production queue for the same upgrade
// already queued by the same player (one in-flight research per
// upgrade per player — duplicate admission would double-spend the
// level cost).
func (w *World) researchPending(player uint8, upgrade uint16) bool {
	s := w.Produce
	for r := int32(0); r < s.count; r++ {
		or := w.Owners.Row(s.Entity[r])
		if or == -1 || w.Owners.Player[or] != player {
			continue
		}
		for q := 0; q < int(s.QCount[r]); q++ {
			if s.QFlags[r][q]&TrainFlagResearch != 0 && s.Queue[r][q] == upgrade {
				return true
			}
		}
	}
	return false
}

// completeResearch finishes a flagged head row: level up, event,
// re-derive affected owned units.
func (w *World) completeResearch(r int32, building EntityID, player uint8, upgrade uint16) {
	if w.upgradeLevel[player][upgrade] < uint8(len(w.upgradeDefs[upgrade].Levels)) {
		w.upgradeLevel[player][upgrade]++
	}
	lvl := w.upgradeLevel[player][upgrade]
	w.Emit(Event{Kind: EvResearchFinished, Src: building, Arg: int64(upgrade)<<8 | int64(lvl)})
	w.rederiveUpgradeStats(player, upgrade)
}

// rederiveUpgradeStats recomputes the derived-stat cache of every
// own alive unit the upgrade applies to (dense type-row order; each
// recompute is per-entity independent, so order is presentation
// only).
func (w *World) rederiveUpgradeStats(player uint8, upgrade uint16) {
	def := &w.upgradeDefs[upgrade]
	ut := w.UnitTypes
	for r := int32(0); r < ut.Count(); r++ {
		if !upgradeApplies(def, ut.TypeID[r]) {
			continue
		}
		id := ut.Entity[r]
		if !w.Ents.Alive(id) {
			continue
		}
		or := w.Owners.Row(id)
		if or == -1 || w.Owners.Player[or] != player {
			continue
		}
		w.recomputeBuffStats(id)
	}
}

// upgradeApplies: nil AppliesTo = every unit; else sorted-list scan.
func upgradeApplies(def *data.Upgrade, typeID uint16) bool {
	if len(def.AppliesTo) == 0 {
		return true
	}
	for _, a := range def.AppliesTo {
		if a == typeID {
			return true
		}
		if a > typeID {
			return false
		}
	}
	return false
}

// foldUpgradeStats writes the player's upgrade contribution for one
// entity into the derived-stat cache (called by recomputeBuffStats
// BEFORE the buff fold; upgrade index ascending, Permille applied
// level-many times).
func (w *World) foldUpgradeStats(id EntityID) {
	if w.upgradeDefs == nil {
		return
	}
	or := w.Owners.Row(id)
	utr := w.UnitTypes.Row(id)
	if or == -1 || utr == -1 {
		return
	}
	p := w.Owners.Player[or]
	if p >= MaxPlayers {
		return
	}
	tid := w.UnitTypes.TypeID[utr]
	idx := id.Index()
	for u := range w.upgradeDefs {
		lvl := w.upgradeLevel[p][u]
		if lvl == 0 {
			continue
		}
		def := &w.upgradeDefs[u]
		if !upgradeApplies(def, tid) {
			continue
		}
		for mi := range def.Mods {
			m := &def.Mods[mi]
			mult := fixed.FromInt(m.Permille).Div(fixed.FromInt(1000))
			for l := uint8(0); l < lvl; l++ {
				w.buffAdd[m.Stat][idx] += m.Add
				w.buffMult[m.Stat][idx] = w.buffMult[m.Stat][idx].Mul(mult)
			}
		}
	}
}
