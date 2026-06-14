package sim

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// The 7-phase tick (tick-and-scheduler.md §4, ecs-architecture.md §6).
// Step is THE hand-written tick function: phases run in fixed order
// from one function, systems are concrete code — no System interface,
// no registration slice, nothing dynamic (ecs §7). Game time is the
// integer tick counter; one tick is exactly 50 ms of game time and no
// float delta-time exists anywhere in litd/sim (R-SIM-1).
//
// Order-sensitive same-tick effects use deferred buffers: a kill
// lands in the killed buffer during phase 5, its death event fires in
// phase 6 while the entity still exists, and the entity is removed in
// phase 7's second pass (ecs §6 deferred-effect rule).

// WorldCommand is one player/script command. Commands enqueue into a
// staging buffer at any time; phase 1 of the NEXT tick applies them —
// a command can never act in the tick that issued it.
type WorldCommand struct {
	Kind  uint8
	Unit  EntityID
	Point fixed.Vec2
}

// Tick returns the current game time in ticks (50 ms each).
func (w *World) Tick() uint32 { return w.tick }

// EnqueueCommand stages a command for the next tick's input phase.
// Returns false when the staging buffer is full (gameplay outcome).
func (w *World) EnqueueCommand(c WorldCommand) bool {
	if len(w.cmdStaging) == cap(w.cmdStaging) {
		return false
	}
	w.cmdStaging = append(w.cmdStaging, c)
	return true
}

// KillUnit marks a unit dead this tick. The entity stays alive (and
// addressable by phase-6 event handlers) until phase 7 removes it.
// Duplicate kills of the same entity in one tick collapse to one.
func (w *World) KillUnit(id EntityID) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	for i := range w.killed {
		if w.killed[i] == id {
			return true // already marked this tick
		}
	}
	if len(w.killed) == cap(w.killed) {
		return false
	}
	w.killed = append(w.killed, id)
	return true
}

// Step advances the simulation by exactly one tick.
func (w *World) Step() {
	w.inStep = true
	w.tick++
	w.runPhase(1, "input", (*World).phaseInput)
	w.runPhase(2, "scripts", (*World).phaseScripts)
	w.runPhase(3, "orders", (*World).phaseOrders)
	w.runPhase(4, "movement", (*World).phaseMovement)
	w.runPhase(5, "combat", (*World).phaseCombat)
	w.runPhase(6, "events", (*World).phaseEvents)
	w.runPhase(7, "cleanup", (*World).phaseCleanup)
	w.inStep = false
}

func (w *World) runPhase(n int, name string, f func(*World)) {
	if w.PhaseTrace != nil {
		w.PhaseTrace(w.tick, n, name)
	}
	f(w)
}

// Phase 1 — input: drain pending command records for this tick in
// (Player, Seq) order with deterministic validation (command.go),
// then the legacy WorldCommand double buffer. Records staged DURING
// this tick wait in the staging buffer until the driver's next
// IngestStagedCommands call — a command can never act in the tick
// that issued it.
func (w *World) phaseInput() {
	w.consumePendingCommands()
	w.cmdActive, w.cmdStaging = w.cmdStaging, w.cmdActive[:0]
	for i := range w.cmdActive {
		if w.OnCommand != nil {
			w.OnCommand(w.tick, w.cmdActive[i])
		}
		// order-component application lands with #144/#146; the
		// double-buffer timing contract is authoritative already
	}
}

// Phase 2 — scripts: the deterministic scheduler drains due
// suspensions in (wakeTick, seq) order (script_phase.go).
func (w *World) phaseScripts() { w.scriptPhase() }

// Phase 3 — orders: drive current orders, pop completed ones, fall
// through to the default order (orders.go), then advance production
// queues (produce.go #302).
func (w *World) phaseOrders() {
	w.pathSeq = 0
	w.ordersSystem()
	w.produceSystem()
	w.constructionSystem() // rising structures ramp HP / complete (#301)
}

// Phase 4 — movement: waypoint following, fixed-point integration,
// turn-rate-limited facing (movement.go), then the incremental
// bucket-grid rebuild over everything that moved (buckets.go §3.1).
func (w *World) phaseMovement() {
	w.pathingSystem()
	w.movementSystem()
	w.flySystem()     // fly-height climb integration (#367)
	w.missileSystem() // flight at the movement-phase tail (#158)
	w.bucketReconcile()
	w.visibilitySystem()
}

// Phase 5 — combat: throttled target acquisition (acquire.go), then
// attack cycles (#150), then the SINGLE deferred-damage apply pass
// (damage.go) — everything queued this phase lands in append order.
// Kills mark the deferred buffer — removal is phase 7's job.
func (w *World) phaseCombat() {
	w.abilitySystem() // casts before autoattacks: an ordered cast owns the unit
	w.acquisitionSystem()
	w.attackSystem()
	w.auraSystem()         // throttled child maintenance before the periodic pass (#164)
	w.buffPeriodicSystem() // periodic ticks land in this tick's apply pass (#162)
	if w.OnCombatPhase != nil {
		w.OnCombatPhase(w.tick)
	}
	w.damageApplySystem()
}

// Phase 6 — events: deterministically ordered dispatch (#88). Death
// events for this tick's kills fire here, while the entities still
// exist and their components remain readable.
func (w *World) phaseEvents() {
	for i := range w.killed {
		if w.OnDeathEvent != nil {
			w.OnDeathEvent(w.tick, w.killed[i])
		}
		w.Emit(Event{Kind: EvUnitDeath, Src: w.killed[i]})
	}
	w.regionSystem() // region enter/leave (incl. death-inside leaves) before flush (#241)
	w.resolveMatchResults()
	w.flushEvents()
}

// Phase 7 — cleanup: snapshot publish FIRST (entities killed this
// tick appear one last time with the death cue), then the deferred
// removals (second pass), then state hash on cadence.
func (w *World) phaseCleanup() {
	w.buffExpirySystem() // before removals: dying carriers still resolvable (#162)
	w.advanceTimeOfDay()
	w.publishSnapshot()
	for i := range w.killed {
		w.DestroyUnit(w.killed[i])
	}
	w.killed = w.killed[:0]
	if w.OnSnapshot != nil {
		w.OnSnapshot(w.tick)
	}
	if w.OnHash != nil && w.HashEvery > 0 && w.tick%w.HashEvery == 0 {
		w.OnHash(w.tick)
	}
}
