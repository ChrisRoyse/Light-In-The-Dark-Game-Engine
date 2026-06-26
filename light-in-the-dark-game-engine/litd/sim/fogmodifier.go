package sim

// Fog-of-war authoring surface (#243; visibility-and-fog.md). The
// visibility grid (visibility.go) is computed from unit sight each tick;
// this file adds the script-facing overrides on top of it:
//
//   - FogModifier: a persistent per-player area override that re-stamps a
//     fog state (visible/explored/hidden) onto the grid after every vision
//     finalize, until stopped/destroyed.
//   - SetFogState*: a one-shot stamp (no lifetime) — overwritten at the
//     next vision update, mirroring WC3.
//   - Global toggles FogEnable / FogMaskEnable, applied at query time.
//   - UnitShareVision: a unit's sight also stamps an allied player's grid.
//
// Everything defaults to inert: no modifiers, fog on, mask on, no shares.
// In that state applyFogModifiers is a no-op, FogStateAt returns the raw
// grid cell, and stampEntityVision stamps only the owner — so the golden
// and determinism traces are byte-identical to pre-#243.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// maxFogModifiers bounds the modifier pool (script-scale; CRUD at setup,
// not per tick). Creation past the cap fails closed.
const maxFogModifiers = 256

// fog modifier shapes.
const (
	fogShapeRect   uint8 = 0
	fogShapeCircle uint8 = 1
)

// FogModifierID is a generational handle into the modifier pool: low 16
// bits = slot, high 16 = generation. The zero value is never valid (live
// modifiers start at generation 1), so a zero handle is a safe no-op.
type FogModifierID uint32

func makeFogModID(slot int32, gen uint16) FogModifierID {
	return FogModifierID(uint32(slot)&0xffff | uint32(gen)<<16)
}
func (id FogModifierID) slot() int32 { return int32(uint32(id) & 0xffff) }
func (id FogModifierID) gen() uint16 { return uint16(uint32(id) >> 16) }

// FogModifierStore is a small generational pool. Columns are slot-indexed;
// a free-list recycles destroyed slots (generation bumped so stale handles
// miss).
type FogModifierStore struct {
	player []uint8
	state  []uint8
	kind   []uint8
	ax, ay []fixed.F64 // rect: min corner / circle: center
	bx, by []fixed.F64 // rect: max corner / circle: radius in bx
	shared []bool
	active []bool
	alive  []bool
	gen    []uint16
	count  int32 // high-water mark of slots touched
	free   []int32
}

func NewFogModifierStore(cap int) *FogModifierStore {
	if cap <= 0 {
		cap = maxFogModifiers
	}
	return &FogModifierStore{
		player: make([]uint8, cap),
		state:  make([]uint8, cap),
		kind:   make([]uint8, cap),
		ax:     make([]fixed.F64, cap),
		ay:     make([]fixed.F64, cap),
		bx:     make([]fixed.F64, cap),
		by:     make([]fixed.F64, cap),
		shared: make([]bool, cap),
		active: make([]bool, cap),
		alive:  make([]bool, cap),
		gen:    make([]uint16, cap),
		free:   make([]int32, 0, cap),
	}
}

func (s *FogModifierStore) capacity() int { return len(s.alive) }

// alloc returns a free slot (recycled or fresh) or -1 if the pool is full.
func (s *FogModifierStore) alloc() int32 {
	if n := len(s.free); n > 0 {
		slot := s.free[n-1]
		s.free = s.free[:n-1]
		return slot
	}
	if int(s.count) >= len(s.alive) {
		return -1
	}
	slot := s.count
	s.count++
	return slot
}

// valid reports whether id refers to a live modifier (slot in range, alive,
// generation matches).
func (s *FogModifierStore) valid(id FogModifierID) bool {
	slot := id.slot()
	if slot < 0 || int(slot) >= len(s.alive) || !s.alive[slot] {
		return false
	}
	return s.gen[slot] == id.gen()
}

// ---- world surface: modifier CRUD ----

func (w *World) createFogModifier(player, state, kind uint8, ax, ay, bx, by fixed.F64, shared, started bool) (FogModifierID, bool) {
	if player >= MaxPlayers || state > FogVisible || w.Visibility == nil {
		return 0, false
	}
	s := w.FogMods
	slot := s.alloc()
	if slot < 0 {
		return 0, false
	}
	if s.gen[slot] == 0 {
		s.gen[slot] = 1 // first use: generation 1 so the zero handle is invalid
	}
	s.player[slot] = player
	s.state[slot] = state
	s.kind[slot] = kind
	s.ax[slot], s.ay[slot], s.bx[slot], s.by[slot] = ax, ay, bx, by
	s.shared[slot] = shared
	s.active[slot] = started
	s.alive[slot] = true
	return makeFogModID(slot, s.gen[slot]), true
}

