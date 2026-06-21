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
	Units              int
	Projectiles        int
	Effects            int
	BuffInstances      int
	OrderQueueEntries  int
	PendingEvents      int // per-tick ring
	PathRequests       int // in flight
	ScriptedDoodads    int
	Destructables      int // killable, pathing-blocking widgets (trees/gates) (#229)
	RuntimeAbilityDefs int // dynamic ability rows appended after bound data
	RuntimeEffects     int // modder-registered effect primitives (#477)
	Triggers           int // first-class ECA trigger slab (#456)
}

// EngineCaps are the engine ceilings — the §2 pool table, exactly.
var EngineCaps = Caps{
	Units:              4000,
	Projectiles:        2000,
	Effects:            1024,
	BuffInstances:      8000,
	OrderQueueEntries:  16000,
	PendingEvents:      4096,
	PathRequests:       512,
	ScriptedDoodads:    1024,
	Destructables:      2048,
	RuntimeAbilityDefs: 1024,
	RuntimeEffects:     256,
	Triggers:           4096,
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
		Units:              clampCap(c.Units, EngineCaps.Units),
		Projectiles:        clampCap(c.Projectiles, EngineCaps.Projectiles),
		Effects:            clampCap(c.Effects, EngineCaps.Effects),
		BuffInstances:      clampCap(c.BuffInstances, EngineCaps.BuffInstances),
		OrderQueueEntries:  clampCap(c.OrderQueueEntries, EngineCaps.OrderQueueEntries),
		PendingEvents:      clampCap(c.PendingEvents, EngineCaps.PendingEvents),
		PathRequests:       clampCap(c.PathRequests, EngineCaps.PathRequests),
		ScriptedDoodads:    clampCap(c.ScriptedDoodads, EngineCaps.ScriptedDoodads),
		Destructables:      clampCap(c.Destructables, EngineCaps.Destructables),
		RuntimeAbilityDefs: clampCap(c.RuntimeAbilityDefs, EngineCaps.RuntimeAbilityDefs),
		RuntimeEffects:     clampCap(c.RuntimeEffects, EngineCaps.RuntimeEffects),
		Triggers:           clampCap(c.Triggers, EngineCaps.Triggers),
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
	data   uint16
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
	caps   Caps
	tick   uint32
	inStep bool
	// deterministic game clock (clock.go): tod is fixed-point hours
	// in [0,24); todCarry stores the fractional raw-hour remainder
	// that makes full-day advancement drift-free.
	tod            fixed.F64
	todScale       fixed.F64
	todFrozen      bool
	todCarry       uint64
	dayLengthTicks uint32

	Ents          *Entities
	Transforms    *TransformStore
	Movements     *MovementStore
	Collisions    *CollisionStore
	Healths       *HealthStore
	Owners        *OwnerStore
	UnitTypes     *UnitTypeStore
	UserDatas     *UserDataStore
	UnitNames     *UnitNameStore
	Hiddens       *presenceSet
	XPSuspends    *presenceSet
	Pauses        *presenceSet
	Flys          *FlyStore
	PropWindows   *PropWindowStore
	Regions       *RegionStore
	Combats         *CombatStore
	Abilities       *AbilityStore
	AbilityFields   *AbilityFieldStore
	WeaponOverrides *WeaponFieldStore
	Invents         *InventoryStore
	Orders        *OrderStore
	Buffs         *BuffPool
	Missiles      *MissileStore // first-class missile entities (#158, ADR #295)
	Effects       *EffectStore  // persistent script effects, first-class entities (#348)
	Heroes        *HeroStore
	Items         *ItemStore   // item entities (#305): type + charges + carrier
	Patrol        *PatrolStore // patrol endpoints (#306)
	Build         *BuildStore  // building footprints + construction progress (#301)
	Visibility    *VisibilityGrid
	FogMods       *FogModifierStore // fog-of-war area overrides (#243)
	ShareVisions  *ShareVisionStore // per-unit shared-vision bitmasks (#243)
	Nodes         *ResourceNodeStore
	Econs         *EconStore
	Harvests      *HarvestStore
	Produce       *ProduceStore
	Doodads       *DoodadStore
	Destructables *DestructableStore
	Paths         *path.PathStore // pooled per-unit waypoint buffers (§7)
	Grid          *path.Grid      // pathing grid; nil until SetGrid (map load)
	pathDilated   *path.DilatedSet
	pathHPA       *path.HPA
	pathQueue     *path.Queue
	pathFlow      *path.FlowSet
	pathProvider  *path.Provider
	pathSeq       uint16
	pathLastExp   int32
	flowRefs      [path.FlowSlots]uint16
	// local avoidance (avoidance.go): cell → owning entity, plus the
	// stall threshold override (0 = DefaultStallRepathTicks)
	reservedBy  []EntityID
	stallRepath uint16
	// match result state (gamestate.go): one immutable terminal result
	// per player plus same-tick pending requests resolved in phase 6.
	results       [MaxPlayers]uint8
	resultPending [MaxPlayers]uint8
	// smart-order resolution (smartorder.go): data table + TypeID →
	// capability-class column
	smart           *data.SmartTable
	unitClassByType []uint8
	// compiled effect-composition arena (effect.go, ADR #294);
	// installed by BindEffects, read-only thereafter
	effects []data.CompiledEffect
	// loaded ability rows (ability.go #160); refs are defIndex+1
	abilityDefs []data.Ability
	// runtime ability rows (#355), appended after loaded abilityDefs.
	// Backing storage is capped by Caps.RuntimeAbilityDefs.
	runtimeAbilityDefs []data.Ability
	// modder-registered effect primitives (#477): names hash + serialize
	// (the per-match contract); execs are re-bound in setup on load. Capped
	// by Caps.RuntimeEffects, frozen at first Step.
	effectRegNames []string
	effectRegExecs []RuntimeEffectExec
	// name→trigger bindings (#478): a data ability's TriggerName resolves here to
	// the trigger that backs its EFFECT edge. Parallel slices, setup-bound; the
	// pairs hash + serialize (zero when empty).
	trigNameKeys []string
	trigNameIDs  []TriggerID
	// loaded buff-type rows (buff.go #162); BuffInstance.BuffID
	// indexes this slice
	buffTypes      []data.BuffType
	buffTypeByCode map[string]uint16 // code -> typeIdx, built at BindBuffTypes
	// loaded unit rows (produce.go #302); UnitTypeStore.TypeID indexes
	// this slice; BindUnitDefs installs, read-only thereafter
	unitDefs []data.Unit
	// code (data.Unit.ID, e.g. "hfoo") -> typeID, built by BindUnitDefs.
	// Lookup only, never iterated (the API UnitType resolver, #217).
	unitDefByCode map[string]uint16
	// loaded resource-node type rows (#401); BindResourceNodeDefs installs,
	// read-only thereafter. CreateResourceNodeByID indexes this slice.
	nodeDefs []data.ResourceNodeType
	// code (data.ResourceNodeType.ID) -> node typeID, built by
	// BindResourceNodeDefs. Lookup only, never iterated (the API resolver).
	nodeDefByCode map[string]uint16
	// tech-requirement admission hook (#303); nil = allow (documented
	// canonical default while no requirement table is bound)
	techGate func(player uint8, typeID uint16) bool
	// tech tree (#303): bound upgrade/requirement tables, per-target
	// lookup, and the per-player level/cap arrays (state, hashed)
	upgradeDefs  []data.Upgrade
	techReqs     []data.Require
	reqOfUnit    []int32
	reqOfUpgrade []int32
	upgradeLevel [MaxPlayers][]uint8
	techMax      [MaxPlayers][]uint8
	// hero rule set (#304) + per-player dead-hero pools (D-15 records)
	heroTables *data.HeroTables
	deadHeroes [MaxPlayers][MaxDeadHeroes]HeroRecord
	// item type table (#305)
	itemDefs      []data.Item
	itemDefByCode map[string]uint16 // code -> typeID, built at BindItemDefs
	// data-table content hash (#208 SaveData versioning)
	dataFingerprint uint64
	// patrol/follow behavior thresholds (#306, world units; 0 = default)
	patrolLeash  fixed.F64
	followRepath fixed.F64
	// derived-stat cache (buff.go): per stat, per entity index, the
	// folded flat Add and multiplicative factor; identity (+0, ×One)
	// when the entity carries no modifying buff. Recomputed only on
	// buff-set change.
	buffAdd     [data.BuffStatCount][]int64
	buffMult    [data.BuffStatCount][]fixed.F64
	buffScratch []int32    // recompute gather scratch, cap = BuffInstances
	auraScratch []EntityID // aura candidate scratch (#164), cap = Units
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
	atkTypes   []string  // declared attack-type names, table order (#472 config)
	armTypes   []string  // declared armor-type names, table order (#472 config)
	formula    []DamageStage             // ordered damage-formula pipeline (#473); base by default
	fOverride  []bool                    // parallel to formula: true = stage replaced from base
	fReplaced  bool                      // SetDamageFormula installed a wholesale custom formula
	dmgCtx     DamageCtx                 // reused per-packet pipeline context (zero-alloc)
	armorLUT   [armorLUTSize]fixed.F64   // per-world armor multiplier LUT (#474)
	armorK     fixed.F64                 // positive-branch reduction coefficient (default 0.06)
	armorKOver bool                      // true once SetArmorCoefficient set a non-default k
	// the sim PRNG (R-SIM-2): every gameplay roll draws here, one
	// deterministic call order; reseeded per match via SetSeed
	rng *prng.Stream
	// area-effect candidate scratch (damage.go execArea), cap = the
	// schema's max-targets ceiling — reused, never reallocated
	areaScratch []EntityID
	areaDistHi  []uint64
	areaDistLo  []uint64

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
	// OnAIPhase fires at the tail of tick phase 2, AFTER the map-script
	// scheduler has drained — the dedicated AI sub-phase (R-EXEC-3,
	// tick-and-scheduler.md §3.4). The api layer installs the AI domain tick
	// here so computer-player decisions are deterministic sim input that runs
	// every tick, inside the determinism boundary, ordered after map scripts.
	OnAIPhase     func(tick uint32)
	OnCombatPhase func(tick uint32)
	// missile flight traces (#158 FSV SoT)
	OnMissileImpact func(tick uint32, id EntityID, at fixed.Vec2, tgt EntityID)
	OnMissileExpire func(tick uint32, id EntityID, last fixed.Vec2)
	// cast-machine transitions (#160 FSV SoT)
	OnCastTransition func(tick uint32, id EntityID, slot int, from, to uint8)
	// OnAttackTransition fires on every per-weapon state flip
	// (attack.go #150 — the tick-stamped trace that is the FSV SoT).
	OnAttackTransition func(tick uint32, id EntityID, slot int, from, to uint8)
	OnDeathEvent       func(tick uint32, id EntityID)
	OnSnapshot         func(tick uint32)
	OnHash             func(tick uint32)
	OnEventDrop        func(tick uint32, e Event)
	HashEvery          uint32

	unitCount int

	// #300 economy: per-player resource counters (sized by
	// BindEconomy) and the food/supply ledger (EconStore-maintained)
	resources     [][]int64
	resourceCount int
	foodUsed      [MaxPlayers]int32
	foodCap       [MaxPlayers]int32
	// #375 upkeep economy + inter-player tax. upkeep* are the food-tier
	// income-tax brackets (BindUpkeep); upkeepLost is the cumulative
	// per-player/per-resource amount withheld at deposit (the
	// PLAYER_SCORE_*_LOST_UPKEEP counters). taxRate[a][b][res] is a's
	// transfer-tax fraction toward b (GetPlayerTaxRate/SetPlayerTaxRate),
	// applied in TransferResource. All default zero = no tax (golden-safe);
	// all hashed + serialized.
	upkeepCount int
	upkeepFood  [maxUpkeepTiers]int32
	upkeepRate  [maxUpkeepTiers][data.MaxResourceTypes]fixed.F64
	upkeepLost  [MaxPlayers][data.MaxResourceTypes]int64
	taxRate     [MaxPlayers][MaxPlayers][data.MaxResourceTypes]fixed.F64
	// #243 global fog toggles. Inverted so the zero value = fog on / mask on
	// (the default, golden-safe); FogStateAt applies them at query time.
	fogDisabled     bool
	fogMaskDisabled bool
	// #257 AI hook state: per-player difficulty/paused/attached flags and the
	// integer-pair command inbox. The AIController itself lives at the api
	// layer (a Go behavior, not deterministic state); only these replay-safe
	// inputs live in the sim. Zero value = no AI attached, golden-safe.
	ai aiState
	// #218 player roster: per-player metadata + the asymmetric alliance
	// relation. Resources/food (above) carry the rest of the player-state
	// matrix. alliance[a][b] is a flags bitset for a's stance toward b
	// (A→B and B→A independent — alliance is one-directional). All hashed
	// + serialized; initPlayers seeds defaults in NewWorld.
	players playerRoster
	// #371 terrain heightfield: a grid of fixed-point height samples that
	// TerrainHeight bilinearly interpolates (GetLocationZ). Zero value =
	// unbound = flat world at height 0. Hashed + serialized.
	height heightfield
	// #219 writable-damage hook: a synchronous pre-apply modifier invoked
	// in damageApplySystem on the final post-mitigation amount, letting a
	// script scale damage deterministically. nil = no modification (the
	// default — byte-identical to the no-hook path, so the golden trace is
	// unperturbed). Not serialized: script wiring re-installed on load,
	// like event handlers.
	damageMod func(src, dst EntityID, amount fixed.F64) fixed.F64
	// pooled intrusive order-queue entries (orders.go): LIFO free
	// list threaded through orderEntry.next
	orderPool      []orderEntry
	orderFreeHead  int32
	orderFreeCount int32
	events         []Event // per-tick pending ring (events.go)
	eventCount     int
	eventsDropped  uint64
	// structured event log (#203, R-FSV-3): nil = disabled, zero tick
	// cost; set via AttachEventLog
	eventLog    interface{ Write([]byte) (int, error) }
	eventLogErr error
	handlers    map[HandlerID]EventHandler // registry: lookup only, never iterated
	subs        []kindSubs                 // kind-sorted, registration-ordered lists
	// ECA handler-identity registry (#455): stable name <-> HandlerRef <->
	// TriggerHandler. Conditions/actions are stored by ref so the trigger
	// graph is serializable data (ADR #451). Cold-path registration only.
	handlerReg handlerRegistry
	// first-class ECA trigger slab (#456): gen-checked handles holding
	// events/condition/actions/enabled/initially-on. Hashes + serializes.
	Triggers *TriggerStore
	// boolexpr condition arena (#457): flat And/Or/Not/Cond nodes indexed
	// by ExprRef. Cold-path authoring; hashes + serializes.
	exprArena []exprNode
	// trigger event index (#458): inverted kind->triggers dispatch index,
	// derived from the trigger slab (rebuilt lazily on the store's dirty
	// bit). Not serialized — reconstructed at load.
	trigIndex triggerIndex
	// trigger dispatch scratch (#459): the per-event fire list copied out
	// of the index, and the pending TriggerSleep request set by an action
	// and consumed by the action-runner. Both transient within a flush.
	dispatchBuf  []TriggerID
	trigSleepReq uint32
	// DebugExprImpure, when set (debug/test only), fires loudly if a
	// condition leaf returns different results on a double-eval — a purity
	// violation (execution-model.md §4). nil in release (no double-eval).
	DebugExprImpure func(ref ExprRef)
	// OnTriggerDispatch is the ECA-layer observability hook (R-FSV-3): when
	// set, it is called once per (trigger, event) the dispatcher considers,
	// with the outcome — fired, or why it was skipped. This makes the
	// condition gate and enabled state visible in a log without re-deriving
	// them. nil (the default, including release) is a single branch per
	// trigger and allocates nothing — it never runs on the steady-state path
	// unless an observer is explicitly installed.
	OnTriggerDispatch func(d TriggerDispatch)
	pathReqs    []pathRequest
}

