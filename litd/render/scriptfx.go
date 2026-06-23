package render

import "github.com/g3n/engine/math32"

// Script-effect billboard pool (#351, R-GC-2).
//
// Scripts create two kinds of presentation effect over the shared FX class:
//   - persistent effects — long-lived entities from the sim effect store (#348),
//     each a billboard (+ optional point light) keyed by its EntityID and
//     lifecycle-driven by the RenderEffectSpawn/End cues (#350). Their world
//     position comes from the snapshot (resolved per frame via Update), so they
//     interpolate like any other entity.
//   - one-shots — transient bursts with a finite lifetime and a fixed spawn
//     position, auto-released on expiry.
//
// Both share one fixed pool (zero steady-state alloc, data-only — the renderer
// draws the active slots, same posture as the impact/aura pools). The eviction
// rule is the #309 FX rule: a contested slot evicts the OLDEST one-shot, never a
// persistent effect; if only persistents remain the request is denied. Eviction
// is cosmetic and never touches the sim.

// MaxScriptFX is the script-effect pool capacity. Billboards batch, so the slot
// count is generous relative to the FX draw-call budget (which the draw layer /
// counter #102 enforces separately).
const MaxScriptFX = 128

// ScriptFXDesc is the render descriptor for an effect billboard: which sprite,
// how big, what tint, and whether it also wants a pooled point light. The driver
// converts the sim effect store's fixed-point/packed fields to these at the seam
// (render imports no sim).
type ScriptFXDesc struct {
	Model    uint16       // sprite/model id (RenderEffectSpawn Data)
	Scale    float32      // billboard scale (world units)
	Color    math32.Color // tint
	HasLight bool         // also bind a pooled point light for this effect
}

// ScriptFXSlotInfo is a read-only snapshot of one pool slot for the render frame
// and the FSV dump. For a persistent slot Visible is false when its entity was
// missing at the last Update (despawned/culled). Remaining is -1 for persistent
// effects, the ticks left for one-shots.
type ScriptFXSlotInfo struct {
	Active     bool           `json:"active"`
	Persistent bool           `json:"persistent"`
	Key        uint32         `json:"key"`
	Pos        math32.Vector3 `json:"-"`
	Visible    bool           `json:"visible"`
	Scale      float32        `json:"scale"`
	Color      math32.Color   `json:"-"`
	Model      uint16         `json:"model"`
	HasLight   bool           `json:"hasLight"`
	Remaining  int32          `json:"remaining"`
	Handle     uint64         `json:"handle"`
}

// ScriptFXDecision records the outcome of a Spawn/OneShot, for the FSV log.
type ScriptFXDecision struct {
	Granted bool   `json:"granted"`
	Slot    int    `json:"slot"`   // -1 when refused
	Victim  int    `json:"victim"` // evicted one-shot slot, -1 when none
	Reason  string `json:"reason"`
}

type scriptFXSlot struct {
	active     bool
	persistent bool
	visible    bool
	key        uint32
	pos        math32.Vector3
	scale      float32
	color      math32.Color
	model      uint16
	hasLight   bool
	remaining  int32  // -1 = persistent
	order      uint64 // spawn sequence — oldest one-shot has the smallest
	handle     uint64
}

// ScriptFXPool is the fixed pool. The zero value is not usable — call
// NewScriptFXPool.
type ScriptFXPool struct {
	slots     [MaxScriptFX]scriptFXSlot
	nextID    uint64
	nextOrder uint64
}

// NewScriptFXPool returns an empty pool. No per-effect allocation happens after
// this — Spawn/End/OneShot/Tick/Update are all allocation-free.
func NewScriptFXPool() *ScriptFXPool { return &ScriptFXPool{} }

// Spawn registers (or updates) a persistent effect keyed by its entity id, from
// a RenderEffectSpawn cue. Re-spawning a live key updates its descriptor in
// place (idempotent). A new key takes a free slot, else evicts the oldest
// one-shot, else is refused (only persistents remain — never evict a persistent).
func (p *ScriptFXPool) Spawn(key uint32, d ScriptFXDesc) (uint64, ScriptFXDecision) {
	if key == 0 {
		return 0, ScriptFXDecision{Slot: -1, Victim: -1, Reason: "refused:zero-key"}
	}
	if i := p.findKey(key); i >= 0 {
		s := &p.slots[i]
		s.scale, s.color, s.model, s.hasLight = d.Scale, d.Color, d.Model, d.HasLight
		return s.handle, ScriptFXDecision{Granted: true, Slot: i, Victim: -1, Reason: "update"}
	}
	i, victim, reason := p.acquireSlot()
	if i < 0 {
		return 0, ScriptFXDecision{Slot: -1, Victim: -1, Reason: reason}
	}
	p.bind(i, true, key, d, -1)
	return p.slots[i].handle, ScriptFXDecision{Granted: true, Slot: i, Victim: victim, Reason: reason}
}

// End frees the persistent effect for key (a RenderEffectEnd cue). Returns false
// if no persistent slot holds it.
func (p *ScriptFXPool) End(key uint32) bool {
	if i := p.findKey(key); i >= 0 {
		p.free(i)
		return true
	}
	return false
}

