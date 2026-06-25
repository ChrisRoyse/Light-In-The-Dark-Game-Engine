package sim

// Render snapshot publication (tick-and-scheduler.md §2): the ENTIRE
// sim→render surface. After each tick, phase 7 copies per-entity
// position, facing, animation cue, and life fraction into one of two
// preallocated buffers; render interpolates lerp(Prev, Curr, alpha)
// and never touches live ECS stores (PRD §4.1 — render reads sim
// state, never mutates, and here it doesn't even read the stores).
//
// Discontinuities don't interpolate: spawns, teleports, and deaths
// set SnapNoLerp so render does not smear a blink across 50 ms. A
// dying entity is present in its death tick's snapshot (with the
// death cue — publish runs BEFORE phase 7's deferred removal) and
// absent from the next.
//
// Threading contract: publish happens inside Step (sim thread); the
// driver must not let render read while a Step is in flight. The
// M0.5 demo's single-threaded frame loop satisfies this trivially; a
// render thread adds its own handover at the driver seam — never
// locks in here (the sim stays lock-free).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// Snapshot entry flags.
const (
	// SnapNoLerp: do not interpolate into this tick's pose —
	// teleport, spawn, or death discontinuity.
	SnapNoLerp uint8 = 1 << 0
	// SnapDeath: the entity died this tick; render starts the death
	// presentation. The entry is absent from the next snapshot.
	SnapDeath uint8 = 1 << 1
)

// SnapshotEntry is one entity's render state — pure values, no
// pointers into sim stores.
type SnapshotEntry struct {
	ID       EntityID
	Pos      fixed.Vec2
	Facing   fixed.Angle
	Anim     uint16 // animation cue (movement state today; systems refine)
	LifeFrac uint16 // 0..65535 = dead..full; full when no Health row
	Flags    uint8
}

// MissileSnapEntry is one in-flight missile's render state (#309).
// Missiles are first-class entities (#158) with Transforms rows, so
// Pos/Facing come from the same place as units; the presentation-only
// Arc (parabola peak height — flight itself is straight) and the
// guidance kind come from the missile store. Render draws these as
// arced billboards, never as unit models, so they are published
// separately from Entries — the model mirror never sees a missile.
type MissileSnapEntry struct {
	ID         EntityID
	Pos        fixed.Vec2
	Facing     fixed.Angle
	Arc        fixed.F64
	GuidanceID uint16
	// LifeFrac is flight progress on [0,65535] = launch..impact, derived from sim
	// missile state (#528): the render places the billboard on its parabola via
	// ArcHeight(Arc, LifeFrac/65535) without a host-side launch side channel.
	// Render-only — this surface is never hashed.
	LifeFrac uint16
}

// RenderEvent is a one-shot presentation cue (attack impact sound,
// death animation start) tagged with the tick it belongs to; render
// fires it when crossing that tick boundary. Pos carries a world point
// for cues whose render reaction needs a position that won't survive in
// the next snapshot — e.g. a missile impact removes its missile that
// tick (#309); it is the zero vector for cues that don't use it.
type RenderEvent struct {
	Tick  uint32
	Kind  uint8
	Ent   EntityID
	Data  uint16
	Owner int16 // owning player slot at emit time, -1 if none (#666)
	Pos   fixed.Vec2
}

// RenderEvent kind values. Append-only: render consumers may switch on
// these stable byte values in recorded snapshots.
const (
	RenderEffectSpawn       uint8 = 1  // Ent = effect, Data = ModelID
	RenderEffectEnd         uint8 = 2  // Ent = effect, Data = ModelID
	RenderUnitDeath         uint8 = 3  // Ent = dying unit, Data = unit-type id (#313 sound/anim cue)
	RenderUnitReady         uint8 = 4  // Ent = trained unit, Data = unit-type id (#313 "ready" cue)
	RenderUnitAttack        uint8 = 5  // Ent = attacker, Data = unit-type id (#313 attack-swing cue)
	RenderSpellCue          uint8 = 6  // Ent = cued unit, Data = unit-type id (#479 script-emitted spell VFX cue)
	RenderUnitOrderAck      uint8 = 7  // Ent = ordered unit, Data = unit-type id (#313 order-ack cue; render filters to local player)
	RenderUnderAttack       uint8 = 8  // Ent = damaged unit, Data = unit-type id (#313 under-attack stinger; sim-throttled per defender, render filters to local player)
	RenderMissileImpact     uint8 = 9  // Ent = impacting missile, Data = MissileImpact* id, Pos = impact point (#309 impact one-shot VFX)
	RenderDestructableDeath uint8 = 10 // Ent = dying destructable, Data = destructable type id, Pos = its location (#72 chunk doodad-mesh rebuild + death VFX)
)