// CreateFogModifierRect creates a rectangular fog-state modifier.
// CreateFogModifierRect.
func (w *World) CreateFogModifierRect(player, state uint8, minx, miny, maxx, maxy fixed.F64, shared, started bool) (FogModifierID, bool) {
	if maxx < minx {
		minx, maxx = maxx, minx
	}
	if maxy < miny {
		miny, maxy = maxy, miny
	}
	return w.createFogModifier(player, state, fogShapeRect, minx, miny, maxx, maxy, shared, started)
}

// CreateFogModifierRadius creates a circular fog-state modifier.
// CreateFogModifierRadius / ...RadiusLoc.
func (w *World) CreateFogModifierRadius(player, state uint8, cx, cy, radius fixed.F64, shared, started bool) (FogModifierID, bool) {
	if radius < 0 {
		radius = 0
	}
	return w.createFogModifier(player, state, fogShapeCircle, cx, cy, radius, 0, shared, started)
}

// StartFogModifier activates a modifier (FogModifierStart). No-op on a
// stale/invalid handle.
func (w *World) StartFogModifier(id FogModifierID) bool {
	if !w.FogMods.valid(id) {
		return false
	}
	w.FogMods.active[id.slot()] = true
	return true
}

// StopFogModifier deactivates a modifier (FogModifierStop). The grid reverts
// to computed vision at the next finalize.
func (w *World) StopFogModifier(id FogModifierID) bool {
	if !w.FogMods.valid(id) {
		return false
	}
	w.FogMods.active[id.slot()] = false
	return true
}

// DestroyFogModifier frees a modifier slot (DestroyFogModifier). The
// generation is bumped so the old handle can never alias the recycled slot.
func (w *World) DestroyFogModifier(id FogModifierID) bool {
	s := w.FogMods
	if !s.valid(id) {
		return false
	}
	slot := id.slot()
	s.alive[slot] = false
	s.active[slot] = false
	s.gen[slot]++ // invalidate the old handle
	s.free = append(s.free, slot)
	return true
}

// FogModifierActive reports whether a modifier handle is live and running
// (read SoT for tests).
func (w *World) FogModifierActive(id FogModifierID) bool {
	return w.FogMods.valid(id) && w.FogMods.active[id.slot()]
}

// FogModifierValid reports whether a handle refers to a live modifier.
func (w *World) FogModifierValid(id FogModifierID) bool { return w.FogMods.valid(id) }

// ---- instant state writes (no modifier lifetime) ----

// SetFogStateRect stamps a fog state over a rectangle immediately. It is
// overwritten at the next vision finalize. SetFogStateRect.
func (w *World) SetFogStateRect(player, state uint8, minx, miny, maxx, maxy fixed.F64, shared bool) {
	if player >= MaxPlayers || state > FogVisible {
		return
	}
	if maxx < minx {
		minx, maxx = maxx, minx
	}
	if maxy < miny {
		miny, maxy = maxy, miny
	}
	w.rasterFogArea(player, state, fogShapeRect, minx, miny, maxx, maxy, shared)
}

// SetFogStateRadius stamps a fog state over a circle immediately.
// SetFogStateRadius / ...RadiusLoc.
func (w *World) SetFogStateRadius(player, state uint8, cx, cy, radius fixed.F64, shared bool) {
	if player >= MaxPlayers || state > FogVisible {
		return
	}
	if radius < 0 {
		radius = 0
	}
	w.rasterFogArea(player, state, fogShapeCircle, cx, cy, radius, 0, shared)
}

// ---- application onto the grid ----

// applyFogModifiers overlays every active modifier onto the finalized grid.
// Called right after finalizeCycle (visibilitySystem and RecomputeVisibility).
// Deterministic: slot order, then shared-vision allies in player order.
func (w *World) applyFogModifiers() {
	s := w.FogMods
	if s == nil || w.Visibility == nil {
		return
	}
	for slot := int32(0); slot < s.count; slot++ {
		if !s.alive[slot] || !s.active[slot] {
			continue
		}
		w.rasterFogArea(s.player[slot], s.state[slot], s.kind[slot],
			s.ax[slot], s.ay[slot], s.bx[slot], s.by[slot], s.shared[slot])
	}
}

// rasterFogArea sets every fog cell inside the area to state for player (and,
// when shared, for each player granted shared vision by player via the
// alliance table). Cell selection mirrors stampVision's quantization.
func (w *World) rasterFogArea(player, state, kind uint8, ax, ay, bx, by fixed.F64, shared bool) {
	v := w.Visibility
	if v == nil {
		return
	}
	w.rasterFogAreaFor(player, state, kind, ax, ay, bx, by)
	if !shared {
		return
	}
	for q := uint8(0); q < MaxPlayers; q++ {
		if q == player {
			continue
		}
		if w.HasAllianceFlag(player, q, AllianceSharedVision) {
			w.rasterFogAreaFor(q, state, kind, ax, ay, bx, by)
		}
	}
}

