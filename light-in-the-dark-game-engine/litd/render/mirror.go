package render

// EntityID render mirror (#308): the bookkeeping half of the model-rendering
// pipeline. It maps each live sim entity to exactly one pooled render instance,
// keyed by the entity's stable per-slot key, and emits the spawn/despawn/model-
// change deltas the scene-graph layer applies (create a mesh, drop a mesh, swap a
// model). It owns NO GL and imports NO sim — it is pure identity↔slot bookkeeping
// over whatever (key, model) set the render driver hands it each frame, so it is
// headlessly testable (drive entry sequences, assert lifecycle counts) and the
// import-graph check (render ⊥ sim/GL) stays green.
//
// Why a mirror is needed: the published transform snapshot is SLOT-indexed and
// slots RECYCLE — when an entity dies, the next spawn can take its slot. A naive
// "one mesh per slot" would silently show the dead unit's model on the new
// occupant. The mirror keys on the stable entity key (not the raw slot), so a
// recycled slot whose key changed is a despawn-then-spawn (mesh rebuilt), and a
// surviving entity whose model changed (morph/upgrade) is an explicit model swap.
//
// Pooling: a despawned instance's slot returns to a free list and is reused by
// the next spawn, so a steady-state churn (one death + one spawn per tick) never
// grows the instance table and never allocates (R-GC-3). The render driver keys
// its own scene-node table by the instance Slot the deltas carry.

// ModelID identifies a render model (the asset the scene layer instantiates).
// Zero means "no model" — an entity with ModelNone is tracked for identity but
// the driver renders nothing for it.
type ModelID uint16

// ModelNone is the absent-model sentinel.
const ModelNone ModelID = 0

// MirrorEntry is one live entity to reflect this frame: its stable key and the
// model it should render as. Key 0 is invalid (the reserved entity slot) and is
// skipped, so a zero-value entry is a safe no-op.
type MirrorEntry struct {
	Key   uint32
	Model ModelID
}

// MirrorSpawn names a render instance the driver must create (or re-skin, on a
// model change): the entity Key, the Model to instantiate, and the pooled Slot to
// key the scene node by.
type MirrorSpawn struct {
	Key   uint32
	Model ModelID
	Slot  int
}

// MirrorDelta is one Sync's worth of scene-graph work. Despawned lists pooled
// slots whose mesh must be destroyed (the entity died or its model changed);
// Spawned lists slots whose mesh must be created (a new entity, or the rebuilt
// side of a model change). On a model change for a surviving key the SAME slot
// appears in both lists (destroy old mesh, build new) — the driver applies
// Despawned before Spawned. The slices are owned by the ModelMirror and reused
// across calls; copy out anything kept past the next Sync.
type MirrorDelta struct {
	Spawned   []MirrorSpawn
	Despawned []int
}

type mirrorInstance struct {
	key   uint32
	model ModelID
	live  bool   // occupies a slot this generation (false = on the free list)
	seen  uint32 // last Sync generation that referenced this instance
}

// ModelMirror tracks the live entity→instance mapping with a reuse pool.
type ModelMirror struct {
	byKey map[uint32]int // entity key -> index into inst
	inst  []mirrorInstance
	free  []int // indices of inst slots available for reuse

	gen        uint32 // bumped each Sync to detect instances not referenced this frame
	spawnBuf   []MirrorSpawn
	despawnBuf []int

	// lifetime counters (FSV observability — not gameplay state).
	created, reused, destroyed, swapped uint64
}

// NewModelMirror builds an empty mirror. hint pre-sizes the tables to the expected
// peak live-entity count so the warm-up does not realloc mid-match.
func NewModelMirror(hint int) *ModelMirror {
	if hint < 0 {
		hint = 0
	}
	return &ModelMirror{
		byKey:      make(map[uint32]int, hint),
		inst:       make([]mirrorInstance, 0, hint),
		free:       make([]int, 0, hint),
		spawnBuf:   make([]MirrorSpawn, 0, hint),
		despawnBuf: make([]int, 0, hint),
	}
}

