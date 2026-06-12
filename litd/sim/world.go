package sim

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// Caps fixes every pool capacity for a match. The zero value of a
// field means "engine default". A map header may LOWER a cap, never
// raise it: NewWorld clamps every request to the engine ceiling
// (ecs-architecture.md §2, R-GC-2).
type Caps struct {
	Units             int
	Projectiles       int
	BuffInstances     int
	OrderQueueEntries int
	PendingEvents     int // per-tick ring
	PathRequests      int // in flight
	ScriptedDoodads   int
}

// EngineCaps are the engine ceilings — the §2 pool table, exactly.
var EngineCaps = Caps{
	Units:             4000,
	Projectiles:       2000,
	BuffInstances:     8000,
	OrderQueueEntries: 16000,
	PendingEvents:     4096,
	PathRequests:      512,
	ScriptedDoodads:   1024,
}

func clampCap(requested, ceiling int) int {
	if requested <= 0 || requested > ceiling {
		return ceiling
	}
	return requested
}

// resolve clamps every requested cap to the engine ceiling.
func (c Caps) resolve() Caps {
	return Caps{
		Units:             clampCap(c.Units, EngineCaps.Units),
		Projectiles:       clampCap(c.Projectiles, EngineCaps.Projectiles),
		BuffInstances:     clampCap(c.BuffInstances, EngineCaps.BuffInstances),
		OrderQueueEntries: clampCap(c.OrderQueueEntries, EngineCaps.OrderQueueEntries),
		PendingEvents:     clampCap(c.PendingEvents, EngineCaps.PendingEvents),
		PathRequests:      clampCap(c.PathRequests, EngineCaps.PathRequests),
		ScriptedDoodads:   clampCap(c.ScriptedDoodads, EngineCaps.ScriptedDoodads),
	}
}

// Pool element shapes. These rows will grow fields as their systems
// land (orders #144, buffs #162, events #88, pathing #110, doodads
// D-13); the capacity discipline and backing arrays land here.

type orderEntry struct {
	next   int32 // pooled free-list / queue link
	kind   uint8
	target EntityID
	point  fixed.Vec2
}

type pathRequest struct {
	unit  EntityID
	goal  fixed.Vec2
	state uint8
}

// World owns every store and pool of one match. All capacities are
// fixed by NewWorld — `make` runs exactly once per pool at map load
// and nothing here ever reallocates mid-match (R-GC-2). Exhaustion is
// a gameplay outcome: creation fails, like WC3 refusing past its
// handle limits.
type World struct {
	caps Caps
	tick uint32

	Ents       *Entities
	Transforms *TransformStore
	Movements  *MovementStore
	Collisions *CollisionStore
	Healths    *HealthStore
	Owners     *OwnerStore
	UnitTypes  *UnitTypeStore
	Combats    *CombatStore
	Abilities  *AbilityStore
	Invents    *InventoryStore
	Orders     *OrderStore
	Buffs      *BuffPool
	Projs      *ProjectilePool
	Doodads    *DoodadStore
	Paths      *path.PathStore // pooled per-unit waypoint buffers (§7)
	Grid       *path.Grid      // pathing grid; nil until SetGrid (map load)
	// local avoidance (avoidance.go): cell → owning entity, plus the
	// stall threshold override (0 = DefaultStallRepathTicks)
	reservedBy  []EntityID
	stallRepath uint16
	// smart-order resolution (smartorder.go): data table + TypeID →
	// capability-class column
	smart           *data.SmartTable
	unitClassByType []uint8
	// compiled effect-composition arena (effect.go, ADR #294);
	// installed by BindEffects, read-only thereafter
	effects []data.CompiledEffect
	// spatial bucket grid (buckets.go) — derived from Transform
	// positions, excluded from the state hash
	bucketHead []int32
	bucketNext []int32
	bucketPrev []int32
	bucketCell []int32
	bucketID   []EntityID
	// acquisition scan throttle in ticks (acquire.go, §3.1 default 5)
	acquireEvery uint16
	Sched        *sched.Scheduler // phase-2 script scheduler, lockstep with tick

	// Cmds is the binary command-record front door (command.go):
	// Stage any time (mutex), driver ingests between ticks, phase 1
	// validates and applies in (Tick, Player, Seq) order.
	Cmds *CommandQueue
	// scratch actor list for phase-1 validation (no per-record alloc)
	cmdActors   [MaxCommandUnits]EntityID
	cmdApplied  uint64
	cmdRejected uint64

	// double-buffered command staging (step.go): enqueue any time,
	// applied at the NEXT tick's phase 1
	cmdStaging []WorldCommand
	cmdActive  []WorldCommand
	// deferred kills: marked phase 5, events phase 6, removed phase 7
	killed []EntityID
	// deferred damage (damage.go #152): queued during phase 5, ONE
	// apply pass at combat-phase end; drops are counted, never silent
	dmgBuf     []DamagePacket
	dmgDropped uint32
	coeff      [][]int32 // per-mille attack×armor matrix (BindDamageMatrix)
	// the sim PRNG (R-SIM-2): every gameplay roll draws here, one
	// deterministic call order; reseeded per match via SetSeed
	rng *prng.Stream

	// render snapshot publication (snapshot.go): double buffer plus
	// per-tick discontinuity marks and staged presentation cues
	Snaps           *SnapshotBuffers
	snapNoLerp      []bool
	snapDeath       []bool
	snapMarked      []EntityID
	renderEvStaging []RenderEvent
	renderEvDropped uint64

	// Debug/integration hooks; nil-safe stubs until their issues land.
	PhaseTrace func(tick uint32, phase int, name string)
	OnCommand  func(tick uint32, c WorldCommand)
	// OnCommandRecord fires for every VALIDATED record in phase 1
	// with the surviving (alive + owned) actor list.
	OnCommandRecord func(tick uint32, r *CommandRecord, actors []EntityID)
	// OnShove fires when a mover displaces an idle occupant to cell
	// (avoidance.go §5 — the decision log for FSV).
	OnShove       func(tick uint32, mover, shoved EntityID, cell int32)
	OnScriptPhase func(tick uint32)
	OnCombatPhase func(tick uint32)
	// OnAttackTransition fires on every per-weapon state flip
	// (attack.go #150 — the tick-stamped trace that is the FSV SoT).
	OnAttackTransition func(tick uint32, id EntityID, slot int, from, to uint8)
	OnDeathEvent       func(tick uint32, id EntityID)
	OnSnapshot         func(tick uint32)
	OnHash             func(tick uint32)
	OnEventDrop        func(tick uint32, e Event)
	HashEvery          uint32

	unitCount int
	// pooled intrusive order-queue entries (orders.go): LIFO free
	// list threaded through orderEntry.next
	orderPool      []orderEntry
	orderFreeHead  int32
	orderFreeCount int32
	events         []Event // per-tick pending ring (events.go)
	eventCount     int
	eventsDropped  uint64
	handlers       map[HandlerID]EventHandler // registry: lookup only, never iterated
	subs           []kindSubs                 // kind-sorted, registration-ordered lists
	pathReqs       []pathRequest
}

