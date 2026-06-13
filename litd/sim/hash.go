package sim

// World state hash (#203, wiring deferred from #196): per-system
// sub-hashes over the REAL stores through the litd/statehash registry.
// Iteration is row order only — SoA row order is itself sim state, so
// hashing it is correct AND map-free (R-SIM-2). The snapshot's
// FirstDivergence localizes any determinism break to one system.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// RNGCursor exposes the sim PRNG position (read-only) for hashing,
// dumps, and the replay header.
func (w *World) RNGCursor() prng.Cursor { return w.rng.Cursor() }

// HashSystems is the registration order of NewHashRegistry — the
// fixed system vocabulary of the state hash.
var HashSystems = []string{
	"tick", "entities", "transforms", "movement", "health", "owners",
	"combat", "abilities", "orders", "buffs", "missiles", "doodads",
	"sched", "prng",
	// appended by #334 (gap found in #206): these were live state the
	// hash was blind to. Appending keeps prior sub indices stable;
	// replays with the old 14-system vocabulary refuse via the
	// existing sub-count check (fail closed, self-describing).
	"collisions", "unittypes", "invents",
	// appended by #300: resource/food counters + the three economy
	// stores. Same append discipline as #334.
	"economy",
	// appended by #304: hero rows + per-player dead-hero pools.
	"heroes",
	// appended by #305: item rows (type/charges/carrier).
	"items",
	// appended by #306: patrol endpoints + leg/leash flags.
	"patrol",
	// appended by #301: building footprints + construction progress.
	"build",
	// appended by #339: deterministic game clock state.
	"clock",
	// appended by #345: per-player terminal match results.
	"gamestate",
	// appended by #349: persistent script-spawned effect rows.
	"effects",
	// appended by #354: sparse per-instance ability field overrides.
	"abilityfields",
	// appended by #355: runtime ability definitions appended after static data.
	"abilitydefs",
}

// NewHashRegistry builds a registry with the canonical system set.
// Callers retain it (and a Snapshot) across ticks for zero-alloc
// hashing.
func NewHashRegistry() *statehash.Registry {
	reg := statehash.NewRegistry()
	for _, n := range HashSystems {
		reg.Register(n)
	}
	return reg
}

