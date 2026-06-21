package sim

// Full sim-state serialization (#206, R-SIM-6). The saved set IS the
// hashed set (determinism.md §5): every field HashState folds is
// written field-by-field — never raw struct memory — little-endian,
// fixed widths, versioned, prefixed with the data-table fingerprint
// and capability table. Load restores into the preallocated stores of
// a world built with IDENTICAL caps; row order is state and is
// preserved exactly, including pool free-list order (it steers every
// future allocation).
//
// Save runs between ticks only, enforced fail-closed: mid-tick
// residue (pending events, kill buffer, queued damage) refuses the
// save rather than silently dropping transient state.
//
// Load is two-phase: phase A decodes and validates the ENTIRE stream
// into staging (any malformation is a named error and the world is
// untouched); phase B applies. The scheduler blob applies first —
// sched.Load is itself atomic and is the only fallible apply step
// (unregistered ContID) — then the infallible store copies.
//
// Derived state is NOT saved and is rebuilt at load: store rowOf
// indexes (from Entity columns), the spatial bucket grid (from
// transform rows — acquisition outcomes are bucket-order independent
// by design, buckets.go), the buff derived-stat cache (canonical
// recompute per carrier), doodad byPlacement (re-sorted), and grid
// dynamic reservations (from Movement.ResCell). Pending staged
// commands are driver state, not sim state, and are not saved.

