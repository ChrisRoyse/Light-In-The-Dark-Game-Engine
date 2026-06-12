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

// RenderEvent is a one-shot presentation cue (attack impact sound,
// death animation start) tagged with the tick it belongs to; render
// fires it when crossing that tick boundary.
type RenderEvent struct {
	Tick uint32
	Kind uint8
	Ent  EntityID
	Data uint16
}

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
	if len(w.renderEvStaging) == cap(w.renderEvStaging) {
		w.renderEvDropped++
		return false
	}
	w.renderEvStaging = append(w.renderEvStaging, RenderEvent{Kind: kind, Ent: ent, Data: data})
	return true
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
			frac = lifeFrac(w.Healths.Life[hr], w.Healths.MaxLife[hr])
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