// NewWorld allocates every pool at the resolved capacities. The
// entity table covers everything that can hold an EntityID: units,
// projectiles, and promoted doodads.
func NewWorld(requested Caps) *World {
	caps := requested.resolve()
	entityCap := caps.Units + caps.Projectiles + caps.ScriptedDoodads
	// entity indices run 1..entityCap (slot 0 reserved, entity.go) —
	// every array indexed by EntityID.Index() needs entityCap+1 room
	idxSpace := entityCap + 1
	w := &World{
		caps:            caps,
		Cmds:            newCommandQueue(),
		cmdStaging:      make([]WorldCommand, 0, 1024),
		cmdActive:       make([]WorldCommand, 0, 1024),
		killed:          make([]EntityID, 0, caps.Units),
		dmgBuf:          make([]DamagePacket, 0, caps.Units*4),
		rng:             prng.New(0, 0),
		Snaps:           newSnapshotBuffers(idxSpace, caps.PendingEvents),
		snapNoLerp:      make([]bool, idxSpace),
		snapDeath:       make([]bool, idxSpace),
		snapMarked:      make([]EntityID, 0, idxSpace),
		renderEvStaging: make([]RenderEvent, 0, caps.PendingEvents),
		Sched:           sched.New(),
		Ents:            NewEntities(entityCap),
		Transforms:      NewTransformStore(entityCap, idxSpace),
		Movements:       NewMovementStore(caps.Units, idxSpace),
		Collisions:      NewCollisionStore(caps.Units, idxSpace),
		Healths:         NewHealthStore(caps.Units, idxSpace),
		Owners:          NewOwnerStore(caps.Units, idxSpace),
		UnitTypes:       NewUnitTypeStore(caps.Units, idxSpace),
		Combats:         NewCombatStore(caps.Units, idxSpace),
		Abilities:       NewAbilityStore(caps.Units, idxSpace),
		Invents:         NewInventoryStore(caps.Units, idxSpace),
		Orders:          NewOrderStore(caps.Units, idxSpace),
		Buffs:           NewBuffPool(caps.BuffInstances),
		Projs:           NewProjectilePool(caps.Projectiles),
		orderPool:       make([]orderEntry, caps.OrderQueueEntries),
		events:          make([]Event, caps.PendingEvents),
		handlers:        make(map[HandlerID]EventHandler),
		pathReqs:        make([]pathRequest, caps.PathRequests),
		Doodads:         NewDoodadStore(caps.ScriptedDoodads, idxSpace),
		Paths:           path.NewPathStore(caps.PathRequests, 1024),
		bucketHead:      make([]int32, bucketCount),
		bucketNext:      make([]int32, idxSpace),
		bucketPrev:      make([]int32, idxSpace),
		bucketCell:      make([]int32, idxSpace),
		bucketID:        make([]EntityID, idxSpace),
		acquireEvery:    DefaultAcquireInterval,
	}
	for i := range w.orderPool {
		w.orderPool[i].next = int32(i) + 1
	}
	w.orderPool[len(w.orderPool)-1].next = -1
	w.orderFreeHead = 0
	w.orderFreeCount = int32(len(w.orderPool))
	for i := range w.bucketHead {
		w.bucketHead[i] = -1
	}
	for i := range w.bucketCell {
		w.bucketCell[i] = -1
	}
	return w
}