import (
	"encoding/binary"
	"fmt"
	"io"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// SaveMagic opens every save file.
const SaveMagic = "LITDSAV\x01"

// SaveFormatVersion bumps on any layout change.
// v30: first-class ECA trigger slab appended after the handler registry:
// every slot (gen/alive + alive fields: enabled/initially-on/condition-ref/
// events/actions) in slot order, then the free list (#456, ADR #451).
// v29: ECA handler-identity registry name table appended after
// destructables: the stable condition/action names in ref order, used as a
// load-time manifest to validate the re-registered registry (#455, ADR #451).
// v27: AI hook state appended after fog overrides: per-player difficulty,
// paused/attached flags, and the integer-pair command stack (#257).
// v26: fog-of-war overrides appended after upkeep: the global fog/mask
// toggles, the modifier pool + free list, and per-unit shared vision (#243).
// v25: upkeep economy appended after the heightfield: tax brackets,
// per-player upkeep-lost counters, and the sparse inter-player tax matrix (#375).
// v24: per-unit propulsion-window override rows as a sparse section after
// flyheight (#376).
// v23: per-unit flight-height animation rows (current/target/climb-rate)
// as a sparse section after unitname (#367).
// v22: terrain heightfield (grid dims/origin/cell + row-major samples)
// appended after the player section (#371).
// v21: four per-player difficulty handicaps (handicap / handicapDamage /
// handicapXP / handicapReviveTime) interleaved into each roster slot (#373).
// v20: player section appended after regions: per-player roster
// (controller/race/color/team/start/name/allied-victory) + the
// asymmetric alliance bitset (#218). Resources/food stay in the economy
// section.
// v19: region section grows per-region membership (units currently inside,
// in presence-row order) after the cell list — needed so enter/leave edge
// triggers match the no-save path across a load (#241).
// v18: region section appended after the subscription table: per region
// (id order) gen/alive + ascending set-cell indices, then the free-list
// order (#241).
// v17: visibility section appended after runtime ability definitions:
// fog state/cycle bytes, entity detectability flags, last-seen buildings (#299).
// v16: missile rows grow Accel, HitMask, GuidanceID, ImpactID after
// Speed/Flags (#336).
// v15: runtime ability definitions appended after ability field
// overrides (#355); capability table includes RuntimeAbilityDefs.
// v14: ability field override rows + free-list order appended after
// effects (#354).
// v13: persistent effect rows appended after gamestate (#349).
// v12: capability table includes persistent effect cap (#348);
// live effect rows are serialized starting in v13.
// v11: gamestate section (per-player match results) appended after
// the clock section (#345).
// v10: clock section (tod, scale, frozen, carry, day length) appended
// after the build rows (#339).
// v9: build rows (footprint + construction progress + builder) appended
// after the patrol rows (#301).
// v8: missile rows grow linear/pierce skillshot fields (Dir, RangeLeft,
// PierceLeft, Decay) appended after BirthTick (#331).
// v7: patrol rows (endpoints + leg/leash flags) appended after the
// item rows (#306).
// v6: item rows appended after the hero section; the inventory
// section grows per-class use cooldowns (#305).
// v5: hero section (#304) — hero rows + per-player dead-hero pools
// appended after the tech arrays.
// v4: tech section (#303) — per-player upgrade level/cap arrays
// appended after the produce rows.
// v3: production section (#302) — produce rows (queues, head clock,
// rally) appended after the harvest rows.
// v2: economy sections (#300) — resource counters, node/econ/harvest
// stores — appended after doodads.
const SaveFormatVersion uint32 = 30

// ---- little-endian writer / reader ----

type saveWriter struct {
	w       io.Writer
	scratch [8]byte
	err     error
}

func (s *saveWriter) write(b []byte) {
	if s.err == nil {
		_, s.err = s.w.Write(b)
	}
}
func (s *saveWriter) u8(v uint8) { s.scratch[0] = v; s.write(s.scratch[:1]) }
func (s *saveWriter) u16(v uint16) {
	binary.LittleEndian.PutUint16(s.scratch[:2], v)
	s.write(s.scratch[:2])
}
func (s *saveWriter) u32(v uint32) {
	binary.LittleEndian.PutUint32(s.scratch[:4], v)
	s.write(s.scratch[:4])
}
func (s *saveWriter) u64(v uint64) {
	binary.LittleEndian.PutUint64(s.scratch[:8], v)
	s.write(s.scratch[:8])
}
func (s *saveWriter) i32(v int32)       { s.u32(uint32(v)) }
func (s *saveWriter) i64(v int64)       { s.u64(uint64(v)) }
func (s *saveWriter) f64(v fixed.F64)   { s.i64(int64(v)) }
func (s *saveWriter) vec2(v fixed.Vec2) { s.f64(v.X); s.f64(v.Y) }
func (s *saveWriter) ent(v EntityID)    { s.u32(uint32(v)) }
func (s *saveWriter) str(v string) {
	s.u32(uint32(len(v)))
	s.write([]byte(v))
}
func (s *saveWriter) boolean(v bool) {
	if v {
		s.u8(1)
	} else {
		s.u8(0)
	}
}

type saveReader struct {
	r       io.Reader
	scratch [8]byte
	err     error
	what    string // section being read, for truncation errors
}

func (s *saveReader) fail() {
	if s.err == nil {
		s.err = fmt.Errorf("sim: save: truncated while reading %s", s.what)
	}
}
func (s *saveReader) read(n int) []byte {
	if s.err != nil {
		return s.scratch[:n]
	}
	if _, err := io.ReadFull(s.r, s.scratch[:n]); err != nil {
		s.fail()
	}
	return s.scratch[:n]
}
func (s *saveReader) u8() uint8      { return s.read(1)[0] }
func (s *saveReader) u16() uint16    { return binary.LittleEndian.Uint16(s.read(2)) }
func (s *saveReader) u32() uint32    { return binary.LittleEndian.Uint32(s.read(4)) }
func (s *saveReader) u64() uint64    { return binary.LittleEndian.Uint64(s.read(8)) }
func (s *saveReader) i32() int32     { return int32(s.u32()) }
func (s *saveReader) i64() int64     { return int64(s.u64()) }
func (s *saveReader) f64() fixed.F64 { return fixed.F64(s.i64()) }
func (s *saveReader) vec2() fixed.Vec2 {
	x := s.f64()
	return fixed.Vec2{X: x, Y: s.f64()}
}
func (s *saveReader) ent() EntityID { return EntityID(s.u32()) }
func (s *saveReader) str(max int) (string, error) {
	n := s.u32()
	if s.err != nil {
		return "", s.err
	}
	if n > uint32(max) {
		return "", fmt.Errorf("sim: save: %s string length %d exceeds limit %d", s.what, n, max)
	}
	b := make([]byte, n)
	if _, err := io.ReadFull(s.r, b); err != nil {
		s.fail()
		return "", s.err
	}
	return string(b), nil
}
func (s *saveReader) boolean() bool { return s.u8() != 0 }

// ---- save ----

// SaveState writes the full authoritative sim state. fingerprint is
// the bound data-table content hash (0 when no tables are bound); it
// is embedded so Load can refuse a save against different content.
// Save refuses mid-tick state — call it between Step()s only.
func (w *World) SaveState(out io.Writer, fingerprint uint64) error {
	if w.eventCount != 0 || len(w.killed) != 0 || len(w.dmgBuf) != 0 {
		return fmt.Errorf("sim: save: mid-tick state (events=%d killed=%d damage=%d) — save between ticks only",
			w.eventCount, len(w.killed), len(w.dmgBuf))
	}
	for player, pending := range w.resultPending {
		if pending != ResultPlaying {
			return fmt.Errorf("sim: save: pending match result request for player %d — step before saving", player)
		}
	}
	s := &saveWriter{w: out}
	if _, err := io.WriteString(out, SaveMagic); err != nil {
		return err
	}
	s.u32(SaveFormatVersion)
	s.u64(fingerprint)
	s.u32(uint32(w.caps.Units))
	s.u32(uint32(w.caps.Projectiles))
	s.u32(uint32(w.caps.Effects))
	s.u32(uint32(w.caps.BuffInstances))
	s.u32(uint32(w.caps.OrderQueueEntries))
	s.u32(uint32(w.caps.PendingEvents))
	s.u32(uint32(w.caps.PathRequests))
	s.u32(uint32(w.caps.ScriptedDoodads))
	s.u32(uint32(w.caps.Destructables))
	s.u32(uint32(w.caps.RuntimeAbilityDefs))
	s.u32(uint32(w.caps.Triggers))
	s.u32(w.tick)
	s.u32(uint32(w.unitCount))
	cur := w.rng.Cursor()
	s.u64(cur.State)
	s.u64(cur.Inc)

	// entities: full table — generations, alive bits, free links
	s.u32(uint32(len(w.Ents.slots)))
	s.i32(w.Ents.freeHead)
	s.u32(uint32(w.Ents.count))
	for i := range w.Ents.slots {
		sl := &w.Ents.slots[i]
		s.u8(sl.gen)
		s.boolean(sl.alive)
		s.i32(sl.next)
	}

	// transforms
	t := w.Transforms
	s.u32(uint32(t.Count()))
	for i := int32(0); i < t.Count(); i++ {
		s.ent(t.Entity[i])
		s.vec2(t.Pos[i])
		s.u16(uint16(t.Facing[i]))
	}

	// movement
	m := w.Movements
	s.u32(uint32(m.count))
	for i := int32(0); i < m.count; i++ {
		s.ent(m.Entity[i])
		s.f64(m.Speed[i])
		s.u16(uint16(m.TurnRate[i]))
		s.vec2(m.Target[i])
		s.u32(m.PathHandle[i])
		s.i32(m.WaypointIdx[i])
		s.u16(m.Stall[i])
		s.i32(m.ResCell[i])
		s.u8(m.State[i])
	}

	// collisions
	co := w.Collisions
	s.u32(uint32(co.count))
	for i := int32(0); i < co.count; i++ {
		s.ent(co.Entity[i])
		s.u8(co.SizeClass[i])
		s.u8(co.PathFlags[i])
		s.i32(co.StampRef[i])
	}

	// unit types
	ut := w.UnitTypes
	s.u32(uint32(ut.count))
	for i := int32(0); i < ut.count; i++ {
		s.ent(ut.Entity[i])
		s.u16(ut.TypeID[i])
	}

	// userdata (#217): custom value per unit
	ud := w.UserDatas
	s.u32(uint32(ud.count))
	for i := int32(0); i < ud.count; i++ {
		s.ent(ud.Entity[i])
		s.i32(ud.Value[i])
	}

	// hidden (#217): presence = hidden
	hd := w.Hiddens
	s.u32(uint32(hd.count))
	for i := int32(0); i < hd.count; i++ {
		s.ent(hd.Entity[i])
	}

	// xpsuspend (#217): presence = XP suspended
	xs := w.XPSuspends
	s.u32(uint32(xs.count))
	for i := int32(0); i < xs.count; i++ {
		s.ent(xs.Entity[i])
	}

	// pause (#217): presence = paused (frozen)
	pau := w.Pauses
	s.u32(uint32(pau.count))
	for i := int32(0); i < pau.count; i++ {
		s.ent(pau.Entity[i])
	}

	// unitname (#217): per-instance name overrides
	un := w.UnitNames
	s.u32(uint32(un.count))
	for i := int32(0); i < un.count; i++ {
		s.ent(un.Entity[i])
		s.str(un.Name[i])
	}

	// flyheight (#367): current/target/climb-rate per set unit
	fly := w.Flys
	s.u32(uint32(fly.count))
	for i := int32(0); i < fly.count; i++ {
		s.ent(fly.Entity[i])
		s.f64(fly.Height[i])
		s.f64(fly.Target[i])
		s.f64(fly.Rate[i])
	}

	// propwindow (#376): window override per set unit
	pw := w.PropWindows
	s.u32(uint32(pw.count))
	for i := int32(0); i < pw.count; i++ {
		s.ent(pw.Entity[i])
		s.u16(uint16(pw.Value[i]))
	}

	// health
	hl := w.Healths
	s.u32(uint32(hl.Count()))
	for i := int32(0); i < hl.Count(); i++ {
		s.ent(hl.Entity[i])
		s.f64(hl.Life[i])
		s.f64(hl.MaxLife[i])
		s.f64(hl.Regen[i])
		s.u16(uint16(hl.ArmorValue[i]))
		s.u8(hl.ArmorType[i])
		s.u8(hl.DeathState[i])
		s.u32(hl.DecayTicks[i])
		s.boolean(hl.Invulnerable[i])
	}

	// owners
	ow := w.Owners
	s.u32(uint32(ow.count))
	for i := int32(0); i < ow.count; i++ {
		s.ent(ow.Entity[i])
		s.u8(ow.Player[i])
		s.u8(ow.Team[i])
		s.u8(ow.Color[i])
	}

	// combat
	c := w.Combats
	s.u32(uint32(c.count))
	for i := int32(0); i < c.count; i++ {
		s.ent(c.Entity[i])
		s.f64(c.AcquisitionRange[i])
		s.ent(c.Target[i])
		s.ent(c.LastAttacker[i])
		s.u32(c.LastDamagedTick[i])
		for sl := 0; sl < WeaponSlots; sl++ {
			s.i32(c.DmgBase[i][sl])
			s.u8(c.DmgDice[i][sl])
			s.u8(c.DmgSides[i][sl])
			s.u8(c.AttackType[i][sl])
			s.u16(c.Cooldown[i][sl])
			s.u16(c.DamagePt[i][sl])
			s.u16(c.Backswing[i][sl])
			s.f64(c.Range[i][sl])
			s.u16(c.ProjRef[i][sl])
			s.f64(c.ProjSpeed[i][sl])
			s.u32(c.ReadyAt[i][sl])
			s.u8(c.WFlags[i][sl])
			s.u8(c.AtkState[i][sl])
			s.u32(c.PhaseEnd[i][sl])
			s.u16(c.Effects[i][sl].Off)
			s.u16(c.Effects[i][sl].Len)
		}
	}

	// abilities
	a := w.Abilities
	s.u32(uint32(a.count))
	for i := int32(0); i < a.count; i++ {
		s.ent(a.Entity[i])
		s.f64(a.Mana[i])
		s.f64(a.MaxMana[i])
		s.f64(a.ManaRegen[i])
		s.u8(uint8(a.CastSlot[i]))
		s.u32(a.CastEnd[i])
		for sl := 0; sl < AbilitySlots; sl++ {
			s.u16(a.AbilityID[i][sl])
			s.u8(a.Level[i][sl])
			s.u32(a.ReadyAt[i][sl])
			s.u8(a.CastState[i][sl])
		}
	}

	// inventories (v6 appends the per-class use cooldowns, #305)
	in := w.Invents
	s.u32(uint32(in.count))
	for i := int32(0); i < in.count; i++ {
		s.ent(in.Entity[i])
		for sl := 0; sl < InventorySlots; sl++ {
			s.ent(in.Slots[i][sl])
		}
		for c := 0; c < data.ItemClassCount; c++ {
			s.u32(in.ClassReady[i][c])
		}
	}

	// orders: rows, then the RAW pool (links + free list are state)
	o := w.Orders
	s.u32(uint32(o.count))
	for i := int32(0); i < o.count; i++ {
		s.ent(o.Entity[i])
		s.u8(o.Kind[i])
		s.u8(o.Phase[i])
		s.ent(o.Target[i])
		s.vec2(o.Point[i])
		s.u16(o.Data[i])
		s.i32(o.QueueHead[i])
	}
	s.u32(uint32(len(w.orderPool)))
	s.i32(w.orderFreeHead)
	s.i32(w.orderFreeCount)
	// canonical form: free entries write a ZERO payload (their link is
	// state — free order steers allocation — but their payload is
	// recycled junk; zeroing it makes identical logical state produce
	// byte-identical files, and lets Load refuse non-canonical bytes)
	isFree := make([]bool, len(w.orderPool))
	for e := w.orderFreeHead; e != NoOrderEntry; e = w.orderPool[e].next {
		isFree[e] = true
	}
	for i := range w.orderPool {
		e := &w.orderPool[i]
		s.i32(e.next)
		if isFree[i] {
			s.u8(0)
			s.ent(0)
			s.vec2(fixed.Vec2{})
			s.u16(0)
			continue
		}
		s.u8(e.kind)
		s.ent(e.target)
		s.vec2(e.point)
		s.u16(e.data)
	}

	// buffs: live rows ascending by pool index, then the free stack
	p := w.Buffs
	s.u32(uint32(p.Cap()))
	s.u32(uint32(p.Live()))
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] {
			continue
		}
		r := &p.rows[i]
		s.u32(uint32(i))
		s.u16(r.BuffID)
		s.u8(r.Stacks)
		s.u8(r.Flags)
		s.ent(r.Target)
		s.ent(r.Source)
		s.u32(r.RemainingTicks)
		s.u32(r.PeriodicClock)
	}
	s.u32(uint32(len(p.free)))
	for _, f := range p.free {
		s.i32(f)
	}

	// missiles
	ms := w.Missiles
	s.u32(uint32(ms.Count()))
	for i := int32(0); i < ms.Count(); i++ {
		s.ent(ms.Entity[i])
		s.f64(ms.Speed[i])
		s.f64(ms.Accel[i])
		s.f64(ms.Arc[i])
		s.u8(ms.Flags[i])
		s.u16(ms.HitMask[i])
		s.u16(ms.GuidanceID[i])
		s.u16(ms.ImpactID[i])
		s.ent(ms.GuideEnt[i])
		s.vec2(ms.GuidePt[i])
		s.u16(ms.Payload[i].Off)
		s.u16(ms.Payload[i].Len)
		s.ent(ms.Packet[i].Source)
		s.ent(ms.Packet[i].Target)
		s.f64(ms.Packet[i].Amount)
		s.u8(ms.Packet[i].AttackType)
		s.ent(ms.Source[i])
		s.u32(ms.BirthTick[i])
		s.vec2(ms.Dir[i])
		s.f64(ms.RangeLeft[i])
		s.i32(ms.PierceLeft[i])
		s.u16(ms.Decay[i])
	}

	// doodads (byPlacement is derived, rebuilt at load)
	d := w.Doodads
	s.u32(uint32(d.count))
	for i := int32(0); i < d.count; i++ {
		s.i32(d.Placement[i])
		s.boolean(d.Visible[i])
		s.u16(d.Anim[i])
		s.vec2(d.Pos[i])
		s.u16(uint16(d.Facing[i]))
		s.u8(d.Overrides[i])
		s.ent(d.Entity[i])
	}

	// economy (#300): counters + the three stores. Food ledgers are
	// derived (recomputed from econ rows at load) and not written.
	s.u32(uint32(w.resourceCount))
	for pl := 0; pl < MaxPlayers; pl++ {
		if w.resourceCount == 0 {
			continue
		}
		for _, v := range w.resources[pl] {
			s.i64(v)
		}
	}
	nd := w.Nodes
	s.u32(uint32(nd.count))
	for i := int32(0); i < nd.count; i++ {
		s.ent(nd.Entity[i])
		s.u8(nd.Resource[i])
		s.i64(nd.Remaining[i])
		s.u8(nd.Flags[i])
		s.ent(nd.Busy[i])
	}
	ec := w.Econs
	s.u32(uint32(ec.count))
	for i := int32(0); i < ec.count; i++ {
		s.ent(ec.Entity[i])
		s.u8(ec.FoodCost[i])
		s.u8(ec.FoodProvided[i])
		s.u16(ec.DepotMask[i])
	}
	hv := w.Harvests
	s.u32(uint32(hv.count))
	for i := int32(0); i < hv.count; i++ {
		s.ent(hv.Entity[i])
		s.u8(hv.State[i])
		s.ent(hv.Node[i])
		s.ent(hv.Depot[i])
		s.u32(uint32(hv.Carried[i]))
		s.u8(hv.CarriedRes[i])
		s.u32(hv.Clock[i])
		s.u32(uint32(hv.Capacity[i]))
		s.u16(hv.GatherTicks[i])
		s.u16(hv.Mask[i])
	}
	pq := w.Produce
	s.u32(uint32(pq.count))
	for i := int32(0); i < pq.count; i++ {
		s.ent(pq.Entity[i])
		s.u8(pq.QCount[i])
		for q := 0; q < ProduceQueueCap; q++ {
			s.u16(pq.Queue[i][q])
			s.u8(pq.QFlags[i][q])
		}
		s.u32(pq.Done[i])
		s.u8(pq.RallyKind[i])
		s.ent(pq.RallyEnt[i])
		s.vec2(pq.RallyPoint[i])
	}

	// tech (#303): per-player level + cap arrays; the count must
	// match the bound upgrade table at load
	s.u32(uint32(len(w.upgradeDefs)))
	if len(w.upgradeDefs) > 0 {
		for pl := 0; pl < MaxPlayers; pl++ {
			for u := range w.upgradeDefs {
				s.u8(w.upgradeLevel[pl][u])
				s.u8(w.techMax[pl][u])
			}
		}
	}

	// heroes (#304): rows + dead pools
	hs := w.Heroes
	s.u32(uint32(hs.count))
	for i := int32(0); i < hs.count; i++ {
		s.ent(hs.Entity[i])
		s.u16(hs.HeroType[i])
		s.i64(hs.XP[i])
		s.u8(hs.Level[i])
		s.i64(int64(hs.Str[i]))
		s.i64(int64(hs.Agi[i]))
		s.i64(int64(hs.Int[i]))
		s.u8(hs.SkillPoints[i])
		for sl := 0; sl < data.MaxHeroSkills; sl++ {
			s.u8(hs.SkillLevel[i][sl])
		}
	}
	for pl := 0; pl < MaxPlayers; pl++ {
		for sl := 0; sl < MaxDeadHeroes; sl++ {
			rec := &w.deadHeroes[pl][sl]
			if !rec.Used {
				s.u8(0)
				continue
			}
			s.u8(1)
			s.u16(rec.HeroType)
			s.i64(rec.XP)
			s.u8(rec.Level)
			s.i64(int64(rec.Str))
			s.i64(int64(rec.Agi))
			s.i64(int64(rec.Int))
			s.u8(rec.SkillPoints)
			for k := 0; k < data.MaxHeroSkills; k++ {
				s.u8(rec.SkillLevel[k])
			}
		}
	}

	// items (#305): type/charges/carrier per row
	is := w.Items
	s.u32(uint32(is.count))
	for i := int32(0); i < is.count; i++ {
		s.ent(is.Entity[i])
		s.u16(is.TypeID[i])
		s.u16(is.Charges[i])
		s.ent(is.Carrier[i])
	}

	// patrol (#306): endpoints + leg/leash flags per row
	ps := w.Patrol
	s.u32(uint32(ps.count))
	for i := int32(0); i < ps.count; i++ {
		s.ent(ps.Entity[i])
		s.vec2(ps.A[i])
		s.vec2(ps.B[i])
		s.u8(ps.Flags[i])
	}

	// build (#301): footprint + construction progress per structure
	bs := w.Build
	s.u32(uint32(bs.count))
	for i := int32(0); i < bs.count; i++ {
		s.ent(bs.Entity[i])
		s.ent(bs.Builder[i])
		s.i32(bs.FX[i])
		s.i32(bs.FY[i])
		s.i32(bs.FW[i])
		s.u16(bs.Progress[i])
	}

	// clock (#339): all deterministic fields, including the fractional
	// carry that makes 24h/dayLength advancement drift-free.
	s.f64(w.tod)
	s.f64(w.todScale)
	s.boolean(w.todFrozen)
	s.u64(w.todCarry)
	s.u32(w.dayLengthTicks)

	// gamestate (#345): per-player terminal result latches. Pending
	// requests are refused above and are never serialized.
	for player := 0; player < MaxPlayers; player++ {
		s.u8(w.results[player])
	}

	// effects (#349): persistent script-spawned visual entities. Their
	// Transform rows are saved in the normal transform section above.
	fx := w.Effects
	s.u32(uint32(fx.Count()))
	for i := int32(0); i < fx.Count(); i++ {
		s.u16(fx.ModelID[i])
		s.f64(fx.Scale[i])
		s.u32(fx.ColorRGBA[i])
		s.u32(fx.BirthTick[i])
		s.ent(fx.Entity[i])
	}

	// ability field overrides (#354): live rows ascending by pool index,
	// then the free stack in allocation order, plus the rejection counter.
	af := w.AbilityFields
	s.u32(uint32(af.Count()))
	for i := 0; i < af.Cap(); i++ {
		if !af.live[i] {
			continue
		}
		s.u32(uint32(i))
		s.ent(af.Ent[i])
		s.u8(af.Slot[i])
		s.u8(af.Field[i])
		s.f64(af.Value[i])
	}
	s.u32(uint32(len(af.free)))
	for _, f := range af.free {
		s.i32(f)
	}
	s.u64(af.Rejected())

	// runtime ability definitions (#355): static defs are data-table
	// fingerprinted; only match-created rows are sim state.
	s.u32(uint32(len(w.runtimeAbilityDefs)))
	for i := range w.runtimeAbilityDefs {
		abilityDefSave(s, &w.runtimeAbilityDefs[i])
	}

	// visibility (#299): authoritative fog grid and ghost-building
	// records, plus the in-progress phase-spread stamp cycle.
	w.saveVisibility(s)

	// scheduler blob (sched/serialize.go owns its own format)
	blob := w.Sched.Save(make([]byte, 0, w.Sched.SaveSize()))
	s.u32(uint32(len(blob)))
	s.write(blob)

	// subscriptions: kinds ascending, handler IDs in dispatch order
	s.u32(uint32(len(w.subs)))
	for i := range w.subs {
		s.u16(w.subs[i].kind)
		s.u32(uint32(len(w.subs[i].list)))
		for _, id := range w.subs[i].list {
			s.u32(uint32(id))
		}
	}

	// regions (#241): all slots in id order — gen/alive, then the live
	// region's set cells in ascending index order, then the free list.
	rs := w.Regions
	s.u32(uint32(len(rs.entries)))
	for id := range rs.entries {
		e := &rs.entries[id]
		s.u32(e.gen)
		s.boolean(e.alive)
		if !e.alive {
			continue
		}
		s.u32(uint32(e.popcount()))
		e.eachSetCell(func(cell int32) { s.u32(uint32(cell)) })
		// membership (#241): units currently inside, presence-row order
		if e.members == nil {
			s.u32(0)
		} else {
			s.u32(uint32(e.members.count))
			for i := int32(0); i < e.members.count; i++ {
				s.ent(e.members.Entity[i])
			}
		}
	}
	s.u32(uint32(len(rs.free)))
	for _, f := range rs.free {
		s.u32(f)
	}

	// players (#218): per-player roster in slot order, then the alliance
	// bitset. Resources/food are already in the economy section.
	pr := &w.players
	for p := 0; p < MaxPlayers; p++ {
		s.str(pr.name[p])
		s.u8(pr.controller[p])
		s.u8(pr.race[p])
		s.u8(pr.color[p])
		s.u8(pr.team[p])
		s.f64(pr.startX[p])
		s.f64(pr.startY[p])
		s.boolean(pr.alliedVictory[p])
		// handicaps (#373, v21)
		s.f64(pr.handicap[p])
		s.f64(pr.handicapDamage[p])
		s.f64(pr.handicapXP[p])
		s.f64(pr.handicapReviveTime[p])
	}
	for a := 0; a < MaxPlayers; a++ {
		for b := 0; b < MaxPlayers; b++ {
			s.u16(pr.alliance[a][b])
		}
	}

	// heightfield (#371, v22): dims/origin/cell then row-major samples.
	hf := &w.height
	s.u32(uint32(hf.cols))
	s.u32(uint32(hf.rows))
	s.f64(hf.originX)
	s.f64(hf.originY)
	s.f64(hf.cellSize)
	for _, v := range hf.samples {
		s.f64(v)
	}

	// upkeep (#375, v25): brackets, per-player lost counters, sparse tax matrix.
	s.u32(uint32(w.upkeepCount))
	for i := 0; i < w.upkeepCount; i++ {
		s.u32(uint32(w.upkeepFood[i]))
		for r := 0; r < data.MaxResourceTypes; r++ {
			s.f64(w.upkeepRate[i][r])
		}
	}
	for p := 0; p < MaxPlayers; p++ {
		for r := 0; r < data.MaxResourceTypes; r++ {
			s.u64(uint64(w.upkeepLost[p][r]))
		}
	}
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
	s.u32(taxN)
	for a := 0; a < MaxPlayers; a++ {
		for b := 0; b < MaxPlayers; b++ {
			for r := 0; r < data.MaxResourceTypes; r++ {
				if v := w.taxRate[a][b][r]; v != 0 {
					s.u8(uint8(a))
					s.u8(uint8(b))
					s.u8(uint8(r))
					s.f64(v)
				}
			}
		}
	}

	// fog overrides (#243, v26): toggles, modifier pool + free list, shares.
	s.boolean(w.fogDisabled)
	s.boolean(w.fogMaskDisabled)
	fm := w.FogMods
	s.u32(uint32(fm.count))
	for i := int32(0); i < fm.count; i++ {
		s.boolean(fm.alive[i])
		s.u16(fm.gen[i])
		s.u8(fm.player[i])
		s.u8(fm.state[i])
		s.u8(fm.kind[i])
		s.boolean(fm.shared[i])
		s.boolean(fm.active[i])
		s.f64(fm.ax[i])
		s.f64(fm.ay[i])
		s.f64(fm.bx[i])
		s.f64(fm.by[i])
	}
	s.u32(uint32(len(fm.free)))
	for _, slot := range fm.free {
		s.u32(uint32(slot))
	}
	sv := w.ShareVisions
	s.u32(uint32(sv.count))
	for i := int32(0); i < sv.count; i++ {
		s.ent(sv.Entity[i])
		s.u16(sv.Mask[i])
	}

	// AI hooks (#257, v27): per-player difficulty/paused/attached + command stack.
	for p := 0; p < MaxPlayers; p++ {
		s.u8(w.ai.difficulty[p])
		s.boolean(w.ai.paused[p])
		s.boolean(w.ai.attached[p])
		s.u32(uint32(len(w.ai.inbox[p])))
		for _, c := range w.ai.inbox[p] {
			s.i32(c.Command)
			s.i32(c.Data)
		}
	}

	// destructables (#229, v28): killable/pathing-blocking widget rows in
	// creation order. The static pathing stamp is derived — rebuilt from
	// blocks+footprint+dead at load, like doodad byPlacement.
	de := w.Destructables
	s.u32(uint32(de.count))
	for i := int32(0); i < de.count; i++ {
		s.u16(de.Type[i])
		s.vec2(de.Pos[i])
		s.u16(uint16(de.Facing[i]))
		s.i32(de.Life[i])
		s.i32(de.MaxLife[i])
		s.boolean(de.Dead[i])
		s.boolean(de.Invuln[i])
		s.boolean(de.Blocks[i])
		s.u8(de.Footprint[i])
		s.ent(de.Entity[i])
	}

	// ECA handler-identity registry (#455, v29): the stable name table in
	// ref order. This is a load-time manifest, not mutable match state — a
	// fresh world re-registers the same names in the same order and must
	// reproduce this exact table (validateSave fails closed otherwise).
	s.u32(uint32(len(w.handlerReg.names)))
	for _, name := range w.handlerReg.names {
		s.str(name)
	}

	// first-class ECA trigger slab (#456, v30): every slot in slot order
	// (gen/alive, then alive fields), then the free list — slab reuse order
	// is state, so it round-trips exactly like the entity table.
	ts := w.Triggers
	s.u32(uint32(len(ts.slots)))
	for i := range ts.slots {
		sl := &ts.slots[i]
		s.u32(sl.gen)
		s.boolean(sl.alive)
		if !sl.alive {
			continue
		}
		s.boolean(sl.enabled)
		s.boolean(sl.on)
		s.i32(int32(sl.cond))
		s.u32(uint32(len(sl.events)))
		for _, ev := range sl.events {
			s.u16(ev.Kind)
			s.ent(ev.Scope)
		}
		s.u32(uint32(len(sl.actions)))
		for _, a := range sl.actions {
			s.u32(uint32(a))
		}
	}
	s.u32(uint32(len(ts.free)))
	for _, f := range ts.free {
		s.u32(f)
	}

	return s.err
}

