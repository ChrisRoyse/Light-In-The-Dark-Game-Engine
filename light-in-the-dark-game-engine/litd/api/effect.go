package litd

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

const defaultEffectColorRGBA uint32 = 0xffffffff

// RegisterEffectModel binds a public script model key to the
// setup-time asset/model id used by the sim effect store. Unknown keys
// in AddSpecialEffect fail closed with a zero handle.
func (g *Game) RegisterEffectModel(model string, id uint16) {
	if g == nil || model == "" {
		return
	}
	if g.effectModels == nil {
		g.effectModels = make(map[string]uint16)
	}
	g.effectModels[model] = id
}

// AddSpecialEffect creates a persistent special effect at pos and
// returns its handle, or the zero Effect on an unknown model or pool
// exhaustion. JASS: AddSpecialEffect/AddSpecialEffectLoc.
// JASS: AddSpecialEffect, AddSpecialEffectLoc, AddSpecialEffectLocBJ
func (g *Game) AddSpecialEffect(model string, pos Vec2) Effect {
	if g == nil || g.w == nil {
		return Effect{}
	}
	modelID, ok := g.effectModel(model)
	if !ok {
		g.reportInvalid("Game.AddSpecialEffect (unknown model)")
		return Effect{}
	}
	id, ok := g.w.SpawnEffect(sim.EffectSpec{
		ModelID:   modelID,
		Pos:       vec(pos),
		Scale:     fixed.One,
		ColorRGBA: defaultEffectColorRGBA,
	})
	if !ok {
		g.reportInvalid("Game.AddSpecialEffect (pool exhausted)")
		return Effect{}
	}
	return Effect{id: id, g: g}
}

// Effects returns every live special effect in ascending creation order,
// non-nil. Render harnesses and tools use this to draw or inspect
// script-spawned effects the way AllUnits exposes units (#529).
func (g *Game) Effects() []Effect {
	return g.AppendEffects(make([]Effect, 0))
}

// AppendEffects appends every live special effect to dst and returns the
// extended slice. Pass a reused backing slice for allocation-free enumeration.
func (g *Game) AppendEffects(dst []Effect) []Effect {
	if g == nil || g.w == nil {
		return dst
	}
	es := g.w.Effects
	n := es.Count()
	for r := int32(0); r < n; r++ {
		dst = append(dst, Effect{id: es.Entity[r], g: g})
	}
	return dst
}

// ID returns the effect's stable entity id (0 for an invalid handle) — the
// effect counterpart of Unit.ID, for FSV dumps and render-side keying. JASS:
// GetHandleId.
func (e Effect) ID() uint32 {
	if !e.Valid() {
		return 0
	}
	return uint32(e.id)
}

// Position returns the effect's world position, or the zero Vec2 on an
// invalid handle. JASS: BlzGetLocalSpecialEffectX, BlzGetLocalSpecialEffectY.
func (e Effect) Position() Vec2 {
	if !e.Valid() {
		e.g.reportInvalid("Effect.Position")
		return Vec2{}
	}
	r := e.g.w.Transforms.Row(e.id)
	if r < 0 {
		return Vec2{}
	}
	p := e.g.w.Transforms.Pos[r]
	return Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}
}

// Destroy removes the effect. It emits the sim-side end cue before the
// row is freed, matching WC3 DestroyEffect's presentation lifetime.
// JASS: DestroyEffect, DestroyEffectBJ
func (e Effect) Destroy() {
	if !e.Valid() {
		e.g.reportInvalid("Effect.Destroy")
		return
	}
	e.g.w.DestroyEffect(e.id)
}

// SetScale changes the effect's presentation scale. No-op on an
// invalid handle.
// JASS: BlzSetSpecialEffectMatrixScale, BlzSetSpecialEffectScale
func (e Effect) SetScale(scale float64) {
	if !e.Valid() {
		e.g.reportInvalid("Effect.SetScale")
		return
	}
	e.g.w.SetEffectScale(e.id, fromFloat(scale))
}

// SetColor changes the effect's RGB tint and keeps it fully opaque.
// The sim store layout is RGBA, so SetColor(0xaa,0xbb,0xcc) writes
// 0xaabbccff.
// JASS: BlzSetSpecialEffectAlpha, BlzSetSpecialEffectColor, BlzSetSpecialEffectColorByPlayer
func (e Effect) SetColor(r, g, b uint8) {
	if !e.Valid() {
		e.g.reportInvalid("Effect.SetColor")
		return
	}
	e.g.w.SetEffectColor(e.id, colorRGBA(r, g, b))
}

// SetPosition moves the effect to pos. No-op on an invalid handle.
// JASS: BlzSetSpecialEffectHeight, BlzSetSpecialEffectPosition, BlzSetSpecialEffectPositionLoc, BlzSetSpecialEffectX, BlzSetSpecialEffectY, BlzSetSpecialEffectZ
func (e Effect) SetPosition(pos Vec2) {
	if !e.Valid() {
		e.g.reportInvalid("Effect.SetPosition")
		return
	}
	e.g.w.SetEffectPos(e.id, vec(pos))
}

func (g *Game) effectAlive(id sim.EntityID) bool {
	return g != nil && g.w != nil && g.w.Ents.Alive(id) && g.w.Effects.Row(id) != -1
}

func (g *Game) effectModel(model string) (uint16, bool) {
	if g == nil || model == "" || g.effectModels == nil {
		return 0, false
	}
	id, ok := g.effectModels[model]
	return id, ok
}

func colorRGBA(r, g, b uint8) uint32 {
	return uint32(r)<<24 | uint32(g)<<16 | uint32(b)<<8 | 0xff
}
