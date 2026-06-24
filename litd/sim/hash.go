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
	// appended by #299: per-player fog grid, entity detectability flags,
	// and building last-seen ghosts.
	"visibility",
	// #217 "userdata" retired (#571) — folded into "kv" via a reserved key.
	// appended by #217: per-unit hidden bit (ShowUnit/IsUnitHidden). Sparse
	// presence store — only hidden units contribute.
	"hidden",
	// appended by #217: per-hero XP-suspended bit (SuspendHeroXP). Sparse
	// presence store — only suspended heroes contribute.
	"xpsuspend",
	// appended by #217: per-unit paused bit (PauseUnit/IsUnitPaused). Sparse
	// presence store — only paused (frozen) units contribute.
	"pause",
	// appended by #217: per-instance name overrides (BlzSetUnitName). Sparse
	// value store — only renamed units contribute.
	"unitname",
	// appended by #241: script-created region cell sets. Gameplay state —
	// scripts branch on containment — so it must be hashed. Sparse: only
	// live regions with cells contribute (id order, ascending set cells).
	"regions",
	// appended by #218: per-player roster (controller/race/color/team/
	// start/name/allied-victory) + the asymmetric alliance bitset. Slot
	// order, append discipline keeps prior sub indices stable.
	"players",
	// appended by #371: terrain heightfield (grid dims/origin/cell + samples
	// in row-major order). Deterministic sim read behind TerrainHeight;
	// unbound contributes only the zero dims.
	"heightfield",
	// appended by #367: per-unit flight-height animation (current/target/
	// climb-rate). Sparse — only units with an explicitly set height
	// contribute, in row order.
	"flyheight",
	// appended by #376: per-unit propulsion-window overrides. Sparse — only
	// units with an explicitly set window contribute, in row order.
	"propwindow",
	// appended by #375: upkeep brackets + per-player upkeep-lost counters +
	// the inter-player transfer-tax matrix (sparse, non-zero entries only).
	"upkeep",
	// appended by #243: fog-of-war area modifiers (pool + free list), the two
	// global fog toggles, and per-unit shared-vision bitmasks.
	"fogmod",
	// appended by #257: per-player AI hook state — difficulty, paused/attached
	// flags, and the integer-pair command inbox (stack). Empty by default so an
	// AI-free match leaves this sub-hash at its zero contribution.
	"ai",
	// appended by #229: destructable rows (type/pos/facing/life/max/dead/
	// invuln/blocks/footprint). Creation order; empty default leaves the zero
	// contribution. Dead+blocking transitions also move the grid sub-hash.
	"destructables",
	// appended by #455: ECA handler-identity registry — the count and the
	// stable condition/action name table in ref order (ADR #451, R-SIM-6).
	// Registration is setup-only, so this contributes a constant for a given
	// script set; including it binds the trigger graph's identity into the
	// state hash so a divergent registration is caught here, not only at load.
	"handlers",
	// appended by #456: the first-class ECA trigger slab — every slot's
	// gen/alive/enabled/initially-on/condition-ref/events/actions in slot
	// order, then the free list (slab reuse order is state). Empty default
	// leaves the zero contribution.
	"triggers",
	// appended by #457: the boolexpr condition arena — every And/Or/Not/Cond
	// node (op + operands) in node order. Empty default leaves the zero
	// contribution.
	"boolexpr",
	// appended by #555 (PRD2 01, first of the five primitives): the
	// serializable timer wheel — count/nextSeq/Dropped, then live timers
	// in slot-ascending (canonical, wheel-independent) order, then the
	// free-list LIFO order (steers future slot assignment, R-SIM-6).
	// Empty default leaves the zero contribution.
	"timers",
	// appended by #565 (PRD2 02): the persistent unit-group store —
	// count/DroppedGroups/DroppedMembers, then live groups in slot-
	// ascending order each with Len + members (insertion order, the
	// hashed truth, R-UGR-2), then the free-list LIFO order with per-slot
	// generations (steers future slot assignment — same #612 lesson as
	// the timer pool). Start/Cap are derived (slot×perCap) and not hashed.
	"unitgroups",
	// appended by #572 (PRD2 03): the generic key-value store —
	// count/kvDropped, then pairs in array order (already canonical
	// ascending (Owner,Key)), then the key + string-value intern tables
	// in id order. The optional row index is derived, not hashed (R-KV-7).
	"kv",
	// appended by #617 (PRD2 04): the custom-event-kind registry —
	// Dropped + the name intern table in id order. Custom-kind
	// subscriptions ride the existing subscription tables (R-SIM-6); this
	// only fixes the name→kind mapping so it round-trips (R-EVT-8).
	"customevents",
	// appended by #590 (PRD2 05): the unified motion controller pool —
	// count/Dropped/wpCount + the used waypoint arena, then live movers
	// slot-ascending (every column), then the free-list with generations
	// (#612 lesson) so two worlds mint identical MoverIDs next.
	"movers",
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
		hh.WriteBool(hl.Invulnerable[i])
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
	// #476: live weapon-field overrides ride the combat sub-hash, canonical
	// order, zero contribution when empty (golden-stable).
	w.hashWeaponOverrides(hc)

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
	// #477: runtime effect-primitive registry rides the same sub. Only the names
	// (in id order) are the per-match contract. Zero contribution when empty, so
	// a base game stays byte-identical (no golden churn).
	if n := len(w.effectRegNames); n > 0 {
		had.WriteU32(uint32(n))
		for i := range w.effectRegNames {
			hashString(had, w.effectRegNames[i])
		}
	}

	hvis := h.next() // visibility (#299)
	w.Visibility.HashInto(hvis)

	// userdata (#217) retired (#571): the per-unit custom value now lives
	// in the "kv" sub via a reserved key — no dedicated hash section.

	hhd := h.next() // hidden (#217): presence = hidden
	hd2 := w.Hiddens
	hhd.WriteU32(uint32(hd2.count))
	for i := int32(0); i < hd2.count; i++ {
		hhd.WriteU32(uint32(hd2.Entity[i]))
	}

	hxs := h.next() // xpsuspend (#217): presence = XP suspended
	xs := w.XPSuspends
	hxs.WriteU32(uint32(xs.count))
	for i := int32(0); i < xs.count; i++ {
		hxs.WriteU32(uint32(xs.Entity[i]))
	}

	hpau := h.next() // pause (#217): presence = paused (frozen)
	pau := w.Pauses
	hpau.WriteU32(uint32(pau.count))
	for i := int32(0); i < pau.count; i++ {
		hpau.WriteU32(uint32(pau.Entity[i]))
	}

	hun := h.next() // unitname (#217): per-instance name overrides
	un := w.UnitNames
	hun.WriteU32(uint32(un.count))
	for i := int32(0); i < un.count; i++ {
		hun.WriteU32(uint32(un.Entity[i]))
		hashString(hun, un.Name[i])
	}

	hrg := h.next() // regions (#241): live region cell sets in id order
	rs := w.Regions
	hrg.WriteU32(uint32(len(rs.entries)))
	for id := range rs.entries {
		e := &rs.entries[id]
		hrg.WriteU32(uint32(id))
		hrg.WriteU32(e.gen)
		hrg.WriteBool(e.alive)
		if !e.alive {
			continue
		}
		hrg.WriteU32(uint32(e.popcount()))
		e.eachSetCell(func(cell int32) { hrg.WriteU32(uint32(cell)) })
		// membership (#241): units currently inside, in presence-row order
		if e.members == nil {
			hrg.WriteU32(0)
		} else {
			hrg.WriteU32(uint32(e.members.count))
			for i := int32(0); i < e.members.count; i++ {
				hrg.WriteU32(uint32(e.members.Entity[i]))
			}
		}
	}
	// free-list order steers future region-slot reuse (mirror #334 discipline)
	hrg.WriteU32(uint32(len(rs.free)))
	for _, f := range rs.free {
		hrg.WriteU32(f)
	}

	hpl := h.next() // players (#218): roster table + alliance bitset, slot order
	pr := &w.players
	for p := 0; p < MaxPlayers; p++ {
		hashString(hpl, pr.name[p])
		hpl.WriteU8(pr.controller[p])
		hpl.WriteU8(pr.race[p])
		hpl.WriteU8(pr.color[p])
		hpl.WriteU8(pr.team[p])
		hpl.WriteI64(int64(pr.startX[p]))
		hpl.WriteI64(int64(pr.startY[p]))
		hpl.WriteBool(pr.alliedVictory[p])
		// handicaps (#373): real state, affects damage/XP/revive outcomes.
		hpl.WriteI64(int64(pr.handicap[p]))
		hpl.WriteI64(int64(pr.handicapDamage[p]))
		hpl.WriteI64(int64(pr.handicapXP[p]))
		hpl.WriteI64(int64(pr.handicapReviveTime[p]))
	}
	for a := 0; a < MaxPlayers; a++ {
		for b := 0; b < MaxPlayers; b++ {
			hpl.WriteU16(pr.alliance[a][b])
		}
	}

	hhf := h.next() // heightfield (#371): dims/origin/cell, then samples
	hf := &w.height
	hhf.WriteU32(uint32(hf.cols))
	hhf.WriteU32(uint32(hf.rows))
	hhf.WriteI64(int64(hf.originX))
	hhf.WriteI64(int64(hf.originY))
	hhf.WriteI64(int64(hf.cellSize))
	for _, s := range hf.samples {
		hhf.WriteI64(int64(s))
	}

	hfl := h.next() // flyheight (#367): current/target/rate per set unit
	fs := w.Flys
	hfl.WriteU32(uint32(fs.count))
	for i := int32(0); i < fs.count; i++ {
		hfl.WriteU32(uint32(fs.Entity[i]))
		hfl.WriteI64(int64(fs.Height[i]))
		hfl.WriteI64(int64(fs.Target[i]))
		hfl.WriteI64(int64(fs.Rate[i]))
	}

	hpw := h.next() // propwindow (#376): window override per set unit
	pw := w.PropWindows
	hpw.WriteU32(uint32(pw.count))
	for i := int32(0); i < pw.count; i++ {
		hpw.WriteU32(uint32(pw.Entity[i]))
		hpw.WriteU16(uint16(pw.Value[i]))
	}

	hup := h.next() // upkeep (#375): brackets, lost counters, tax matrix
	hup.WriteU32(uint32(w.upkeepCount))
	for i := 0; i < w.upkeepCount; i++ {
		hup.WriteU32(uint32(w.upkeepFood[i]))
		for r := 0; r < data.MaxResourceTypes; r++ {
			hup.WriteI64(int64(w.upkeepRate[i][r]))
		}
	}
	for p := 0; p < MaxPlayers; p++ {
		for r := 0; r < data.MaxResourceTypes; r++ {
			hup.WriteU64(uint64(w.upkeepLost[p][r]))
		}
	}
	// tax matrix: sparse, non-zero entries in fixed (a,b,res) order.
	var taxN uint32
	for a := 0; a < MaxPlayers; a++ {
		for b := 0; b < MaxPlayers; b++ {
			for r := 0; r < data.MaxResourceTypes; r++ {
				if w.taxRate[a][b][r] != 0 {
					taxN++
				}
			}
		}
	}
	hup.WriteU32(taxN)
	for a := 0; a < MaxPlayers; a++ {
		for b := 0; b < MaxPlayers; b++ {
			for r := 0; r < data.MaxResourceTypes; r++ {
				if v := w.taxRate[a][b][r]; v != 0 {
					hup.WriteU8(uint8(a))
					hup.WriteU8(uint8(b))
					hup.WriteU8(uint8(r))
					hup.WriteI64(int64(v))
				}
			}
		}
	}

	hfm := h.next() // fogmod (#243): modifier pool, toggles, shared vision
	hfm.WriteBool(w.fogDisabled)
	hfm.WriteBool(w.fogMaskDisabled)
	fm := w.FogMods
	hfm.WriteU32(uint32(fm.count))
	for i := int32(0); i < fm.count; i++ {
		hfm.WriteBool(fm.alive[i])
		hfm.WriteU16(fm.gen[i])
		hfm.WriteU8(fm.player[i])
		hfm.WriteU8(fm.state[i])
		hfm.WriteU8(fm.kind[i])
		hfm.WriteBool(fm.shared[i])
		hfm.WriteBool(fm.active[i])
		hfm.WriteI64(int64(fm.ax[i]))
		hfm.WriteI64(int64(fm.ay[i]))
		hfm.WriteI64(int64(fm.bx[i]))
		hfm.WriteI64(int64(fm.by[i]))
	}
	hfm.WriteU32(uint32(len(fm.free)))
	for _, slot := range fm.free {
		hfm.WriteU32(uint32(slot))
	}
	sv := w.ShareVisions
	hfm.WriteU32(uint32(sv.count))
	for i := int32(0); i < sv.count; i++ {
		hfm.WriteU32(uint32(sv.Entity[i]))
		hfm.WriteU16(sv.Mask[i])
	}

	hai := h.next() // ai (#257): per-player difficulty/paused/attached + command inbox
	for p := 0; p < MaxPlayers; p++ {
		hai.WriteU8(w.ai.difficulty[p])
		hai.WriteBool(w.ai.paused[p])
		hai.WriteBool(w.ai.attached[p])
		hai.WriteU32(uint32(len(w.ai.inbox[p])))
		for _, c := range w.ai.inbox[p] {
			hai.WriteI64(int64(c.Command))
			hai.WriteI64(int64(c.Data))
		}
	}

	hde := h.next() // destructables (#229): killable/pathing-blocking widget rows
	w.Destructables.HashInto(hde)

	hhreg := h.next() // handlers (#455): ECA handler-identity registry (name table)
	w.hashHandlerReg(hhreg)

	htrg := h.next() // triggers (#456): first-class ECA trigger slab
	w.Triggers.HashInto(htrg)
	// #478: name→trigger bindings ride the trigger sub. Names in bind order +
	// the backing TriggerID. Zero contribution when empty (golden-stable).
	if n := len(w.trigNameKeys); n > 0 {
		htrg.WriteU32(uint32(n))
		for i := range w.trigNameKeys {
			hashString(htrg, w.trigNameKeys[i])
			htrg.WriteU64(uint64(w.trigNameIDs[i]))
		}
	}

	hbe := h.next() // boolexpr (#457): condition arena nodes
	w.hashExprArena(hbe)

	htm := h.next() // timers (#555): serializable timer wheel
	w.hashTimers(htm)

	hug := h.next() // unitgroups (#565): persistent unit-group store
	w.hashGroups(hug)

	hkv := h.next() // kv (#572): generic typed key-value store
	w.hashKV(hkv)

	hce := h.next() // customevents (#617): custom-event-kind registry
	w.hashCustomEvents(hce)

	hmv := h.next() // movers (#590): unified motion controller pool
	w.hashMovers(hmv)

	return reg.Sum(dst)
}

