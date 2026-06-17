package ai

import (
	"encoding/binary"
	"errors"
	"fmt"
)

// Attack-wave machinery (#278; jass-mapping/ai-natives.md attack-wave family;
// execution-model.md §6). The common.ai wave family — FormGroup /
// AttackMoveKill / SuicideUnit / SetAllianceTarget class — as a serializable
// wave lifecycle: compose → gather → launch → resolve.
//
// The AI assembles a wave's composition (unit-type quotas) from its own unit
// counts and stages a group at a gather point; once the survivors have formed
// (or the patience deadline elapses) the wave launches with an attack-move at
// the target. Member orders go through the sim's order system via WaveSource —
// there is NO AI-side pathfinding; the Order is the typed command and the sim
// does the work (R-EXEC-3).
//
// Determinism: member selection is entity-id order, and the formation check is
// integer world-unit squared-distance — float distance inside the determinism
// boundary would be a platform-rounding hazard. A gathering wave is plain data
// (roster + points + deadline), so it serializes directly and resumes to launch
// on the identical tick (no scheduler continuation to save).
//
// WC3 staging semantics, decided and documented:
//   - Members lost during gather are pruned from the roster; the wave launches
//     with the survivors when they form or the deadline hits (partial-wave
//     launch — WC3 does not wait forever for the dead).
//   - A wave whose roster empties before launch aborts (WaveDone, no launch).
//   - A stage that finds zero eligible units creates no wave (deterministic
//     no-op) — there is nothing to gather.
//   - A unit already committed to a gathering or launched wave is never drafted
//     into a second wave (disjoint membership).

// Wave lifecycle states.
const (
	WaveNone      uint8 = 0 // not a wave / invalid id
	WaveGathering uint8 = 1 // members ordered to the gather point, forming up
	WaveLaunched  uint8 = 2 // attack-move issued at the target
	WaveDone      uint8 = 3 // resolved or aborted (roster empty)
)

// Quota is one unit-type requirement of a wave composition: up to Count units
// of TypeID.
type Quota struct {
	TypeID int
	Count  int
}

// WaveSource is the read+order surface the WaveManager drives, satisfied by a
// sim adapter at the integration boundary (the methods take ints / integer
// world units; the sim's take EntityID / fixed-point). It exposes exactly:
// eligible-unit enumeration, member position/liveness, and the two typed orders
// a wave issues (move-to-gather, attack-move). No pathfinding, no mutation of
// AI state — orders are handed to the sim's order system.
type WaveSource interface {
	// EligibleUnits appends the player's living units of typeID to dst in
	// ascending entity-id order and returns the grown slice.
	EligibleUnits(player, typeID int, dst []int32) []int32
	// UnitPos returns a unit's position in integer world units and whether it is
	// alive. A dead/absent unit returns alive == false.
	UnitPos(id int32) (x, y int32, alive bool)
	// OrderMoveTo issues a move order toward (x,y) (the gather step).
	OrderMoveTo(id, x, y int32)
	// OrderAttackTo issues an attack-move order toward (x,y) (the launch step).
	OrderAttackTo(id, x, y int32)
}

// Wave is one staged group. A gathering wave is fully described by this data,
// so it serializes directly.
type Wave struct {
	ID               uint32
	Player           int32
	Members          []int32 // entity ids, ascending; dead members pruned each Tick
	GatherX, GatherY int32
	TargetX, TargetY int32
	State            uint8
	Deadline         uint32 // tick by which to launch even if not fully formed
}

// WaveManager stages and drives waves for the AI domain. Polled once per AI
// phase via Tick; holds no scheduler continuations.
type WaveManager struct {
	src             WaveSource
	formationRadius int32  // world units: a member is "formed" within this of the gather point
	gatherTicks     uint32 // patience: launch at stage-tick + gatherTicks even if not formed
	waves           []*Wave
	nextID          uint32

	scratch []int32 // reused eligible-unit buffer
}

// NewWaveManager builds a manager. formationRadius is in world units; a member
// within that distance of the gather point counts as formed. gatherTicks is the
// patience window after staging before a partial wave launches anyway.
func NewWaveManager(src WaveSource, formationRadius int32, gatherTicks uint32) *WaveManager {
	if formationRadius < 0 {
		formationRadius = 0
	}
	return &WaveManager{src: src, formationRadius: formationRadius, gatherTicks: gatherTicks, nextID: 1}
}