// Snapshot is one published frame of sim state plus that tick's
// render events. TimeOfDay is the game clock quantized onto the u16
// ring: render interpolates the shortest wrap-safe delta as
// int16(curr.TimeOfDay - prev.TimeOfDay). Entries and Events are
// reslices of preallocated backing arrays — never reallocated after
// NewWorld (R-GC-1/2).
type Snapshot struct {
	Tick      uint32
	TimeOfDay uint16
	Entries   []SnapshotEntry
	Missiles  []MissileSnapEntry
	Events    []RenderEvent
}

// SnapshotBuffers double-buffers snapshots: publish fills the back
// buffer and flips. Backing arrays are stable for the life of the
// world — the two Entries pointers ping-pong, render can hold either.
type SnapshotBuffers struct {
	bufs      [2]Snapshot
	curr      int    // index of the most recently published buffer
	published uint64 // lifetime publish count
}

func newSnapshotBuffers(entityCap, eventCap int) *SnapshotBuffers {
	sb := &SnapshotBuffers{}
	for i := range sb.bufs {
		sb.bufs[i].Entries = make([]SnapshotEntry, 0, entityCap)
		sb.bufs[i].Missiles = make([]MissileSnapEntry, 0, entityCap)
		sb.bufs[i].Events = make([]RenderEvent, 0, eventCap)
	}
	return sb
}

// Curr returns the most recently published snapshot.
func (sb *SnapshotBuffers) Curr() *Snapshot { return &sb.bufs[sb.curr] }

// Prev returns the snapshot before Curr — the lerp source. Before
// the second publish it is the empty tick-0 snapshot.
func (sb *SnapshotBuffers) Prev() *Snapshot { return &sb.bufs[1-sb.curr] }

// Published returns the lifetime publish count.
func (sb *SnapshotBuffers) Published() uint64 { return sb.published }

// MarkSnap flags an entity's next snapshot entry as a discontinuity
// (no interpolation). Spawns and deaths mark automatically; teleports
// and any future blink effect call this.
func (w *World) MarkSnap(id EntityID) {
	idx := id.Index()
	if idx >= uint32(len(w.snapNoLerp)) {
		return
	}
	if !w.snapNoLerp[idx] {
		w.snapNoLerp[idx] = true
		w.snapMarked = append(w.snapMarked, id)
	}
}

// TeleportUnit moves a unit instantly and marks the discontinuity so
// render snaps instead of smearing the blink across 50 ms.
func (w *World) TeleportUnit(id EntityID, pos fixed.Vec2) bool {
	r := w.Transforms.Row(id)
	if r == -1 || !w.Ents.Alive(id) {
		return false
	}
	w.Transforms.Pos[r] = pos
	w.MarkSnap(id)
	return true
}

// EmitRenderEvent stages a one-shot presentation cue for this tick's
// snapshot. Fails closed (dropped, counted) when the staging buffer
// is full — presentation cues never block the sim.
func (w *World) EmitRenderEvent(kind uint8, ent EntityID, data uint16) bool {
	return w.EmitRenderEventAt(kind, ent, data, fixed.Vec2{})
}

// EmitRenderEventAt stages a positioned presentation cue — for events whose
// render reaction needs a world point not present in the next snapshot (a
// missile impact removes its missile that tick, #309). Fails closed (dropped,
// counted) like EmitRenderEvent.
func (w *World) EmitRenderEventAt(kind uint8, ent EntityID, data uint16, pos fixed.Vec2) bool {
	if len(w.renderEvStaging) == cap(w.renderEvStaging) {
		w.renderEvDropped++
		return false
	}
	// Capture the owning player NOW, while ent is still alive. A death cue
	// (#666) is drained after the unit is removed, so a live Owners.Row lookup at
	// drain time would return -1 — the owner must be snapshot at emit time so a
	// consumer can attribute "whose unit died". Non-hashing presentation data, so
	// this never affects the state hash.
	owner := int16(-1)
	if or := w.Owners.Row(ent); or >= 0 {
		owner = int16(w.Owners.Player[or])
	}
	w.renderEvStaging = append(w.renderEvStaging, RenderEvent{Kind: kind, Ent: ent, Data: data, Owner: owner, Pos: pos})
	return true
}

// EmitUnitRenderCue stages a render cue for a unit, filling Data with the unit's
// type id (so a render consumer can pick the right model/anim) — the shape the
// engine uses for its built-in unit cues (#313). Fail-closed on a non-unit
// entity. The seam a script-facing presentation verb (#479) sits on.
func (w *World) EmitUnitRenderCue(kind uint8, id EntityID) bool {
	ur := w.UnitTypes.Row(id)
	if ur < 0 {
		return false
	}
	return w.EmitRenderEvent(kind, id, w.UnitTypes.TypeID[ur])
}

// lifeFrac quantizes life/maxLife to u16. Entities without a Health
// row read as full; a zero MaxLife reads as dead (fail closed).
func lifeFrac(life, maxLife fixed.F64) uint16 {
	if maxLife <= 0 {
		return 0
	}
	if life >= maxLife {
		return 65535
	}
	if life <= 0 {
		return 0
	}
	return uint16(int64(life) * 65535 / int64(maxLife))
}

