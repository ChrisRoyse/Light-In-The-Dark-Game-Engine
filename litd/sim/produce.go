package sim

// Production/training queues + rally points (#302,
// combat-and-orders.md §2): every producing building holds a FIFO of
// up to ProduceQueueCap pending trains in a fixed per-row slot array
// (preallocated, R-GC-2 — nothing here ever allocates mid-match).
//
// Admission runs at ENQUEUE: cost is deducted and food is reserved
// (foodUsed) the moment the entry is accepted; a refusal is
// deterministic with a named reason and touches nothing. Cancel
// refunds the full cost and releases the reservation. Completion
// spawns the unit from its data-table row at a deterministic
// footprint-adjacent cell, releases the reservation (the spawned
// unit's own EconStore row takes over the food cost), emits
// EvUnitTrained, and auto-issues the building's rally order resolved
// through the smart table (point→move, resource→harvest, enemy→
// attack). A building destroyed mid-queue releases its food
// reservations but refunds no resources (destruction is not a
// cancel — the ledger stays exact either way).
//
// The tech gate is a hook (#303): nil means allow — the documented
// canonical default while no requirement table is bound, not a
// silent fallback. Hero revive rides the same queue via the
// TrainFlagHeroRevive slot flag (#304 fills the entry point).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// ProduceQueueCap is the per-building queue depth (WC3's 7).
const ProduceQueueCap = 7

// Production events (#302).
const (
	// EvUnitTrained: Src = building, Dst = the spawned unit,
	// Arg = typeID.
	EvUnitTrained uint16 = 11
	// EvTrainRefused: Src = building, Arg = reason<<16 | typeID.
	EvTrainRefused uint16 = 12
)

// EnqueueTrain refusal reasons (deterministic, the FSV trace
// vocabulary).
const (
	TrainOK           uint8 = 0
	TrainNoProducer   uint8 = 1 // no produce row / defs unbound / no owner
	TrainUnknownType  uint8 = 2 // typeID outside the bound def table
	TrainNotTrainable uint8 = 3 // no train time, or not in the producer's list
	TrainQueueFull    uint8 = 4
	TrainTechLocked   uint8 = 5
	TrainNoFood       uint8 = 6
	TrainNoResources  uint8 = 7
)

// Queue-slot flags.
const (
	TrainFlagHeroRevive uint8 = 1 << 0
	// TrainFlagResearch: the slot value is an UPGRADE index, not a
	// unit type — research riding the queue (#303)
	TrainFlagResearch uint8 = 1 << 1
)

// Rally kinds (ProduceStore.RallyKind).
const (
	RallyNone   uint8 = 0
	RallyPoint  uint8 = 1
	RallyEntity uint8 = 2
)

// spawnRings bounds the footprint-adjacent scan: 8 directions per
// ring, rings one pathing cell (32 wu) apart.
const spawnRings = 4

// ---- produce store (T2 pattern) ----

type ProduceStore struct {
	Queue      [][ProduceQueueCap]uint16 // typeIDs, live prefix [0,QCount)
	QFlags     [][ProduceQueueCap]uint8  // per-slot Train* flags
	QCount     []uint8
	Done       []uint32 // tick the HEAD completes (0 = empty queue)
	RallyKind  []uint8
	RallyEnt   []EntityID
	RallyPoint []fixed.Vec2
	Entity     []EntityID

	rowOf []int32
	count int32
}

