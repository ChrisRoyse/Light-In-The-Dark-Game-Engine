package sim

// Hero progression (#304, combat-and-orders.md §5.3): XP, the
// data-driven level curve (D4 — no formula in code), attribute
// growth, skill points, and revive through the #302 production
// queue.
//
// XP: a lethal damage packet pays the victim's table bounty to every
// alive hero ON THE KILLER'S TEAM within the share radius of the
// victim (entity-index ascending; split per table rule — equal uses
// integer division, the remainder is dropped deterministically).
// Level-ups happen inside the grant: attributes grow by the hero
// table, +1 skill point, EvHeroLevel per level.
//
// Attributes apply in two ways: str→max-life/life/regen and
// int→max-mana/mana/mana-regen write store DELTAS at grant time
// (saved with their stores); agi→armor/attack-cooldown folds into
// the #162 derived-stat cache (upgrades → hero attributes → buffs).
// Folds truncate to FULL attribute points (WC3).
//
// Death captures a D-15 self-contained record (type IDs + instance
// fields, no EntityIDs) into the owner's dead-hero pool, XP scaled
// by the death-penalty rule (never below the current level's
// floor — heroes never de-level). Revive rides the production queue
// as a TrainFlagHeroRevive row on an altar-class building: cost/time
// from the revive table by record level, food reserved like a train,
// full refund on cancel, the pool slot stays locked while queued.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Hero events (#304).
const (
	// EvHeroLevel: Src = hero, Arg = the new level.
	EvHeroLevel uint16 = 14
	// EvHeroDied: Src = hero, Arg = dead-pool slot index, or -1 when
	// the pool was full and the record was dropped (visible, counted).
	EvHeroDied uint16 = 15
)

// MaxDeadHeroes bounds each player's dead-hero pool.
const MaxDeadHeroes = 16

// LearnSkill outcomes.
const (
	SkillOK         uint8 = 0
	SkillNoHero     uint8 = 1
	SkillUnknown    uint8 = 2
	SkillNoPoints   uint8 = 3
	SkillMaxLevel   uint8 = 4
	SkillTierLocked uint8 = 5
)

// HeroRecord is the D-15 self-contained progression record: type IDs
// and instance fields only — no EntityIDs, valid across worlds.
type HeroRecord struct {
	HeroType    uint16 // index into the bound hero table
	XP          int64
	Level       uint8
	Str         fixed.F64
	Agi         fixed.F64
	Int         fixed.F64
	SkillPoints uint8
	SkillLevel  [data.MaxHeroSkills]uint8
	Used        bool // pool occupancy
}

// ---- hero store (T2 pattern) ----

type HeroStore struct {
	HeroType    []uint16
	XP          []int64
	Level       []uint8
	Str         []fixed.F64
	Agi         []fixed.F64
	Int         []fixed.F64
	SkillPoints []uint8
	SkillLevel  [][data.MaxHeroSkills]uint8
	Entity      []EntityID

	rowOf []int32
	count int32
}

func NewHeroStore(rowCap, entityCap int) *HeroStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &HeroStore{
		HeroType:    make([]uint16, rowCap),
		XP:          make([]int64, rowCap),
		Level:       make([]uint8, rowCap),
		Str:         make([]fixed.F64, rowCap),
		Agi:         make([]fixed.F64, rowCap),
		Int:         make([]fixed.F64, rowCap),
		SkillPoints: make([]uint8, rowCap),
		SkillLevel:  make([][data.MaxHeroSkills]uint8, rowCap),
		Entity:      make([]EntityID, rowCap),
		rowOf:       make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *HeroStore) add(e *Entities, id EntityID, rec *HeroRecord) bool {
	if !e.Alive(id) || s.rowOf[id.Index()] != -1 || int(s.count) == len(s.XP) {
		return false
	}
	r := s.count
	s.HeroType[r] = rec.HeroType
	s.XP[r] = rec.XP
	s.Level[r] = rec.Level
	s.Str[r] = rec.Str
	s.Agi[r] = rec.Agi
	s.Int[r] = rec.Int
	s.SkillPoints[r] = rec.SkillPoints
	s.SkillLevel[r] = rec.SkillLevel
	s.Entity[r] = id
	s.rowOf[id.Index()] = r
	s.count++
	return true
}