func snapshotTimeOfDay(tod fixed.F64) uint16 {
	return uint16(uint64(tod) * 65536 / clockDayRaw)
}

// publishSnapshot fills the back buffer from the live stores and
// flips. Runs in phase 7 BEFORE deferred removal, so entities killed
// this tick appear one last time carrying SnapDeath|SnapNoLerp.
// Zero allocations: every slice is a reslice of preallocated backing.
func (w *World) publishSnapshot() {
	// death marks: killed buffer is still intact here
	for i := range w.killed {
		idx := w.killed[i].Index()
		if idx < uint32(len(w.snapDeath)) {
			w.snapDeath[idx] = true
		}
		w.MarkSnap(w.killed[i])
	}

	sb := w.Snaps
	back := &sb.bufs[1-sb.curr]
	back.Tick = w.tick
	back.TimeOfDay = snapshotTimeOfDay(w.tod)
	back.Entries = back.Entries[:0]
	for r := int32(0); r < w.Transforms.Count(); r++ {
		id := w.Transforms.Entity[r]
		// Mover-driven projectile bodies (#590) share the Transforms store
		// but render as arced billboards, not unit models — published to
		// back.Missiles below, never as a unit entry (#309).
		if w.ProjRender.Row(id) != -1 {
			continue
		}
		idx := id.Index()
		var flags uint8
		if w.snapNoLerp[idx] {
			flags |= SnapNoLerp
		}
		if w.snapDeath[idx] {
			flags |= SnapDeath
		}
		anim := uint16(0)
		if mr := w.Movements.Row(id); mr != -1 {
			anim = uint16(w.Movements.State[mr])
		}
		frac := uint16(65535)
		if hr := w.Healths.Row(id); hr != -1 {
			// Health bar fills against the buffed cap so a +max-life unit reads
			// proportionally (render mirror, not hashed) (#522).
			frac = lifeFrac(w.Healths.Life[hr], w.BuffedMaxLife(id, w.Healths.MaxLife[hr]))
		}
		back.Entries = append(back.Entries, SnapshotEntry{
			ID:       id,
			Pos:      w.Transforms.Pos[r],
			Facing:   w.Transforms.Facing[r],
			Anim:     anim,
			LifeFrac: frac,
			Flags:    flags,
		})
	}

	// mover-driven projectiles (#590): arced-billboard surface, body motion from
	// a mover and render statics from ProjRender. Render-only — never hashed
	// (HashState is wholly separate), so the add is determinism-inert. Flight
	// progress: (Span - remaining)/Span, where remaining is the linear mover's
	// RangeLeft or a point/homing mover's distance to its live goal.
	back.Missiles = back.Missiles[:0]
	pr := w.ProjRender
	for r := int32(0); r < pr.Count(); r++ {
		id := pr.Entity[r]
		tr := w.Transforms.Row(id)
		if tr == -1 {
			continue
		}
		pos := w.Transforms.Pos[tr]
		lifeFrac := uint16(65535)
		if span := int64(pr.Span[r]); span > 0 {
			remaining := span
			if mr, ok := w.Movers.resolve(pr.Mover[r]); ok {
				if MoverKind(w.Movers.Kind[mr]) == MoverLinear {
					remaining = w.Movers.RangeLeft[mr].Floor()
				} else {
					goal := w.Movers.Goal[mr]
					if MoverKind(w.Movers.Kind[mr]) == MoverHoming {
						if ar := w.Transforms.Row(w.Movers.Anchor[mr]); ar != -1 {
							goal = w.Transforms.Pos[ar]
						}
					}
					remaining = flightUnits(pos, goal)
				}
			}
			traveled := span - remaining
			switch {
			case traveled <= 0:
				lifeFrac = 0
			case traveled >= span:
				lifeFrac = 65535
			default:
				lifeFrac = uint16(traveled * 65535 / span)
			}
		}
		back.Missiles = append(back.Missiles, MissileSnapEntry{
			ID:         id,
			Pos:        pos,
			Facing:     w.Transforms.Facing[tr],
			Arc:        pr.Arc[r],
			GuidanceID: pr.Guidance[r],
			LifeFrac:   lifeFrac,
		})
	}

	// render events: copy staged cues, stamped with this tick
	back.Events = back.Events[:0]
	for i := range w.renderEvStaging {
		ev := w.renderEvStaging[i]
		ev.Tick = w.tick
		back.Events = append(back.Events, ev)
	}
	w.renderEvStaging = w.renderEvStaging[:0]

	// clear the per-tick marks (only the dirtied indices)
	for i := range w.snapMarked {
		idx := w.snapMarked[i].Index()
		w.snapNoLerp[idx] = false
		w.snapDeath[idx] = false
	}
	w.snapMarked = w.snapMarked[:0]

	sb.curr = 1 - sb.curr
	sb.published++
}