func NewProduceStore(rowCap, entityCap int) *ProduceStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &ProduceStore{
		Queue:      make([][ProduceQueueCap]uint16, rowCap),
		QFlags:     make([][ProduceQueueCap]uint8, rowCap),
		QCount:     make([]uint8, rowCap),
		Done:       make([]uint32, rowCap),
		RallyKind:  make([]uint8, rowCap),
		RallyEnt:   make([]EntityID, rowCap),
		RallyPoint: make([]fixed.Vec2, rowCap),
		Entity:     make([]EntityID, rowCap),
		rowOf:      make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *ProduceStore) Add(e *Entities, id EntityID) bool {
	if !e.Alive(id) || s.rowOf[id.Index()] != -1 || int(s.count) == len(s.QCount) {
		return false
	}
	r := s.count
	s.Queue[r] = [ProduceQueueCap]uint16{}
	s.QFlags[r] = [ProduceQueueCap]uint8{}
	s.QCount[r] = 0
	s.Done[r] = 0
	s.RallyKind[r] = RallyNone
	s.RallyEnt[r] = 0
	s.RallyPoint[r] = fixed.Vec2{}
	s.Entity[r] = id
	s.rowOf[id.Index()] = r
	s.count++
	return true
}

func (s *ProduceStore) Remove(id EntityID) bool {
	r := s.rowOf[id.Index()]
	if r == -1 {
		return false
	}
	last := s.count - 1
	s.Queue[r] = s.Queue[last]
	s.QFlags[r] = s.QFlags[last]
	s.QCount[r] = s.QCount[last]
	s.Done[r] = s.Done[last]
	s.RallyKind[r] = s.RallyKind[last]
	s.RallyEnt[r] = s.RallyEnt[last]
	s.RallyPoint[r] = s.RallyPoint[last]
	s.Entity[r] = s.Entity[last]
	s.rowOf[s.Entity[r].Index()] = r
	s.rowOf[id.Index()] = -1
	s.count--
	return true
}

func (s *ProduceStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}
func (s *ProduceStore) Count() int32 { return s.count }

// ---- world surface ----

// BindUnitDefs installs the loaded unit rows. UnitTypeStore TypeIDs
// index this slice (the data tables sort units by ID; Trains lists
// already resolve to these indices). Fail-closed on a set too large
// for the uint16 ID space; rebinding a different table is refused
// (the defs are admission/spawn state).
func (w *World) BindUnitDefs(defs []data.Unit) bool {
	if len(defs) == 0 || len(defs) > 1<<16 {
		return false
	}
	if w.unitDefs != nil && len(w.unitDefs) != len(defs) {
		return false
	}
	w.unitDefs = defs
	// Build the code -> typeID index for UnitTypeID (#217). First binding of a
	// code wins; later duplicates are ignored (the def slice stays canonical).
	idx := make(map[string]uint16, len(defs))
	for i := range defs {
		if defs[i].ID == "" {
			continue
		}
		if _, ok := idx[defs[i].ID]; !ok {
			idx[defs[i].ID] = uint16(i)
		}
	}
	w.unitDefByCode = idx
	return true
}

// UnitTypeID resolves a unit code (data.Unit.ID, e.g. "hfoo") to its bound
// typeID. Returns ok=false for an unknown code or before BindUnitDefs. A
// deterministic map lookup, never an iteration; the backing index is built
// once at bind time.
func (w *World) UnitTypeID(code string) (uint16, bool) {
	id, ok := w.unitDefByCode[code]
	return id, ok
}

// SetTechGate installs the #303 requirement check. nil = the
// documented canonical default: every bound unit type is allowed.
func (w *World) SetTechGate(f func(player uint8, typeID uint16) bool) { w.techGate = f }

// AddProducer attaches an empty production queue to a building.
// (SpawnFromTable attaches one automatically when the unit row
// declares a trains list.)
func (w *World) AddProducer(id EntityID) bool {
	return w.Produce.Add(w.Ents, id)
}