func (s *HeroStore) Remove(id EntityID) bool {
	r := s.rowOf[id.Index()]
	if r == -1 {
		return false
	}
	last := s.count - 1
	s.HeroType[r] = s.HeroType[last]
	s.XP[r] = s.XP[last]
	s.Level[r] = s.Level[last]
	s.Str[r] = s.Str[last]
	s.Agi[r] = s.Agi[last]
	s.Int[r] = s.Int[last]
	s.SkillPoints[r] = s.SkillPoints[last]
	s.SkillLevel[r] = s.SkillLevel[last]
	s.Entity[r] = s.Entity[last]
	s.rowOf[s.Entity[r].Index()] = r
	s.rowOf[id.Index()] = -1
	s.count--
	return true
}

func (s *HeroStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}
func (s *HeroStore) Count() int32 { return s.count }

// ---- world surface ----

// IsHero reports whether the unit carries a hero record.
func (w *World) IsHero(id EntityID) bool { return w.Heroes.Row(id) != -1 }

// HeroLevel returns the hero's current level, or 0 when the unit is not a hero
// (GetHeroLevel returns 0 for non-heroes).
func (w *World) HeroLevel(id EntityID) uint8 {
	if r := w.Heroes.Row(id); r != -1 {
		return w.Heroes.Level[r]
	}
	return 0
}

// HeroXP returns the hero's accumulated experience, or 0 when not a hero
// (GetHeroXP).
func (w *World) HeroXP(id EntityID) int64 {
	if r := w.Heroes.Row(id); r != -1 {
		return w.Heroes.XP[r]
	}
	return 0
}

// HeroStr/HeroAgi/HeroInt return the hero's primary attributes. The engine
// stores a single effective attribute value per hero — there is no separate
// base/bonus split — so GetHeroStr's includeBonuses parameter is moot and is
// dropped in the Go API (see the type:discovery filed with #217). 0 when the
// unit is not a hero.
func (w *World) HeroStr(id EntityID) fixed.F64 {
	if r := w.Heroes.Row(id); r != -1 {
		return w.Heroes.Str[r]
	}
	return 0
}

func (w *World) HeroAgi(id EntityID) fixed.F64 {
	if r := w.Heroes.Row(id); r != -1 {
		return w.Heroes.Agi[r]
	}
	return 0
}

func (w *World) HeroInt(id EntityID) fixed.F64 {
	if r := w.Heroes.Row(id); r != -1 {
		return w.Heroes.Int[r]
	}
	return 0
}

// SetHeroStr sets the hero's strength to an absolute value and applies the
// derived consequences (max life + regen) through the same delta path level-ups
// use, then refolds derived stats. No-op on a non-hero or before hero tables are
// bound. SetHeroStr's `permanent` arg is moot here — there is no temporary
// attribute layer (#366). Returns false if it could not apply.
func (w *World) SetHeroStr(id EntityID, newStr fixed.F64) bool {
	r := w.Heroes.Row(id)
	if r == -1 || w.heroTables == nil {
		return false
	}
	d := newStr.Sub(w.Heroes.Str[r])
	w.Heroes.Str[r] = newStr
	w.applyAttrDelta(id, d, 0)
	w.recomputeBuffStats(id)
	return true
}

// SetHeroAgi sets the hero's agility to an absolute value. Agility's derived
// effects (armor, attack cooldown) are recomputed from the stored attribute, so
// no delta is threaded — just refold. No-op on a non-hero / unbound tables.
func (w *World) SetHeroAgi(id EntityID, newAgi fixed.F64) bool {
	r := w.Heroes.Row(id)
	if r == -1 || w.heroTables == nil {
		return false
	}
	w.Heroes.Agi[r] = newAgi
	w.recomputeBuffStats(id)
	return true
}

// SetHeroInt sets the hero's intelligence to an absolute value and applies the
// derived consequences (max mana + regen) through the delta path. No-op on a
// non-hero / unbound tables.
func (w *World) SetHeroInt(id EntityID, newInt fixed.F64) bool {
	r := w.Heroes.Row(id)
	if r == -1 || w.heroTables == nil {
		return false
	}
	d := newInt.Sub(w.Heroes.Int[r])
	w.Heroes.Int[r] = newInt
	w.applyAttrDelta(id, 0, d)
	w.recomputeBuffStats(id)
	return true
}