func (w *World) rasterFogAreaFor(player, state, kind uint8, ax, ay, bx, by fixed.F64) {
	v := w.Visibility
	var minFX, minFY, maxFX, maxFY int32
	switch kind {
	case fogShapeCircle:
		cx, cy, r := ax, ay, bx
		minFX, minFY = worldToFogCellClamped(cx.Sub(r), cy.Sub(r))
		maxFX, maxFY = worldToFogCellClamped(cx.Add(r), cy.Add(r))
	default: // rect
		minFX, minFY = worldToFogCellClamped(ax, ay)
		maxFX, maxFY = worldToFogCellClamped(bx, by)
	}
	for fy := minFY; fy <= maxFY; fy++ {
		for fx := minFX; fx <= maxFX; fx++ {
			cell := fogCellIndex(fx, fy)
			if kind == fogShapeCircle {
				if !distSqLE(fogCellCenter(cell), fixed.Vec2{X: ax, Y: ay}, bx) {
					continue
				}
			}
			v.setStateCell(player, cell, state)
		}
	}
}

// worldToFogCellClamped maps a world point to fog-cell coordinates, clamped
// to the grid (so an area partly off-map still rasterizes its on-map part).
func worldToFogCellClamped(x, y fixed.F64) (fx, fy int32) {
	px := int32(x.Floor() >> 5)
	py := int32(y.Floor() >> 5)
	return clampFogCell(px / FogCellPathingSize), clampFogCell(py / FogCellPathingSize)
}

// ---- global toggles (query-time overrides) ----

// SetFogEnabled turns fog of war on/off globally (FogEnable). When off,
// every point reads back visible. Default: on.
func (w *World) SetFogEnabled(on bool) { w.fogDisabled = !on }

// FogEnabled reports whether fog of war is on (IsFogEnabled).
func (w *World) FogEnabled() bool { return !w.fogDisabled }

// SetFogMaskEnabled turns the black mask on/off (FogMaskEnable). When off,
// never-seen cells read back as explored instead of hidden. Default: on.
func (w *World) SetFogMaskEnabled(on bool) { w.fogMaskDisabled = !on }

// FogMaskEnabled reports whether the black mask is on (IsFogMaskEnabled).
func (w *World) FogMaskEnabled() bool { return !w.fogMaskDisabled }

// ---- per-unit shared vision ----

// ShareVisionStore is a sparse per-unit bitmask: bit q set means the unit's
// owner shares this unit's sight with player q. Empty by default.
type ShareVisionStore struct {
	Mask   []uint16
	Entity []EntityID
	rowOf  []int32
	count  int32
}

func NewShareVisionStore(rowCap, entityCap int) *ShareVisionStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &ShareVisionStore{
		Mask:   make([]uint16, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *ShareVisionStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *ShareVisionStore) Count() int32 { return s.count }

func (s *ShareVisionStore) setBit(id EntityID, player uint8, on bool) bool {
	idx := id.Index()
	if int(idx) >= len(s.rowOf) || player >= MaxPlayers {
		return false
	}
	r := s.rowOf[idx]
	if r == -1 {
		if !on {
			return true // clearing an unset bit: nothing to do
		}
		if int(s.count) == len(s.Mask) {
			return false
		}
		r = s.count
		s.Mask[r] = 0
		s.Entity[r] = id
		s.rowOf[idx] = r
		s.count++
	}
	if on {
		s.Mask[r] |= 1 << player
	} else {
		s.Mask[r] &^= 1 << player
	}
	return true
}

func (s *ShareVisionStore) Remove(id EntityID) bool {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return false
	}
	r := s.rowOf[idx]
	if r == -1 {
		return false
	}
	last := s.count - 1
	if r != last {
		s.Mask[r] = s.Mask[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

// SetShareVision grants or revokes sharing of a unit's sight with a player
// (UnitShareVision). No-op on a dead unit, a unit without a transform, or
// sharing with the unit's own owner. Returns false when it could not apply.
func (w *World) SetShareVision(id EntityID, player uint8, share bool) bool {
	if !w.Ents.Alive(id) || w.Transforms.Row(id) == -1 || player >= MaxPlayers {
		return false
	}
	return w.ShareVisions.setBit(id, player, share)
}

// SharesVisionWith reports whether unit id shares its sight with player
// (read SoT for tests).
func (w *World) SharesVisionWith(id EntityID, player uint8) bool {
	if player >= MaxPlayers {
		return false
	}
	r := w.ShareVisions.Row(id)
	return r != -1 && w.ShareVisions.Mask[r]&(1<<player) != 0
}