// hashMovers folds the mover pool into its sub-hash, mirroring the save
// block (#590). Header counts + the used spline waypoint arena, then live
// movers slot-ascending (allocation-history order — every serialized
// column, in struct-declaration order: the mirror invariant), then the
// free-list with per-slot generations so two worlds mint identical
// MoverIDs on the next Create (#612). The custom-step function table is
// code, not state — never hashed (re-registered at setup).
func (w *World) hashMovers(hm *statehash.Hasher) {
	ms := w.Movers
	hm.WriteU32(uint32(ms.count))
	hm.WriteU32(ms.Dropped)
	hm.WriteU32(uint32(ms.wpCount))
	for i := int32(0); i < ms.wpCount; i++ {
		hm.WriteI64(int64(ms.waypoints[i].X))
		hm.WriteI64(int64(ms.waypoints[i].Y))
	}
	for r := int32(1); r < int32(len(ms.live)); r++ {
		if !ms.live[r] {
			continue
		}
		hm.WriteU32(uint32(ms.Gen[r])<<24 | uint32(r))
		hm.WriteU8(ms.Kind[r])
		hm.WriteU32(uint32(ms.Target[r]))
		hm.WriteU32(uint32(ms.Anchor[r]))
		hm.WriteI64(int64(ms.Goal[r].X))
		hm.WriteI64(int64(ms.Goal[r].Y))
		hm.WriteI64(int64(ms.Dir[r].X))
		hm.WriteI64(int64(ms.Dir[r].Y))
		hm.WriteI64(int64(ms.Speed[r]))
		hm.WriteI64(int64(ms.Accel[r]))
		hm.WriteI64(int64(ms.Radius[r]))
		hm.WriteU16(uint16(ms.AngVel[r]))
		hm.WriteU16(uint16(ms.Angle[r]))
		hm.WriteI64(int64(ms.RangeLeft[r]))
		hm.WriteI64(int64(ms.Range0[r]))
		hm.WriteI64(int64(ms.Height[r]))
		hm.WriteU16(uint16(ms.TurnRate[r]))
		hm.WriteU32(uint32(ms.WpStart[r]))
		hm.WriteU32(uint32(ms.WpLen[r]))
		hm.WriteI64(int64(ms.WpParam[r]))
		hm.WriteU16(ms.Cont[r])
		for k := 0; k < 4; k++ {
			hm.WriteI64(ms.CState[r][k])
		}
		hm.WriteU16(ms.HitMask[r])
		hm.WriteU32(uint32(ms.Pierce[r]))
		hm.WriteU16(ms.Decay[r])
		hm.WriteU16(ms.Payload[r].Off)
		hm.WriteU16(ms.Payload[r].Len)
		hm.WriteU32(uint32(ms.Packet[r].Source))
		hm.WriteU32(uint32(ms.Packet[r].Target))
		hm.WriteI64(int64(ms.Packet[r].Amount))
		hm.WriteU8(ms.Packet[r].AttackType)
		hm.WriteU8(ms.Packet[r].Flags)
		hm.WriteU16(ms.OnDone[r])
		hm.WriteU8(ms.DoneMode[r])
		hm.WriteU8(ms.Flags[r])
		hm.WriteU32(uint32(ms.Owner[r]))
		hm.WriteU32(uint32(ms.HitN[r]))
		hm.WriteU32(uint32(len(ms.Hit[r])))
		for k := 0; k < len(ms.Hit[r]); k++ {
			hm.WriteU32(uint32(ms.Hit[r][k]))
		}
	}
	hm.WriteU32(uint32(len(ms.free)))
	for _, slot := range ms.free {
		hm.WriteU32(uint32(slot))
		hm.WriteU8(ms.Gen[slot])
	}
}