// UnitRally returns a building's rally: the kind (RallyNone/RallyPoint/
// RallyEntity), the rally point, and the rally entity. For a point rally
// pt is the set point; for an entity rally pt is the target's current
// position (resolved live) and ent is the target; RallyNone returns the
// zero values. A unit with no produce row reports RallyNone.
// Backs GetUnitRallyPoint / GetUnitRallyUnit.
func (w *World) UnitRally(id EntityID) (kind uint8, pt fixed.Vec2, ent EntityID) {
	r := w.Produce.Row(id)
	if r == -1 {
		return RallyNone, fixed.Vec2{}, 0
	}
	kind = w.Produce.RallyKind[r]
	switch kind {
	case RallyPoint:
		return RallyPoint, w.Produce.RallyPoint[r], 0
	case RallyEntity:
		ent = w.Produce.RallyEnt[r]
		if tr := w.Transforms.Row(ent); tr != -1 && w.Ents.Alive(ent) {
			pt = w.Transforms.Pos[tr]
		}
		return RallyEntity, pt, ent
	default:
		return RallyNone, fixed.Vec2{}, 0
	}
}

// UnitRallyUnit returns the entity a building is rallied to and true,
// or (0,false) when the rally is a point, absent, dead, or the unit has
// no produce row. Backs GetUnitRallyUnit. Keeps the RallyEntity check on
// the sim side so the api layer needs no sim rally constants.
func (w *World) UnitRallyUnit(id EntityID) (EntityID, bool) {
	kind, _, ent := w.UnitRally(id)
	if kind != RallyEntity || !w.Ents.Alive(ent) {
		return 0, false
	}
	return ent, true
}

// SetRallyPoint / SetRallyTarget / ClearRally set a building's rally.
func (w *World) SetRallyPoint(id EntityID, pt fixed.Vec2) bool {
	r := w.Produce.Row(id)
	if r == -1 {
		return false
	}
	w.Produce.RallyKind[r] = RallyPoint
	w.Produce.RallyPoint[r] = pt
	w.Produce.RallyEnt[r] = 0
	return true
}

func (w *World) SetRallyTarget(id EntityID, target EntityID) bool {
	r := w.Produce.Row(id)
	if r == -1 || !w.Ents.Alive(target) {
		return false
	}
	w.Produce.RallyKind[r] = RallyEntity
	w.Produce.RallyEnt[r] = target
	w.Produce.RallyPoint[r] = fixed.Vec2{}
	return true
}

func (w *World) ClearRally(id EntityID) bool {
	r := w.Produce.Row(id)
	if r == -1 {
		return false
	}
	w.Produce.RallyKind[r] = RallyNone
	w.Produce.RallyEnt[r] = 0
	w.Produce.RallyPoint[r] = fixed.Vec2{}
	return true
}

// EnqueueTrain admits one train request on a producing building.
// Returns TrainOK or the refusal reason; every refusal also emits
// EvTrainRefused and changes NOTHING. Admission deducts the full
// cost and reserves the food immediately.
func (w *World) EnqueueTrain(building EntityID, typeID uint16) uint8 {
	return w.enqueueTrain(building, typeID, 0)
}

// TrainForPlayer admits one train request of typeID on behalf of player,
// choosing the producer building deterministically rather than naming one — the
// WC3 AI training model (common.ai SetProduce/TrainUnits), where the script
// asks for a unit type and the engine picks the building. R-EXEC-3: the AI
// never selects a building or mutates a queue directly; it names a typeID and
// the sim chooses, so production stays authoritative sim state.
//
// Selection: among the player's living producers that can train typeID and have
// queue space, the least-loaded (smallest queue), ties broken by lowest entity
// index — balancing load across buildings while staying fully deterministic
// (one idle producer ⇒ the lowest-index one). Returns the chosen building and
// the EnqueueTrain reason (TrainOK on success). When no producer can take it,
// returns (0, TrainNoProducer). A global refusal (no gold/food/tech) returns
// the chosen building with the refusal reason and changes nothing — a
// deterministic no-op the AI observes via the production counts.
func (w *World) TrainForPlayer(player uint8, typeID uint16) (EntityID, uint8) {
	if player >= MaxPlayers || w.unitDefs == nil {
		return 0, TrainNoProducer
	}
	s := w.Produce
	var best EntityID
	bestRow := int32(-1)
	var bestLoad uint8
	for r := int32(0); r < s.count; r++ {
		b := s.Entity[r]
		or := w.Owners.Row(b)
		if or == -1 || w.Owners.Player[or] != player || !w.Ents.Alive(b) {
			continue
		}
		if s.QCount[r] >= ProduceQueueCap || !w.producerTrains(b, typeID) {
			continue
		}
		load := s.QCount[r]
		if bestRow == -1 || load < bestLoad || (load == bestLoad && b.Index() < best.Index()) {
			best, bestRow, bestLoad = b, r, load
		}
	}
	if bestRow == -1 {
		return 0, TrainNoProducer
	}
	return best, w.EnqueueTrain(best, typeID)
}