func abilityDefSave(s *saveWriter, def *data.Ability) {
	s.str(def.ID)
	s.str(def.Name)
	s.u16(def.Effects.Off)
	s.u16(def.Effects.Len)
	s.i32(def.ManaCost)
	s.u16(def.CooldownTicks)
	s.u16(def.CastPointTicks)
	s.u16(def.BackswingTicks)
	s.u16(def.ChannelTicks)
	s.f64(def.CastRange)
}

// ---- load ----

// decodedSave is the phase-A staging area: the whole stream decodes
// and validates here before one byte of world state changes.
type decodedSave struct {
	tick      uint32
	unitCount uint32
	rngCursor prng.Cursor

	entSlots    []entitySlot
	entFreeHead int32
	entCount    int32

	trN int32
	trE []EntityID
	trP []fixed.Vec2
	trF []fixed.Angle

	mvN      int32
	mvE      []EntityID
	mvSpeed  []fixed.F64
	mvTurn   []fixed.Angle
	mvTarget []fixed.Vec2
	mvPath   []uint32
	mvWp     []int32
	mvStall  []uint16
	mvRes    []int32
	mvState  []uint8

	coN     int32
	coE     []EntityID
	coSize  []uint8
	coFlags []uint8
	coStamp []int32

	utN  int32
	utE  []EntityID
	utID []uint16

	udN   int32
	udE   []EntityID
	udVal []int32

	hdN int32
	hdE []EntityID

	xsN int32
	xsE []EntityID

	psN int32
	psE []EntityID

	unN    int32
	unE    []EntityID
	unName []string

	// flyheight (#367)
	flyN int32
	flyE []EntityID
	flyH []fixed.F64
	flyT []fixed.F64
	flyR []fixed.F64

	// propwindow (#376)
	pwN int32
	pwE []EntityID
	pwV []fixed.Angle

	hlN     int32
	hlE     []EntityID
	hlLife  []fixed.F64
	hlMax   []fixed.F64
	hlRegen []fixed.F64
	hlAV    []int16
	hlAT    []uint8
	hlDS    []uint8
	hlDecay []uint32
	hlInv   []bool

	owN int32
	owE []EntityID
	owP []uint8
	owT []uint8
	owC []uint8

	cbN     int32
	cbE     []EntityID
	cbAcq   []fixed.F64
	cbTgt   []EntityID
	cbLA    []EntityID
	cbLDT   []uint32
	cbSlots []combatSlotSave

	abN     int32
	abE     []EntityID
	abMana  []fixed.F64
	abMax   []fixed.F64
	abRegen []fixed.F64
	abCS    []int8
	abCE    []uint32
	abSlots []abilitySlotSave

	inN     int32
	inE     []EntityID
	inSlots [][InventorySlots]EntityID
	inReady [][data.ItemClassCount]uint32

	orN     int32
	orE     []EntityID
	orKind  []uint8
	orPhase []uint8
	orTgt   []EntityID
	orPt    []fixed.Vec2
	orData  []uint16
	orHead  []int32

	pool          []orderEntry
	poolFreeHead  int32
	poolFreeCount int32

	buffLive []bool
	buffRows []BuffInstance
	buffFree []int32

	msN      int32
	msE      []EntityID
	msSpeed  []fixed.F64
	msAccel  []fixed.F64
	msArc    []fixed.F64
	msFlags  []uint8
	msHit    []uint16
	msGuid   []uint16
	msImpact []uint16
	msGE     []EntityID
	msGP     []fixed.Vec2
	msPay    []data.EffectList
	msPkt    []DamagePacket
	msSrc    []EntityID
	msBirth  []uint32
	msDir    []fixed.Vec2
	msRange  []fixed.F64
	msPierce []int32
	msDecay  []uint16

	doN     int32
	doPlace []int32
	doVis   []bool
	doAnim  []uint16
	doPos   []fixed.Vec2
	doFace  []fixed.Angle
	doOv    []uint8
	doE     []EntityID

	resourceCount int32
	resources     [][]int64

	ndN    int32
	ndE    []EntityID
	ndRes  []uint8
	ndRem  []int64
	ndFl   []uint8
	ndBusy []EntityID

	ecN    int32
	ecE    []EntityID
	ecCost []uint8
	ecProv []uint8
	ecMask []uint16

	hvN     int32
	hvE     []EntityID
	hvState []uint8
	hvNode  []EntityID
	hvDepot []EntityID
	hvCarr  []int32
	hvCRes  []uint8
	hvClock []uint32
	hvCap   []int32
	hvGT    []uint16
	hvMask  []uint16

	pqN     int32
	pqE     []EntityID
	pqQ     [][ProduceQueueCap]uint16
	pqFl    [][ProduceQueueCap]uint8
	pqCount []uint8
	pqDone  []uint32
	pqRK    []uint8
	pqRE    []EntityID
	pqRP    []fixed.Vec2

	upgCount int32
	upgLevel [][]uint8
	upgMax   [][]uint8

	hrN     int32
	hrE     []EntityID
	hrRec   []HeroRecord
	deadRec [][MaxDeadHeroes]HeroRecord

	itN       int32
	itE       []EntityID
	itType    []uint16
	itCharges []uint16
	itCarrier []EntityID

	paN     int32
	paE     []EntityID
	paA     []fixed.Vec2
	paB     []fixed.Vec2
	paFlags []uint8

	bdN    int32
	bdE    []EntityID
	bdBldr []EntityID
	bdFX   []int32
	bdFY   []int32
	bdFW   []int32
	bdProg []uint16

	clockTOD       fixed.F64
	clockScale     fixed.F64
	clockFrozen    bool
	clockCarry     uint64
	clockDayLength uint32

	results [MaxPlayers]uint8

	efN     int32
	efModel []uint16
	efScale []fixed.F64
	efColor []uint32
	efBirth []uint32
	efE     []EntityID

	afN        int32
	afLive     []bool
	afEnt      []EntityID
	afSlot     []uint8
	afField    []uint8
	afValue    []fixed.F64
	afFree     []int32
	afRejected uint64

	runtimeAbilityDefs []data.Ability

	visPresent       bool
	visEvery         uint16
	visState         []byte
	visCycle         []byte
	visFlags         []byte
	visLastSeen      []LastSeenBuilding
	visLastSeenCount [MaxPlayers]int32

	schedBlob []byte

	subs []kindSubs

	// regions (#241): decoded region slots in id order + free list, plus
	// per-region membership (units inside) rebuilt into presence sets at
	// apply time (the store caps are needed to size them).
	regEntries []regionEntry
	regMembers [][]EntityID
	regFree    []uint32

	// players (#218): roster table + alliance bitset, staged then applied
	// wholesale. Fixed-size, so plain arrays (no count prefix).
	plName          [MaxPlayers]string
	plController    [MaxPlayers]uint8
	plRace          [MaxPlayers]uint8
	plColor         [MaxPlayers]uint8
	plTeam          [MaxPlayers]uint8
	plStartX        [MaxPlayers]fixed.F64
	plStartY        [MaxPlayers]fixed.F64
	plAlliedVictory [MaxPlayers]bool
	plAlliance      [MaxPlayers][MaxPlayers]uint16
	// handicaps (#373, v21)
	plHandicap           [MaxPlayers]fixed.F64
	plHandicapDamage     [MaxPlayers]fixed.F64
	plHandicapXP         [MaxPlayers]fixed.F64
	plHandicapReviveTime [MaxPlayers]fixed.F64

	// heightfield (#371, v22): staged then applied wholesale.
	hfCols, hfRows               int32
	hfOriginX, hfOriginY, hfCell fixed.F64
	hfSamples                    []fixed.F64

	// upkeep (#375, v25): brackets, lost counters, sparse tax matrix.
	upCount int
	upFood  [maxUpkeepTiers]int32
	upRate  [maxUpkeepTiers][data.MaxResourceTypes]fixed.F64
	upLost  [MaxPlayers][data.MaxResourceTypes]int64
	upTax   [MaxPlayers][MaxPlayers][data.MaxResourceTypes]fixed.F64

	// fog overrides (#243, v26): toggles, modifier pool, free list, shares.
	fogDisabled, fogMaskDisabled bool
	fmCount                      int32
	fmAlive, fmShared, fmActive  []bool
	fmGen                        []uint16
	fmPlayer, fmState, fmKind    []uint8
	fmAX, fmAY, fmBX, fmBY       []fixed.F64
	fmFree                       []int32
	svEntity                     []EntityID
	svMask                       []uint16

	// AI hooks (#257, v27): per-player difficulty/paused/attached + command stack.
	aiDifficulty [MaxPlayers]uint8
	aiPaused     [MaxPlayers]bool
	aiAttached   [MaxPlayers]bool
	aiInbox      [MaxPlayers][]aiCommand

	// destructables (#229, v28): killable/pathing-blocking widget rows.
	deN     int32
	deType  []uint16
	dePos   []fixed.Vec2
	deFace  []fixed.Angle
	deLife  []int32
	deMax   []int32
	deDead  []bool
	deInvul []bool
	deBlock []bool
	deFoot  []uint8
	deE     []EntityID

	// ECA handler-identity registry (#455, v29): stable name table in ref
	// order, validated against the re-registered registry at apply time.
	handlerNames []string

	// first-class ECA trigger slab (#456, v30): decoded slots in slot order
	// + free list, validated then swapped into the store at apply.
	trigSlots []triggerSlot
	trigFree  []uint32
}

type combatSlotSave [WeaponSlots]struct {
	DmgBase    int32
	DmgDice    uint8
	DmgSides   uint8
	AttackType uint8
	Cooldown   uint16
	DamagePt   uint16
	Backswing  uint16
	Range      fixed.F64
	ProjRef    uint16
	ProjSpeed  fixed.F64
	ReadyAt    uint32
	WFlags     uint8
	AtkState   uint8
	PhaseEnd   uint32
	Effects    data.EffectList
}

type abilitySlotSave [AbilitySlots]struct {
	AbilityID uint16
	Level     uint8
	ReadyAt   uint32
	CastState uint8
}

// LoadState restores a save into this world. The world must have been
// constructed with the SAME caps and have its handler/continuation
// registries and data tables already bound — registries are code, not
// state. fingerprint is this world's bound data-table hash; a save
// recorded against different content is refused before any decode of
// the body. On any error the world is untouched.
func (w *World) LoadState(in io.Reader, fingerprint uint64) error {
	r := &saveReader{r: in, what: "magic"}
	magic := make([]byte, len(SaveMagic))
	if _, err := io.ReadFull(in, magic); err != nil {
		return fmt.Errorf("sim: save: truncated while reading magic")
	}
	if string(magic) != SaveMagic {
		return fmt.Errorf("sim: save: bad magic %q (not a LITD save)", magic)
	}
	r.what = "header"
	if v := r.u32(); r.err == nil && v != SaveFormatVersion {
		return fmt.Errorf("sim: save: format version %d, this engine reads %d", v, SaveFormatVersion)
	}
	if fp := r.u64(); r.err == nil && fp != fingerprint {
		return fmt.Errorf("sim: save: data-table fingerprint %016x does not match this world's %016x — refusing", fp, fingerprint)
	}
	got := Caps{
		Units:              int(r.u32()),
		Projectiles:        int(r.u32()),
		Effects:            int(r.u32()),
		BuffInstances:      int(r.u32()),
		OrderQueueEntries:  int(r.u32()),
		PendingEvents:      int(r.u32()),
		PathRequests:       int(r.u32()),
		ScriptedDoodads:    int(r.u32()),
		Destructables:      int(r.u32()),
		RuntimeAbilityDefs: int(r.u32()),
		Triggers:           int(r.u32()),
	}
	if r.err == nil && got != w.caps {
		return fmt.Errorf("sim: save: capability table %+v does not match this world's %+v — load into a world with identical caps", got, w.caps)
	}
	var d decodedSave
	d.tick = r.u32()
	d.unitCount = r.u32()
	d.rngCursor = prng.Cursor{State: r.u64(), Inc: r.u64()}
	if r.err != nil {
		return r.err
	}

	if err := decodeBody(r, &d, w); err != nil {
		return err
	}
	// the stream must end exactly here
	var one [1]byte
	if n, _ := in.Read(one[:]); n != 0 {
		return fmt.Errorf("sim: save: trailing bytes after subscription table")
	}
	if err := validateSave(&d, w); err != nil {
		return err
	}

	// ---- phase B: apply ----
	// sched first: its Load is atomic and the only fallible apply
	if err := w.Sched.Load(d.schedBlob); err != nil {
		return fmt.Errorf("sim: save: scheduler: %w", err)
	}
	applySave(&d, w)
	return nil
}

// section reads one count-prefixed section header with bounds check.
func (r *saveReader) section(what string, max int) (int32, error) {
	r.what = what
	n := r.u32()
	if r.err != nil {
		return 0, r.err
	}
	if int(n) > max {
		return 0, fmt.Errorf("sim: save: %s count %d exceeds capacity %d", what, n, max)
	}
	return int32(n), nil
}

