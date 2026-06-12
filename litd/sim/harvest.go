package sim

// Worker resource economy (#300, combat-and-orders.md §2.2): the
// harvest cycle state machine, resource-node entities, per-player
// resource counters, and food/supply accounting.
//
// The cycle: MOVE_TO_NODE → GATHER(ticks) → RETURN → DEPOSIT →
// repeat. Gather ticks, carry capacity, and the harvestable-resource
// mask come from the unit table (data.HarvestSpec); node type/amount/
// exclusivity from the node table (data.ResourceNodeType). Node
// exhaustion kills the node entity and emits EvResourceDepleted;
// workers re-select the nearest surviving node of the same resource
// by the §3.2 tuple (distSq, entityIndex). Deposit requires an own
// depot (DepotMask bit for the carried resource) and emits
// EvResourceDeposited in flush order.
//
// Food is provided/consumed per unit table and lives in per-player
// counters maintained by EconStore add/remove; the cap is enforced
// at ADMISSION (CanAddFood — #302's train check), never by killing
// retroactively.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// MaxPlayers bounds the per-player counter tables.
const MaxPlayers = 16

// Built-in economy events (#300).
const (
	// EvResourceDeposited: Src = worker, Dst = depot,
	// Arg = amount<<8 | resource index.
	EvResourceDeposited uint16 = 9
	// EvResourceDepleted: Src = the exhausted node (still alive at
	// dispatch — it dies in phase 7), Arg = resource index.
	EvResourceDepleted uint16 = 10
)

// OrderHarvest drives the harvest cycle; Target = the resource node.
// (Order kinds live in store_order.go; this one is #300's.)
const OrderHarvest uint8 = 6

// Harvest cycle states (HarvestStore.State).
const (
	HIdle   uint8 = 0 // no cycle running (carried amount may persist)
	HToNode uint8 = 1
	HGather uint8 = 2
	HReturn uint8 = 3
)

// NodeExclusive: one gatherer at a time (table flag).
const NodeExclusive uint8 = 1 << 0

// HarvestRange is how close a worker must be to gather or deposit
// (world units, squared test).
var HarvestRange = fixed.FromInt(6)

// ---- resource node store ----