// Caps returns the resolved (effective) capacities.
func (w *World) Caps() Caps { return w.caps }

// UnitCount returns the number of live units.
func (w *World) UnitCount() int { return w.unitCount }

// CreateUnit spawns a unit entity with a Transform. Fails (gameplay
// outcome) when the unit cap is reached — never reallocates.
func (w *World) CreateUnit(pos fixed.Vec2, facing fixed.Angle) (EntityID, bool) {
	if w.unitCount >= w.caps.Units {
		return 0, false
	}
	id, ok := w.Ents.Create()
	if !ok {
		return 0, false
	}
	if !w.Transforms.Add(w.Ents, id, pos, facing) {
		w.Ents.Destroy(id)
		return 0, false
	}
	w.bucketInsert(id, pos) // spatial bucket membership (buckets.go)
	w.unitCount++
	w.MarkSnap(id) // spawn discontinuity: render must not lerp from nowhere
	return id, true
}

// DestroyUnit removes a unit and every component row it holds. Stale
// handles are no-ops (R-API-5). Presence-checked removals: absent
// optional components are not contract violations, so no assert fires.
func (w *World) DestroyUnit(id EntityID) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	w.bucketRemove(id)
	w.Transforms.Remove(id)
	if r := w.Movements.Row(id); r != -1 {
		w.releaseReservation(r, id) // free the avoidance cell (no leak)
		w.Movements.Remove(id)
	}
	if w.Collisions.Row(id) != -1 {
		w.Collisions.Remove(id)
	}
	if w.Healths.Row(id) != -1 {
		w.Healths.Remove(id)
	}
	if w.Owners.Row(id) != -1 {
		w.Owners.Remove(id)
	}
	if w.UnitTypes.Row(id) != -1 {
		w.UnitTypes.Remove(id)
	}
	if w.Combats.Row(id) != -1 {
		w.Combats.Remove(id)
	}
	if w.Abilities.Row(id) != -1 {
		w.Abilities.Remove(id)
	}
	if w.Invents.Row(id) != -1 {
		w.Invents.Remove(id)
	}
	if r := w.Orders.Row(id); r != -1 {
		w.clearOrderQueue(r) // recycle pooled entries (no leak)
		w.Orders.Remove(id)
	}
	if !w.Ents.Destroy(id) {
		return false
	}
	w.unitCount--
	return true
}

// PreallocatedBytes reports the total bytes held by the fixed pools —
// the ecs §5.1 sanity number printed at load in debug builds.
func (w *World) PreallocatedBytes() int {
	const (
		slotB  = 8  // entitySlot
		vec2B  = 16 // fixed.Vec2
		rowOfB = 4
		// per-row column-byte sums of the wide stores
		combatRowB  = 8 + 2 + 2 + 2 + 4 + 4 + 16 + 4 + 8 + 8 + 4 + 4 + 4 + 4
		abilityRowB = AbilitySlots*(2+1+4+2+1) + 4
	)
	n := 0
	n += len(w.Ents.slots) * slotB
	n += len(w.Transforms.Pos) * vec2B
	n += len(w.Transforms.Facing) * 2
	n += len(w.Transforms.Entity) * 4
	n += len(w.Transforms.rowOf) * rowOfB
	n += len(w.Movements.Speed) * (8 + 2 + 16 + 4 + 4 + 2 + 4 + 1 + 4) // per-row column bytes
	n += len(w.Movements.rowOf) * rowOfB
	n += len(w.Collisions.SizeClass) * (1 + 1 + 4 + 4)
	n += len(w.Collisions.rowOf) * rowOfB
	n += len(w.Healths.Life) * (8 + 8 + 8 + 2 + 1 + 1 + 4 + 4)
	n += len(w.Healths.rowOf) * rowOfB
	n += len(w.Owners.Player) * (1 + 1 + 1 + 4)
	n += len(w.Owners.rowOf) * rowOfB
	n += len(w.UnitTypes.TypeID) * (2 + 4)
	n += len(w.UnitTypes.rowOf) * rowOfB
	n += len(w.Combats.DmgBase) * combatRowB
	n += len(w.Combats.rowOf) * rowOfB
	n += len(w.Abilities.AbilityID) * abilityRowB
	n += len(w.Abilities.rowOf) * rowOfB
	n += len(w.Invents.Slots) * (InventorySlots*4 + 4)
	n += len(w.Invents.rowOf) * rowOfB
	n += len(w.Orders.Kind) * (1 + 1 + 4 + 16 + 4 + 4)
	n += len(w.Orders.rowOf) * rowOfB
	n += w.Projs.Cap() * 96 // ProjectileInstance + free/live bookkeeping
	n += w.Buffs.Cap() * 24 // BuffInstance + free/live bookkeeping
	n += len(w.orderPool) * 32
	n += len(w.events) * 24
	n += len(w.pathReqs) * 24
	n += int(0) + len(w.Doodads.Placement)*32
	return n
}