// hashCustomEvents folds the custom-event registry into its sub-hash,
// mirroring the save block (#617). Dropped + the name intern table in id
// order; a kind id is KBuiltinMax + nameId, so the names alone fix the
// whole mapping.
func (w *World) hashCustomEvents(hc *statehash.Hasher) {
	ce := w.CustomEvents
	hc.WriteU32(ce.Dropped)
	hashInternTable(hc, &ce.names)
}

// hashKV folds the key-value store into its sub-hash, mirroring the save
// block (#572, spec §7). Pairs are walked in array order — already the
// canonical ascending (Owner,Key) order, an invariant not a per-hash
// sort. Then the two intern tables in id order so a Key/string-value id
// resolves identically across engines.
func (w *World) hashKV(hk *statehash.Hasher) {
	kv := w.KV
	hk.WriteU32(uint32(kv.count))
	hk.WriteU32(kv.Dropped)
	for i := int32(0); i < kv.count; i++ {
		hk.WriteU64(kv.Owner[i])
		hk.WriteU32(kv.Key[i])
		hk.WriteU8(kv.Type[i])
		hk.WriteI64(kv.Val[i])
		hk.WriteI64(kv.Val2[i])
	}
	hashInternTable(hk, &kv.keys)
	hashInternTable(hk, &kv.strs)
}