// isCommitted reports whether id already belongs to a gathering or launched
// wave — such a unit is never drafted into another wave (disjoint membership).
func (m *WaveManager) isCommitted(id int32) bool {
	for _, w := range m.waves {
		if w.State != WaveGathering && w.State != WaveLaunched {
			continue
		}
		for _, mem := range w.Members {
			if mem == id {
				return true
			}
		}
	}
	return false
}

// Stage composes a wave from quotas (selecting living, uncommitted units in
// entity-id order, up to each quota's count), records it as gathering, orders
// every member to the gather point, and returns the new wave id. Returns 0 and
// creates no wave when no eligible units are found (deterministic no-op).
func (m *WaveManager) Stage(player int, gatherX, gatherY, targetX, targetY int32, quotas []Quota, now uint32) uint32 {
	var members []int32
	for _, q := range quotas {
		if q.Count <= 0 {
			continue
		}
		m.scratch = m.src.EligibleUnits(player, q.TypeID, m.scratch[:0])
		picked := 0
		for _, id := range m.scratch { // ascending entity-id order
			if picked >= q.Count {
				break
			}
			if m.isCommitted(id) || containsID(members, id) {
				continue
			}
			members = append(members, id)
			picked++
		}
	}
	if len(members) == 0 {
		return 0 // nothing to gather — no wave
	}
	w := &Wave{
		ID:       m.nextID,
		Player:   int32(player),
		Members:  members,
		GatherX:  gatherX,
		GatherY:  gatherY,
		TargetX:  targetX,
		TargetY:  targetY,
		State:    WaveGathering,
		Deadline: now + m.gatherTicks,
	}
	m.nextID++
	m.waves = append(m.waves, w)
	for _, id := range members {
		m.src.OrderMoveTo(id, gatherX, gatherY)
	}
	return w.ID
}

// Tick advances every gathering wave: it prunes dead members, aborts a wave
// whose roster empties, and launches (attack-move at the target) once the
// surviving members are formed at the gather point or the patience deadline has
// passed. Launched and done waves are left to the sim to resolve; a launched
// wave that loses all members is marked done.
func (m *WaveManager) Tick(now uint32) {
	for _, w := range m.waves {
		switch w.State {
		case WaveGathering:
			w.Members = m.pruneDead(w.Members)
			if len(w.Members) == 0 {
				w.State = WaveDone // aborted: everyone died before launch
				continue
			}
			if m.formed(w) || now >= w.Deadline {
				for _, id := range w.Members {
					m.src.OrderAttackTo(id, w.TargetX, w.TargetY)
				}
				w.State = WaveLaunched
			}
		case WaveLaunched:
			w.Members = m.pruneDead(w.Members)
			if len(w.Members) == 0 {
				w.State = WaveDone
			}
		}
	}
}

// formed reports whether every (alive) member is within formationRadius of the
// gather point — integer squared-distance, fully deterministic.
func (m *WaveManager) formed(w *Wave) bool {
	r := int64(m.formationRadius)
	rr := r * r
	for _, id := range w.Members {
		x, y, alive := m.src.UnitPos(id)
		if !alive {
			return false // a not-yet-pruned dead member; next Tick prunes it
		}
		dx := int64(x - w.GatherX)
		dy := int64(y - w.GatherY)
		if dx*dx+dy*dy > rr {
			return false
		}
	}
	return true
}

// pruneDead returns members with dead/absent units removed, preserving order.
// Reuses the backing array (prune is in-place compaction).
func (m *WaveManager) pruneDead(members []int32) []int32 {
	out := members[:0]
	for _, id := range members {
		if _, _, alive := m.src.UnitPos(id); alive {
			out = append(out, id)
		}
	}
	return out
}

// ---- read surface for inspection / FSV ----

// ActiveWaves counts waves still gathering or launched.
func (m *WaveManager) ActiveWaves() int {
	n := 0
	for _, w := range m.waves {
		if w.State == WaveGathering || w.State == WaveLaunched {
			n++
		}
	}
	return n
}

// WaveCount returns the total number of waves the manager has ever staged that
// it still tracks (including done ones).
func (m *WaveManager) WaveCount() int { return len(m.waves) }

// WaveByID returns a read-only copy of the wave with the given id, ok=false if
// unknown. The Members slice is a fresh copy (caller may not mutate manager
// state through it).
func (m *WaveManager) WaveByID(id uint32) (Wave, bool) {
	for _, w := range m.waves {
		if w.ID == id {
			cp := *w
			cp.Members = append([]int32(nil), w.Members...)
			return cp, true
		}
	}
	return Wave{}, false
}