// PlayerTrainInProgress counts the player's producers whose HEAD slot (the entry
// actively training now) is a unit of typeID — the in-progress production of
// that type. Research/revive head slots are not unit production and are skipped.
func (w *World) PlayerTrainInProgress(player uint8, typeID uint16) int {
	s := w.Produce
	n := 0
	for r := int32(0); r < s.count; r++ {
		if s.QCount[r] == 0 {
			continue
		}
		or := w.Owners.Row(s.Entity[r])
		if or == -1 || w.Owners.Player[or] != player {
			continue
		}
		if s.QFlags[r][0]&(TrainFlagResearch|TrainFlagHeroRevive) != 0 {
			continue
		}
		if s.Queue[r][0] == typeID {
			n++
		}
	}
	return n
}

// PlayerTrainQueued counts the player's queued-but-not-yet-started slots of
// typeID — the entries behind the head, across all producers, still waiting.
// Together with PlayerTrainInProgress (head) and the completed-entity count
// (the AIView GetUnitCountDone analogue) these distinguish in-queue,
// in-progress, and done production of a type.
func (w *World) PlayerTrainQueued(player uint8, typeID uint16) int {
	s := w.Produce
	n := 0
	for r := int32(0); r < s.count; r++ {
		or := w.Owners.Row(s.Entity[r])
		if or == -1 || w.Owners.Player[or] != player {
			continue
		}
		for slot := uint8(1); slot < s.QCount[r]; slot++ {
			if s.QFlags[r][slot]&(TrainFlagResearch|TrainFlagHeroRevive) != 0 {
				continue
			}
			if s.Queue[r][slot] == typeID {
				n++
			}
		}
	}
	return n
}

func (w *World) enqueueTrain(building EntityID, typeID uint16, flags uint8) uint8 {
	refuse := func(reason uint8) uint8 {
		w.Emit(Event{Kind: EvTrainRefused, Src: building, Arg: int64(reason)<<16 | int64(typeID)})
		return reason
	}
	r := w.Produce.Row(building)
	or := w.Owners.Row(building)
	if r == -1 || or == -1 || !w.Ents.Alive(building) || w.unitDefs == nil {
		return refuse(TrainNoProducer)
	}
	if int(typeID) >= len(w.unitDefs) {
		return refuse(TrainUnknownType)
	}
	def := &w.unitDefs[typeID]
	if def.TrainTicks == 0 || !w.producerTrains(building, typeID) {
		return refuse(TrainNotTrainable)
	}
	if w.Produce.QCount[r] >= ProduceQueueCap {
		return refuse(TrainQueueFull)
	}
	p := w.Owners.Player[or]
	if p >= MaxPlayers {
		return refuse(TrainNoProducer)
	}
	if w.techGate != nil && !w.techGate(p, typeID) {
		return refuse(TrainTechLocked)
	}
	if def.FoodCost > 0 && !w.CanAddFood(p, def.FoodCost) {
		return refuse(TrainNoFood)
	}
	for i := 0; i < len(def.Costs) && i < w.resourceCount; i++ {
		if w.resources[p][i] < def.Costs[i] {
			return refuse(TrainNoResources)
		}
	}
	// admitted: deduct cost, reserve food, append
	for i := 0; i < len(def.Costs) && i < w.resourceCount; i++ {
		w.resources[p][i] -= def.Costs[i]
	}
	w.foodUsed[p] += int32(def.FoodCost)
	q := w.Produce.QCount[r]
	w.Produce.Queue[r][q] = typeID
	w.Produce.QFlags[r][q] = flags
	w.Produce.QCount[r] = q + 1
	if q == 0 {
		w.Produce.Done[r] = w.tick + uint32(def.TrainTicks)
	}
	return TrainOK
}