// OneShot spawns a transient effect at pos with a finite lifetime (ticks). Takes
// a free slot, else evicts the oldest one-shot, else is refused (pool full of
// persistents). Fail-closed on a non-positive lifetime or scale.
func (p *ScriptFXPool) OneShot(pos math32.Vector3, d ScriptFXDesc, lifetime int32) (uint64, ScriptFXDecision) {
	if lifetime <= 0 {
		return 0, ScriptFXDecision{Slot: -1, Victim: -1, Reason: "refused:invalid-lifetime"}
	}
	if d.Scale <= 0 {
		return 0, ScriptFXDecision{Slot: -1, Victim: -1, Reason: "refused:invalid-scale"}
	}
	i, victim, reason := p.acquireSlot()
	if i < 0 {
		return 0, ScriptFXDecision{Slot: -1, Victim: -1, Reason: reason}
	}
	p.bind(i, false, 0, d, lifetime)
	p.slots[i].pos = pos
	p.slots[i].visible = true
	return p.slots[i].handle, ScriptFXDecision{Granted: true, Slot: i, Victim: victim, Reason: reason}
}

// findKey returns the slot index for a live persistent key, or -1.
func (p *ScriptFXPool) findKey(key uint32) int {
	for i := range p.slots {
		if p.slots[i].active && p.slots[i].persistent && p.slots[i].key == key {
			return i
		}
	}
	return -1
}

func (p *ScriptFXPool) freeSlot() int {
	for i := range p.slots {
		if !p.slots[i].active {
			return i
		}
	}
	return -1
}

// acquireSlot returns a slot for a new effect: a free slot, else the oldest
// active one-shot (evicted), else -1 (only persistents remain). The priority
// rule — persistents are never evicted.
func (p *ScriptFXPool) acquireSlot() (idx, victim int, reason string) {
	if i := p.freeSlot(); i >= 0 {
		return i, -1, "free-slot"
	}
	oldest := -1
	for i := range p.slots {
		s := &p.slots[i]
		if s.active && !s.persistent {
			if oldest < 0 || s.order < p.slots[oldest].order {
				oldest = i
			}
		}
	}
	if oldest < 0 {
		return -1, -1, "refused:all-persistent"
	}
	return oldest, oldest, "evict:oldest-oneshot"
}

func (p *ScriptFXPool) bind(i int, persistent bool, key uint32, d ScriptFXDesc, remaining int32) {
	p.nextID++
	p.nextOrder++
	s := &p.slots[i]
	s.active = true
	s.persistent = persistent
	s.visible = !persistent // one-shots are placed visible; persistents wait for Update
	s.key = key
	s.scale = d.Scale
	s.color = d.Color
	s.model = d.Model
	s.hasLight = d.HasLight
	s.remaining = remaining
	s.order = p.nextOrder
	s.handle = p.nextID
}

func (p *ScriptFXPool) free(i int) {
	s := &p.slots[i]
	s.active = false
	s.persistent = false
	s.visible = false
	s.key = 0
	s.handle = 0
}

// Tick advances one sim tick: decrement every one-shot's remaining lifetime,
// releasing any that reach zero. Persistent effects are untouched. Returns the
// number released.
func (p *ScriptFXPool) Tick() int {
	released := 0
	for i := range p.slots {
		s := &p.slots[i]
		if !s.active || s.persistent {
			continue
		}
		s.remaining--
		if s.remaining <= 0 {
			p.free(i)
			released++
		}
	}
	return released
}

// Update resolves each persistent effect's world position from its entity via
// lookup (the interpolated snapshot position). A persistent whose entity lookup
// misses is marked not visible (despawned/culled) but is NOT released — release
// is the RenderEffectEnd cue's job. One-shots keep their fixed spawn position.
// Allocation-free.
func (p *ScriptFXPool) Update(lookup func(key uint32) (math32.Vector3, bool)) {
	for i := range p.slots {
		s := &p.slots[i]
		if !s.active || !s.persistent {
			continue
		}
		pos, ok := lookup(s.key)
		if !ok {
			s.visible = false
			continue
		}
		s.pos = pos
		s.visible = true
	}
}

// ActiveCount returns the number of live effects (≤ MaxScriptFX always).
func (p *ScriptFXPool) ActiveCount() int {
	n := 0
	for i := range p.slots {
		if p.slots[i].active {
			n++
		}
	}
	return n
}

// PersistentCount returns the number of live persistent effects.
func (p *ScriptFXPool) PersistentCount() int {
	n := 0
	for i := range p.slots {
		if p.slots[i].active && p.slots[i].persistent {
			n++
		}
	}
	return n
}

// OneShotCount returns the number of live one-shot effects.
func (p *ScriptFXPool) OneShotCount() int { return p.ActiveCount() - p.PersistentCount() }

// LightCount returns how many live effects requested a point light — the input
// to the VFX light pool's per-frame budget.
func (p *ScriptFXPool) LightCount() int {
	n := 0
	for i := range p.slots {
		if p.slots[i].active && p.slots[i].hasLight {
			n++
		}
	}
	return n
}

// SnapshotInto fills dst with per-slot state, allocation-free. Returns dst
// resliced to MaxScriptFX.
func (p *ScriptFXPool) SnapshotInto(dst []ScriptFXSlotInfo) []ScriptFXSlotInfo {
	dst = dst[:0]
	for i := range p.slots {
		s := &p.slots[i]
		dst = append(dst, ScriptFXSlotInfo{
			Active: s.active, Persistent: s.persistent, Key: s.key, Pos: s.pos,
			Visible: s.visible, Scale: s.scale, Color: s.color, Model: s.model,
			HasLight: s.hasLight, Remaining: s.remaining, Handle: s.handle,
		})
	}
	return dst
}