// BindHeroes installs the loaded hero rule set. Requires
// BindUnitDefs (+ BindAbilityDefs for skills, BindEconomy for revive
// costs) first; rebinding is refused.
func (w *World) BindHeroes(h *data.HeroTables) bool {
	if h == nil || w.unitDefs == nil || len(h.Curve) < 2 {
		return false
	}
	if w.heroTables != nil {
		return false
	}
	if len(h.Bounty) != len(w.unitDefs) {
		return false
	}
	for i := range h.Heroes {
		hd := &h.Heroes[i]
		if int(hd.Unit) >= len(w.unitDefs) || len(hd.Skills) > data.MaxHeroSkills ||
			len(hd.Skills) > AbilitySlots {
			return false
		}
		for si := range hd.Skills {
			if int(hd.Skills[si].Ability) >= len(w.abilityDefs) {
				return false
			}
		}
	}
	for _, c := range [][]int64{h.Revive.CostsBase, h.Revive.CostsPerLevel} {
		if c != nil && len(c) != w.resourceCount {
			return false
		}
	}
	w.heroTables = h
	return true
}

// SpawnHero spawns the hero's unit row and attaches a fresh level-1
// hero component (base attributes, the table's starting skill
// points).
func (w *World) SpawnHero(heroType uint16, player, team uint8, pos fixed.Vec2) (EntityID, bool) {
	if w.heroTables == nil || int(heroType) >= len(w.heroTables.Heroes) {
		return 0, false
	}
	hd := &w.heroTables.Heroes[heroType]
	rec := HeroRecord{
		HeroType: heroType, Level: 1,
		Str: hd.Str, Agi: hd.Agi, Int: hd.Int,
		SkillPoints: w.heroTables.StartSkillPts, Used: true,
	}
	return w.InstantiateHero(&rec, player, team, pos)
}

// InstantiateHero spawns a unit from a D-15 record — campaign
// carry-over and revive both land here. Fail-closed teardown on any
// component refusal.
func (w *World) InstantiateHero(rec *HeroRecord, player, team uint8, pos fixed.Vec2) (EntityID, bool) {
	if w.heroTables == nil || int(rec.HeroType) >= len(w.heroTables.Heroes) ||
		rec.Level < 1 || int(rec.Level) > len(w.heroTables.Curve) {
		return 0, false
	}
	hd := &w.heroTables.Heroes[rec.HeroType]
	id, ok := w.SpawnFromTable(hd.Unit, player, team, pos)
	if !ok {
		return 0, false
	}
	if w.Abilities.Row(id) == -1 && !w.Abilities.Add(w.Ents, id) {
		w.DestroyUnit(id)
		return 0, false
	}
	if !w.Heroes.add(w.Ents, id, rec) {
		w.DestroyUnit(id)
		return 0, false
	}
	// str/int store deltas from the FULL attribute values (the unit
	// table row is the zero-attribute baseline)
	w.applyAttrDelta(id, rec.Str, rec.Int)
	// learned skills occupy ability slots 0..len(skills)-1
	for si := range hd.Skills {
		if lvl := rec.SkillLevel[si]; lvl > 0 {
			if !w.SetAbility(id, si, int(hd.Skills[si].Ability)) {
				w.DestroyUnit(id)
				return 0, false
			}
			ar := w.Abilities.Row(id)
			w.Abilities.Level[ar][si] = lvl
		}
	}
	w.recomputeBuffStats(id) // agi fold
	return id, true
}

// ExtractHeroRecord snapshots a live hero as a D-15 record.
func (w *World) ExtractHeroRecord(id EntityID) (HeroRecord, bool) {
	r := w.Heroes.Row(id)
	if r == -1 {
		return HeroRecord{}, false
	}
	h := w.Heroes
	return HeroRecord{
		HeroType: h.HeroType[r], XP: h.XP[r], Level: h.Level[r],
		Str: h.Str[r], Agi: h.Agi[r], Int: h.Int[r],
		SkillPoints: h.SkillPoints[r], SkillLevel: h.SkillLevel[r], Used: true,
	}, true
}

// DeadHero reads one pool slot (zero record, Used=false when empty).
func (w *World) DeadHero(player uint8, slot int) HeroRecord {
	if player >= MaxPlayers || slot < 0 || slot >= MaxDeadHeroes {
		return HeroRecord{}
	}
	return w.deadHeroes[player][slot]
}