// producerTrains reports whether typeID is in the building's
// data-table trains list.
func (w *World) producerTrains(building EntityID, typeID uint16) bool {
	ut := w.UnitTypes.Row(building)
	if ut == -1 {
		return false
	}
	btid := w.UnitTypes.TypeID[ut]
	if int(btid) >= len(w.unitDefs) {
		return false
	}
	for _, tr := range w.unitDefs[btid].Trains {
		if tr == typeID {
			return true
		}
	}
	return false
}

// CancelTrain removes queue slot `slot` (0 = the in-progress head)
// with a FULL refund — cost back, food reservation released. Later
// entries shift forward; canceling the head restarts the next entry
// from zero progress (WC3 semantics).
func (w *World) CancelTrain(building EntityID, slot int) bool {
	r := w.Produce.Row(building)
	or := w.Owners.Row(building)
	if r == -1 || or == -1 || slot < 0 || slot >= int(w.Produce.QCount[r]) || w.unitDefs == nil {
		return false
	}
	p := w.Owners.Player[or]
	if p >= MaxPlayers {
		return false
	}
	v := w.Produce.Queue[r][slot]
	if w.Produce.QFlags[r][slot]&TrainFlagHeroRevive != 0 {
		rec := &w.deadHeroes[p][v] // slot stays in the pool (still dead)
		for i := 0; i < w.resourceCount; i++ {
			w.resources[p][i] += w.reviveCostAt(rec.Level, i)
		}
		w.foodUsed[p] -= int32(w.unitDefs[w.heroTables.Heroes[rec.HeroType].Unit].FoodCost)
		w.shiftQueue(r, slot)
		return true
	}
	if w.Produce.QFlags[r][slot]&TrainFlagResearch != 0 {
		cur := w.upgradeLevel[p][v] // pending level (duplicates refused)
		costs := w.upgradeDefs[v].Levels[cur].Costs
		for i := 0; i < len(costs) && i < w.resourceCount; i++ {
			w.resources[p][i] += costs[i]
		}
		w.shiftQueue(r, slot)
		return true
	}
	def := &w.unitDefs[v]
	for i := 0; i < len(def.Costs) && i < w.resourceCount; i++ {
		w.resources[p][i] += def.Costs[i]
	}
	w.foodUsed[p] -= int32(def.FoodCost)
	w.shiftQueue(r, slot)
	return true
}

// shiftQueue removes one slot, zero-fills the freed tail slot
// (canonical state for the hash/save), and restarts the head clock
// when the head changed.
func (w *World) shiftQueue(r int32, slot int) {
	s := w.Produce
	n := int(s.QCount[r])
	for i := slot; i < n-1; i++ {
		s.Queue[r][i] = s.Queue[r][i+1]
		s.QFlags[r][i] = s.QFlags[r][i+1]
	}
	s.Queue[r][n-1] = 0
	s.QFlags[r][n-1] = 0
	s.QCount[r] = uint8(n - 1)
	if slot == 0 {
		if s.QCount[r] > 0 {
			s.Done[r] = w.tick + uint32(w.headTicks(r))
		} else {
			s.Done[r] = 0
		}
	}
}

