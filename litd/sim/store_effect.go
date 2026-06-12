package sim

// Persistent script effects (#348). These are long-lived, deterministic
// presentation entities created by scripts (AddSpecialEffect-style) and
// distinct from one-shot RenderEvent VFX. Each effect has a Transform
// row so snapshot publication already exposes its position.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

type EffectSpec struct {
	ModelID   uint16
	Pos       fixed.Vec2
	Scale     fixed.F64
	ColorRGBA uint32
}

type EffectStore struct {
	ModelID   []uint16
	Scale     []fixed.F64
	ColorRGBA []uint32
	BirthTick []uint32
	Entity    []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewEffectStore(rowCap, entityCap int) *EffectStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &EffectStore{
		ModelID:   make([]uint16, rowCap),
		Scale:     make([]fixed.F64, rowCap),
		ColorRGBA: make([]uint32, rowCap),
		BirthTick: make([]uint32, rowCap),
		Entity:    make([]EntityID, rowCap),
		rowOf:     make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *EffectStore) Add(e *Entities, id EntityID, spec EffectSpec, birthTick uint32) bool {
	if !e.Alive(id) {
		s.assert("Add on dead entity", id)
		return false
	}
	idx := id.Index()
	if s.rowOf[idx] != -1 {
		s.assert("double Add", id)
		return false
	}
	if int(s.count) == len(s.Entity) {
		return false
	}
	r := s.count
	s.ModelID[r] = spec.ModelID
	s.Scale[r] = spec.Scale
	s.ColorRGBA[r] = spec.ColorRGBA
	s.BirthTick[r] = birthTick
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *EffectStore) Remove(id EntityID) bool {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		s.assert("Remove with malformed handle", id)
		return false
	}
	r := s.rowOf[idx]
	if r == -1 {
		s.assert("Remove of absent component", id)
		return false
	}
	last := s.count - 1
	if r != last {
		s.ModelID[r] = s.ModelID[last]
		s.Scale[r] = s.Scale[last]
		s.ColorRGBA[r] = s.ColorRGBA[last]
		s.BirthTick[r] = s.BirthTick[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *EffectStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *EffectStore) Count() int32 { return s.count }
func (s *EffectStore) Cap() int     { return len(s.Entity) }

func (s *EffectStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

func (w *World) SpawnEffect(spec EffectSpec) (EntityID, bool) {
	if int(w.Effects.Count()) == w.Effects.Cap() {
		return 0, false
	}
	id, ok := w.Ents.Create()
	if !ok {
		return 0, false
	}
	if !w.Transforms.Add(w.Ents, id, spec.Pos, 0) {
		w.Ents.Destroy(id)
		return 0, false
	}
	if !w.Effects.Add(w.Ents, id, spec, w.tick) {
		w.Transforms.Remove(id)
		w.Ents.Destroy(id)
		return 0, false
	}
	w.bucketInsert(id, spec.Pos)
	w.MarkSnap(id)
	w.EmitRenderEvent(RenderEffectSpawn, id, spec.ModelID)
	return id, true
}

func (w *World) DestroyEffect(id EntityID) bool {
	r := w.Effects.Row(id)
	if !w.Ents.Alive(id) || r == -1 {
		return false
	}
	model := w.Effects.ModelID[r]
	w.EmitRenderEvent(RenderEffectEnd, id, model)
	w.bucketRemove(id)
	w.Effects.Remove(id)
	w.Transforms.Remove(id)
	return w.Ents.Destroy(id)
}

func (w *World) SetEffectScale(id EntityID, scale fixed.F64) bool {
	r := w.Effects.Row(id)
	if r == -1 || !w.Ents.Alive(id) {
		return false
	}
	w.Effects.Scale[r] = scale
	return true
}

func (w *World) SetEffectColor(id EntityID, colorRGBA uint32) bool {
	r := w.Effects.Row(id)
	if r == -1 || !w.Ents.Alive(id) {
		return false
	}
	w.Effects.ColorRGBA[r] = colorRGBA
	return true
}

func (w *World) SetEffectPos(id EntityID, pos fixed.Vec2) bool {
	if w.Effects.Row(id) == -1 {
		return false
	}
	return w.TeleportUnit(id, pos)
}