// applyAttrDelta writes the str/int-driven store deltas for a CHANGE
// of dStr/dInt attribute points (fixed). Life/mana gain the same
// amount their maxima gain (WC3 level-up behavior).
func (w *World) applyAttrDelta(id EntityID, dStr, dInt fixed.F64) {
	a := &w.heroTables.Attr
	if hr := w.Healths.Row(id); hr != -1 && dStr != 0 {
		dHP := a.StrHP.Mul(dStr)
		w.Healths.MaxLife[hr] = w.Healths.MaxLife[hr].Add(dHP)
		w.Healths.Life[hr] = w.Healths.Life[hr].Add(dHP)
		w.Healths.Regen[hr] = w.Healths.Regen[hr].Add(a.StrRegen.Mul(dStr))
	}
	if ar := w.Abilities.Row(id); ar != -1 && dInt != 0 {
		dM := a.IntMana.Mul(dInt)
		w.Abilities.MaxMana[ar] = w.Abilities.MaxMana[ar].Add(dM)
		w.Abilities.Mana[ar] = w.Abilities.Mana[ar].Add(dM)
		w.Abilities.ManaRegen[ar] = w.Abilities.ManaRegen[ar].Add(a.IntManaRegen.Mul(dInt))
	}
}

// AddXP grants XP (kill bounties and script grants both land here)
// and resolves level-ups at exact curve boundaries. XP caps at the
// curve top.
func (w *World) AddXP(id EntityID, amount int64) bool {
	r := w.Heroes.Row(id)
	if r == -1 || amount <= 0 || w.heroTables == nil {
		return false
	}
	h := w.Heroes
	curve := w.heroTables.Curve
	xp := h.XP[r] + amount
	if top := curve[len(curve)-1]; xp > top {
		xp = top
	}
	h.XP[r] = xp
	hd := &w.heroTables.Heroes[h.HeroType[r]]
	for int(h.Level[r]) < len(curve) && xp >= curve[h.Level[r]] {
		h.Level[r]++
		h.Str[r] = h.Str[r].Add(hd.StrG)
		h.Agi[r] = h.Agi[r].Add(hd.AgiG)
		h.Int[r] = h.Int[r].Add(hd.IntG)
		h.SkillPoints[r]++
		w.applyAttrDelta(id, hd.StrG, hd.IntG)
		w.Emit(Event{Kind: EvHeroLevel, Src: id, Arg: int64(h.Level[r])})
	}
	w.recomputeBuffStats(id)
	return true
}

// grantKillXP pays the victim's bounty to the killer team's heroes
// in share range of the victim (damage apply pass, lethal crossing
// only).
func (w *World) grantKillXP(killer, victim EntityID) {
	ht := w.heroTables
	if ht == nil {
		return
	}
	utr := w.UnitTypes.Row(victim)
	if utr == -1 {
		return
	}
	bounty := ht.Bounty[w.UnitTypes.TypeID[utr]]
	if bounty == 0 {
		return
	}
	kor := w.Owners.Row(killer)
	vtr := w.Transforms.Row(victim)
	if kor == -1 || vtr == -1 || !w.Ents.Alive(killer) {
		return
	}
	team := w.Owners.Team[kor]
	pos := w.Transforms.Pos[vtr]
	// pass 1: count receivers (entity-index ascending = row scan with
	// deterministic membership; order is irrelevant for the split)
	n := int64(0)
	for hr := int32(0); hr < w.Heroes.count; hr++ {
		if w.heroInShare(hr, team, pos) {
			n++
		}
	}
	if n == 0 {
		return
	}
	share := bounty
	if ht.Split == data.SplitEqual {
		share = bounty / n // integer division; remainder dropped deterministically
	}
	if share == 0 {
		return
	}
	for hr := int32(0); hr < w.Heroes.count; hr++ {
		if w.heroInShare(hr, team, pos) {
			w.AddXP(w.Heroes.Entity[hr], share)
		}
	}
}

func (w *World) heroInShare(hr int32, team uint8, pos fixed.Vec2) bool {
	id := w.Heroes.Entity[hr]
	if !w.Ents.Alive(id) {
		return false
	}
	or := w.Owners.Row(id)
	if or == -1 || w.Owners.Team[or] != team {
		return false
	}
	tr := w.Transforms.Row(id)
	if tr == -1 {
		return false
	}
	return fixed.DistSqLess(pos, w.Transforms.Pos[tr], w.heroTables.ShareRadius)
}