type ResourceNodeStore struct {
	Resource  []uint8
	Remaining []int64
	Flags     []uint8
	Busy      []EntityID // exclusive nodes: current gatherer (0 = free)
	Entity    []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewResourceNodeStore(rowCap, entityCap int) *ResourceNodeStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &ResourceNodeStore{
		Resource:  make([]uint8, rowCap),
		Remaining: make([]int64, rowCap),
		Flags:     make([]uint8, rowCap),
		Busy:      make([]EntityID, rowCap),
		Entity:    make([]EntityID, rowCap),
		rowOf:     make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *ResourceNodeStore) Add(e *Entities, id EntityID, resource uint8, remaining int64, flags uint8) bool {
	if !e.Alive(id) || s.rowOf[id.Index()] != -1 || int(s.count) == len(s.Resource) {
		return false
	}
	r := s.count
	s.Resource[r] = resource
	s.Remaining[r] = remaining
	s.Flags[r] = flags
	s.Busy[r] = 0
	s.Entity[r] = id
	s.rowOf[id.Index()] = r
	s.count++
	return true
}

func (s *ResourceNodeStore) Remove(id EntityID) bool {
	r := s.rowOf[id.Index()]
	if r == -1 {
		return false
	}
	last := s.count - 1
	s.Resource[r] = s.Resource[last]
	s.Remaining[r] = s.Remaining[last]
	s.Flags[r] = s.Flags[last]
	s.Busy[r] = s.Busy[last]
	s.Entity[r] = s.Entity[last]
	s.rowOf[s.Entity[r].Index()] = r
	s.rowOf[id.Index()] = -1
	s.count--
	return true
}

func (s *ResourceNodeStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}
func (s *ResourceNodeStore) Count() int32 { return s.count }

// ---- econ (food + depot) store ----

type EconStore struct {
	FoodCost     []uint8
	FoodProvided []uint8
	DepotMask    []uint16
	Entity       []EntityID

	rowOf []int32
	count int32
}

func NewEconStore(rowCap, entityCap int) *EconStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &EconStore{
		FoodCost:     make([]uint8, rowCap),
		FoodProvided: make([]uint8, rowCap),
		DepotMask:    make([]uint16, rowCap),
		Entity:       make([]EntityID, rowCap),
		rowOf:        make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *EconStore) add(e *Entities, id EntityID, cost, provided uint8, depot uint16) bool {
	if !e.Alive(id) || s.rowOf[id.Index()] != -1 || int(s.count) == len(s.FoodCost) {
		return false
	}
	r := s.count
	s.FoodCost[r] = cost
	s.FoodProvided[r] = provided
	s.DepotMask[r] = depot
	s.Entity[r] = id
	s.rowOf[id.Index()] = r
	s.count++
	return true
}

func (s *EconStore) remove(id EntityID) (cost, provided uint8, ok bool) {
	r := s.rowOf[id.Index()]
	if r == -1 {
		return 0, 0, false
	}
	cost, provided = s.FoodCost[r], s.FoodProvided[r]
	last := s.count - 1
	s.FoodCost[r] = s.FoodCost[last]
	s.FoodProvided[r] = s.FoodProvided[last]
	s.DepotMask[r] = s.DepotMask[last]
	s.Entity[r] = s.Entity[last]
	s.rowOf[s.Entity[r].Index()] = r
	s.rowOf[id.Index()] = -1
	s.count--
	return cost, provided, true
}

func (s *EconStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}
func (s *EconStore) Count() int32 { return s.count }

// ---- harvest (worker) store ----

type HarvestStore struct {
	State       []uint8
	Node        []EntityID
	Depot       []EntityID
	Carried     []int32
	CarriedRes  []uint8
	Clock       []uint32 // gather completes at this tick
	Capacity    []int32
	GatherTicks []uint16
	Mask        []uint16
	Entity      []EntityID

	rowOf []int32
	count int32
}

func NewHarvestStore(rowCap, entityCap int) *HarvestStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &HarvestStore{
		State:       make([]uint8, rowCap),
		Node:        make([]EntityID, rowCap),
		Depot:       make([]EntityID, rowCap),
		Carried:     make([]int32, rowCap),
		CarriedRes:  make([]uint8, rowCap),
		Clock:       make([]uint32, rowCap),
		Capacity:    make([]int32, rowCap),
		GatherTicks: make([]uint16, rowCap),
		Mask:        make([]uint16, rowCap),
		Entity:      make([]EntityID, rowCap),
		rowOf:       make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *HarvestStore) Add(e *Entities, id EntityID, spec *data.HarvestSpec) bool {
	if !e.Alive(id) || s.rowOf[id.Index()] != -1 || int(s.count) == len(s.State) ||
		spec == nil || spec.Capacity <= 0 || spec.GatherTicks == 0 || spec.Mask == 0 {
		return false
	}
	r := s.count
	s.State[r] = HIdle
	s.Node[r] = 0
	s.Depot[r] = 0
	s.Carried[r] = 0
	s.CarriedRes[r] = 0
	s.Clock[r] = 0
	s.Capacity[r] = spec.Capacity
	s.GatherTicks[r] = spec.GatherTicks
	s.Mask[r] = spec.Mask
	s.Entity[r] = id
	s.rowOf[id.Index()] = r
	s.count++
	return true
}

func (s *HarvestStore) Remove(id EntityID) bool {
	r := s.rowOf[id.Index()]
	if r == -1 {
		return false
	}
	last := s.count - 1
	s.State[r] = s.State[last]
	s.Node[r] = s.Node[last]
	s.Depot[r] = s.Depot[last]
	s.Carried[r] = s.Carried[last]
	s.CarriedRes[r] = s.CarriedRes[last]
	s.Clock[r] = s.Clock[last]
	s.Capacity[r] = s.Capacity[last]
	s.GatherTicks[r] = s.GatherTicks[last]
	s.Mask[r] = s.Mask[last]
	s.Entity[r] = s.Entity[last]
	s.rowOf[s.Entity[r].Index()] = r
	s.rowOf[id.Index()] = -1
	s.count--
	return true
}

func (s *HarvestStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}
func (s *HarvestStore) Count() int32 { return s.count }

// ---- world surface ----

// BindEconomy sizes the per-player resource counters to the loaded
// resource-type registry. Must run before any node spawns or deposit;
// rebinding with a different count is refused (counter identity is
// state).
func (w *World) BindEconomy(resourceTypes int) bool {
	if resourceTypes <= 0 || resourceTypes > data.MaxResourceTypes {
		return false
	}
	if w.resourceCount != 0 && w.resourceCount != resourceTypes {
		return false
	}
	if w.resourceCount == resourceTypes {
		return true
	}
	w.resourceCount = resourceTypes
	w.resources = make([][]int64, MaxPlayers)
	for p := range w.resources {
		w.resources[p] = make([]int64, resourceTypes)
	}
	return true
}

// CreateResourceNode spawns a node entity from its table row.
func (w *World) CreateResourceNode(pos fixed.Vec2, nt *data.ResourceNodeType) (EntityID, bool) {
	if w.resourceCount == 0 || int(nt.Resource) >= w.resourceCount {
		return 0, false
	}
	flags := uint8(0)
	if nt.Exclusive {
		flags |= NodeExclusive
	}
	id, ok := w.CreateUnit(pos, 0)
	if !ok {
		return 0, false
	}
	if !w.Nodes.Add(w.Ents, id, nt.Resource, nt.Amount, flags) {
		w.DestroyUnit(id)
		return 0, false
	}
	return id, true
}

// AddEcon attaches the food/depot row of a unit (table values) and
// updates its owner's food counters. Requires an Owner component.
func (w *World) AddEcon(id EntityID, foodCost, foodProvided uint8, depotMask uint16) bool {
	or := w.Owners.Row(id)
	if or == -1 {
		return false
	}
	p := w.Owners.Player[or]
	if p >= MaxPlayers || !w.Econs.add(w.Ents, id, foodCost, foodProvided, depotMask) {
		return false
	}
	w.foodUsed[p] += int32(foodCost)
	w.foodCap[p] += int32(foodProvided)
	return true
}

// Resources reads a player's counter for one resource index.
func (w *World) Resources(player uint8, resource int) int64 {
	if player >= MaxPlayers || resource < 0 || resource >= w.resourceCount {
		return 0
	}
	return w.resources[player][resource]
}

// FoodUsed / FoodCap read a player's supply ledger.
func (w *World) FoodUsed(player uint8) int32 {
	if player >= MaxPlayers {
		return 0
	}
	return w.foodUsed[player]
}
func (w *World) FoodCap(player uint8) int32 {
	if player >= MaxPlayers {
		return 0
	}
	return w.foodCap[player]
}

// CanAddFood is the ADMISSION check (#302's train/build gate): true
// when cost more food fits under the player's cap. The cap is never
// enforced retroactively.
func (w *World) CanAddFood(player uint8, cost uint8) bool {
	if player >= MaxPlayers {
		return false
	}
	return w.foodUsed[player]+int32(cost) <= w.foodCap[player]
}

// ---- the cycle (driven from ordersSystem, phase 3) ----

// driveHarvest advances one worker's OrderHarvest. Order row r,
// worker id.
func (w *World) driveHarvest(r int32, id EntityID) {
	s := w.Orders
	hr := w.Harvests.Row(id)
	if hr == -1 {
		w.completeOrder(r, id, false) // not a harvester
		return
	}
	h := w.Harvests
	if s.Phase[r] == orderFresh {
		node := s.Target[r]
		nr := w.Nodes.Row(node)
		if !w.Ents.Alive(node) || nr == -1 || h.Mask[hr]&(1<<uint(w.Nodes.Resource[nr])) == 0 {
			w.completeOrder(r, id, false)
			return
		}
		if !w.startLeg(id, node) {
			w.completeOrder(r, id, false)
			return
		}
		h.Node[hr] = node
		h.State[hr] = HToNode
		s.Phase[r] = orderRunning
		return
	}
	switch h.State[hr] {
	case HToNode:
		node := h.Node[hr]
		nr := w.Nodes.Row(node)
		if nr == -1 { // node died en route: re-select or finish
			if !w.retargetNode(hr, r, id) {
				w.finishCycle(hr, r, id, false)
			}
			return
		}
		switch w.moveState(id) {
		case MoveIdle:
			if !w.inRangeOf(id, node) {
				w.finishCycle(hr, r, id, false) // arrived somewhere ≠ node: unreachable
				return
			}
			// exclusive nodes admit one gatherer; the rest wait in place
			if w.Nodes.Flags[nr]&NodeExclusive != 0 {
				if b := w.Nodes.Busy[nr]; b != 0 && b != id && w.Ents.Alive(b) {
					return // occupied: stand and retry next tick
				}
				w.Nodes.Busy[nr] = id
			}
			h.State[hr] = HGather
			h.Clock[hr] = w.tick + uint32(h.GatherTicks[hr])
		case MoveBlocked:
			w.finishCycle(hr, r, id, false)
		}
	case HGather:
		node := h.Node[hr]
		nr := w.Nodes.Row(node)
		if nr == -1 { // depleted by someone else mid-gather
			if !w.retargetNode(hr, r, id) {
				w.finishCycle(hr, r, id, false)
			}
			return
		}
		if w.tick < h.Clock[hr] {
			return
		}
		take := int64(h.Capacity[hr])
		if take > w.Nodes.Remaining[nr] {
			take = w.Nodes.Remaining[nr]
		}
		w.Nodes.Remaining[nr] -= take
		h.Carried[hr] = int32(take)
		h.CarriedRes[hr] = w.Nodes.Resource[nr]
		if w.Nodes.Flags[nr]&NodeExclusive != 0 && w.Nodes.Busy[nr] == id {
			w.Nodes.Busy[nr] = 0
		}
		if w.Nodes.Remaining[nr] == 0 {
			w.Emit(Event{Kind: EvResourceDepleted, Src: node, Arg: int64(w.Nodes.Resource[nr])})
			w.KillUnit(node)
		}
		if !w.headToDepot(hr, id) {
			w.finishCycle(hr, r, id, false) // no depot anywhere: idle holding cargo
			return
		}
		h.State[hr] = HReturn
	case HReturn:
		depot := h.Depot[hr]
		if !w.Ents.Alive(depot) || w.Econs.Row(depot) == -1 {
			if !w.headToDepot(hr, id) { // depot died: re-path to the next
				w.finishCycle(hr, r, id, false)
			}
			return
		}
		switch w.moveState(id) {
		case MoveIdle:
			if !w.inRangeOf(id, depot) {
				w.finishCycle(hr, r, id, false)
				return
			}
			// DEPOSIT
			or := w.Owners.Row(id)
			if or != -1 && w.resourceCount > 0 {
				p := w.Owners.Player[or]
				if p < MaxPlayers && int(h.CarriedRes[hr]) < w.resourceCount {
					w.resources[p][h.CarriedRes[hr]] += int64(h.Carried[hr])
				}
			}
			w.Emit(Event{
				Kind: EvResourceDeposited, Src: id, Dst: depot,
				Arg: int64(h.Carried[hr])<<8 | int64(h.CarriedRes[hr]),
			})
			h.Carried[hr] = 0
			h.Depot[hr] = 0
			// next trip: same node, or the nearest survivor
			if w.Nodes.Row(h.Node[hr]) != -1 && w.startLeg(id, h.Node[hr]) {
				h.State[hr] = HToNode
				return
			}
			if !w.retargetNode(hr, r, id) {
				w.finishCycle(hr, r, id, true) // economy exhausted: done
			}
		case MoveBlocked:
			w.finishCycle(hr, r, id, false)
		}
	default: // HIdle under a running harvest order: treat as finished
		w.completeOrder(r, id, true)
	}
}

// moveState reads the worker's movement state (MoveIdle when the
// component is missing — fail closed into "arrived").
func (w *World) moveState(id EntityID) uint8 {
	mr := w.Movements.Row(id)
	if mr == -1 {
		return MoveBlocked
	}
	return w.Movements.State[mr]
}

// inRangeOf tests the §2.2 interaction radius against a target's
// transform.
func (w *World) inRangeOf(id, target EntityID) bool {
	a, b := w.Transforms.Row(id), w.Transforms.Row(target)
	if a == -1 || b == -1 {
		return false
	}
	return fixed.DistSqLess(w.Transforms.Pos[a], w.Transforms.Pos[b], HarvestRange)
}

// startLeg paths the worker at a target entity's position.
func (w *World) startLeg(id, target EntityID) bool {
	tr := w.Transforms.Row(target)
	if tr == -1 {
		return false
	}
	return w.StartMoveTo(id, w.Transforms.Pos[tr])
}

// retargetNode re-selects the nearest live node matching the worker's
// carried/last resource by the §3.2 tuple (distSq, entityIndex) and
// starts the leg. Deterministic: pure fold over node rows.
func (w *World) retargetNode(hr, r int32, id EntityID) bool {
	tr := w.Transforms.Row(id)
	if tr == -1 {
		return false
	}
	pos := w.Transforms.Pos[tr]
	h := w.Harvests
	var best EntityID
	var bestHi, bestLo uint64
	for n := int32(0); n < w.Nodes.count; n++ {
		if h.Mask[hr]&(1<<uint(w.Nodes.Resource[n])) == 0 || w.Nodes.Remaining[n] <= 0 {
			continue
		}
		cand := w.Nodes.Entity[n]
		if !w.Ents.Alive(cand) {
			continue
		}
		ctr := w.Transforms.Row(cand)
		if ctr == -1 {
			continue
		}
		hi, lo := fixed.DistSq(pos, w.Transforms.Pos[ctr])
		if best == 0 || hi < bestHi || (hi == bestHi && lo < bestLo) ||
			(hi == bestHi && lo == bestLo && cand.Index() < best.Index()) {
			best, bestHi, bestLo = cand, hi, lo
		}
	}
	if best == 0 || !w.startLeg(id, best) {
		return false
	}
	h.Node[hr] = best
	h.State[hr] = HToNode
	return true
}

// headToDepot selects the nearest own depot accepting the carried
// resource (same tuple) and starts the leg.
func (w *World) headToDepot(hr int32, id EntityID) bool {
	or := w.Owners.Row(id)
	tr := w.Transforms.Row(id)
	if or == -1 || tr == -1 {
		return false
	}
	player := w.Owners.Player[or]
	pos := w.Transforms.Pos[tr]
	h := w.Harvests
	bit := uint16(1) << uint(h.CarriedRes[hr])
	var best EntityID
	var bestHi, bestLo uint64
	for d := int32(0); d < w.Econs.count; d++ {
		if w.Econs.DepotMask[d]&bit == 0 {
			continue
		}
		cand := w.Econs.Entity[d]
		cor := w.Owners.Row(cand)
		if cor == -1 || w.Owners.Player[cor] != player || !w.Ents.Alive(cand) {
			continue
		}
		ctr := w.Transforms.Row(cand)
		if ctr == -1 {
			continue
		}
		hi, lo := fixed.DistSq(pos, w.Transforms.Pos[ctr])
		if best == 0 || hi < bestHi || (hi == bestHi && lo < bestLo) ||
			(hi == bestHi && lo == bestLo && cand.Index() < best.Index()) {
			best, bestHi, bestLo = cand, hi, lo
		}
	}
	if best == 0 || !w.startLeg(id, best) {
		return false
	}
	h.Depot[hr] = best
	return true
}

// finishCycle ends the harvest order. Carried cargo persists in the
// component (a worker holding resources stays holding them); done is
// the order outcome (true = economy ran dry after delivering, false =
// the cycle broke: unreachable, blocked, no depot).
func (w *World) finishCycle(hr, r int32, id EntityID, done bool) {
	h := w.Harvests
	// release an exclusive claim if we held one
	if nr := w.Nodes.Row(h.Node[hr]); nr != -1 && w.Nodes.Busy[nr] == id {
		w.Nodes.Busy[nr] = 0
	}
	h.State[hr] = HIdle
	h.Node[hr] = 0
	h.Depot[hr] = 0
	h.Clock[hr] = 0
	w.completeOrder(r, id, done)
}