func hashInternTable(hk *statehash.Hasher, t *internTable) {
	hk.WriteU32(uint32(len(t.list)))
	for _, s := range t.list {
		hashString(hk, s)
	}
}

// hashGroups folds the unit-group store into its sub-hash, mirroring the
// save block (#565). Live groups are walked slot-ascending (allocation-
// history order, independent of any future allocator), each contributing
// its Len then its members in insertion order — the hashed truth. The
// free-list contributes its LIFO order with each slot's generation so two
// worlds mint identical handles on the next CreateGroup (the #612 lesson).
func (w *World) hashGroups(hg *statehash.Hasher) {
	gs := w.Groups
	hg.WriteU32(uint32(gs.count))
	hg.WriteU32(gs.DroppedGroups)
	hg.WriteU32(gs.DroppedMembers)
	for idx := int32(1); idx < int32(len(gs.live)); idx++ {
		if !gs.live[idx] {
			continue
		}
		hg.WriteU32(uint32(gs.Gen[idx])<<24 | uint32(idx))
		hg.WriteU32(uint32(gs.Len[idx]))
		start := gs.Start[idx]
		for k := int32(0); k < gs.Len[idx]; k++ {
			hg.WriteU32(uint32(gs.Members[start+k]))
		}
	}
	hg.WriteU32(uint32(len(gs.free)))
	for _, slot := range gs.free {
		hg.WriteU32(uint32(slot))
		hg.WriteU8(gs.Gen[slot])
	}
}