// LearnSkill spends one skill point on skill index `skill` (= the
// ability slot it occupies). Tier gates come from the hero table.
func (w *World) LearnSkill(id EntityID, skill int) uint8 {
	r := w.Heroes.Row(id)
	if r == -1 || w.heroTables == nil {
		return SkillNoHero
	}
	h := w.Heroes
	hd := &w.heroTables.Heroes[h.HeroType[r]]
	if skill < 0 || skill >= len(hd.Skills) {
		return SkillUnknown
	}
	if h.SkillPoints[r] == 0 {
		return SkillNoPoints
	}
	sk := &hd.Skills[skill]
	cur := h.SkillLevel[r][skill]
	if int(cur) >= len(sk.MinHeroLevel) {
		return SkillMaxLevel
	}
	if h.Level[r] < sk.MinHeroLevel[cur] {
		return SkillTierLocked
	}
	h.SkillPoints[r]--
	h.SkillLevel[r][skill] = cur + 1
	ar := w.Abilities.Row(id)
	if cur == 0 {
		w.SetAbility(id, skill, int(sk.Ability))
		ar = w.Abilities.Row(id)
	}
	w.Abilities.Level[ar][skill] = cur + 1
	return SkillOK
}

// captureHeroDeath snapshots a dying hero into the owner's pool
// (DestroyUnit, before the owner row goes). The death penalty scales
// XP but never below the current level's curve floor — heroes do not
// de-level.
func (w *World) captureHeroDeath(id EntityID) {
	r := w.Heroes.Row(id)
	if r == -1 {
		return
	}
	defer w.Heroes.Remove(id)
	ht := w.heroTables
	or := w.Owners.Row(id)
	if ht == nil || or == -1 {
		return
	}
	p := w.Owners.Player[or]
	if p >= MaxPlayers {
		return
	}
	rec, _ := w.ExtractHeroRecord(id)
	if ht.DeathPenalty > 0 {
		kept := rec.XP * int64(1000-ht.DeathPenalty) / 1000
		if floor := ht.Curve[rec.Level-1]; kept < floor {
			kept = floor
		}
		rec.XP = kept
	}
	for s := 0; s < MaxDeadHeroes; s++ {
		if !w.deadHeroes[p][s].Used {
			w.deadHeroes[p][s] = rec
			w.Emit(Event{Kind: EvHeroDied, Src: id, Arg: int64(s)})
			return
		}
	}
	w.Emit(Event{Kind: EvHeroDied, Src: id, Arg: -1}) // pool full: dropped, visibly
}

// reviveCost / reviveTicks: table cost for a record level.
func (w *World) reviveCostAt(level uint8, res int) int64 {
	rv := &w.heroTables.Revive
	c := int64(0)
	if rv.CostsBase != nil {
		c += rv.CostsBase[res]
	}
	if rv.CostsPerLevel != nil {
		c += rv.CostsPerLevel[res] * int64(level-1)
	}
	return c
}

func (w *World) reviveTicksAt(level uint8) uint16 {
	rv := &w.heroTables.Revive
	return rv.BaseTicks + rv.TicksPerLevel*uint16(level-1)
}