func decodeBody(r *saveReader, d *decodedSave, w *World) error {
	// entities
	r.what = "entity table"
	nSlots := r.u32()
	if r.err != nil {
		return r.err
	}
	if int(nSlots) != len(w.Ents.slots) {
		return fmt.Errorf("sim: save: entity table has %d slots, this world has %d", nSlots, len(w.Ents.slots))
	}
	d.entFreeHead = r.i32()
	d.entCount = int32(r.u32())
	d.entSlots = make([]entitySlot, nSlots)
	for i := range d.entSlots {
		d.entSlots[i] = entitySlot{gen: r.u8(), alive: r.boolean(), next: r.i32()}
	}

	var n int32
	var err error

	// transforms
	if n, err = r.section("transforms", len(w.Transforms.Pos)); err != nil {
		return err
	}
	d.trN = n
	d.trE = make([]EntityID, n)
	d.trP = make([]fixed.Vec2, n)
	d.trF = make([]fixed.Angle, n)
	for i := int32(0); i < n; i++ {
		d.trE[i] = r.ent()
		d.trP[i] = r.vec2()
		d.trF[i] = fixed.Angle(r.u16())
	}

	// movement
	if n, err = r.section("movement", len(w.Movements.Speed)); err != nil {
		return err
	}
	d.mvN = n
	d.mvE = make([]EntityID, n)
	d.mvSpeed = make([]fixed.F64, n)
	d.mvTurn = make([]fixed.Angle, n)
	d.mvTarget = make([]fixed.Vec2, n)
	d.mvPath = make([]uint32, n)
	d.mvWp = make([]int32, n)
	d.mvStall = make([]uint16, n)
	d.mvRes = make([]int32, n)
	d.mvState = make([]uint8, n)
	for i := int32(0); i < n; i++ {
		d.mvE[i] = r.ent()
		d.mvSpeed[i] = r.f64()
		d.mvTurn[i] = fixed.Angle(r.u16())
		d.mvTarget[i] = r.vec2()
		d.mvPath[i] = r.u32()
		d.mvWp[i] = r.i32()
		d.mvStall[i] = r.u16()
		d.mvRes[i] = r.i32()
		d.mvState[i] = r.u8()
	}

	// collisions
	if n, err = r.section("collisions", len(w.Collisions.SizeClass)); err != nil {
		return err
	}
	d.coN = n
	d.coE = make([]EntityID, n)
	d.coSize = make([]uint8, n)
	d.coFlags = make([]uint8, n)
	d.coStamp = make([]int32, n)
	for i := int32(0); i < n; i++ {
		d.coE[i] = r.ent()
		d.coSize[i] = r.u8()
		d.coFlags[i] = r.u8()
		d.coStamp[i] = r.i32()
	}

	// unit types
	if n, err = r.section("unit types", len(w.UnitTypes.TypeID)); err != nil {
		return err
	}
	d.utN = n
	d.utE = make([]EntityID, n)
	d.utID = make([]uint16, n)
	for i := int32(0); i < n; i++ {
		d.utE[i] = r.ent()
		d.utID[i] = r.u16()
	}

	// userdata (#217)
	if n, err = r.section("userdata", len(w.UserDatas.Value)); err != nil {
		return err
	}
	d.udN = n
	d.udE = make([]EntityID, n)
	d.udVal = make([]int32, n)
	for i := int32(0); i < n; i++ {
		d.udE[i] = r.ent()
		d.udVal[i] = r.i32()
	}

	// hidden (#217)
	if n, err = r.section("hidden", len(w.Hiddens.Entity)); err != nil {
		return err
	}
	d.hdN = n
	d.hdE = make([]EntityID, n)
	for i := int32(0); i < n; i++ {
		d.hdE[i] = r.ent()
	}

	// xpsuspend (#217)
	if n, err = r.section("xpsuspend", len(w.XPSuspends.Entity)); err != nil {
		return err
	}
	d.xsN = n
	d.xsE = make([]EntityID, n)
	for i := int32(0); i < n; i++ {
		d.xsE[i] = r.ent()
	}

	// pause (#217)
	if n, err = r.section("pause", len(w.Pauses.Entity)); err != nil {
		return err
	}
	d.psN = n
	d.psE = make([]EntityID, n)
	for i := int32(0); i < n; i++ {
		d.psE[i] = r.ent()
	}

	// unitname (#217)
	if n, err = r.section("unitname", len(w.UnitNames.Name)); err != nil {
		return err
	}
	d.unN = n
	d.unE = make([]EntityID, n)
	d.unName = make([]string, n)
	for i := int32(0); i < n; i++ {
		d.unE[i] = r.ent()
		if d.unName[i], err = r.str(maxUnitNameLen); err != nil {
			return err
		}
	}

	// flyheight (#367)
	if n, err = r.section("flyheight", len(w.Flys.Height)); err != nil {
		return err
	}
	d.flyN = n
	d.flyE = make([]EntityID, n)
	d.flyH = make([]fixed.F64, n)
	d.flyT = make([]fixed.F64, n)
	d.flyR = make([]fixed.F64, n)
	for i := int32(0); i < n; i++ {
		d.flyE[i] = r.ent()
		d.flyH[i] = r.f64()
		d.flyT[i] = r.f64()
		d.flyR[i] = r.f64()
	}

	// propwindow (#376)
	if n, err = r.section("propwindow", len(w.PropWindows.Value)); err != nil {
		return err
	}
	d.pwN = n
	d.pwE = make([]EntityID, n)
	d.pwV = make([]fixed.Angle, n)
	for i := int32(0); i < n; i++ {
		d.pwE[i] = r.ent()
		d.pwV[i] = fixed.Angle(r.u16())
	}

	// health
	if n, err = r.section("health", len(w.Healths.Life)); err != nil {
		return err
	}
	d.hlN = n
	d.hlE = make([]EntityID, n)
	d.hlLife = make([]fixed.F64, n)
	d.hlMax = make([]fixed.F64, n)
	d.hlRegen = make([]fixed.F64, n)
	d.hlAV = make([]int16, n)
	d.hlAT = make([]uint8, n)
	d.hlDS = make([]uint8, n)
	d.hlDecay = make([]uint32, n)
	d.hlInv = make([]bool, n)
	for i := int32(0); i < n; i++ {
		d.hlE[i] = r.ent()
		d.hlLife[i] = r.f64()
		d.hlMax[i] = r.f64()
		d.hlRegen[i] = r.f64()
		d.hlAV[i] = int16(r.u16())
		d.hlAT[i] = r.u8()
		d.hlDS[i] = r.u8()
		d.hlDecay[i] = r.u32()
		d.hlInv[i] = r.boolean()
	}

	// owners
	if n, err = r.section("owners", len(w.Owners.Player)); err != nil {
		return err
	}
	d.owN = n
	d.owE = make([]EntityID, n)
	d.owP = make([]uint8, n)
	d.owT = make([]uint8, n)
	d.owC = make([]uint8, n)
	for i := int32(0); i < n; i++ {
		d.owE[i] = r.ent()
		d.owP[i] = r.u8()
		d.owT[i] = r.u8()
		d.owC[i] = r.u8()
	}

	// combat
	if n, err = r.section("combat", len(w.Combats.AcquisitionRange)); err != nil {
		return err
	}
	d.cbN = n
	d.cbE = make([]EntityID, n)
	d.cbAcq = make([]fixed.F64, n)
	d.cbTgt = make([]EntityID, n)
	d.cbLA = make([]EntityID, n)
	d.cbLDT = make([]uint32, n)
	d.cbSlots = make([]combatSlotSave, n)
	for i := int32(0); i < n; i++ {
		d.cbE[i] = r.ent()
		d.cbAcq[i] = r.f64()
		d.cbTgt[i] = r.ent()
		d.cbLA[i] = r.ent()
		d.cbLDT[i] = r.u32()
		for sl := 0; sl < WeaponSlots; sl++ {
			ws := &d.cbSlots[i][sl]
			ws.DmgBase = r.i32()
			ws.DmgDice = r.u8()
			ws.DmgSides = r.u8()
			ws.AttackType = r.u8()
			ws.Cooldown = r.u16()
			ws.DamagePt = r.u16()
			ws.Backswing = r.u16()
			ws.Range = r.f64()
			ws.ProjRef = r.u16()
			ws.ProjSpeed = r.f64()
			ws.ReadyAt = r.u32()
			ws.WFlags = r.u8()
			ws.AtkState = r.u8()
			ws.PhaseEnd = r.u32()
			ws.Effects = data.EffectList{Off: r.u16(), Len: r.u16()}
		}
	}

	// abilities
	if n, err = r.section("abilities", len(w.Abilities.Mana)); err != nil {
		return err
	}
	d.abN = n
	d.abE = make([]EntityID, n)
	d.abMana = make([]fixed.F64, n)
	d.abMax = make([]fixed.F64, n)
	d.abRegen = make([]fixed.F64, n)
	d.abCS = make([]int8, n)
	d.abCE = make([]uint32, n)
	d.abSlots = make([]abilitySlotSave, n)
	for i := int32(0); i < n; i++ {
		d.abE[i] = r.ent()
		d.abMana[i] = r.f64()
		d.abMax[i] = r.f64()
		d.abRegen[i] = r.f64()
		d.abCS[i] = int8(r.u8())
		d.abCE[i] = r.u32()
		for sl := 0; sl < AbilitySlots; sl++ {
			as := &d.abSlots[i][sl]
			as.AbilityID = r.u16()
			as.Level = r.u8()
			as.ReadyAt = r.u32()
			as.CastState = r.u8()
		}
	}

	// inventories
	if n, err = r.section("inventories", len(w.Invents.Slots)); err != nil {
		return err
	}
	d.inN = n
	d.inE = make([]EntityID, n)
	d.inSlots = make([][InventorySlots]EntityID, n)
	d.inReady = make([][data.ItemClassCount]uint32, n)
	for i := int32(0); i < n; i++ {
		d.inE[i] = r.ent()
		for sl := 0; sl < InventorySlots; sl++ {
			d.inSlots[i][sl] = r.ent()
		}
		for c := 0; c < data.ItemClassCount; c++ {
			d.inReady[i][c] = r.u32()
		}
	}

	// orders
	if n, err = r.section("orders", len(w.Orders.Kind)); err != nil {
		return err
	}
	d.orN = n
	d.orE = make([]EntityID, n)
	d.orKind = make([]uint8, n)
	d.orPhase = make([]uint8, n)
	d.orTgt = make([]EntityID, n)
	d.orPt = make([]fixed.Vec2, n)
	d.orData = make([]uint16, n)
	d.orHead = make([]int32, n)
	for i := int32(0); i < n; i++ {
		d.orE[i] = r.ent()
		d.orKind[i] = r.u8()
		d.orPhase[i] = r.u8()
		d.orTgt[i] = r.ent()
		d.orPt[i] = r.vec2()
		d.orData[i] = r.u16()
		d.orHead[i] = r.i32()
	}
	r.what = "order pool"
	poolLen := r.u32()
	if r.err != nil {
		return r.err
	}
	if int(poolLen) != len(w.orderPool) {
		return fmt.Errorf("sim: save: order pool has %d entries, this world has %d", poolLen, len(w.orderPool))
	}
	d.poolFreeHead = r.i32()
	d.poolFreeCount = r.i32()
	d.pool = make([]orderEntry, poolLen)
	for i := range d.pool {
		e := &d.pool[i]
		e.next = r.i32()
		e.kind = r.u8()
		e.target = r.ent()
		e.point = r.vec2()
		e.data = r.u16()
	}

	// buffs
	r.what = "buff pool"
	bCap := r.u32()
	if r.err != nil {
		return r.err
	}
	if int(bCap) != w.Buffs.Cap() {
		return fmt.Errorf("sim: save: buff pool capacity %d, this world has %d", bCap, w.Buffs.Cap())
	}
	nLive := r.u32()
	if r.err != nil {
		return r.err
	}
	if nLive > bCap {
		return fmt.Errorf("sim: save: buff live count %d exceeds capacity %d", nLive, bCap)
	}
	d.buffLive = make([]bool, bCap)
	d.buffRows = make([]BuffInstance, bCap)
	prevIdx := int32(-1)
	for i := uint32(0); i < nLive; i++ {
		idx := int32(r.u32())
		if r.err != nil {
			return r.err
		}
		if idx <= prevIdx || idx >= int32(bCap) {
			return fmt.Errorf("sim: save: buff row index %d out of order or range", idx)
		}
		prevIdx = idx
		d.buffLive[idx] = true
		d.buffRows[idx] = BuffInstance{
			BuffID: r.u16(), Stacks: r.u8(), Flags: r.u8(),
			Target: r.ent(), Source: r.ent(),
			RemainingTicks: r.u32(), PeriodicClock: r.u32(),
		}
	}
	r.what = "buff free list"
	nFree := r.u32()
	if r.err != nil {
		return r.err
	}
	if nFree != bCap-nLive {
		return fmt.Errorf("sim: save: buff free count %d, want %d (cap %d − live %d)", nFree, bCap-nLive, bCap, nLive)
	}
	d.buffFree = make([]int32, nFree)
	for i := range d.buffFree {
		d.buffFree[i] = r.i32()
	}

	// missiles
	if n, err = r.section("missiles", len(w.Missiles.Speed)); err != nil {
		return err
	}
	d.msN = n
	d.msE = make([]EntityID, n)
	d.msSpeed = make([]fixed.F64, n)
	d.msAccel = make([]fixed.F64, n)
	d.msArc = make([]fixed.F64, n)
	d.msFlags = make([]uint8, n)
	d.msHit = make([]uint16, n)
	d.msGuid = make([]uint16, n)
	d.msImpact = make([]uint16, n)
	d.msGE = make([]EntityID, n)
	d.msGP = make([]fixed.Vec2, n)
	d.msPay = make([]data.EffectList, n)
	d.msPkt = make([]DamagePacket, n)
	d.msSrc = make([]EntityID, n)
	d.msBirth = make([]uint32, n)
	d.msDir = make([]fixed.Vec2, n)
	d.msRange = make([]fixed.F64, n)
	d.msPierce = make([]int32, n)
	d.msDecay = make([]uint16, n)
	for i := int32(0); i < n; i++ {
		d.msE[i] = r.ent()
		d.msSpeed[i] = r.f64()
		d.msAccel[i] = r.f64()
		d.msArc[i] = r.f64()
		d.msFlags[i] = r.u8()
		d.msHit[i] = r.u16()
		d.msGuid[i] = r.u16()
		d.msImpact[i] = r.u16()
		d.msGE[i] = r.ent()
		d.msGP[i] = r.vec2()
		d.msPay[i] = data.EffectList{Off: r.u16(), Len: r.u16()}
		d.msPkt[i] = DamagePacket{Source: r.ent(), Target: r.ent(), Amount: r.f64(), AttackType: r.u8()}
		d.msSrc[i] = r.ent()
		d.msBirth[i] = r.u32()
		d.msDir[i] = r.vec2()
		d.msRange[i] = r.f64()
		d.msPierce[i] = r.i32()
		d.msDecay[i] = r.u16()
	}

	// doodads
	if n, err = r.section("doodads", len(w.Doodads.Placement)); err != nil {
		return err
	}
	d.doN = n
	d.doPlace = make([]int32, n)
	d.doVis = make([]bool, n)
	d.doAnim = make([]uint16, n)
	d.doPos = make([]fixed.Vec2, n)
	d.doFace = make([]fixed.Angle, n)
	d.doOv = make([]uint8, n)
	d.doE = make([]EntityID, n)
	for i := int32(0); i < n; i++ {
		d.doPlace[i] = r.i32()
		d.doVis[i] = r.boolean()
		d.doAnim[i] = r.u16()
		d.doPos[i] = r.vec2()
		d.doFace[i] = fixed.Angle(r.u16())
		d.doOv[i] = r.u8()
		d.doE[i] = r.ent()
	}

	// economy
	r.what = "economy counters"
	d.resourceCount = r.i32()
	if r.err != nil {
		return r.err
	}
	if d.resourceCount < 0 || int(d.resourceCount) != w.resourceCount {
		return fmt.Errorf("sim: save: %d resource types, this world has %d bound — BindEconomy before LoadState", d.resourceCount, w.resourceCount)
	}
	if d.resourceCount > 0 {
		d.resources = make([][]int64, MaxPlayers)
		for pl := 0; pl < MaxPlayers; pl++ {
			d.resources[pl] = make([]int64, d.resourceCount)
			for ri := range d.resources[pl] {
				d.resources[pl][ri] = r.i64()
			}
		}
	}
	if n, err = r.section("resource nodes", len(w.Nodes.Resource)); err != nil {
		return err
	}
	d.ndN = n
	d.ndE = make([]EntityID, n)
	d.ndRes = make([]uint8, n)
	d.ndRem = make([]int64, n)
	d.ndFl = make([]uint8, n)
	d.ndBusy = make([]EntityID, n)
	for i := int32(0); i < n; i++ {
		d.ndE[i] = r.ent()
		d.ndRes[i] = r.u8()
		d.ndRem[i] = r.i64()
		d.ndFl[i] = r.u8()
		d.ndBusy[i] = r.ent()
	}
	if n, err = r.section("econ rows", len(w.Econs.FoodCost)); err != nil {
		return err
	}
	d.ecN = n
	d.ecE = make([]EntityID, n)
	d.ecCost = make([]uint8, n)
	d.ecProv = make([]uint8, n)
	d.ecMask = make([]uint16, n)
	for i := int32(0); i < n; i++ {
		d.ecE[i] = r.ent()
		d.ecCost[i] = r.u8()
		d.ecProv[i] = r.u8()
		d.ecMask[i] = r.u16()
	}
	if n, err = r.section("harvest rows", len(w.Harvests.State)); err != nil {
		return err
	}
	d.hvN = n
	d.hvE = make([]EntityID, n)
	d.hvState = make([]uint8, n)
	d.hvNode = make([]EntityID, n)
	d.hvDepot = make([]EntityID, n)
	d.hvCarr = make([]int32, n)
	d.hvCRes = make([]uint8, n)
	d.hvClock = make([]uint32, n)
	d.hvCap = make([]int32, n)
	d.hvGT = make([]uint16, n)
	d.hvMask = make([]uint16, n)
	for i := int32(0); i < n; i++ {
		d.hvE[i] = r.ent()
		d.hvState[i] = r.u8()
		d.hvNode[i] = r.ent()
		d.hvDepot[i] = r.ent()
		d.hvCarr[i] = r.i32()
		d.hvCRes[i] = r.u8()
		d.hvClock[i] = r.u32()
		d.hvCap[i] = r.i32()
		d.hvGT[i] = r.u16()
		d.hvMask[i] = r.u16()
	}
	if n, err = r.section("produce rows", len(w.Produce.QCount)); err != nil {
		return err
	}
	d.pqN = n
	d.pqE = make([]EntityID, n)
	d.pqQ = make([][ProduceQueueCap]uint16, n)
	d.pqFl = make([][ProduceQueueCap]uint8, n)
	d.pqCount = make([]uint8, n)
	d.pqDone = make([]uint32, n)
	d.pqRK = make([]uint8, n)
	d.pqRE = make([]EntityID, n)
	d.pqRP = make([]fixed.Vec2, n)
	for i := int32(0); i < n; i++ {
		d.pqE[i] = r.ent()
		d.pqCount[i] = r.u8()
		for q := 0; q < ProduceQueueCap; q++ {
			d.pqQ[i][q] = r.u16()
			d.pqFl[i][q] = r.u8()
		}
		d.pqDone[i] = r.u32()
		d.pqRK[i] = r.u8()
		d.pqRE[i] = r.ent()
		d.pqRP[i] = r.vec2()
	}
	r.what = "tech arrays"
	d.upgCount = r.i32()
	if r.err != nil {
		return r.err
	}
	if d.upgCount < 0 || int(d.upgCount) != len(w.upgradeDefs) {
		return fmt.Errorf("sim: save: %d upgrades, this world has %d bound — BindTech before LoadState", d.upgCount, len(w.upgradeDefs))
	}
	if d.upgCount > 0 {
		d.upgLevel = make([][]uint8, MaxPlayers)
		d.upgMax = make([][]uint8, MaxPlayers)
		for pl := 0; pl < MaxPlayers; pl++ {
			d.upgLevel[pl] = make([]uint8, d.upgCount)
			d.upgMax[pl] = make([]uint8, d.upgCount)
			for u := int32(0); u < d.upgCount; u++ {
				d.upgLevel[pl][u] = r.u8()
				d.upgMax[pl][u] = r.u8()
			}
		}
	}

	if n, err = r.section("hero rows", len(w.Heroes.XP)); err != nil {
		return err
	}
	d.hrN = n
	d.hrE = make([]EntityID, n)
	d.hrRec = make([]HeroRecord, n)
	for i := int32(0); i < n; i++ {
		d.hrE[i] = r.ent()
		rec := &d.hrRec[i]
		rec.Used = true
		rec.HeroType = r.u16()
		rec.XP = r.i64()
		rec.Level = r.u8()
		rec.Str = fixed.F64(r.i64())
		rec.Agi = fixed.F64(r.i64())
		rec.Int = fixed.F64(r.i64())
		rec.SkillPoints = r.u8()
		for sl := 0; sl < data.MaxHeroSkills; sl++ {
			rec.SkillLevel[sl] = r.u8()
		}
	}
	r.what = "dead-hero pools"
	d.deadRec = make([][MaxDeadHeroes]HeroRecord, MaxPlayers)
	for pl := 0; pl < MaxPlayers; pl++ {
		for sl := 0; sl < MaxDeadHeroes; sl++ {
			used := r.u8()
			if r.err != nil {
				return r.err
			}
			if used > 1 {
				return fmt.Errorf("sim: save: dead-hero slot flag %d not 0/1", used)
			}
			if used == 0 {
				continue
			}
			rec := &d.deadRec[pl][sl]
			rec.Used = true
			rec.HeroType = r.u16()
			rec.XP = r.i64()
			rec.Level = r.u8()
			rec.Str = fixed.F64(r.i64())
			rec.Agi = fixed.F64(r.i64())
			rec.Int = fixed.F64(r.i64())
			rec.SkillPoints = r.u8()
			for k := 0; k < data.MaxHeroSkills; k++ {
				rec.SkillLevel[k] = r.u8()
			}
		}
	}

	// items (#305)
	if n, err = r.section("item rows", len(w.Items.TypeID)); err != nil {
		return err
	}
	d.itN = n
	d.itE = make([]EntityID, n)
	d.itType = make([]uint16, n)
	d.itCharges = make([]uint16, n)
	d.itCarrier = make([]EntityID, n)
	for i := int32(0); i < n; i++ {
		d.itE[i] = r.ent()
		d.itType[i] = r.u16()
		d.itCharges[i] = r.u16()
		d.itCarrier[i] = r.ent()
	}

	// patrol (#306)
	if n, err = r.section("patrol rows", len(w.Patrol.A)); err != nil {
		return err
	}
	d.paN = n
	d.paE = make([]EntityID, n)
	d.paA = make([]fixed.Vec2, n)
	d.paB = make([]fixed.Vec2, n)
	d.paFlags = make([]uint8, n)
	for i := int32(0); i < n; i++ {
		d.paE[i] = r.ent()
		d.paA[i] = r.vec2()
		d.paB[i] = r.vec2()
		d.paFlags[i] = r.u8()
	}

	// build (#301)
	if n, err = r.section("build rows", len(w.Build.FX)); err != nil {
		return err
	}
	d.bdN = n
	d.bdE = make([]EntityID, n)
	d.bdBldr = make([]EntityID, n)
	d.bdFX = make([]int32, n)
	d.bdFY = make([]int32, n)
	d.bdFW = make([]int32, n)
	d.bdProg = make([]uint16, n)
	for i := int32(0); i < n; i++ {
		d.bdE[i] = r.ent()
		d.bdBldr[i] = r.ent()
		d.bdFX[i] = r.i32()
		d.bdFY[i] = r.i32()
		d.bdFW[i] = r.i32()
		d.bdProg[i] = r.u16()
	}

	// clock (#339)
	r.what = "clock"
	d.clockTOD = r.f64()
	d.clockScale = r.f64()
	d.clockFrozen = r.boolean()
	d.clockCarry = r.u64()
	d.clockDayLength = r.u32()
	if r.err != nil {
		return r.err
	}

	// gamestate (#345)
	r.what = "gamestate"
	for player := 0; player < MaxPlayers; player++ {
		d.results[player] = r.u8()
	}
	if r.err != nil {
		return r.err
	}

	// effects (#349)
	if n, err = r.section("effects", len(w.Effects.ModelID)); err != nil {
		return err
	}
	d.efN = n
	d.efModel = make([]uint16, n)
	d.efScale = make([]fixed.F64, n)
	d.efColor = make([]uint32, n)
	d.efBirth = make([]uint32, n)
	d.efE = make([]EntityID, n)
	for i := int32(0); i < n; i++ {
		d.efModel[i] = r.u16()
		d.efScale[i] = r.f64()
		d.efColor[i] = r.u32()
		d.efBirth[i] = r.u32()
		d.efE[i] = r.ent()
	}
	if r.err != nil {
		return r.err
	}

	// ability field overrides (#354)
	if n, err = r.section("ability fields", w.AbilityFields.Cap()); err != nil {
		return err
	}
	d.afN = n
	afCap := w.AbilityFields.Cap()
	d.afLive = make([]bool, afCap)
	d.afEnt = make([]EntityID, afCap)
	d.afSlot = make([]uint8, afCap)
	d.afField = make([]uint8, afCap)
	d.afValue = make([]fixed.F64, afCap)
	for i := int32(0); i < n; i++ {
		row := r.u32()
		if r.err != nil {
			return r.err
		}
		if row >= uint32(afCap) {
			return fmt.Errorf("sim: save: ability field row index %d exceeds cap %d", row, afCap)
		}
		if d.afLive[row] {
			return fmt.Errorf("sim: save: duplicate ability field row index %d", row)
		}
		d.afLive[row] = true
		d.afEnt[row] = r.ent()
		d.afSlot[row] = r.u8()
		d.afField[row] = r.u8()
		d.afValue[row] = r.f64()
	}
	if r.err != nil {
		return r.err
	}
	r.what = "ability field free list"
	nFree = r.u32()
	if r.err != nil {
		return r.err
	}
	if nFree > uint32(afCap) {
		return fmt.Errorf("sim: save: ability field free count %d exceeds cap %d", nFree, afCap)
	}
	d.afFree = make([]int32, nFree)
	for i := range d.afFree {
		d.afFree[i] = r.i32()
	}
	r.what = "ability field rejected count"
	d.afRejected = r.u64()
	if r.err != nil {
		return r.err
	}

	// runtime ability definitions (#355)
	if n, err = r.section("runtime ability definitions", cap(w.runtimeAbilityDefs)); err != nil {
		return err
	}
	d.runtimeAbilityDefs = make([]data.Ability, n)
	for i := range d.runtimeAbilityDefs {
		def, err := abilityDefLoad(r)
		if err != nil {
			return err
		}
		d.runtimeAbilityDefs[i] = def
	}
	if r.err != nil {
		return r.err
	}

	// visibility (#299)
	if err := readVisibility(r, w, d); err != nil {
		return err
	}

	// scheduler blob
	r.what = "scheduler blob"
	blobLen := r.u32()
	if r.err != nil {
		return r.err
	}
	const maxSchedBlob = 64 << 20
	if blobLen > maxSchedBlob {
		return fmt.Errorf("sim: save: scheduler blob length %d exceeds limit", blobLen)
	}
	d.schedBlob = make([]byte, blobLen)
	if _, err := io.ReadFull(r.r, d.schedBlob); err != nil {
		return fmt.Errorf("sim: save: truncated while reading scheduler blob")
	}

	// subscriptions
	r.what = "subscriptions"
	nKinds := r.u32()
	if r.err != nil {
		return r.err
	}
	const maxKinds = 1 << 16
	if nKinds > maxKinds {
		return fmt.Errorf("sim: save: subscription kind count %d exceeds limit", nKinds)
	}
	d.subs = make([]kindSubs, nKinds)
	for i := range d.subs {
		d.subs[i].kind = r.u16()
		hn := r.u32()
		if r.err != nil {
			return r.err
		}
		if i > 0 && d.subs[i-1].kind >= d.subs[i].kind {
			return fmt.Errorf("sim: save: subscription kinds not strictly ascending at %d", d.subs[i].kind)
		}
		if hn > 1<<20 {
			return fmt.Errorf("sim: save: handler count %d for kind %d exceeds limit", hn, d.subs[i].kind)
		}
		d.subs[i].list = make([]HandlerID, hn)
		for j := range d.subs[i].list {
			d.subs[i].list[j] = HandlerID(r.u32())
		}
	}

	// regions (#241): id-ordered slots, then the free list.
	r.what = "regions"
	nReg := r.u32()
	if r.err != nil {
		return r.err
	}
	const maxRegions = 1 << 20
	if nReg > maxRegions {
		return fmt.Errorf("sim: save: region count %d exceeds limit", nReg)
	}
	d.regEntries = make([]regionEntry, nReg)
	d.regMembers = make([][]EntityID, nReg)
	for i := range d.regEntries {
		e := &d.regEntries[i]
		e.gen = r.u32()
		e.alive = r.boolean()
		if r.err != nil {
			return r.err
		}
		if !e.alive {
			continue
		}
		nCells := r.u32()
		if nCells > regionCellCount {
			return fmt.Errorf("sim: save: region %d cell count %d exceeds %d", i, nCells, regionCellCount)
		}
		if nCells > 0 {
			b := make([]uint64, regionBitsetWords)
			prev := int64(-1)
			for c := uint32(0); c < nCells; c++ {
				cell := int32(r.u32())
				if r.err != nil {
					return r.err
				}
				if cell < 0 || cell >= regionCellCount {
					return fmt.Errorf("sim: save: region %d cell index %d out of range", i, cell)
				}
				if int64(cell) <= prev {
					return fmt.Errorf("sim: save: region %d cells not strictly ascending at %d", i, cell)
				}
				prev = int64(cell)
				setBit(b, cell)
			}
			e.cells = b
		}
		// membership (#241): units currently inside, presence-row order
		nMem := r.u32()
		if nMem > uint32(w.Regions.rowCap) {
			return fmt.Errorf("sim: save: region %d member count %d exceeds cap %d", i, nMem, w.Regions.rowCap)
		}
		if nMem > 0 {
			mem := make([]EntityID, nMem)
			for m := range mem {
				mem[m] = r.ent()
			}
			d.regMembers[i] = mem
		}
	}
	nFree = r.u32()
	if r.err != nil {
		return r.err
	}
	if nFree > nReg {
		return fmt.Errorf("sim: save: region free count %d exceeds region count %d", nFree, nReg)
	}
	d.regFree = make([]uint32, nFree)
	for i := range d.regFree {
		d.regFree[i] = r.u32()
	}

	// players (#218): per-player roster + alliance bitset, slot order.
	r.what = "players"
	const maxPlayerNameLen = 256
	for p := 0; p < MaxPlayers; p++ {
		name, err := r.str(maxPlayerNameLen)
		if err != nil {
			return err
		}
		d.plName[p] = name
		d.plController[p] = r.u8()
		d.plRace[p] = r.u8()
		d.plColor[p] = r.u8()
		d.plTeam[p] = r.u8()
		d.plStartX[p] = r.f64()
		d.plStartY[p] = r.f64()
		d.plAlliedVictory[p] = r.boolean()
		d.plHandicap[p] = r.f64()
		d.plHandicapDamage[p] = r.f64()
		d.plHandicapXP[p] = r.f64()
		d.plHandicapReviveTime[p] = r.f64()
	}
	for a := 0; a < MaxPlayers; a++ {
		for b := 0; b < MaxPlayers; b++ {
			d.plAlliance[a][b] = r.u16()
		}
	}

	// heightfield (#371, v22): dims/origin/cell then row-major samples.
	r.what = "heightfield"
	d.hfCols = int32(r.u32())
	d.hfRows = int32(r.u32())
	d.hfOriginX = r.f64()
	d.hfOriginY = r.f64()
	d.hfCell = r.f64()
	if r.err != nil {
		return r.err
	}
	// fail-closed on a corrupt/oversized grid before allocating.
	const maxHeightSamples = 1 << 24
	if d.hfCols < 0 || d.hfRows < 0 {
		return fmt.Errorf("sim: save: negative heightfield dims %dx%d", d.hfCols, d.hfRows)
	}
	nh := int64(d.hfCols) * int64(d.hfRows)
	if nh > maxHeightSamples {
		return fmt.Errorf("sim: save: heightfield %dx%d exceeds cap %d", d.hfCols, d.hfRows, maxHeightSamples)
	}
	if nh > 0 {
		d.hfSamples = make([]fixed.F64, nh)
		for i := range d.hfSamples {
			d.hfSamples[i] = r.f64()
		}
	}

	// upkeep (#375, v25): brackets, lost counters, sparse tax matrix.
	r.what = "upkeep"
	upN := int(r.u32())
	if r.err != nil {
		return r.err
	}
	if upN < 0 || upN > maxUpkeepTiers {
		return fmt.Errorf("sim: save: %d upkeep tiers exceeds cap %d", upN, maxUpkeepTiers)
	}
	d.upCount = upN
	for i := 0; i < upN; i++ {
		d.upFood[i] = int32(r.u32())
		for res := 0; res < data.MaxResourceTypes; res++ {
			d.upRate[i][res] = r.f64()
		}
	}
	for p := 0; p < MaxPlayers; p++ {
		for res := 0; res < data.MaxResourceTypes; res++ {
			d.upLost[p][res] = int64(r.u64())
		}
	}
	taxN := int(r.u32())
	if r.err != nil {
		return r.err
	}
	const maxTaxEntries = MaxPlayers * MaxPlayers * data.MaxResourceTypes
	if taxN < 0 || taxN > maxTaxEntries {
		return fmt.Errorf("sim: save: %d tax-matrix entries exceeds cap %d", taxN, maxTaxEntries)
	}
	for i := 0; i < taxN; i++ {
		a := r.u8()
		b := r.u8()
		res := r.u8()
		v := r.f64()
		if int(a) >= MaxPlayers || int(b) >= MaxPlayers || int(res) >= data.MaxResourceTypes {
			return fmt.Errorf("sim: save: tax entry out of range (a=%d b=%d res=%d)", a, b, res)
		}
		d.upTax[a][b][res] = v
	}

	// fog overrides (#243, v26).
	r.what = "fog overrides"
	d.fogDisabled = r.boolean()
	d.fogMaskDisabled = r.boolean()
	fmN := int32(r.u32())
	if r.err != nil {
		return r.err
	}
	if fmN < 0 || int(fmN) > w.FogMods.capacity() {
		return fmt.Errorf("sim: save: fog modifier count %d exceeds cap %d", fmN, w.FogMods.capacity())
	}
	d.fmCount = fmN
	d.fmAlive = make([]bool, fmN)
	d.fmShared = make([]bool, fmN)
	d.fmActive = make([]bool, fmN)
	d.fmGen = make([]uint16, fmN)
	d.fmPlayer = make([]uint8, fmN)
	d.fmState = make([]uint8, fmN)
	d.fmKind = make([]uint8, fmN)
	d.fmAX = make([]fixed.F64, fmN)
	d.fmAY = make([]fixed.F64, fmN)
	d.fmBX = make([]fixed.F64, fmN)
	d.fmBY = make([]fixed.F64, fmN)
	for i := int32(0); i < fmN; i++ {
		d.fmAlive[i] = r.boolean()
		d.fmGen[i] = r.u16()
		d.fmPlayer[i] = r.u8()
		d.fmState[i] = r.u8()
		d.fmKind[i] = r.u8()
		d.fmShared[i] = r.boolean()
		d.fmActive[i] = r.boolean()
		d.fmAX[i] = r.f64()
		d.fmAY[i] = r.f64()
		d.fmBX[i] = r.f64()
		d.fmBY[i] = r.f64()
	}
	freeN := int32(r.u32())
	if r.err != nil {
		return r.err
	}
	if freeN < 0 || freeN > fmN {
		return fmt.Errorf("sim: save: fog free-list count %d exceeds modifier count %d", freeN, fmN)
	}
	d.fmFree = make([]int32, freeN)
	for i := int32(0); i < freeN; i++ {
		slot := int32(r.u32())
		if slot < 0 || slot >= fmN {
			return fmt.Errorf("sim: save: fog free-list slot %d out of range", slot)
		}
		d.fmFree[i] = slot
	}
	svN := int32(r.u32())
	if r.err != nil {
		return r.err
	}
	if svN < 0 || int(svN) > len(w.ShareVisions.Mask) {
		return fmt.Errorf("sim: save: shared-vision count %d exceeds cap %d", svN, len(w.ShareVisions.Mask))
	}
	d.svEntity = make([]EntityID, svN)
	d.svMask = make([]uint16, svN)
	for i := int32(0); i < svN; i++ {
		d.svEntity[i] = r.ent()
		d.svMask[i] = r.u16()
	}

	// AI hooks (#257, v27): per-player difficulty/paused/attached + command stack.
	r.what = "ai hooks"
	for p := 0; p < MaxPlayers; p++ {
		d.aiDifficulty[p] = r.u8()
		d.aiPaused[p] = r.boolean()
		d.aiAttached[p] = r.boolean()
		cmdN := int32(r.u32())
		if r.err != nil {
			return r.err
		}
		if cmdN < 0 || cmdN > maxAIInbox {
			return fmt.Errorf("sim: save: AI command count %d exceeds cap %d for player %d", cmdN, maxAIInbox, p)
		}
		if cmdN > 0 {
			d.aiInbox[p] = make([]aiCommand, cmdN)
			for i := int32(0); i < cmdN; i++ {
				d.aiInbox[p][i] = aiCommand{Command: r.i32(), Data: r.i32()}
			}
		}
	}

	// destructables (#229, v28).
	deN, err := r.section("destructables", w.Destructables.Cap())
	if err != nil {
		return err
	}
	d.deN = deN
	d.deType = make([]uint16, deN)
	d.dePos = make([]fixed.Vec2, deN)
	d.deFace = make([]fixed.Angle, deN)
	d.deLife = make([]int32, deN)
	d.deMax = make([]int32, deN)
	d.deDead = make([]bool, deN)
	d.deInvul = make([]bool, deN)
	d.deBlock = make([]bool, deN)
	d.deFoot = make([]uint8, deN)
	d.deE = make([]EntityID, deN)
	for i := int32(0); i < deN; i++ {
		d.deType[i] = r.u16()
		d.dePos[i] = r.vec2()
		d.deFace[i] = fixed.Angle(r.u16())
		d.deLife[i] = r.i32()
		d.deMax[i] = r.i32()
		d.deDead[i] = r.boolean()
		d.deInvul[i] = r.boolean()
		d.deBlock[i] = r.boolean()
		d.deFoot[i] = r.u8()
		d.deE[i] = r.ent()
	}

	// ECA handler-identity registry (#455, v29): the stable name table in
	// ref order, bounded so a corrupt save cannot drive an unbounded alloc.
	r.what = "handlers"
	hrN := r.u32()
	if r.err != nil {
		return r.err
	}
	if hrN > maxHandlerNames {
		return fmt.Errorf("sim: save: handler-registry name count %d exceeds limit %d", hrN, maxHandlerNames)
	}
	d.handlerNames = make([]string, hrN)
	for i := range d.handlerNames {
		name, err := r.str(maxHandlerNameLen)
		if err != nil {
			return err
		}
		d.handlerNames[i] = name
	}

	// first-class ECA trigger slab (#456, v30): slots in slot order, then
	// the free list. Counts are bounded so a corrupt save cannot drive an
	// unbounded allocation.
	r.what = "triggers"
	tsN := r.u32()
	if r.err != nil {
		return r.err
	}
	if int(tsN) > w.Triggers.Cap() {
		return fmt.Errorf("sim: save: trigger slot count %d exceeds this world's cap %d", tsN, w.Triggers.Cap())
	}
	d.trigSlots = make([]triggerSlot, tsN)
	for i := range d.trigSlots {
		sl := &d.trigSlots[i]
		sl.gen = r.u32()
		sl.alive = r.boolean()
		if !sl.alive {
			continue
		}
		sl.enabled = r.boolean()
		sl.on = r.boolean()
		sl.cond = ExprRef(r.i32())
		evN := r.u32()
		if r.err != nil {
			return r.err
		}
		if evN > maxTriggerEvents {
			return fmt.Errorf("sim: save: trigger %d event count %d exceeds limit %d", i, evN, maxTriggerEvents)
		}
		sl.events = make([]EventReg, evN)
		for j := range sl.events {
			sl.events[j].Kind = r.u16()
			sl.events[j].Scope = r.ent()
		}
		acN := r.u32()
		if r.err != nil {
			return r.err
		}
		if acN > maxTriggerActions {
			return fmt.Errorf("sim: save: trigger %d action count %d exceeds limit %d", i, acN, maxTriggerActions)
		}
		sl.actions = make([]HandlerRef, acN)
		for j := range sl.actions {
			sl.actions[j] = HandlerRef(r.u32())
		}
	}
	tfN := r.u32()
	if r.err != nil {
		return r.err
	}
	if tfN > tsN {
		return fmt.Errorf("sim: save: trigger free-list count %d exceeds slot count %d", tfN, tsN)
	}
	d.trigFree = make([]uint32, tfN)
	for i := range d.trigFree {
		d.trigFree[i] = r.u32()
	}
	return r.err
}