// WaveState returns the lifecycle state of a wave (WaveNone if unknown).
func (m *WaveManager) WaveState(id uint32) uint8 {
	for _, w := range m.waves {
		if w.ID == id {
			return w.State
		}
	}
	return WaveNone
}

// ---- serialization ----

var waveSaveMagic = [8]byte{'L', 'I', 'T', 'D', 'W', 'A', 'V', 'E'}

const waveSaveVersion uint16 = 1

var (
	errWaveMagic   = errors.New("ai: bad wave-manager save magic")
	errWaveVersion = errors.New("ai: unsupported wave-manager save version")
)

// Save serializes the manager's wave roster (the gathering/launched/done waves
// and the id counter) onto dst and returns the grown slice. A gathering wave is
// pure data, so its full launch context survives the round-trip.
func (m *WaveManager) Save(dst []byte) []byte {
	dst = append(dst, waveSaveMagic[:]...)
	dst = binary.LittleEndian.AppendUint16(dst, waveSaveVersion)
	dst = binary.LittleEndian.AppendUint32(dst, m.nextID)
	dst = binary.LittleEndian.AppendUint32(dst, uint32(len(m.waves)))
	for _, w := range m.waves {
		dst = binary.LittleEndian.AppendUint32(dst, w.ID)
		dst = binary.LittleEndian.AppendUint32(dst, uint32(w.Player))
		dst = append(dst, w.State)
		dst = binary.LittleEndian.AppendUint32(dst, uint32(w.GatherX))
		dst = binary.LittleEndian.AppendUint32(dst, uint32(w.GatherY))
		dst = binary.LittleEndian.AppendUint32(dst, uint32(w.TargetX))
		dst = binary.LittleEndian.AppendUint32(dst, uint32(w.TargetY))
		dst = binary.LittleEndian.AppendUint32(dst, w.Deadline)
		dst = binary.LittleEndian.AppendUint32(dst, uint32(len(w.Members)))
		for _, id := range w.Members {
			dst = binary.LittleEndian.AppendUint32(dst, uint32(id))
		}
	}
	return dst
}

// Load replaces the manager's roster from blob. Fail-closed: it parses into
// locals and validates the whole blob before touching live state, so a
// truncated or corrupt blob leaves the manager unchanged.
func (m *WaveManager) Load(blob []byte) error {
	const header = 8 + 2 + 4 + 4
	if len(blob) < header {
		return fmt.Errorf("ai: wave blob too short (%d bytes)", len(blob))
	}
	for i := range waveSaveMagic {
		if blob[i] != waveSaveMagic[i] {
			return errWaveMagic
		}
	}
	off := 8
	if v := binary.LittleEndian.Uint16(blob[off:]); v != waveSaveVersion {
		return fmt.Errorf("%w: %d (want %d)", errWaveVersion, v, waveSaveVersion)
	}
	off += 2
	nextID := binary.LittleEndian.Uint32(blob[off:])
	off += 4
	count := int(binary.LittleEndian.Uint32(blob[off:]))
	off += 4

	parsed := make([]*Wave, 0, count)
	for i := 0; i < count; i++ {
		const fixed = 4 + 4 + 1 + 4*5 + 4 // id,player,state,4 coords+deadline(5×4),memberCount
		if off+fixed > len(blob) {
			return fmt.Errorf("ai: truncated wave header at offset %d", off)
		}
		w := &Wave{}
		w.ID = binary.LittleEndian.Uint32(blob[off:])
		off += 4
		w.Player = int32(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		w.State = blob[off]
		off++
		w.GatherX = int32(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		w.GatherY = int32(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		w.TargetX = int32(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		w.TargetY = int32(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		w.Deadline = binary.LittleEndian.Uint32(blob[off:])
		off += 4
		n := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		if n < 0 || off+n*4 > len(blob) {
			return fmt.Errorf("ai: wave %d member list (%d) exceeds remaining bytes", w.ID, n)
		}
		w.Members = make([]int32, n)
		for j := 0; j < n; j++ {
			w.Members[j] = int32(binary.LittleEndian.Uint32(blob[off:]))
			off += 4
		}
		parsed = append(parsed, w)
	}
	// validated — commit
	m.waves = parsed
	m.nextID = nextID
	return nil
}

func containsID(s []int32, id int32) bool {
	for _, v := range s {
		if v == id {
			return true
		}
	}
	return false
}