// hashTimers folds the timer-wheel state into its sub-hash, mirroring
// the save block (serialization-and-hashing.md §3). Live timers are
// walked in slot-ascending order — a pure function of allocation
// history, independent of the (non-hashed) schedule index — so two
// engines using different index structures hash identically. The
// free-list LIFO order is included because it steers future slot
// assignment; a divergent free-list would silently mint divergent
// handles on the next create.
func (w *World) hashTimers(ht *statehash.Hasher) {
	ts := w.Timers
	ht.WriteU32(uint32(ts.count))
	ht.WriteU32(ts.nextSeq)
	ht.WriteU32(ts.Dropped)
	for idx := int32(1); idx < int32(len(ts.live)); idx++ {
		if !ts.live[idx] {
			continue
		}
		ht.WriteU32(uint32(ts.Gen[idx])<<24 | uint32(idx))
		ht.WriteU8(ts.Mode[idx])
		ht.WriteU32(ts.Interval[idx])
		ht.WriteU32(ts.WakeTick[idx])
		ht.WriteU32(ts.Remaining[idx])
		ht.WriteU32(ts.Seq[idx])
		ht.WriteU16(ts.Cont[idx])
		for k := 0; k < 4; k++ {
			ht.WriteI64(ts.State[idx][k])
		}
		ht.WriteU32(uint32(ts.Owner[idx]))
		ht.WriteBool(ts.Paused[idx])    // #611
		ht.WriteU32(ts.PausedRem[idx])  // #611
	}
	ht.WriteU32(uint32(len(ts.free)))
	for _, slot := range ts.free {
		ht.WriteU32(uint32(slot))
		// Hash the free slot's generation too (#bugfix found by #559): a
		// free slot's Gen is the value the NEXT alloc stamps, so it is
		// live state — two worlds with divergent free-slot gens mint
		// divergent handles on the next create. The entity allocator
		// hashes every slot's gen for exactly this reason; the timer pool
		// must match or the divergence only surfaces on reuse.
		ht.WriteU8(ts.Gen[slot])
	}
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