// NewWorld allocates every pool at the resolved capacities. The
// entity table covers everything that can hold an EntityID: units,
// projectiles, persistent effects, and promoted doodads.
func NewWorld(requested Caps) *World {
	caps := requested.resolve()
	entityCap := caps.Units + caps.Projectiles + caps.Effects + caps.ScriptedDoodads + caps.Destructables
	// entity indices run 1..entityCap (slot 0 reserved, entity.go) —
	// every array indexed by EntityID.Index() needs entityCap+1 room
	idxSpace := entityCap + 1
	w := &World{
		caps:               caps,
		todScale:           fixed.One,
		dayLengthTicks:     DefaultDayLengthTicks,
		Cmds:               newCommandQueue(),
		cmdStaging:         make([]WorldCommand, 0, 1024),
		cmdActive:          make([]WorldCommand, 0, 1024),
		killed:             make([]EntityID, 0, caps.Units+caps.Projectiles),
		dmgBuf:             make([]DamagePacket, 0, caps.Units*4),
		rng:                prng.New(0, 0),
		Snaps:              newSnapshotBuffers(idxSpace, caps.PendingEvents),
		snapNoLerp:         make([]bool, idxSpace),
		snapDeath:          make([]bool, idxSpace),
		snapMarked:         make([]EntityID, 0, idxSpace),
		renderEvStaging:    make([]RenderEvent, 0, caps.PendingEvents),
		Sched:              sched.New(),
		Ents:               NewEntities(entityCap),
		Transforms:         NewTransformStore(entityCap, idxSpace),
		Movements:          NewMovementStore(caps.Units, idxSpace),
		Collisions:         NewCollisionStore(caps.Units, idxSpace),
		Healths:            NewHealthStore(caps.Units, idxSpace),
		Owners:             NewOwnerStore(caps.Units, idxSpace),
		UnitTypes:          NewUnitTypeStore(caps.Units, idxSpace),
		UserDatas:          NewUserDataStore(caps.Units, idxSpace),
		UnitNames:          NewUnitNameStore(caps.Units, idxSpace),
		Hiddens:            newPresenceSet(caps.Units, idxSpace),
		XPSuspends:         newPresenceSet(caps.Units, idxSpace),
		Pauses:             newPresenceSet(caps.Units, idxSpace),
		Flys:               NewFlyStore(caps.Units, idxSpace),
		PropWindows:        NewPropWindowStore(caps.Units, idxSpace),
		Regions:            NewRegionStore(caps.Units, idxSpace),
		Combats:            NewCombatStore(caps.Units, idxSpace),
		Abilities:          NewAbilityStore(caps.Units, idxSpace),
		AbilityFields:      NewAbilityFieldStore(caps.Units*AbilityOverrideCapPerUnit, idxSpace),
		WeaponOverrides:    NewWeaponFieldStore(caps.Units*WeaponOverrideCapPerUnit, idxSpace),
		Invents:            NewInventoryStore(caps.Units, idxSpace),
		Orders:             NewOrderStore(caps.Units, idxSpace),
		Buffs:              NewBuffPool(caps.BuffInstances),
		Missiles:           NewMissileStore(caps.Projectiles, idxSpace),
		Effects:            NewEffectStore(caps.Effects, idxSpace),
		Nodes:              NewResourceNodeStore(caps.Units, idxSpace),
		Econs:              NewEconStore(caps.Units, idxSpace),
		Harvests:           NewHarvestStore(caps.Units, idxSpace),
		Produce:            NewProduceStore(caps.Units, idxSpace),
		Heroes:             NewHeroStore(caps.Units, idxSpace),
		Items:              NewItemStore(caps.Units, idxSpace),
		Patrol:             NewPatrolStore(caps.Units, idxSpace),
		Build:              NewBuildStore(caps.Units, idxSpace),
		Visibility:         newVisibilityGrid(idxSpace, caps.Units),
		FogMods:            NewFogModifierStore(maxFogModifiers),
		ShareVisions:       NewShareVisionStore(caps.Units, idxSpace),
		orderPool:          make([]orderEntry, caps.OrderQueueEntries),
		events:             make([]Event, caps.PendingEvents),
		handlers:           make(map[HandlerID]EventHandler),
		handlerReg:         newHandlerRegistry(),
		Triggers:           NewTriggerStore(caps.Triggers),
		pathReqs:           make([]pathRequest, caps.PathRequests),
		Doodads:            NewDoodadStore(caps.ScriptedDoodads, idxSpace),
		Destructables:      NewDestructableStore(caps.Destructables, idxSpace),
		Paths:              path.NewPathStore(caps.PathRequests, 1024),
		runtimeAbilityDefs: make([]data.Ability, 0, caps.RuntimeAbilityDefs),
		effectRegNames:     make([]string, 0, caps.RuntimeEffects),
		effectRegExecs:     make([]RuntimeEffectExec, 0, caps.RuntimeEffects),
		trigNameKeys:       make([]string, 0, maxNamedTriggers),
		trigNameIDs:        make([]TriggerID, 0, maxNamedTriggers),
		bucketHead:         make([]int32, bucketCount),
		bucketNext:         make([]int32, idxSpace),
		bucketPrev:         make([]int32, idxSpace),
		bucketCell:         make([]int32, idxSpace),
		bucketID:           make([]EntityID, idxSpace),
		acquireEvery:       DefaultAcquireInterval,
		areaScratch:        make([]EntityID, 0, 64),
		areaDistHi:         make([]uint64, 0, 64),
		areaDistLo:         make([]uint64, 0, 64),
		buffScratch:        make([]int32, 0, caps.BuffInstances),
		auraScratch:        make([]EntityID, 0, caps.Units),
	}
	for s := 0; s < int(data.BuffStatCount); s++ {
		w.buffAdd[s] = make([]int64, idxSpace)
		w.buffMult[s] = make([]fixed.F64, idxSpace)
		for i := range w.buffMult[s] {
			w.buffMult[s][i] = fixed.One
		}
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
	w.initPlayers()
	w.registerTriggerDispatch()   // ECA action-runner continuation (#459)
	w.installBaseDamageFormula() // ordered damage-formula pipeline (#473)
	w.armorK = defaultArmorK     // configurable armor reduction (#474); default LUT
	w.armorLUT = armorMult
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
	w.detachItem(id) // a dying ITEM clears its carrier's slot (#305)
	if ir := w.Invents.Row(id); ir != -1 {
		// before the Transform goes: death drops need the carrier's
		// position to find adjacent ground cells
		w.releaseInventory(id, ir)
	}
	w.bucketRemove(id)
	w.Transforms.Remove(id)
	if r := w.Movements.Row(id); r != -1 {
		w.releaseMoveHandle(r)      // free pooled path or shared flow slot
		w.releaseReservation(r, id) // free the avoidance cell (no leak)
		w.Movements.Remove(id)
	}
	if w.Collisions.Row(id) != -1 {
		w.Collisions.Remove(id)
	}
	if w.Healths.Row(id) != -1 {
		w.Healths.Remove(id)
	}
	if w.Econs.Row(id) != -1 { // before Owners.Remove: the ledger needs the player
		if or := w.Owners.Row(id); or != -1 {
			if p := w.Owners.Player[or]; p < MaxPlayers {
				cost, provided, _ := w.Econs.remove(id)
				w.foodUsed[p] -= int32(cost)
				w.foodCap[p] -= int32(provided)
			}
		} else {
			w.Econs.remove(id)
		}
	}
	if pr := w.Produce.Row(id); pr != -1 { // before Owners.Remove: the food ledger needs the player
		w.releaseTrainReservations(pr) // resources stay spent (destruction is not a cancel)
		w.Produce.Remove(id)
	}
	w.captureHeroDeath(id) // before Owners.Remove: the dead pool needs the player
	if w.Owners.Row(id) != -1 {
		w.Owners.Remove(id)
	}
	if w.UnitTypes.Row(id) != -1 {
		w.UnitTypes.Remove(id)
	}
	if w.UserDatas.Row(id) != -1 {
		w.UserDatas.Remove(id)
	}
	if w.UnitNames.Row(id) != -1 {
		w.UnitNames.Remove(id)
	}
	if w.Hiddens.Row(id) != -1 {
		w.Hiddens.Remove(id)
	}
	if w.XPSuspends.Row(id) != -1 {
		w.XPSuspends.Remove(id)
	}
	if w.Pauses.Row(id) != -1 {
		w.Pauses.Remove(id)
	}
	if w.Flys.Row(id) != -1 {
		w.Flys.Remove(id)
	}
	if w.PropWindows.Row(id) != -1 {
		w.PropWindows.Remove(id)
	}
	if w.ShareVisions.Row(id) != -1 {
		w.ShareVisions.Remove(id)
	}
	if w.Combats.Row(id) != -1 {
		w.Combats.Remove(id)
	}
	w.WeaponOverrides.RemoveEntity(id) // #476: drop live weapon overrides
	if w.Abilities.Row(id) != -1 {
		w.AbilityFields.RemoveEntity(id)
		w.Abilities.Remove(id)
	}
	if w.Items.Row(id) != -1 {
		w.Items.Remove(id)
	}
	if w.Patrol.Row(id) != -1 {
		w.Patrol.Remove(id)
	}
	w.destroyBuild(id) // unstamp a structure's footprint / release a dead builder (#301)
	if w.Invents.Row(id) != -1 {
		w.Invents.Remove(id)
	}
	if r := w.Orders.Row(id); r != -1 {
		w.clearOrderQueue(r) // recycle pooled entries (no leak)
		w.Orders.Remove(id)
	}
	if w.Nodes.Row(id) != -1 {
		w.Nodes.Remove(id)
	}
	if w.Harvests.Row(id) != -1 {
		w.Harvests.Remove(id)
	}
	isMissile := w.Missiles.Row(id) != -1
	if isMissile {
		w.Missiles.Remove(id)
	}
	isEffect := w.Effects.Row(id) != -1
	if isEffect {
		w.Effects.Remove(id)
	}
	if w.Visibility != nil {
		idx := id.Index()
		if idx < uint32(len(w.Visibility.entityFlags)) {
			w.Visibility.entityFlags[idx] = 0
		}
	}
	// derived-stat cache back to identity NOW — the entity index can
	// be reused before the next buff sweep frees the dead carrier's
	// instances (buff.go #162)
	for s := 0; s < int(data.BuffStatCount); s++ {
		w.buffAdd[s][id.Index()] = 0
		w.buffMult[s][id.Index()] = fixed.One
	}
	if !w.Ents.Destroy(id) {
		return false
	}
	if !isMissile && !isEffect {
		w.unitCount-- // missiles never counted against the unit cap
	}
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
		combatRowB         = 8 + 2 + 2 + 2 + 4 + 4 + 16 + 4 + 8 + 8 + 4 + 4 + 4 + 4
		abilityRowB        = AbilitySlots*(2+1+4+2+1) + 4
		runtimeAbilityRowB = 56 // data.Ability struct on supported 64-bit targets
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
	n += len(w.UserDatas.Value) * (4 + 4)
	n += len(w.UserDatas.rowOf) * rowOfB
	n += len(w.UnitNames.Name) * (4 + 32) // 4-byte len + rough name bytes
	n += len(w.UnitNames.rowOf) * rowOfB
	n += len(w.Hiddens.Entity) * 4
	n += len(w.Hiddens.rowOf) * rowOfB
	n += len(w.XPSuspends.Entity) * 4
	n += len(w.XPSuspends.rowOf) * rowOfB
	n += len(w.Pauses.Entity) * 4
	n += len(w.Pauses.rowOf) * rowOfB
	n += len(w.Combats.DmgBase) * combatRowB
	n += len(w.Combats.rowOf) * rowOfB
	n += len(w.Abilities.AbilityID) * abilityRowB
	n += len(w.Abilities.rowOf) * rowOfB
	n += cap(w.runtimeAbilityDefs) * runtimeAbilityRowB
	n += len(w.AbilityFields.Ent)*(4+1+1+8+1) + len(w.AbilityFields.free)*4 +
		len(w.AbilityFields.rowOf)*rowOfB + len(w.AbilityFields.perUnit)
	n += len(w.Invents.Slots) * (InventorySlots*4 + 4)
	n += len(w.Invents.rowOf) * rowOfB
	n += len(w.Orders.Kind) * (1 + 1 + 4 + 16 + 4 + 4)
	n += len(w.Orders.rowOf) * rowOfB
	n += cap(w.Missiles.Entity) * 138 // MissileStore columns
	n += len(w.Effects.ModelID) * (2 + 8 + 4 + 4 + 4)
	n += len(w.Effects.rowOf) * rowOfB
	n += w.Visibility.PreallocatedBytes()
	n += w.Buffs.Cap() * 24    // BuffInstance + free/live bookkeeping
	for s := range w.buffAdd { // derived-stat cache (#162)
		n += len(w.buffAdd[s])*8 + len(w.buffMult[s])*8
	}
	n += cap(w.buffScratch) * 4
	n += len(w.orderPool) * 32
	n += len(w.events) * 24
	n += len(w.pathReqs) * 24
	n += int(0) + len(w.Doodads.Placement)*32
	// destructables (#229): 11 SoA columns (~36 B/row) + the entity->row map.
	n += cap(w.Destructables.Type) * 36
	n += len(w.Destructables.rowOf) * rowOfB
	return n
}
