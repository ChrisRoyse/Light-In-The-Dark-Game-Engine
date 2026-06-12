package sim

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
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

type doodadRow struct {
	placement int32
	visible   bool
	anim      uint16
	pos       fixed.Vec2
	facing    fixed.Angle
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
	Sched      *sched.Scheduler // phase-2 script scheduler, lockstep with tick

	// double-buffered command staging (step.go): enqueue any time,
	// applied at the NEXT tick's phase 1
	cmdStaging []WorldCommand
	cmdActive  []WorldCommand
	// deferred kills: marked phase 5, events phase 6, removed phase 7
	killed []EntityID

	// Debug/integration hooks; nil-safe stubs until their issues land.
	PhaseTrace    func(tick uint32, phase int, name string)
	OnCommand     func(tick uint32, c WorldCommand)
	OnScriptPhase func(tick uint32)
	OnCombatPhase func(tick uint32)
	OnDeathEvent  func(tick uint32, id EntityID)
	OnSnapshot    func(tick uint32)
	OnHash        func(tick uint32)
	OnEventDrop   func(tick uint32, e Event)
	HashEvery     uint32

	unitCount     int
	orderPool     []orderEntry
	events        []Event // per-tick pending ring (events.go)
	eventCount    int
	eventsDropped uint64
	handlers      map[HandlerID]EventHandler // registry: lookup only, never iterated
	subs          []kindSubs                 // kind-sorted, registration-ordered lists
	pathReqs      []pathRequest
	doodads       []doodadRow
	doodadCount   int
}

// NewWorld allocates every pool at the resolved capacities. The
// entity table covers everything that can hold an EntityID: units,
// projectiles, and promoted doodads.
func NewWorld(requested Caps) *World {
	caps := requested.resolve()
	entityCap := caps.Units + caps.Projectiles + caps.ScriptedDoodads
	return &World{
		caps:       caps,
		cmdStaging: make([]WorldCommand, 0, 1024),
		cmdActive:  make([]WorldCommand, 0, 1024),
		killed:     make([]EntityID, 0, caps.Units),
		Sched:      sched.New(),
		Ents:       NewEntities(entityCap),
		Transforms: NewTransformStore(entityCap, entityCap),
		Movements:  NewMovementStore(caps.Units, entityCap),
		Collisions: NewCollisionStore(caps.Units, entityCap),
		Healths:    NewHealthStore(caps.Units, entityCap),
		Owners:     NewOwnerStore(caps.Units, entityCap),
		UnitTypes:  NewUnitTypeStore(caps.Units, entityCap),
		Combats:    NewCombatStore(caps.Units, entityCap),
		Abilities:  NewAbilityStore(caps.Units, entityCap),
		Invents:    NewInventoryStore(caps.Units, entityCap),
		Orders:     NewOrderStore(caps.Units, entityCap),
		Buffs:      NewBuffPool(caps.BuffInstances),
		Projs:      NewProjectilePool(caps.Projectiles),
		orderPool:  make([]orderEntry, caps.OrderQueueEntries),
		events:     make([]Event, caps.PendingEvents),
		handlers:   make(map[HandlerID]EventHandler),
		pathReqs:   make([]pathRequest, caps.PathRequests),
		doodads:    make([]doodadRow, caps.ScriptedDoodads),
	}
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
	w.unitCount++
	return id, true
}

// DestroyUnit removes a unit and every component row it holds. Stale
// handles are no-ops (R-API-5). Presence-checked removals: absent
// optional components are not contract violations, so no assert fires.
func (w *World) DestroyUnit(id EntityID) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	w.Transforms.Remove(id)
	if w.Movements.Row(id) != -1 {
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
	if w.Orders.Row(id) != -1 {
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
	n += len(w.Movements.Speed) * (8 + 2 + 16 + 4 + 1 + 4) // per-row column bytes
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
	n += len(w.Orders.Kind) * (1 + 4 + 16 + 4 + 4)
	n += len(w.Orders.rowOf) * rowOfB
	n += w.Projs.Cap() * 96 // ProjectileInstance + free/live bookkeeping
	n += w.Buffs.Cap() * 24 // BuffInstance + free/live bookkeeping
	n += len(w.orderPool) * 32
	n += len(w.events) * 24
	n += len(w.pathReqs) * 24
	n += len(w.doodads) * 32
	return n
}