// ReviveHero admits a revive of dead-pool slot `slot` on an
// altar-class building. Rides the production queue flagged
// TrainFlagHeroRevive; cost by record level + the hero unit's food
// reservation; the pool slot stays locked while queued.
func (w *World) ReviveHero(altar EntityID, slot int) uint8 {
	refuse := func(reason uint8) uint8 {
		w.Emit(Event{Kind: EvTrainRefused, Src: altar, Arg: int64(reason)<<16 | int64(slot)})
		return reason
	}
	r := w.Produce.Row(altar)
	or := w.Owners.Row(altar)
	if r == -1 || or == -1 || !w.Ents.Alive(altar) || w.heroTables == nil || !w.buildingRevives(altar) {
		return refuse(TrainNoProducer)
	}
	p := w.Owners.Player[or]
	if p >= MaxPlayers {
		return refuse(TrainNoProducer)
	}
	if slot < 0 || slot >= MaxDeadHeroes || !w.deadHeroes[p][slot].Used {
		return refuse(TrainUnknownType)
	}
	if w.Produce.QCount[r] >= ProduceQueueCap {
		return refuse(TrainQueueFull)
	}
	if w.revivePending(p, uint16(slot)) {
		return refuse(TrainResearchBusy)
	}
	rec := &w.deadHeroes[p][slot]
	foodCost := w.unitDefs[w.heroTables.Heroes[rec.HeroType].Unit].FoodCost
	if foodCost > 0 && !w.CanAddFood(p, foodCost) {
		return refuse(TrainNoFood)
	}
	for i := 0; i < w.resourceCount; i++ {
		if w.resources[p][i] < w.reviveCostAt(rec.Level, i) {
			return refuse(TrainNoResources)
		}
	}
	for i := 0; i < w.resourceCount; i++ {
		w.resources[p][i] -= w.reviveCostAt(rec.Level, i)
	}
	w.foodUsed[p] += int32(foodCost)
	q := w.Produce.QCount[r]
	w.Produce.Queue[r][q] = uint16(slot)
	w.Produce.QFlags[r][q] = TrainFlagHeroRevive
	w.Produce.QCount[r] = q + 1
	if q == 0 {
		w.Produce.Done[r] = w.tick + uint32(w.reviveTicksAt(rec.Level))
	}
	return TrainOK
}

func (w *World) buildingRevives(altar EntityID) bool {
	ut := w.UnitTypes.Row(altar)
	if ut == -1 {
		return false
	}
	btid := w.UnitTypes.TypeID[ut]
	return int(btid) < len(w.unitDefs) && w.unitDefs[btid].RevivesHeroes
}

// revivePending scans the queues for the same pool slot already
// queued by this player.
func (w *World) revivePending(player uint8, slot uint16) bool {
	s := w.Produce
	for r := int32(0); r < s.count; r++ {
		or := w.Owners.Row(s.Entity[r])
		if or == -1 || w.Owners.Player[or] != player {
			continue
		}
		for q := 0; q < int(s.QCount[r]); q++ {
			if s.QFlags[r][q]&TrainFlagHeroRevive != 0 && s.Queue[r][q] == slot {
				return true
			}
		}
	}
	return false
}

// completeRevive finishes a flagged revive head: spawn the hero from
// the pool record at the building exit, release the food
// reservation, free the slot. Returns false to retry next tick
// (blocked exit / unit cap).
func (w *World) completeRevive(r int32, building EntityID, player uint8, slot uint16) bool {
	rec := w.deadHeroes[player][slot]
	if !rec.Used { // pool slot vanished (save edits refused; defensive)
		return true // pop the queue entry; nothing to spawn
	}
	hd := &w.heroTables.Heroes[rec.HeroType]
	unitDef := &w.unitDefs[hd.Unit]
	pos, ok := w.spawnCell(building, unitDef)
	if !ok {
		return false
	}
	or := w.Owners.Row(building)
	team := w.Owners.Team[or]
	id, ok := w.InstantiateHero(&rec, player, team, pos)
	if !ok {
		return false
	}
	w.foodUsed[player] -= int32(unitDef.FoodCost)
	w.deadHeroes[player][slot] = HeroRecord{}
	w.Emit(Event{Kind: EvUnitTrained, Src: building, Dst: id, Arg: int64(hd.Unit)})
	w.issueRally(r, id)
	return true
}

// foldHeroAttrStats writes the agility-driven derived contributions
// (called by recomputeBuffStats after the upgrade fold, before
// buffs): armor add = floor(agi × coeff); attack-cooldown permille
// applied once per FULL agility point.
func (w *World) foldHeroAttrStats(id EntityID) {
	if w.heroTables == nil {
		return
	}
	r := w.Heroes.Row(id)
	if r == -1 {
		return
	}
	a := &w.heroTables.Attr
	idx := id.Index()
	agi := w.Heroes.Agi[r]
	if a.AgiArmor > 0 {
		w.buffAdd[data.StatArmor][idx] += agi.Mul(a.AgiArmor).Floor()
	}
	if a.AgiCooldownPermille != 1000 {
		mult := fixed.FromInt(a.AgiCooldownPermille).Div(fixed.FromInt(1000))
		for pts := agi.Floor(); pts > 0; pts-- {
			w.buffMult[data.StatAttackCooldown][idx] = w.buffMult[data.StatAttackCooldown][idx].Mul(mult)
		}
	}
}