func abilityDefLoad(r *saveReader) (data.Ability, error) {
	id, err := r.str(maxRuntimeAbilityStringLen)
	if err != nil {
		return data.Ability{}, err
	}
	name, err := r.str(maxRuntimeAbilityStringLen)
	if err != nil {
		return data.Ability{}, err
	}
	def := data.Ability{
		ID:             id,
		Name:           name,
		Effects:        data.EffectList{Off: r.u16(), Len: r.u16()},
		ManaCost:       r.i32(),
		CooldownTicks:  r.u16(),
		CastPointTicks: r.u16(),
		BackswingTicks: r.u16(),
		ChannelTicks:   r.u16(),
		CastRange:      r.f64(),
	}
	if r.err != nil {
		return data.Ability{}, r.err
	}
	return def, nil
}

// validateSave cross-checks the decoded staging against the world's
// registries and internal consistency before anything applies.
func validateSave(d *decodedSave, w *World) error {
	// entity table: count must match the alive bits
	alive := int32(0)
	for i := 1; i < len(d.entSlots); i++ {
		if d.entSlots[i].alive {
			alive++
		}
	}
	if alive != d.entCount {
		return fmt.Errorf("sim: save: entity count %d but %d alive slots", d.entCount, alive)
	}
	if d.entSlots[0].alive {
		return fmt.Errorf("sim: save: reserved entity slot 0 marked alive")
	}
	entAlive := func(id EntityID) bool {
		idx := id.Index()
		return idx > 0 && idx < uint32(len(d.entSlots)) &&
			d.entSlots[idx].alive && d.entSlots[idx].gen == id.Generation()
	}
	check := func(store string, ents []EntityID) error {
		for i, id := range ents {
			if !entAlive(id) {
				return fmt.Errorf("sim: save: %s row %d references dead/stale entity %d", store, i, id)
			}
		}
		return nil
	}
	for _, c := range []struct {
		name string
		ents []EntityID
	}{
		{"transforms", d.trE}, {"movement", d.mvE}, {"collisions", d.coE},
		{"unit types", d.utE}, {"health", d.hlE}, {"owners", d.owE},
		{"combat", d.cbE}, {"abilities", d.abE}, {"inventories", d.inE},
		{"orders", d.orE}, {"missiles", d.msE}, {"doodads", d.doE},
		{"resource nodes", d.ndE}, {"econ rows", d.ecE}, {"harvest rows", d.hvE},
		{"produce rows", d.pqE}, {"hero rows", d.hrE}, {"item rows", d.itE},
		{"patrol rows", d.paE}, {"build rows", d.bdE}, {"effects", d.efE},
	} {
		if err := check(c.name, c.ents); err != nil {
			return err
		}
	}
	transformRows := make(map[EntityID]struct{}, len(d.trE))
	for _, id := range d.trE {
		transformRows[id] = struct{}{}
	}
	effectRows := make(map[EntityID]int32, len(d.efE))
	for i, id := range d.efE {
		if prev, ok := effectRows[id]; ok {
			return fmt.Errorf("sim: save: effect rows %d and %d both reference entity %d", prev, i, id)
		}
		effectRows[id] = int32(i)
		if _, ok := transformRows[id]; !ok {
			return fmt.Errorf("sim: save: effect row %d entity %d has no transform row", i, id)
		}
	}
	if len(w.abilityDefs)+len(d.runtimeAbilityDefs) > maxAbilityDefs {
		return fmt.Errorf("sim: save: ability def count static=%d runtime=%d exceeds ref cap %d",
			len(w.abilityDefs), len(d.runtimeAbilityDefs), maxAbilityDefs)
	}
	abilityIDs := make(map[string]int, len(w.abilityDefs)+len(d.runtimeAbilityDefs))
	for i := range w.abilityDefs {
		abilityIDs[w.abilityDefs[i].ID] = i
	}
	for i := range d.runtimeAbilityDefs {
		def := &d.runtimeAbilityDefs[i]
		if !w.validRuntimeAbilityDef(def) {
			return fmt.Errorf("sim: save: runtime ability def %d is invalid", i)
		}
		if prev, dup := abilityIDs[def.ID]; dup {
			return fmt.Errorf("sim: save: runtime ability def %d duplicates ability id %q at def %d", i, def.ID, prev)
		}
		abilityIDs[def.ID] = len(w.abilityDefs) + i
	}
	if err := validateVisibilitySave(d, w, entAlive); err != nil {
		return err
	}
	abilityRows := make(map[EntityID]abilitySlotSave, len(d.abE))
	for i, id := range d.abE {
		if _, dup := abilityRows[id]; dup {
			return fmt.Errorf("sim: save: duplicate ability rows for entity %d", id)
		}
		for slot := 0; slot < AbilitySlots; slot++ {
			ref := d.abSlots[i][slot].AbilityID
			if ref != 0 && int(ref) > len(w.abilityDefs)+len(d.runtimeAbilityDefs) {
				return fmt.Errorf("sim: save: ability row %d slot %d references unbound ability ref %d", i, slot, ref)
			}
		}
		abilityRows[id] = d.abSlots[i]
	}
	abilityFieldKeys := make(map[uint64]int, d.afN)
	abilityFieldPerUnit := make(map[EntityID]int)
	abilityFieldLive := 0
	for row := range d.afLive {
		if !d.afLive[row] {
			continue
		}
		abilityFieldLive++
		id := d.afEnt[row]
		slot := int(d.afSlot[row])
		field := AbilityField(d.afField[row])
		if !entAlive(id) {
			return fmt.Errorf("sim: save: ability field row %d references dead/stale entity %d", row, id)
		}
		if !validAbilitySlot(slot) {
			return fmt.Errorf("sim: save: ability field row %d has invalid slot %d", row, slot)
		}
		if !validAbilityField(field) {
			return fmt.Errorf("sim: save: ability field row %d has unknown field %d", row, field)
		}
		slots, ok := abilityRows[id]
		if !ok {
			return fmt.Errorf("sim: save: ability field row %d entity %d has no ability row", row, id)
		}
		if slots[slot].AbilityID == 0 {
			return fmt.Errorf("sim: save: ability field row %d targets empty ability slot %d", row, slot)
		}
		key := uint64(uint32(id))<<16 | uint64(d.afSlot[row])<<8 | uint64(d.afField[row])
		if prev, dup := abilityFieldKeys[key]; dup {
			return fmt.Errorf("sim: save: ability field rows %d and %d duplicate entity-slot-field", prev, row)
		}
		abilityFieldKeys[key] = row
		abilityFieldPerUnit[id]++
		if abilityFieldPerUnit[id] > AbilityOverrideCapPerUnit {
			return fmt.Errorf("sim: save: ability field entity %d has %d overrides, cap %d",
				id, abilityFieldPerUnit[id], AbilityOverrideCapPerUnit)
		}
	}
	if abilityFieldLive != int(d.afN) {
		return fmt.Errorf("sim: save: ability field live count %d, header says %d", abilityFieldLive, d.afN)
	}
	if abilityFieldLive+len(d.afFree) != len(d.afLive) {
		return fmt.Errorf("sim: save: ability field pool reachability live=%d free=%d cap=%d",
			abilityFieldLive, len(d.afFree), len(d.afLive))
	}
	abilityFieldFree := make([]bool, len(d.afLive))
	for _, f := range d.afFree {
		if f < 0 || int(f) >= len(d.afLive) {
			return fmt.Errorf("sim: save: ability field free index %d out of range", f)
		}
		if d.afLive[f] {
			return fmt.Errorf("sim: save: ability field row %d both live and free", f)
		}
		if abilityFieldFree[f] {
			return fmt.Errorf("sim: save: duplicate ability field free index %d", f)
		}
		abilityFieldFree[f] = true
	}
	for row, live := range d.afLive {
		if !live && !abilityFieldFree[row] {
			return fmt.Errorf("sim: save: ability field row %d is neither live nor free", row)
		}
	}

	// order queue links: every queue must terminate without cycles,
	// and chained entries must not be on the free list
	onChain := make([]bool, len(d.pool))
	for i := int32(0); i < d.orN; i++ {
		steps := 0
		for e := d.orHead[i]; e != NoOrderEntry; e = d.pool[e].next {
			if e < 0 || int(e) >= len(d.pool) {
				return fmt.Errorf("sim: save: order queue of row %d links out of range (%d)", i, e)
			}
			if onChain[e] {
				return fmt.Errorf("sim: save: order pool entry %d linked twice", e)
			}
			onChain[e] = true
			if steps++; steps > len(d.pool) {
				return fmt.Errorf("sim: save: order queue of row %d does not terminate", i)
			}
		}
	}
	freeSeen := int32(0)
	for e := d.poolFreeHead; e != NoOrderEntry; e = d.pool[e].next {
		if e < 0 || int(e) >= len(d.pool) {
			return fmt.Errorf("sim: save: order free list links out of range (%d)", e)
		}
		if onChain[e] {
			return fmt.Errorf("sim: save: order pool entry %d on both a queue and the free list", e)
		}
		if p := &d.pool[e]; p.kind != 0 || p.target != 0 || p.point != (fixed.Vec2{}) || p.data != 0 {
			return fmt.Errorf("sim: save: free order pool entry %d has non-canonical (non-zero) payload", e)
		}
		if freeSeen++; freeSeen > int32(len(d.pool)) {
			return fmt.Errorf("sim: save: order free list does not terminate")
		}
	}
	// entries on neither a queue nor the free list are unreachable —
	// refuse rather than carry hidden bytes
	reach := freeSeen
	for i := range onChain {
		if onChain[i] {
			reach++
		}
	}
	if int(reach) != len(d.pool) {
		return fmt.Errorf("sim: save: %d order pool entries unreachable from any queue or the free list", len(d.pool)-int(reach))
	}
	if freeSeen != d.poolFreeCount {
		return fmt.Errorf("sim: save: order free list has %d entries, header says %d", freeSeen, d.poolFreeCount)
	}

	// buff free stack must be exactly the complement of the live set
	for _, f := range d.buffFree {
		if f < 0 || int(f) >= len(d.buffLive) {
			return fmt.Errorf("sim: save: buff free index %d out of range", f)
		}
		if d.buffLive[f] {
			return fmt.Errorf("sim: save: buff index %d both live and free", f)
		}
	}
	// buff types must be bound for any live buff row
	for i := range d.buffRows {
		if d.buffLive[i] && int(d.buffRows[i].BuffID) >= len(w.buffTypes) {
			return fmt.Errorf("sim: save: buff row %d references unbound BuffID %d (bind data tables before load)", i, d.buffRows[i].BuffID)
		}
	}

	// produce rows: canonical queues — live prefix only, zero tail,
	// bound type IDs (the ledger recompute needs the defs' food costs)
	for i := int32(0); i < d.pqN; i++ {
		qc := int(d.pqCount[i])
		if qc > ProduceQueueCap {
			return fmt.Errorf("sim: save: produce row %d queue count %d exceeds cap %d", i, qc, ProduceQueueCap)
		}
		if qc > 0 && w.unitDefs == nil {
			return fmt.Errorf("sim: save: produce row %d has queued trains but no unit defs bound — BindUnitDefs before LoadState", i)
		}
		for q := 0; q < ProduceQueueCap; q++ {
			if q < qc {
				if d.pqFl[i][q]&TrainFlagResearch != 0 {
					if int(d.pqQ[i][q]) >= len(w.upgradeDefs) {
						return fmt.Errorf("sim: save: produce row %d slot %d references unbound upgrade %d — BindTech before LoadState", i, q, d.pqQ[i][q])
					}
				} else if int(d.pqQ[i][q]) >= len(w.unitDefs) {
					return fmt.Errorf("sim: save: produce row %d slot %d references unbound typeID %d", i, q, d.pqQ[i][q])
				}
			} else if d.pqQ[i][q] != 0 || d.pqFl[i][q] != 0 {
				return fmt.Errorf("sim: save: produce row %d slot %d past the live prefix has non-canonical (non-zero) payload", i, q)
			}
		}
		if qc == 0 && d.pqDone[i] != 0 {
			return fmt.Errorf("sim: save: produce row %d idle but head clock %d non-zero", i, d.pqDone[i])
		}
		if d.pqRK[i] > RallyEntity {
			return fmt.Errorf("sim: save: produce row %d rally kind %d unknown", i, d.pqRK[i])
		}
	}
	for pl := 0; pl < int(min(int64(len(d.upgLevel)), MaxPlayers)); pl++ {
		for u := int32(0); u < d.upgCount; u++ {
			if maxL := uint8(len(w.upgradeDefs[u].Levels)); d.upgLevel[pl][u] > maxL || d.upgMax[pl][u] > maxL {
				return fmt.Errorf("sim: save: player %d upgrade %d level %d / cap %d exceeds table max %d",
					pl, u, d.upgLevel[pl][u], d.upgMax[pl][u], maxL)
			}
		}
	}

	// hero rows / pools: require the bound tables, level/type bounds
	heroCount := func() int {
		if w.heroTables == nil {
			return 0
		}
		return len(w.heroTables.Heroes)
	}()
	checkRec := func(where string, rec *HeroRecord) error {
		if w.heroTables == nil {
			return fmt.Errorf("sim: save: %s present but no hero tables bound — BindHeroes before LoadState", where)
		}
		if int(rec.HeroType) >= heroCount {
			return fmt.Errorf("sim: save: %s references unbound hero type %d", where, rec.HeroType)
		}
		if rec.Level < 1 || int(rec.Level) > len(w.heroTables.Curve) {
			return fmt.Errorf("sim: save: %s level %d outside the bound curve [1, %d]", where, rec.Level, len(w.heroTables.Curve))
		}
		return nil
	}
	for i := int32(0); i < d.hrN; i++ {
		if err := checkRec(fmt.Sprintf("hero row %d", i), &d.hrRec[i]); err != nil {
			return err
		}
	}
	for pl := range d.deadRec {
		for sl := range d.deadRec[pl] {
			if d.deadRec[pl][sl].Used {
				if err := checkRec(fmt.Sprintf("dead-hero pool [%d][%d]", pl, sl), &d.deadRec[pl][sl]); err != nil {
					return err
				}
			}
		}
	}
	for i := int32(0); i < d.pqN; i++ { // revive queue entries point at OCCUPIED pool slots
		for q := 0; q < int(d.pqCount[i]); q++ {
			if d.pqFl[i][q]&TrainFlagHeroRevive == 0 {
				continue
			}
			if int(d.pqQ[i][q]) >= MaxDeadHeroes {
				return fmt.Errorf("sim: save: produce row %d slot %d revive index %d out of pool range", i, q, d.pqQ[i][q])
			}
		}
	}

	// clock: fail closed on non-canonical or unsafe values.
	if d.clockTOD < 0 || d.clockTOD >= clockDay {
		return fmt.Errorf("sim: save: clock tod %d outside [0,24h)", int64(d.clockTOD))
	}
	if d.clockScale < 0 {
		return fmt.Errorf("sim: save: clock scale %d is negative", int64(d.clockScale))
	}
	if d.clockDayLength == 0 {
		return fmt.Errorf("sim: save: clock day length is zero")
	}
	if d.clockCarry >= uint64(d.clockDayLength) {
		return fmt.Errorf("sim: save: clock carry %d outside denominator %d", d.clockCarry, d.clockDayLength)
	}

	winners := 0
	for player, result := range d.results {
		if result > ResultLeft {
			return fmt.Errorf("sim: save: player %d match result %d unknown", player, result)
		}
		if result == ResultWon {
			winners++
		}
	}
	if winners > 1 {
		return fmt.Errorf("sim: save: match result has %d winners", winners)
	}

	// items (#305): bound defs, type bounds, and the bidirectional
	// carrier↔slot invariant (no duplication, no orphans)
	if d.itN > 0 && w.itemDefs == nil {
		return fmt.Errorf("sim: save: %d item rows present but no item tables bound — BindItemDefs before LoadState", d.itN)
	}
	itemRow := map[EntityID]int32{}
	for i := int32(0); i < d.itN; i++ {
		if int(d.itType[i]) >= len(w.itemDefs) {
			return fmt.Errorf("sim: save: item row %d type %d outside the bound table (%d)", i, d.itType[i], len(w.itemDefs))
		}
		itemRow[d.itE[i]] = i
	}
	slotOf := map[EntityID]bool{}
	for i := int32(0); i < d.inN; i++ {
		for sl := 0; sl < InventorySlots; sl++ {
			item := d.inSlots[i][sl]
			if item == 0 {
				continue
			}
			ir, ok := itemRow[item]
			if !ok {
				return fmt.Errorf("sim: save: inventory of %d slot %d holds %d which has no item row", d.inE[i], sl, item)
			}
			if d.itCarrier[ir] != d.inE[i] {
				return fmt.Errorf("sim: save: item %d carried by %d but slotted on %d", item, d.itCarrier[ir], d.inE[i])
			}
			if slotOf[item] {
				return fmt.Errorf("sim: save: item %d appears in two inventory slots", item)
			}
			slotOf[item] = true
		}
	}
	for i := int32(0); i < d.itN; i++ {
		if d.itCarrier[i] != 0 && !slotOf[d.itE[i]] {
			return fmt.Errorf("sim: save: item %d claims carrier %d but no inventory slot holds it", d.itE[i], d.itCarrier[i])
		}
	}

	// subscriptions: every handler must already be registered
	for i := range d.subs {
		for _, id := range d.subs[i].list {
			if _, ok := w.handlers[id]; !ok {
				return fmt.Errorf("sim: save: subscription for kind %d references unregistered HandlerID %d", d.subs[i].kind, id)
			}
		}
	}

	// ECA handler-identity registry (#455): the loaded world must already
	// have re-registered the identical name table (same count, same names in
	// the same ref order) — that is the invariant making the trigger graph
	// survive a runtime swap. Any divergence means a stored HandlerRef would
	// resolve to the wrong (or no) function, so fail closed here.
	if len(d.handlerNames) != len(w.handlerReg.names) {
		return fmt.Errorf("sim: save: handler registry has %d names but this world re-registered %d — registration order/script set differs", len(d.handlerNames), len(w.handlerReg.names))
	}
	for ref, name := range d.handlerNames {
		if w.handlerReg.names[ref] != name {
			return fmt.Errorf("sim: save: handler ref %d is %q in the save but %q in this world — re-registration diverged", ref+1, name, w.handlerReg.names[ref])
		}
	}

	// trigger slab (#456): structural integrity. Alive slots need a live
	// generation; free-list entries must point at dead slots; action refs
	// must resolve in the re-registered handler registry (fail-closed — a
	// stored action that no script registers would silently never run).
	hn := len(w.handlerReg.names)
	for i := range d.trigSlots {
		sl := &d.trigSlots[i]
		if sl.alive && sl.gen == 0 {
			return fmt.Errorf("sim: save: trigger slot %d is alive with generation 0", i)
		}
		if !sl.alive {
			continue
		}
		for _, a := range sl.actions {
			if a != NoHandler && int(a) > hn {
				return fmt.Errorf("sim: save: trigger %d action references unregistered HandlerRef %d (registry has %d)", i, a, hn)
			}
		}
	}
	for _, f := range d.trigFree {
		if int(f) >= len(d.trigSlots) {
			return fmt.Errorf("sim: save: trigger free-list index %d out of range (%d slots)", f, len(d.trigSlots))
		}
		if d.trigSlots[f].alive {
			return fmt.Errorf("sim: save: trigger free-list index %d points at a live slot", f)
		}
	}

	// grid reservations need a bound grid to rebuild into
	if w.Grid == nil {
		for i := int32(0); i < d.mvN; i++ {
			if d.mvRes[i] != -1 {
				return fmt.Errorf("sim: save: movement row %d holds grid reservation %d but no grid is bound — SetGrid before LoadState", i, d.mvRes[i])
			}
		}
	}
	return nil
}