// headTicks reads the head entry's duration: train time for unit
// rows, the PENDING level's research time for flagged rows (one
// in-flight research per upgrade per player, so the pending level is
// the player's current level).
func (w *World) headTicks(r int32) uint16 {
	s := w.Produce
	v := s.Queue[r][0]
	if s.QFlags[r][0]&TrainFlagHeroRevive != 0 {
		or := w.Owners.Row(s.Entity[r])
		if or == -1 {
			return w.heroTables.Revive.BaseTicks
		}
		rec := &w.deadHeroes[w.Owners.Player[or]][v]
		lvl := rec.Level
		if lvl == 0 {
			lvl = 1
		}
		return w.reviveTicksAt(lvl, w.Owners.Player[or])
	}
	if s.QFlags[r][0]&TrainFlagResearch == 0 {
		return w.unitDefs[v].TrainTicks
	}
	def := &w.upgradeDefs[v]
	or := w.Owners.Row(s.Entity[r])
	if or == -1 {
		return def.Levels[0].ResearchTicks
	}
	cur := w.upgradeLevel[w.Owners.Player[or]][v]
	if int(cur) >= len(def.Levels) {
		cur = uint8(len(def.Levels) - 1)
	}
	return def.Levels[cur].ResearchTicks
}

// TrainProgress reports (elapsed, total) ticks of the head entry —
// the integer progress surface (0,0 when idle).
func (w *World) TrainProgress(building EntityID) (elapsed, total uint16) {
	r := w.Produce.Row(building)
	if r == -1 || w.Produce.QCount[r] == 0 || w.unitDefs == nil {
		return 0, 0
	}
	// headTicks resolves the head's real duration for ALL head kinds — a unit
	// train, a research entry (Queue[0] is an upgrade id), or a hero revive
	// (Queue[0] is a hero id). Indexing unitDefs[Queue[0]] directly would misread
	// a research/revive head's id as a unit type (wrong total, or an out-of-range
	// panic when the id exceeds len(unitDefs)).
	total = w.headTicks(r)
	remain := w.Produce.Done[r] - w.tick
	if w.Produce.Done[r] <= w.tick {
		remain = 0
	}
	return total - uint16(remain), total
}

// ---- the per-tick drive (phase 3, after orders) ----

// produceSystem completes due heads: spawn at the deterministic
// footprint-adjacent cell, release the food reservation (the spawned
// unit's econ row carries the cost from here), emit EvUnitTrained,
// issue the rally order, pop. A spawn the unit cap (or a fully
// blocked exit) refuses retries every tick — visible stall, never a
// drop.
func (w *World) produceSystem() {
	s := w.Produce
	for r := int32(0); r < s.count; r++ {
		if s.QCount[r] == 0 || w.tick < s.Done[r] {
			continue
		}
		building := s.Entity[r]
		or := w.Owners.Row(building)
		if or == -1 {
			continue
		}
		typeID := s.Queue[r][0]
		if s.QFlags[r][0]&TrainFlagResearch != 0 {
			w.completeResearch(r, building, w.Owners.Player[or], typeID)
			w.shiftQueue(r, 0)
			continue
		}
		if s.QFlags[r][0]&TrainFlagHeroRevive != 0 {
			if w.completeRevive(r, building, w.Owners.Player[or], typeID) {
				w.shiftQueue(r, 0)
			}
			continue
		}
		def := &w.unitDefs[typeID]
		pos, ok := w.spawnCell(building, def)
		if !ok {
			continue // exit blocked: retry next tick
		}
		p, team := w.Owners.Player[or], w.Owners.Team[or]
		unit, ok := w.SpawnFromTable(typeID, p, team, pos)
		if !ok {
			continue // unit cap: retry next tick
		}
		w.foodUsed[p] -= int32(def.FoodCost) // reservation → the unit's own econ row
		w.Emit(Event{Kind: EvUnitTrained, Src: building, Dst: unit, Arg: int64(typeID)})
		w.issueRally(r, unit)
		w.shiftQueue(r, 0)
	}
}

// spawnCell scans rings of 8 directions around the building's
// footprint in fixed order (E,N,W,S,NE,NW,SW,SE; rings one cell
// apart) and returns the first position whose pathing cell is free.
// Without a bound grid every cell is free — the first candidate
// wins.
func (w *World) spawnCell(building EntityID, def *data.Unit) (fixed.Vec2, bool) {
	base := int32(0)
	if ut := w.UnitTypes.Row(building); ut != -1 {
		if btid := w.UnitTypes.TypeID[ut]; int(btid) < len(w.unitDefs) {
			base = w.unitDefs[btid].CollisionSize
		}
	}
	return w.adjacentCell(building, base+def.CollisionSize, 0)
}

