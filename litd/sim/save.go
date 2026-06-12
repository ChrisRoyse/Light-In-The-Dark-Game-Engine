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
// v2: economy sections (#300) — resource counters, node/econ/harvest
// stores — appended after doodads.
const SaveFormatVersion uint32 = 2

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
	s := &saveWriter{w: out}
	if _, err := io.WriteString(out, SaveMagic); err != nil {
		return err
	}
	s.u32(SaveFormatVersion)
	s.u64(fingerprint)
	s.u32(uint32(w.caps.Units))
	s.u32(uint32(w.caps.Projectiles))
	s.u32(uint32(w.caps.BuffInstances))
	s.u32(uint32(w.caps.OrderQueueEntries))
	s.u32(uint32(w.caps.PendingEvents))
	s.u32(uint32(w.caps.PathRequests))
	s.u32(uint32(w.caps.ScriptedDoodads))
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

	// inventories
	in := w.Invents
	s.u32(uint32(in.count))
	for i := int32(0); i < in.count; i++ {
		s.ent(in.Entity[i])
		for sl := 0; sl < InventorySlots; sl++ {
			s.ent(in.Slots[i][sl])
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
		s.f64(ms.Arc[i])
		s.u8(ms.Flags[i])
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

	return s.err
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

	hlN     int32
	hlE     []EntityID
	hlLife  []fixed.F64
	hlMax   []fixed.F64
	hlRegen []fixed.F64
	hlAV    []int16
	hlAT    []uint8
	hlDS    []uint8
	hlDecay []uint32

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

	msN     int32
	msE     []EntityID
	msSpeed []fixed.F64
	msArc   []fixed.F64
	msFlags []uint8
	msGE    []EntityID
	msGP    []fixed.Vec2
	msPay   []data.EffectList
	msPkt   []DamagePacket
	msSrc   []EntityID
	msBirth []uint32

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

	schedBlob []byte

	subs []kindSubs
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
		Units:             int(r.u32()),
		Projectiles:       int(r.u32()),
		BuffInstances:     int(r.u32()),
		OrderQueueEntries: int(r.u32()),
		PendingEvents:     int(r.u32()),
		PathRequests:      int(r.u32()),
		ScriptedDoodads:   int(r.u32()),
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
	for i := int32(0); i < n; i++ {
		d.hlE[i] = r.ent()
		d.hlLife[i] = r.f64()
		d.hlMax[i] = r.f64()
		d.hlRegen[i] = r.f64()
		d.hlAV[i] = int16(r.u16())
		d.hlAT[i] = r.u8()
		d.hlDS[i] = r.u8()
		d.hlDecay[i] = r.u32()
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
	for i := int32(0); i < n; i++ {
		d.inE[i] = r.ent()
		for sl := 0; sl < InventorySlots; sl++ {
			d.inSlots[i][sl] = r.ent()
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
	d.msArc = make([]fixed.F64, n)
	d.msFlags = make([]uint8, n)
	d.msGE = make([]EntityID, n)
	d.msGP = make([]fixed.Vec2, n)
	d.msPay = make([]data.EffectList, n)
	d.msPkt = make([]DamagePacket, n)
	d.msSrc = make([]EntityID, n)
	d.msBirth = make([]uint32, n)
	for i := int32(0); i < n; i++ {
		d.msE[i] = r.ent()
		d.msSpeed[i] = r.f64()
		d.msArc[i] = r.f64()
		d.msFlags[i] = r.u8()
		d.msGE[i] = r.ent()
		d.msGP[i] = r.vec2()
		d.msPay[i] = data.EffectList{Off: r.u16(), Len: r.u16()}
		d.msPkt[i] = DamagePacket{Source: r.ent(), Target: r.ent(), Amount: r.f64(), AttackType: r.u8()}
		d.msSrc[i] = r.ent()
		d.msBirth[i] = r.u32()
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
	return r.err
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
	} {
		if err := check(c.name, c.ents); err != nil {
			return err
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

	// subscriptions: every handler must already be registered
	for i := range d.subs {
		for _, id := range d.subs[i].list {
			if _, ok := w.handlers[id]; !ok {
				return fmt.Errorf("sim: save: subscription for kind %d references unregistered HandlerID %d", d.subs[i].kind, id)
			}
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

	in := w.Invents
	in.count = d.inN
	resetRowOf(in.rowOf)
	for i := int32(0); i < d.inN; i++ {
		in.Entity[i] = d.inE[i]
		in.Slots[i] = d.inSlots[i]
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
		ms.Arc[i] = d.msArc[i]
		ms.Flags[i] = d.msFlags[i]
		ms.GuideEnt[i] = d.msGE[i]
		ms.GuidePt[i] = d.msGP[i]
		ms.Payload[i] = d.msPay[i]
		ms.Packet[i] = d.msPkt[i]
		ms.Source[i] = d.msSrc[i]
		ms.BirthTick[i] = d.msBirth[i]
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

	// subscriptions
	w.subs = d.subs

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
}