// Sync reconciles the mirror to the live entity set and returns the scene-graph
// delta. Entries with Key 0 are ignored. An entry whose key is new takes a pooled
// slot (or a fresh one) and is Spawned; an entry whose key persists with the same
// model is left untouched (no delta — the steady-state, zero-alloc path); an
// entry whose key persists but whose model changed is Despawned then Spawned on
// its same slot; an instance whose key is absent from this frame is Despawned and
// its slot returned to the pool. Allocates nothing once warmed to the peak count.
func (m *ModelMirror) Sync(live []MirrorEntry) MirrorDelta {
	m.gen++
	m.spawnBuf = m.spawnBuf[:0]
	m.despawnBuf = m.despawnBuf[:0]

	// Pass 1 — mark survivors seen and rebuild any whose model changed in place.
	// New keys are left for pass 3 so their slots can come from pass 2's frees.
	for _, e := range live {
		if e.Key == 0 {
			continue // reserved/invalid entity — never mirrored
		}
		idx, ok := m.byKey[e.Key]
		if !ok {
			continue
		}
		in := &m.inst[idx]
		if in.model != e.Model {
			// Model change on a surviving entity: rebuild the mesh in place (same slot).
			m.despawnBuf = append(m.despawnBuf, idx)
			in.model = e.Model
			m.spawnBuf = append(m.spawnBuf, MirrorSpawn{Key: e.Key, Model: e.Model, Slot: idx})
			m.swapped++
		}
		in.seen = m.gen
	}

	// Pass 2 — despawn instances not referenced this generation, freeing their
	// slots BEFORE pass 3 so a death-and-spawn in the same frame reuses the slot.
	for idx := range m.inst {
		in := &m.inst[idx]
		if in.live && in.seen != m.gen {
			m.despawnBuf = append(m.despawnBuf, idx)
			delete(m.byKey, in.key)
			in.live = false
			in.key = 0
			in.model = ModelNone
			m.free = append(m.free, idx)
			m.destroyed++
		}
	}

	// Pass 3 — spawn the new keys, drawing from the just-freed pool.
	for _, e := range live {
		if e.Key == 0 {
			continue
		}
		if _, ok := m.byKey[e.Key]; ok {
			continue // already live (survivor or just-swapped)
		}
		idx := m.alloc(e.Key, e.Model)
		m.byKey[e.Key] = idx
		m.spawnBuf = append(m.spawnBuf, MirrorSpawn{Key: e.Key, Model: e.Model, Slot: idx})
	}

	return MirrorDelta{Spawned: m.spawnBuf, Despawned: m.despawnBuf}
}

// alloc returns a slot index for a new entity, reusing the free list when
// possible. The slot is marked live and stamped seen for this generation.
func (m *ModelMirror) alloc(key uint32, model ModelID) int {
	var idx int
	if n := len(m.free); n > 0 {
		idx = m.free[n-1]
		m.free = m.free[:n-1]
		m.reused++
	} else {
		idx = len(m.inst)
		m.inst = append(m.inst, mirrorInstance{})
		m.created++
	}
	m.inst[idx] = mirrorInstance{key: key, model: model, live: true, seen: m.gen}
	return idx
}

// Live returns the number of entities currently mirrored.
func (m *ModelMirror) Live() int { return len(m.byKey) }

// Slot returns the pooled instance slot backing key, and whether it is live.
func (m *ModelMirror) Slot(key uint32) (int, bool) {
	idx, ok := m.byKey[key]
	return idx, ok
}

// Stats reports cumulative lifecycle counts plus the current pool size — the SoT
// for the mirror's FSV (pool reuse means created stays flat under churn while
// reused climbs). PoolFree is the number of reclaimed slots awaiting reuse;
// Capacity is the high-water instance-table size.
type MirrorStats struct {
	Created, Reused, Destroyed, Swapped uint64
	Live, PoolFree, Capacity            int
}

// Stats snapshots the mirror's lifecycle counters.
func (m *ModelMirror) Stats() MirrorStats {
	return MirrorStats{
		Created:   m.created,
		Reused:    m.reused,
		Destroyed: m.destroyed,
		Swapped:   m.swapped,
		Live:      len(m.byKey),
		PoolFree:  len(m.free),
		Capacity:  len(m.inst),
	}
}