// applySave writes the staging into the world. Infallible by
// construction — everything was validated.
func applySave(d *decodedSave, w *World) {
	w.tick = d.tick
	w.unitCount = int(d.unitCount)
	w.rng = prng.Restore(d.rngCursor)
	w.tod = d.clockTOD
	w.todScale = d.clockScale
	w.todFrozen = d.clockFrozen
	w.todCarry = d.clockCarry
	w.dayLengthTicks = d.clockDayLength
	w.results = d.results
	applyVisibilitySave(d, w)
	for player := range w.resultPending {
		w.resultPending[player] = ResultPlaying
	}

	copy(w.Ents.slots, d.entSlots)
	w.Ents.freeHead = d.entFreeHead
	w.Ents.count = d.entCount

	resetRowOf := func(rowOf []int32) {
		for i := range rowOf {
			rowOf[i] = -1
		}
	}

	t := w.Transforms
	t.count = d.trN
	resetRowOf(t.rowOf)
	for i := int32(0); i < d.trN; i++ {
		t.Entity[i] = d.trE[i]
		t.Pos[i] = d.trP[i]
		t.Facing[i] = d.trF[i]
		t.rowOf[d.trE[i].Index()] = i
	}

	m := w.Movements
	m.count = d.mvN
	resetRowOf(m.rowOf)
	for i := int32(0); i < d.mvN; i++ {
		m.Entity[i] = d.mvE[i]
		m.Speed[i] = d.mvSpeed[i]
		m.TurnRate[i] = d.mvTurn[i]
		m.Target[i] = d.mvTarget[i]
		m.PathHandle[i] = d.mvPath[i]
		m.WaypointIdx[i] = d.mvWp[i]
		m.Stall[i] = d.mvStall[i]
		m.ResCell[i] = d.mvRes[i]
		m.State[i] = d.mvState[i]
		m.rowOf[d.mvE[i].Index()] = i
	}

	co := w.Collisions
	co.count = d.coN
	resetRowOf(co.rowOf)
	for i := int32(0); i < d.coN; i++ {
		co.Entity[i] = d.coE[i]
		co.SizeClass[i] = d.coSize[i]
		co.PathFlags[i] = d.coFlags[i]
		co.StampRef[i] = d.coStamp[i]
		co.rowOf[d.coE[i].Index()] = i
	}

	ut := w.UnitTypes
	ut.count = d.utN
	resetRowOf(ut.rowOf)
	for i := int32(0); i < d.utN; i++ {
		ut.Entity[i] = d.utE[i]
		ut.TypeID[i] = d.utID[i]
		ut.rowOf[d.utE[i].Index()] = i
	}

	ud := w.UserDatas
	ud.count = d.udN
	resetRowOf(ud.rowOf)
	for i := int32(0); i < d.udN; i++ {
		ud.Entity[i] = d.udE[i]
		ud.Value[i] = d.udVal[i]
		ud.rowOf[d.udE[i].Index()] = i
	}

	hd := w.Hiddens
	hd.count = d.hdN
	resetRowOf(hd.rowOf)
	for i := int32(0); i < d.hdN; i++ {
		hd.Entity[i] = d.hdE[i]
		hd.rowOf[d.hdE[i].Index()] = i
	}

	xs := w.XPSuspends
	xs.count = d.xsN
	resetRowOf(xs.rowOf)
	for i := int32(0); i < d.xsN; i++ {
		xs.Entity[i] = d.xsE[i]
		xs.rowOf[d.xsE[i].Index()] = i
	}

	pau := w.Pauses
	pau.count = d.psN
	resetRowOf(pau.rowOf)
	for i := int32(0); i < d.psN; i++ {
		pau.Entity[i] = d.psE[i]
		pau.rowOf[d.psE[i].Index()] = i
	}

	un := w.UnitNames
	un.count = d.unN
	resetRowOf(un.rowOf)
	for i := int32(0); i < d.unN; i++ {
		un.Entity[i] = d.unE[i]
		un.Name[i] = d.unName[i]
		un.rowOf[d.unE[i].Index()] = i
	}

	fly := w.Flys
	fly.count = d.flyN
	resetRowOf(fly.rowOf)
	for i := int32(0); i < d.flyN; i++ {
		fly.Entity[i] = d.flyE[i]
		fly.Height[i] = d.flyH[i]
		fly.Target[i] = d.flyT[i]
		fly.Rate[i] = d.flyR[i]
		fly.rowOf[d.flyE[i].Index()] = i
	}

	pw := w.PropWindows
	pw.count = d.pwN
	resetRowOf(pw.rowOf)
	for i := int32(0); i < d.pwN; i++ {
		pw.Entity[i] = d.pwE[i]
		pw.Value[i] = d.pwV[i]
		pw.rowOf[d.pwE[i].Index()] = i
	}

	hl := w.Healths
	hl.count = d.hlN
	resetRowOf(hl.rowOf)
	for i := int32(0); i < d.hlN; i++ {
		hl.Entity[i] = d.hlE[i]
		hl.Life[i] = d.hlLife[i]
		hl.MaxLife[i] = d.hlMax[i]
		hl.Regen[i] = d.hlRegen[i]
		hl.ArmorValue[i] = d.hlAV[i]
		hl.ArmorType[i] = d.hlAT[i]
		hl.DeathState[i] = d.hlDS[i]
		hl.DecayTicks[i] = d.hlDecay[i]
		hl.Invulnerable[i] = d.hlInv[i]
		hl.rowOf[d.hlE[i].Index()] = i
	}

	ow := w.Owners
	ow.count = d.owN
	resetRowOf(ow.rowOf)
	for i := int32(0); i < d.owN; i++ {
		ow.Entity[i] = d.owE[i]
		ow.Player[i] = d.owP[i]
		ow.Team[i] = d.owT[i]
		ow.Color[i] = d.owC[i]
		ow.rowOf[d.owE[i].Index()] = i
	}

	c := w.Combats
	c.count = d.cbN
	resetRowOf(c.rowOf)
	for i := int32(0); i < d.cbN; i++ {
		c.Entity[i] = d.cbE[i]
		c.AcquisitionRange[i] = d.cbAcq[i]
		c.Target[i] = d.cbTgt[i]
		c.LastAttacker[i] = d.cbLA[i]
		c.LastDamagedTick[i] = d.cbLDT[i]
		for sl := 0; sl < WeaponSlots; sl++ {
			ws := &d.cbSlots[i][sl]
			c.DmgBase[i][sl] = ws.DmgBase
			c.DmgDice[i][sl] = ws.DmgDice
			c.DmgSides[i][sl] = ws.DmgSides
			c.AttackType[i][sl] = ws.AttackType
			c.Cooldown[i][sl] = ws.Cooldown
			c.DamagePt[i][sl] = ws.DamagePt
			c.Backswing[i][sl] = ws.Backswing
			c.Range[i][sl] = ws.Range
			c.ProjRef[i][sl] = ws.ProjRef
			c.ProjSpeed[i][sl] = ws.ProjSpeed
			c.ReadyAt[i][sl] = ws.ReadyAt
			c.WFlags[i][sl] = ws.WFlags
			c.AtkState[i][sl] = ws.AtkState
			c.PhaseEnd[i][sl] = ws.PhaseEnd
			c.Effects[i][sl] = ws.Effects
		}
		c.rowOf[d.cbE[i].Index()] = i
	}

	w.runtimeAbilityDefs = w.runtimeAbilityDefs[:len(d.runtimeAbilityDefs)]
	copy(w.runtimeAbilityDefs, d.runtimeAbilityDefs)

	a := w.Abilities
	a.count = d.abN
	resetRowOf(a.rowOf)
	for i := int32(0); i < d.abN; i++ {
		a.Entity[i] = d.abE[i]
		a.Mana[i] = d.abMana[i]
		a.MaxMana[i] = d.abMax[i]
		a.ManaRegen[i] = d.abRegen[i]
		a.CastSlot[i] = d.abCS[i]
		a.CastEnd[i] = d.abCE[i]
		for sl := 0; sl < AbilitySlots; sl++ {
			as := &d.abSlots[i][sl]
			a.AbilityID[i][sl] = as.AbilityID
			a.Level[i][sl] = as.Level
			a.ReadyAt[i][sl] = as.ReadyAt
			a.CastState[i][sl] = as.CastState
		}
		a.rowOf[d.abE[i].Index()] = i
	}

	af := w.AbilityFields
	af.count = d.afN
	af.rejected = d.afRejected
	for i := range af.live {
		af.live[i] = false
		af.Ent[i] = 0
		af.Slot[i] = 0
		af.Field[i] = 0
		af.Value[i] = 0
	}
	for i := range af.rowOf {
		af.rowOf[i] = -1
	}
	for i := range af.perUnit {
		af.perUnit[i] = 0
	}
	for i := range d.afLive {
		if !d.afLive[i] {
			continue
		}
		af.live[i] = true
		af.Ent[i] = d.afEnt[i]
		af.Slot[i] = d.afSlot[i]
		af.Field[i] = d.afField[i]
		af.Value[i] = d.afValue[i]
		idx := af.Ent[i].Index()
		af.rowOf[af.key(idx, int(af.Slot[i]), AbilityField(af.Field[i]))] = int32(i)
		af.perUnit[idx]++
	}
	af.free = af.free[:len(d.afFree)]
	copy(af.free, d.afFree)

	in := w.Invents
	in.count = d.inN
	resetRowOf(in.rowOf)
	for i := int32(0); i < d.inN; i++ {
		in.Entity[i] = d.inE[i]
		in.Slots[i] = d.inSlots[i]
		in.ClassReady[i] = d.inReady[i]
		in.rowOf[d.inE[i].Index()] = i
	}

	o := w.Orders
	o.count = d.orN
	resetRowOf(o.rowOf)
	for i := int32(0); i < d.orN; i++ {
		o.Entity[i] = d.orE[i]
		o.Kind[i] = d.orKind[i]
		o.Phase[i] = d.orPhase[i]
		o.Target[i] = d.orTgt[i]
		o.Point[i] = d.orPt[i]
		o.Data[i] = d.orData[i]
		o.QueueHead[i] = d.orHead[i]
		o.rowOf[d.orE[i].Index()] = i
	}
	copy(w.orderPool, d.pool)
	w.orderFreeHead = d.poolFreeHead
	w.orderFreeCount = d.poolFreeCount

	p := w.Buffs
	copy(p.rows, d.buffRows)
	copy(p.live, d.buffLive)
	p.free = p.free[:len(d.buffFree)]
	copy(p.free, d.buffFree)

	ms := w.Missiles
	ms.count = d.msN
	resetRowOf(ms.rowOf)
	for i := int32(0); i < d.msN; i++ {
		ms.Entity[i] = d.msE[i]
		ms.Speed[i] = d.msSpeed[i]
		ms.Accel[i] = d.msAccel[i]
		ms.Arc[i] = d.msArc[i]
		ms.Flags[i] = d.msFlags[i]
		ms.HitMask[i] = d.msHit[i]
		ms.GuidanceID[i] = d.msGuid[i]
		ms.ImpactID[i] = d.msImpact[i]
		ms.GuideEnt[i] = d.msGE[i]
		ms.GuidePt[i] = d.msGP[i]
		ms.Payload[i] = d.msPay[i]
		ms.Packet[i] = d.msPkt[i]
		ms.Source[i] = d.msSrc[i]
		ms.BirthTick[i] = d.msBirth[i]
		ms.Dir[i] = d.msDir[i]
		ms.RangeLeft[i] = d.msRange[i]
		ms.PierceLeft[i] = d.msPierce[i]
		ms.Decay[i] = d.msDecay[i]
		ms.rowOf[d.msE[i].Index()] = i
	}

	do := w.Doodads
	do.count = d.doN
	resetRowOf(do.rowOf)
	for i := int32(0); i < d.doN; i++ {
		do.Placement[i] = d.doPlace[i]
		do.Visible[i] = d.doVis[i]
		do.Anim[i] = d.doAnim[i]
		do.Pos[i] = d.doPos[i]
		do.Facing[i] = d.doFace[i]
		do.Overrides[i] = d.doOv[i]
		do.Entity[i] = d.doE[i]
		do.rowOf[d.doE[i].Index()] = i
	}
	// byPlacement is derived: ascending placement order
	do.byPlacement = do.byPlacement[:d.doN]
	for i := int32(0); i < d.doN; i++ {
		do.byPlacement[i] = i
	}
	sort.Slice(do.byPlacement, func(x, y int) bool {
		return do.Placement[do.byPlacement[x]] < do.Placement[do.byPlacement[y]]
	})

	// economy (#300)
	if d.resourceCount > 0 {
		for pl := 0; pl < MaxPlayers; pl++ {
			copy(w.resources[pl], d.resources[pl])
		}
	}
	nd := w.Nodes
	nd.count = d.ndN
	resetRowOf(nd.rowOf)
	for i := int32(0); i < d.ndN; i++ {
		nd.Entity[i] = d.ndE[i]
		nd.Resource[i] = d.ndRes[i]
		nd.Remaining[i] = d.ndRem[i]
		nd.Flags[i] = d.ndFl[i]
		nd.Busy[i] = d.ndBusy[i]
		nd.rowOf[d.ndE[i].Index()] = i
	}
	ec := w.Econs
	ec.count = d.ecN
	resetRowOf(ec.rowOf)
	for pl := range w.foodUsed { // ledger is derived: recompute below
		w.foodUsed[pl] = 0
		w.foodCap[pl] = 0
	}
	for i := int32(0); i < d.ecN; i++ {
		ec.Entity[i] = d.ecE[i]
		ec.FoodCost[i] = d.ecCost[i]
		ec.FoodProvided[i] = d.ecProv[i]
		ec.DepotMask[i] = d.ecMask[i]
		ec.rowOf[d.ecE[i].Index()] = i
		if or := w.Owners.Row(d.ecE[i]); or != -1 {
			if pl := w.Owners.Player[or]; pl < MaxPlayers {
				w.foodUsed[pl] += int32(d.ecCost[i])
				w.foodCap[pl] += int32(d.ecProv[i])
			}
		}
	}
	hv := w.Harvests
	hv.count = d.hvN
	resetRowOf(hv.rowOf)
	for i := int32(0); i < d.hvN; i++ {
		hv.Entity[i] = d.hvE[i]
		hv.State[i] = d.hvState[i]
		hv.Node[i] = d.hvNode[i]
		hv.Depot[i] = d.hvDepot[i]
		hv.Carried[i] = d.hvCarr[i]
		hv.CarriedRes[i] = d.hvCRes[i]
		hv.Clock[i] = d.hvClock[i]
		hv.Capacity[i] = d.hvCap[i]
		hv.GatherTicks[i] = d.hvGT[i]
		hv.Mask[i] = d.hvMask[i]
		hv.rowOf[d.hvE[i].Index()] = i
	}
	pq := w.Produce
	pq.count = d.pqN
	resetRowOf(pq.rowOf)
	for i := int32(0); i < d.pqN; i++ {
		pq.Entity[i] = d.pqE[i]
		pq.Queue[i] = d.pqQ[i]
		pq.QFlags[i] = d.pqFl[i]
		pq.QCount[i] = d.pqCount[i]
		pq.Done[i] = d.pqDone[i]
		pq.RallyKind[i] = d.pqRK[i]
		pq.RallyEnt[i] = d.pqRE[i]
		pq.RallyPoint[i] = d.pqRP[i]
		pq.rowOf[d.pqE[i].Index()] = i
		// queued trains hold food reservations — fold them back into
		// the recomputed ledger (research rows hold no food)
		if or := w.Owners.Row(d.pqE[i]); or != -1 {
			if pl := w.Owners.Player[or]; pl < MaxPlayers {
				for q := 0; q < int(d.pqCount[i]); q++ {
					if d.pqFl[i][q]&TrainFlagResearch != 0 {
						continue
					}
					if d.pqFl[i][q]&TrainFlagHeroRevive != 0 {
						rec := &d.deadRec[pl][d.pqQ[i][q]]
						if rec.Used {
							w.foodUsed[pl] += int32(w.unitDefs[w.heroTables.Heroes[rec.HeroType].Unit].FoodCost)
						}
						continue
					}
					w.foodUsed[pl] += int32(w.unitDefs[d.pqQ[i][q]].FoodCost)
				}
			}
		}
	}
	if d.upgCount > 0 {
		for pl := 0; pl < MaxPlayers; pl++ {
			copy(w.upgradeLevel[pl], d.upgLevel[pl])
			copy(w.techMax[pl], d.upgMax[pl])
		}
	}
	hsr := w.Heroes
	hsr.count = d.hrN
	resetRowOf(hsr.rowOf)
	for i := int32(0); i < d.hrN; i++ {
		rec := &d.hrRec[i]
		hsr.Entity[i] = d.hrE[i]
		hsr.HeroType[i] = rec.HeroType
		hsr.XP[i] = rec.XP
		hsr.Level[i] = rec.Level
		hsr.Str[i] = rec.Str
		hsr.Agi[i] = rec.Agi
		hsr.Int[i] = rec.Int
		hsr.SkillPoints[i] = rec.SkillPoints
		hsr.SkillLevel[i] = rec.SkillLevel
		hsr.rowOf[d.hrE[i].Index()] = i
	}
	for pl := 0; pl < MaxPlayers; pl++ {
		w.deadHeroes[pl] = d.deadRec[pl]
	}

	its := w.Items
	its.count = d.itN
	resetRowOf(its.rowOf)
	for i := int32(0); i < d.itN; i++ {
		its.Entity[i] = d.itE[i]
		its.TypeID[i] = d.itType[i]
		its.Charges[i] = d.itCharges[i]
		its.Carrier[i] = d.itCarrier[i]
		its.rowOf[d.itE[i].Index()] = i
	}

	pas := w.Patrol
	pas.count = d.paN
	resetRowOf(pas.rowOf)
	for i := int32(0); i < d.paN; i++ {
		pas.Entity[i] = d.paE[i]
		pas.A[i] = d.paA[i]
		pas.B[i] = d.paB[i]
		pas.Flags[i] = d.paFlags[i]
		pas.rowOf[d.paE[i].Index()] = i
	}

	bds := w.Build
	bds.count = d.bdN
	resetRowOf(bds.rowOf)
	for i := int32(0); i < d.bdN; i++ {
		bds.Entity[i] = d.bdE[i]
		bds.Builder[i] = d.bdBldr[i]
		bds.FX[i] = d.bdFX[i]
		bds.FY[i] = d.bdFY[i]
		bds.FW[i] = d.bdFW[i]
		bds.Progress[i] = d.bdProg[i]
		bds.rowOf[d.bdE[i].Index()] = i
	}

	fx := w.Effects
	fx.count = d.efN
	resetRowOf(fx.rowOf)
	for i := int32(0); i < d.efN; i++ {
		fx.ModelID[i] = d.efModel[i]
		fx.Scale[i] = d.efScale[i]
		fx.ColorRGBA[i] = d.efColor[i]
		fx.BirthTick[i] = d.efBirth[i]
		fx.Entity[i] = d.efE[i]
		fx.rowOf[d.efE[i].Index()] = i
	}

	// subscriptions
	w.subs = d.subs

	// trigger slab (#456): swap in the decoded slots + free list. The store
	// cap is preserved (load validated count <= cap); the handler registry
	// was re-registered by setup before LoadState, so the action refs in
	// these slots resolve against it.
	w.Triggers.slots = d.trigSlots
	w.Triggers.free = d.trigFree

	// regions (#241): replace the store wholesale with the decoded slots,
	// then rebuild each region's membership presence set from the saved
	// in-order unit list (presence-row order preserved → hash matches).
	w.Regions.entries = d.regEntries
	w.Regions.free = d.regFree
	for i := range d.regMembers {
		mem := d.regMembers[i]
		if len(mem) == 0 {
			continue
		}
		e := &w.Regions.entries[i]
		m := w.Regions.membersForWrite(e)
		for _, u := range mem {
			m.set(u)
		}
	}

	// players (#218): roster + alliance restored wholesale (slot order
	// preserved → hash matches). Resources/food already applied via the
	// economy section.
	pr := &w.players
	for p := 0; p < MaxPlayers; p++ {
		pr.name[p] = d.plName[p]
		pr.controller[p] = d.plController[p]
		pr.race[p] = d.plRace[p]
		pr.color[p] = d.plColor[p]
		pr.team[p] = d.plTeam[p]
		pr.startX[p] = d.plStartX[p]
		pr.startY[p] = d.plStartY[p]
		pr.alliedVictory[p] = d.plAlliedVictory[p]
		pr.handicap[p] = d.plHandicap[p]
		pr.handicapDamage[p] = d.plHandicapDamage[p]
		pr.handicapXP[p] = d.plHandicapXP[p]
		pr.handicapReviveTime[p] = d.plHandicapReviveTime[p]
	}
	pr.alliance = d.plAlliance

	// heightfield (#371): restored wholesale (row-major sample order
	// preserved → hash matches).
	w.height = heightfield{
		cols: d.hfCols, rows: d.hfRows,
		originX: d.hfOriginX, originY: d.hfOriginY, cellSize: d.hfCell,
		samples: d.hfSamples,
	}

	// upkeep (#375): brackets, lost counters, tax matrix restored wholesale.
	w.upkeepCount = d.upCount
	w.upkeepFood = d.upFood
	w.upkeepRate = d.upRate
	w.upkeepLost = d.upLost
	w.taxRate = d.upTax

	// fog overrides (#243): toggles, modifier pool, free list, shares.
	w.fogDisabled = d.fogDisabled
	w.fogMaskDisabled = d.fogMaskDisabled
	fm := w.FogMods
	fm.count = d.fmCount
	fm.free = fm.free[:0]
	for i := int32(0); i < d.fmCount; i++ {
		fm.alive[i] = d.fmAlive[i]
		fm.gen[i] = d.fmGen[i]
		fm.player[i] = d.fmPlayer[i]
		fm.state[i] = d.fmState[i]
		fm.kind[i] = d.fmKind[i]
		fm.shared[i] = d.fmShared[i]
		fm.active[i] = d.fmActive[i]
		fm.ax[i], fm.ay[i], fm.bx[i], fm.by[i] = d.fmAX[i], d.fmAY[i], d.fmBX[i], d.fmBY[i]
	}
	fm.free = append(fm.free, d.fmFree...)
	sv := w.ShareVisions
	for i := range sv.rowOf {
		sv.rowOf[i] = -1
	}
	sv.count = 0
	for i := int32(0); i < int32(len(d.svEntity)); i++ {
		id := d.svEntity[i]
		sv.Mask[sv.count] = d.svMask[i]
		sv.Entity[sv.count] = id
		sv.rowOf[id.Index()] = sv.count
		sv.count++
	}

	// AI hooks (#257): per-player flags + command stacks restored wholesale.
	w.ai.difficulty = d.aiDifficulty
	w.ai.paused = d.aiPaused
	w.ai.attached = d.aiAttached
	for p := 0; p < MaxPlayers; p++ {
		w.ai.inbox[p] = w.ai.inbox[p][:0]
		w.ai.inbox[p] = append(w.ai.inbox[p], d.aiInbox[p]...)
	}

	// destructables (#229): restore rows in creation order; the pathing stamp
	// is re-derived in the grid rebuild below from blocks+footprint+dead.
	de := w.Destructables
	de.Type = de.Type[:0]
	de.Pos = de.Pos[:0]
	de.Facing = de.Facing[:0]
	de.Life = de.Life[:0]
	de.MaxLife = de.MaxLife[:0]
	de.Dead = de.Dead[:0]
	de.Invuln = de.Invuln[:0]
	de.Blocks = de.Blocks[:0]
	de.Footprint = de.Footprint[:0]
	de.Entity = de.Entity[:0]
	resetRowOf(de.rowOf)
	de.count = 0
	for i := int32(0); i < d.deN; i++ {
		de.Type = append(de.Type, d.deType[i])
		de.Pos = append(de.Pos, d.dePos[i])
		de.Facing = append(de.Facing, d.deFace[i])
		de.Life = append(de.Life, d.deLife[i])
		de.MaxLife = append(de.MaxLife, d.deMax[i])
		de.Dead = append(de.Dead, d.deDead[i])
		de.Invuln = append(de.Invuln, d.deInvul[i])
		de.Blocks = append(de.Blocks, d.deBlock[i])
		de.Footprint = append(de.Footprint, d.deFoot[i])
		de.Entity = append(de.Entity, d.deE[i])
		de.rowOf[d.deE[i].Index()] = i
		de.count++
	}

	// mid-tick buffers are clean by the save contract
	w.eventCount = 0
	w.killed = w.killed[:0]
	w.dmgBuf = w.dmgBuf[:0]

	// ---- derived rebuilds ----

	// spatial buckets, from transform rows (order-independent consumers)
	for i := range w.bucketHead {
		w.bucketHead[i] = -1
	}
	for i := range w.bucketNext {
		w.bucketNext[i] = -1
		w.bucketPrev[i] = -1
		w.bucketCell[i] = -1
		w.bucketID[i] = 0
	}
	for i := int32(0); i < t.count; i++ {
		w.bucketInsert(t.Entity[i], t.Pos[i])
	}

	// grid dynamic reservations, from Movement.ResCell
	if w.Grid != nil {
		for cell := range w.reservedBy {
			if w.reservedBy[cell] != 0 {
				w.Grid.ClearFlags(int32(cell)%path.GridSize, int32(cell)/path.GridSize, path.OccupiedDynamic)
				w.reservedBy[cell] = 0
			}
		}
		for i := int32(0); i < m.count; i++ {
			if cl := m.ResCell[i]; cl != -1 {
				w.reservedBy[cl] = m.Entity[i]
				w.Grid.OrFlags(cl%path.GridSize, cl/path.GridSize, path.OccupiedDynamic)
			}
		}
		// building footprints: OccupiedStatic is not in the saved grid
		// (the grid is re-baked from the map) — restamp from build rows
		// so placement queries see the same occupancy (#301).
		for i := int32(0); i < bds.count; i++ {
			w.Grid.StampStatic(path.Rect{X: bds.FX[i], Y: bds.FY[i], W: bds.FW[i], H: bds.FW[i]})
		}
		// destructable footprints: live (not-dead) blocking widgets re-stamp
		// the same static occupancy they held at save time (#229).
		for i := int32(0); i < de.count; i++ {
			if de.Blocks[i] && de.Footprint[i] > 0 && !de.Dead[i] {
				w.Grid.StampStatic(w.destructableRect(i))
			}
		}
	}

	// buff derived-stat cache: reset to identity, recompute per carrier
	for st := 0; st < int(data.BuffStatCount); st++ {
		for i := range w.buffAdd[st] {
			w.buffAdd[st][i] = 0
			w.buffMult[st][i] = fixed.One
		}
	}
	for i := range p.rows {
		if p.live[i] {
			w.recomputeBuffStats(p.rows[i].Target)
		}
	}
	// upgrade contributions live in the same cache: re-derive every
	// typed unit (idempotent over the buff pass above; #303)
	if len(w.upgradeDefs) > 0 {
		ut := w.UnitTypes
		for i := int32(0); i < ut.Count(); i++ {
			w.recomputeBuffStats(ut.Entity[i])
		}
	}
	// carried-item modifiers too: re-derive every carrier (#305 —
	// carriers may be untyped, the loops above can miss them)
	for i := int32(0); i < its.count; i++ {
		if c := its.Carrier[i]; c != 0 {
			w.recomputeBuffStats(c)
		}
	}
}