// adjacentCell is the shared footprint-adjacent ring scan (spawnCell,
// item drops #305). seed rotates the direction order deterministically
// so successive callers spread instead of stacking one candidate.
func (w *World) adjacentCell(around EntityID, base int32, seed int) (fixed.Vec2, bool) {
	tr := w.Transforms.Row(around)
	if tr == -1 {
		return fixed.Vec2{}, false
	}
	center := w.Transforms.Pos[tr]
	for ring := int32(1); ring <= spawnRings; ring++ {
		d := fixed.FromInt(base + ring*32)
		for di := 0; di < 8; di++ {
			dir := (di + seed) & 7
			cand := center
			switch dir {
			case 0:
				cand.X = cand.X.Add(d)
			case 1:
				cand.Y = cand.Y.Add(d)
			case 2:
				cand.X = cand.X.Sub(d)
			case 3:
				cand.Y = cand.Y.Sub(d)
			case 4:
				cand.X = cand.X.Add(d)
				cand.Y = cand.Y.Add(d)
			case 5:
				cand.X = cand.X.Sub(d)
				cand.Y = cand.Y.Add(d)
			case 6:
				cand.X = cand.X.Sub(d)
				cand.Y = cand.Y.Sub(d)
			case 7:
				cand.X = cand.X.Add(d)
				cand.Y = cand.Y.Sub(d)
			}
			if w.Grid == nil {
				return cand, true
			}
			c := cellOfPos(cand)
			if c < 0 {
				continue
			}
			f := w.Grid.FlagsAt(c%path.GridSize, c/path.GridSize)
			if f&path.Walkable != 0 && f&path.OccupiedStatic == 0 && f&path.OccupiedDynamic == 0 {
				return cand, true
			}
		}
	}
	return fixed.Vec2{}, false
}

// issueRally resolves the building's rally through the smart table
// and issues the order on the spawned unit. Point rallies are the
// direct move mapping; entity rallies classify the target (resource
// node → TCResource) and look up [class][unitClass]. An opcode the
// order system cannot drive (or a missing table) issues nothing —
// the unit idles at spawn, visibly.
func (w *World) issueRally(r int32, unit EntityID) {
	s := w.Produce
	switch s.RallyKind[r] {
	case RallyPoint:
		w.IssueOrder(unit, Order{Kind: OrderMove, Point: s.RallyPoint[r]}, false)
	case RallyEntity:
		target := s.RallyEnt[r]
		if !w.Ents.Alive(target) {
			return
		}
		if w.smart == nil {
			return
		}
		var tc uint8
		if w.Nodes.Row(target) != -1 {
			tc = data.TCResource
		} else {
			team := uint8(0)
			if uor := w.Owners.Row(unit); uor != -1 {
				team = w.Owners.Team[uor]
			}
			c, ok := w.ClassifyTarget(team, target)
			if !ok {
				return
			}
			tc = c
		}
		op := w.smart.Rules[tc][w.unitClassOf(unit)]
		ttr := w.Transforms.Row(target)
		pt := fixed.Vec2{}
		if ttr != -1 {
			pt = w.Transforms.Pos[ttr]
		}
		switch op {
		case OpMove:
			w.IssueOrder(unit, Order{Kind: OrderMove, Point: pt}, false)
		case OpAttack:
			w.IssueOrder(unit, Order{Kind: OrderAttack, Target: target}, false)
		case OpHarvest:
			w.IssueOrder(unit, Order{Kind: OrderHarvest, Target: target}, false)
		case OpStop:
			w.IssueOrder(unit, Order{Kind: OrderStop}, false)
		case OpHold:
			w.IssueOrder(unit, Order{Kind: OrderHold}, false)
		}
	}
}