// HashState writes the full authoritative state into reg (which must
// come from NewHashRegistry) and sums it into dst. Read-only over the
// world.
func (w *World) HashState(reg *statehash.Registry, dst *statehash.Snapshot) *statehash.Snapshot {
	reg.Reset()
	h := regHashers{reg: reg}

	ht := h.next() // tick
	ht.WriteU32(w.tick)

	he := h.next() // entities: full table — generations, alive bits,
	// and free-list links are state (LIFO reuse order steers every
	// future allocation; #334)
	he.WriteU32(uint32(w.unitCount))
	he.WriteU32(uint32(w.Ents.count))
	he.WriteU32(uint32(w.Ents.freeHead))
	he.WriteU32(uint32(len(w.Ents.slots)))
	for i := range w.Ents.slots {
		s := &w.Ents.slots[i]
		he.WriteU8(s.gen)
		he.WriteBool(s.alive)
		he.WriteU32(uint32(s.next))
	}

	htr := h.next() // transforms
	n := w.Transforms.Count()
	htr.WriteU32(uint32(n))
	for i := int32(0); i < n; i++ {
		htr.WriteU32(uint32(w.Transforms.Entity[i]))
		htr.WriteI64(int64(w.Transforms.Pos[i].X))
		htr.WriteI64(int64(w.Transforms.Pos[i].Y))
		htr.WriteU16(uint16(w.Transforms.Facing[i]))
	}

	hm := h.next() // movement
	m := w.Movements
	hm.WriteU32(uint32(m.count))
	for i := int32(0); i < m.count; i++ {
		hm.WriteU32(uint32(m.Entity[i]))
		hm.WriteI64(int64(m.Speed[i]))
		hm.WriteU16(uint16(m.TurnRate[i]))
		hm.WriteI64(int64(m.Target[i].X))
		hm.WriteI64(int64(m.Target[i].Y))
		hm.WriteU32(m.PathHandle[i])
		hm.WriteU32(uint32(m.WaypointIdx[i]))
		hm.WriteU16(m.Stall[i])
		hm.WriteU32(uint32(m.ResCell[i]))
		hm.WriteU8(m.State[i])
	}

	hh := h.next() // health
	hl := w.Healths
	hh.WriteU32(uint32(hl.Count()))
	for i := int32(0); i < hl.Count(); i++ {
		hh.WriteU32(uint32(hl.Entity[i]))
		hh.WriteI64(int64(hl.Life[i]))
		hh.WriteI64(int64(hl.MaxLife[i]))
		hh.WriteI64(int64(hl.Regen[i]))
		hh.WriteU16(uint16(hl.ArmorValue[i]))
		hh.WriteU8(hl.ArmorType[i])
		hh.WriteU8(hl.DeathState[i])
		hh.WriteU32(hl.DecayTicks[i])
	}

	ho := h.next() // owners
	ow := w.Owners
	ho.WriteU32(uint32(ow.count))
	for i := int32(0); i < ow.count; i++ {
		ho.WriteU32(uint32(ow.Entity[i]))
		ho.WriteU8(ow.Player[i])
		ho.WriteU8(ow.Team[i])
		ho.WriteU8(ow.Color[i])
	}

	hc := h.next() // combat
	c := w.Combats
	hc.WriteU32(uint32(c.count))
	for i := int32(0); i < c.count; i++ {
		hc.WriteU32(uint32(c.Entity[i]))
		hc.WriteI64(int64(c.AcquisitionRange[i]))
		hc.WriteU32(uint32(c.Target[i]))
		hc.WriteU32(uint32(c.LastAttacker[i]))
		hc.WriteU32(c.LastDamagedTick[i])
		for s := 0; s < WeaponSlots; s++ {
			hc.WriteU32(uint32(c.DmgBase[i][s]))
			hc.WriteU8(c.DmgDice[i][s])
			hc.WriteU8(c.DmgSides[i][s])
			hc.WriteU8(c.AttackType[i][s])
			hc.WriteU16(c.Cooldown[i][s])
			hc.WriteU16(c.DamagePt[i][s])
			hc.WriteU16(c.Backswing[i][s])
			hc.WriteI64(int64(c.Range[i][s]))
			hc.WriteU16(c.ProjRef[i][s])
			hc.WriteI64(int64(c.ProjSpeed[i][s]))
			hc.WriteU32(c.ReadyAt[i][s])
			hc.WriteU8(c.WFlags[i][s])
			hc.WriteU8(c.AtkState[i][s])
			hc.WriteU32(c.PhaseEnd[i][s])
			hc.WriteU16(c.Effects[i][s].Off)
			hc.WriteU16(c.Effects[i][s].Len)
		}
	}

	ha := h.next() // abilities
	a := w.Abilities
	ha.WriteU32(uint32(a.count))
	for i := int32(0); i < a.count; i++ {
		ha.WriteU32(uint32(a.Entity[i]))
		ha.WriteI64(int64(a.Mana[i]))
		ha.WriteI64(int64(a.MaxMana[i]))
		ha.WriteI64(int64(a.ManaRegen[i]))
		ha.WriteU8(uint8(a.CastSlot[i]))
		ha.WriteU32(a.CastEnd[i])
		for s := 0; s < AbilitySlots; s++ {
			ha.WriteU16(a.AbilityID[i][s])
			ha.WriteU8(a.Level[i][s])
			ha.WriteU32(a.ReadyAt[i][s])
			ha.WriteU8(a.CastState[i][s])
		}
	}

	hor := h.next() // orders (head + pooled queue walk per row)
	o := w.Orders
	hor.WriteU32(uint32(o.count))
	for i := int32(0); i < o.count; i++ {
		hor.WriteU32(uint32(o.Entity[i]))
		hor.WriteU8(o.Kind[i])
		hor.WriteU8(o.Phase[i])
		hor.WriteU32(uint32(o.Target[i]))
		hor.WriteI64(int64(o.Point[i].X))
		hor.WriteI64(int64(o.Point[i].Y))
		hor.WriteU16(o.Data[i])
		for e := o.QueueHead[i]; e != NoOrderEntry; e = w.orderPool[e].next {
			q := &w.orderPool[e]
			hor.WriteU8(q.kind)
			hor.WriteU32(uint32(q.target))
			hor.WriteI64(int64(q.point.X))
			hor.WriteI64(int64(q.point.Y))
			hor.WriteU16(q.data)
		}
	}
	// pool free-list ORDER steers every future queue allocation (#334)
	hor.WriteU32(uint32(w.orderFreeHead))
	for e := w.orderFreeHead; e != NoOrderEntry; e = w.orderPool[e].next {
		hor.WriteU32(uint32(e))
	}

	hb := h.next() // buffs (pool-index order: live rows + derived cache)
	p := w.Buffs
	hb.WriteU32(uint32(p.Live()))
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] {
			continue
		}
		r := &p.rows[i]
		hb.WriteU32(uint32(i))
		hb.WriteU16(r.BuffID)
		hb.WriteU8(r.Stacks)
		hb.WriteU8(r.Flags)
		hb.WriteU32(uint32(r.Target))
		hb.WriteU32(uint32(r.Source))
		hb.WriteU32(r.RemainingTicks)
		hb.WriteU32(r.PeriodicClock)
	}
	// pool free-stack ORDER steers every future buff allocation (#334)
	hb.WriteU32(uint32(len(p.free)))
	for _, f := range p.free {
		hb.WriteU32(uint32(f))
	}

	hmi := h.next() // missiles
	ms := w.Missiles
	hmi.WriteU32(uint32(ms.Count()))
	for i := int32(0); i < ms.Count(); i++ {
		hmi.WriteU32(uint32(ms.Entity[i]))
		hmi.WriteI64(int64(ms.Speed[i]))
		hmi.WriteI64(int64(ms.Accel[i]))
		hmi.WriteI64(int64(ms.Arc[i]))
		hmi.WriteU8(ms.Flags[i])
		hmi.WriteU16(ms.HitMask[i])
		hmi.WriteU16(ms.GuidanceID[i])
		hmi.WriteU16(ms.ImpactID[i])
		hmi.WriteU32(uint32(ms.GuideEnt[i]))
		hmi.WriteI64(int64(ms.GuidePt[i].X))
		hmi.WriteI64(int64(ms.GuidePt[i].Y))
		hmi.WriteU16(ms.Payload[i].Off)
		hmi.WriteU16(ms.Payload[i].Len)
		hmi.WriteU32(uint32(ms.Packet[i].Source))
		hmi.WriteU32(uint32(ms.Packet[i].Target))
		hmi.WriteI64(int64(ms.Packet[i].Amount))
		hmi.WriteU8(ms.Packet[i].AttackType)
		hmi.WriteU32(uint32(ms.Source[i]))
		hmi.WriteU32(ms.BirthTick[i])
		hmi.WriteI64(int64(ms.Dir[i].X))
		hmi.WriteI64(int64(ms.Dir[i].Y))
		hmi.WriteI64(int64(ms.RangeLeft[i]))
		hmi.WriteU32(uint32(ms.PierceLeft[i]))
		hmi.WriteU16(ms.Decay[i])
	}

	hd := h.next() // doodads
	w.Doodads.HashInto(hd)

	hsc := h.next() // sched
	hsc.WriteBytes(w.Sched.Save(nil))

	hp := h.next() // prng
	cur := w.rng.Cursor()
	hp.WriteU64(cur.State)
	hp.WriteU64(cur.Inc)

	hco := h.next() // collisions (#334)
	co := w.Collisions
	hco.WriteU32(uint32(co.count))
	for i := int32(0); i < co.count; i++ {
		hco.WriteU32(uint32(co.Entity[i]))
		hco.WriteU8(co.SizeClass[i])
		hco.WriteU8(co.PathFlags[i])
		hco.WriteU32(uint32(co.StampRef[i]))
	}

	hut := h.next() // unittypes (#334)
	ut := w.UnitTypes
	hut.WriteU32(uint32(ut.count))
	for i := int32(0); i < ut.count; i++ {
		hut.WriteU32(uint32(ut.Entity[i]))
		hut.WriteU16(ut.TypeID[i])
	}

	hin := h.next() // invents (#334; class cooldowns appended by #305)
	in := w.Invents
	hin.WriteU32(uint32(in.count))
	for i := int32(0); i < in.count; i++ {
		hin.WriteU32(uint32(in.Entity[i]))
		for s := 0; s < InventorySlots; s++ {
			hin.WriteU32(uint32(in.Slots[i][s]))
		}
		for c := 0; c < data.ItemClassCount; c++ {
			hin.WriteU32(in.ClassReady[i][c])
		}
	}

	heco := h.next() // economy (#300/#302): counters + node/econ/harvest/produce rows
	heco.WriteU32(uint32(w.resourceCount))
	for pl := 0; pl < MaxPlayers; pl++ {
		if w.resourceCount > 0 {
			for _, v := range w.resources[pl] {
				heco.WriteU64(uint64(v))
			}
		}
		heco.WriteU32(uint32(w.foodUsed[pl]))
		heco.WriteU32(uint32(w.foodCap[pl]))
	}
	nd := w.Nodes
	heco.WriteU32(uint32(nd.count))
	for i := int32(0); i < nd.count; i++ {
		heco.WriteU32(uint32(nd.Entity[i]))
		heco.WriteU8(nd.Resource[i])
		heco.WriteU64(uint64(nd.Remaining[i]))
		heco.WriteU8(nd.Flags[i])
		heco.WriteU32(uint32(nd.Busy[i]))
	}
	ec := w.Econs
	heco.WriteU32(uint32(ec.count))
	for i := int32(0); i < ec.count; i++ {
		heco.WriteU32(uint32(ec.Entity[i]))
		heco.WriteU8(ec.FoodCost[i])
		heco.WriteU8(ec.FoodProvided[i])
		heco.WriteU16(ec.DepotMask[i])
	}
	hv := w.Harvests
	heco.WriteU32(uint32(hv.count))
	for i := int32(0); i < hv.count; i++ {
		heco.WriteU32(uint32(hv.Entity[i]))
		heco.WriteU8(hv.State[i])
		heco.WriteU32(uint32(hv.Node[i]))
		heco.WriteU32(uint32(hv.Depot[i]))
		heco.WriteU32(uint32(hv.Carried[i]))
		heco.WriteU8(hv.CarriedRes[i])
		heco.WriteU32(hv.Clock[i])
		heco.WriteU32(uint32(hv.Capacity[i]))
		heco.WriteU16(hv.GatherTicks[i])
		heco.WriteU16(hv.Mask[i])
	}
	pq := w.Produce // production queues (#302): full slot arrays —
	// freed slots are zero-filled canonically (shiftQueue)
	heco.WriteU32(uint32(pq.count))
	for i := int32(0); i < pq.count; i++ {
		heco.WriteU32(uint32(pq.Entity[i]))
		heco.WriteU8(pq.QCount[i])
		for q := 0; q < ProduceQueueCap; q++ {
			heco.WriteU16(pq.Queue[i][q])
			heco.WriteU8(pq.QFlags[i][q])
		}
		heco.WriteU32(pq.Done[i])
		heco.WriteU8(pq.RallyKind[i])
		heco.WriteU32(uint32(pq.RallyEnt[i]))
		heco.WriteI64(int64(pq.RallyPoint[i].X))
		heco.WriteI64(int64(pq.RallyPoint[i].Y))
	}
	heco.WriteU32(uint32(len(w.upgradeDefs))) // tech levels/caps (#303)
	for pl := 0; pl < MaxPlayers; pl++ {
		for u := range w.upgradeDefs {
			heco.WriteU8(w.upgradeLevel[pl][u])
			heco.WriteU8(w.techMax[pl][u])
		}
	}

	hhr := h.next() // heroes (#304): rows + dead pools
	hs := w.Heroes
	hhr.WriteU32(uint32(hs.count))
	for i := int32(0); i < hs.count; i++ {
		hhr.WriteU32(uint32(hs.Entity[i]))
		hhr.WriteU16(hs.HeroType[i])
		hhr.WriteI64(hs.XP[i])
		hhr.WriteU8(hs.Level[i])
		hhr.WriteI64(int64(hs.Str[i]))
		hhr.WriteI64(int64(hs.Agi[i]))
		hhr.WriteI64(int64(hs.Int[i]))
		hhr.WriteU8(hs.SkillPoints[i])
		for sl := 0; sl < data.MaxHeroSkills; sl++ {
			hhr.WriteU8(hs.SkillLevel[i][sl])
		}
	}
	for pl := 0; pl < MaxPlayers; pl++ {
		for sl := 0; sl < MaxDeadHeroes; sl++ {
			rec := &w.deadHeroes[pl][sl]
			hhr.WriteBool(rec.Used)
			if !rec.Used {
				continue
			}
			hhr.WriteU16(rec.HeroType)
			hhr.WriteI64(rec.XP)
			hhr.WriteU8(rec.Level)
			hhr.WriteI64(int64(rec.Str))
			hhr.WriteI64(int64(rec.Agi))
			hhr.WriteI64(int64(rec.Int))
			hhr.WriteU8(rec.SkillPoints)
			for k := 0; k < data.MaxHeroSkills; k++ {
				hhr.WriteU8(rec.SkillLevel[k])
			}
		}
	}

	hit := h.next() // items (#305): type/charges/carrier per row
	is := w.Items
	hit.WriteU32(uint32(is.count))
	for i := int32(0); i < is.count; i++ {
		hit.WriteU32(uint32(is.Entity[i]))
		hit.WriteU16(is.TypeID[i])
		hit.WriteU16(is.Charges[i])
		hit.WriteU32(uint32(is.Carrier[i]))
	}

	hpa := h.next() // patrol (#306): endpoints + leg/leash flags
	ps := w.Patrol
	hpa.WriteU32(uint32(ps.count))
	for i := int32(0); i < ps.count; i++ {
		hpa.WriteU32(uint32(ps.Entity[i]))
		hpa.WriteI64(int64(ps.A[i].X))
		hpa.WriteI64(int64(ps.A[i].Y))
		hpa.WriteI64(int64(ps.B[i].X))
		hpa.WriteI64(int64(ps.B[i].Y))
		hpa.WriteU8(ps.Flags[i])
	}

	hbd := h.next() // build (#301): footprint + construction progress
	bs := w.Build
	hbd.WriteU32(uint32(bs.count))
	for i := int32(0); i < bs.count; i++ {
		hbd.WriteU32(uint32(bs.Entity[i]))
		hbd.WriteU32(uint32(bs.Builder[i]))
		hbd.WriteI64(int64(bs.FX[i]))
		hbd.WriteI64(int64(bs.FY[i]))
		hbd.WriteI64(int64(bs.FW[i]))
		hbd.WriteU16(bs.Progress[i])
	}

	hcl := h.next() // clock (#339): fixed-point ToD + drift-free remainder
	hcl.WriteI64(int64(w.tod))
	hcl.WriteI64(int64(w.todScale))
	hcl.WriteBool(w.todFrozen)
	hcl.WriteU64(w.todCarry)
	hcl.WriteU32(w.dayLengthTicks)

	hgs := h.next() // gamestate (#345): per-player terminal results
	for player := 0; player < MaxPlayers; player++ {
		hgs.WriteU8(w.results[player])
	}

	hfx := h.next() // effects (#349): persistent script effect rows
	fx := w.Effects
	hfx.WriteU32(uint32(fx.Count()))
	for i := int32(0); i < fx.Count(); i++ {
		hfx.WriteU16(fx.ModelID[i])
		hfx.WriteI64(int64(fx.Scale[i]))
		hfx.WriteU32(fx.ColorRGBA[i])
		hfx.WriteU32(fx.BirthTick[i])
		hfx.WriteU32(uint32(fx.Entity[i]))
	}

	haf := h.next() // abilityfields (#354): live rows + free-stack order
	af := w.AbilityFields
	haf.WriteU32(uint32(af.Count()))
	for i := 0; i < af.Cap(); i++ {
		if !af.live[i] {
			continue
		}
		haf.WriteU32(uint32(i))
		haf.WriteU32(uint32(af.Ent[i]))
		haf.WriteU8(af.Slot[i])
		haf.WriteU8(af.Field[i])
		haf.WriteI64(int64(af.Value[i]))
	}
	haf.WriteU32(uint32(len(af.free)))
	for _, f := range af.free {
		haf.WriteU32(uint32(f))
	}
	haf.WriteU64(af.Rejected())

	had := h.next() // abilitydefs (#355): runtime rows only; static rows are fingerprinted data
	had.WriteU32(uint32(len(w.runtimeAbilityDefs)))
	for i := range w.runtimeAbilityDefs {
		hashAbilityDef(had, &w.runtimeAbilityDefs[i])
	}

	return reg.Sum(dst)
}

func hashAbilityDef(h *statehash.Hasher, def *data.Ability) {
	hashString(h, def.ID)
	hashString(h, def.Name)
	h.WriteU16(def.Effects.Off)
	h.WriteU16(def.Effects.Len)
	h.WriteU32(uint32(def.ManaCost))
	h.WriteU16(def.CooldownTicks)
	h.WriteU16(def.CastPointTicks)
	h.WriteU16(def.BackswingTicks)
	h.WriteU16(def.ChannelTicks)
	h.WriteI64(int64(def.CastRange))
}

func hashString(h *statehash.Hasher, s string) {
	h.WriteU32(uint32(len(s)))
	for i := 0; i < len(s); i++ {
		h.WriteU8(s[i])
	}
}

// regHashers walks a registry's hashers in registration order without
// re-registering (Register is build-once).
type regHashers struct {
	reg *statehash.Registry
	i   int
}

func (r *regHashers) next() *statehash.Hasher {
	h := r.reg.Hasher(r.i)
	r.i++
	return h
}