// ---- table-driven spawn ----

// SpawnFromTable assembles a unit entity from its bound data-table
// row: transform, type, owner, health, movement (movable rows),
// collision, weapons, abilities, econ/food, harvest, orders, and a
// production queue when the row trains. Fail-closed: any component
// refusal tears the entity down and returns false.
func (w *World) SpawnFromTable(typeID uint16, player, team uint8, pos fixed.Vec2) (EntityID, bool) {
	if w.unitDefs == nil || int(typeID) >= len(w.unitDefs) {
		return 0, false
	}
	def := &w.unitDefs[typeID]
	id, ok := w.CreateUnit(pos, 0)
	if !ok {
		return 0, false
	}
	ok = w.UnitTypes.Add(w.Ents, id, typeID) &&
		w.Owners.Add(w.Ents, id, player, team, player) &&
		w.Orders.Add(w.Ents, id)
	if ok && def.Life > 0 {
		ok = w.Healths.Add(w.Ents, id, fixed.FromInt(def.Life), def.RegenPerTick, int16(def.Armor), def.ArmorType)
	}
	if ok && def.MoveSpeedPerTick > 0 {
		ok = w.Movements.Add(w.Ents, w.Transforms, id, def.MoveSpeedPerTick, def.TurnRatePerTick)
	}
	if ok {
		ok = w.Collisions.Add(w.Ents, id, def.CollisionClass, def.Pathing)
	}
	if ok && len(def.Attacks) > 0 {
		ok = w.Combats.Add(w.Ents, id)
		for s := 0; ok && s < len(def.Attacks) && s < WeaponSlots; s++ {
			ok = w.SetWeapon(id, s, &def.Attacks[s], 0, data.EffectList{})
		}
		if ok && def.AcquisitionRange > 0 {
			cr := w.Combats.Row(id)
			w.Combats.AcquisitionRange[cr] = def.AcquisitionRange
		}
	}
	if ok && len(def.Abilities) > 0 {
		ok = w.Abilities.Add(w.Ents, id)
		for s := 0; ok && s < len(def.Abilities) && s < AbilitySlots; s++ {
			ok = w.SetAbility(id, s, int(def.Abilities[s]))
		}
	}
	if ok && (def.FoodCost > 0 || def.FoodProvided > 0 || def.DepotMask != 0) {
		ok = w.AddEcon(id, def.FoodCost, def.FoodProvided, def.DepotMask)
	}
	if ok && def.Harvest.Capacity > 0 {
		ok = w.Harvests.Add(w.Ents, id, &def.Harvest)
	}
	if ok && (len(def.Trains) > 0 || def.RevivesHeroes) {
		ok = w.Produce.Add(w.Ents, id)
	}
	if !ok {
		w.DestroyUnit(id)
		return 0, false
	}
	w.recomputeBuffStats(id) // inherit the owner's researched upgrades (#303)
	return id, true
}

// releaseTrainReservations returns the food reserved by every queued
// entry of a dying building (DestroyUnit, before the owner row goes).
// Resources are NOT refunded — destruction is not a cancel.
func (w *World) releaseTrainReservations(r int32) {
	or := w.Owners.Row(w.Produce.Entity[r])
	if or == -1 || w.unitDefs == nil {
		return
	}
	p := w.Owners.Player[or]
	if p >= MaxPlayers {
		return
	}
	for i := 0; i < int(w.Produce.QCount[r]); i++ {
		fl := w.Produce.QFlags[r][i]
		if fl&TrainFlagResearch != 0 {
			continue // research holds no food
		}
		if fl&TrainFlagHeroRevive != 0 {
			rec := &w.deadHeroes[p][w.Produce.Queue[r][i]]
			w.foodUsed[p] -= int32(w.unitDefs[w.heroTables.Heroes[rec.HeroType].Unit].FoodCost)
			continue
		}
		w.foodUsed[p] -= int32(w.unitDefs[w.Produce.Queue[r][i]].FoodCost)
	}
}
