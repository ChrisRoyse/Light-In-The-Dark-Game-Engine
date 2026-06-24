package sim

// Unified motion controller — PRD2 05-movers (epic #548). ONE serializable
// fixed-point mover drives both units and projectiles across eight motion
// kinds (linear/homing/point/orbit/arc/spline/custom), carrying collision
// policy + an effect payload, so `cast → spawn → move → collide → effect`
// is composable data. Supersedes the straight-line-only MissileStore
// (folded in by #590). This file is the pool foundation (#582): the SoA
// store, the MoverID handle, the kind/policy enums, and the shared spline
// waypoint arena. The trig LUT (#583), advance loop (#584/#585), custom
// step (#586), collision (#587), authority (#588), completion (#589), and
// the missile migration + hash/save (#590) build on this layout.
//
// All motion state is fixed-point (fixed.F64/Vec2/Angle); no float ever
// (R-MOV-3).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// MoverID is the packed 32-bit mover handle: [ generation:8 | index:24 ].
// A stale handle resolves to a safe no-op (R-API-5). MoverID(0) is the
// invalid sentinel; slot 0 is reserved.
type MoverID uint32

func (m MoverID) Index() uint32     { return uint32(m) & 0x00FFFFFF }
func (m MoverID) Generation() uint8 { return uint8(m >> 24) }
func makeMoverID(index uint32, gen uint8) MoverID {
	return MoverID(uint32(gen)<<24 | index&0x00FFFFFF)
}

// MoverKind selects the motion model (mover-types.md).
type MoverKind uint8

const (
	MoverLinear    MoverKind = iota // straight line along Dir at Speed
	MoverHoming                     // turn toward Anchor each tick
	MoverPoint                      // move toward Goal, stop on arrival
	MoverOrbitUnit                  // circle around Anchor entity
	MoverOrbitPoint                 // circle around Goal point
	MoverArc                        // ballistic: parabola to Goal (gameplay z)
	MoverSpline                     // Catmull-Rom through waypoint span
	MoverCustom                     // a registered continuation computes the step
	moverKindCount
)

// MoverDoneMode is what happens when a mover's motion completes.
type MoverDoneMode uint8

const (
	MoverDoneExpire   MoverDoneMode = iota // free the mover (and consume projectile)
	MoverDoneLoop                          // restart the motion
	MoverDoneDetonate                      // fire the payload at the final position
	MoverDoneCont                          // invoke OnDone continuation
)

// Mover flag bits.
const (
	MoverAuthority uint8 = 1 << iota // owns the Target unit's transform (suspends pathing)
	MoverFlying                      // air pathing: ignores ground terrain collision
)

// MoverStore is the SoA pool of motion controllers over a shared spline
// waypoint arena. Columns are indexed by slot (1..cap; slot 0 reserved).
// All arrays are sized once at construction (R-GC-2).
type MoverStore struct {
	Kind   []uint8
	Target []EntityID // the transform this mover drives

	Anchor []EntityID   // orbit-unit anchor / homing target
	Goal   []fixed.Vec2 // point goal / orbit-point center / arc landing
	Dir    []fixed.Vec2 // linear direction (fixed unit vector)

	Speed     []fixed.F64
	Accel     []fixed.F64
	Radius    []fixed.F64 // orbit radius; also collision radius
	AngVel    []fixed.Angle
	Angle     []fixed.Angle
	RangeLeft []fixed.F64
	Height    []fixed.F64
	TurnRate  []fixed.Angle

	WpStart []int32
	WpLen   []int32
	WpParam []fixed.F64

	Cont   []uint16
	CState [][4]int64

	HitMask  []uint16
	Pierce   []int32
	Decay    []uint16
	Payload  []data.EffectList
	Packet   []DamagePacket
	OnDone   []uint16
	DoneMode []uint8
	Flags    []uint8

	Owner []EntityID
	Gen   []uint8
	live  []bool
	free  []int32
	count int32

	waypoints []fixed.Vec2 // shared spline arena
	wpCount   int32        // bump cursor into waypoints

	// steps binds custom-step ContIDs to functions (#586). Code, not
	// state: re-registered at setup, never serialized; lookup-by-id only
	// (no iteration in gameplay).
	steps map[uint16]MoverStepFunc

	// Dropped counts Create/AddWaypoints refused at capacity — hashed
	// state (#590) so a capacity divergence fails closed.
	Dropped uint32

	DebugAssert func(msg string, id MoverID)
}

// MoverSpec is the option set for Create — every motion kind fills the
// subset it needs; the zero value is a stationary linear mover. The api
// Move* verbs (#591) build this; the sim advance loop reads the columns.
type MoverSpec struct {
	Kind   MoverKind
	Target EntityID
	Anchor EntityID
	Goal   fixed.Vec2
	Dir    fixed.Vec2

	Speed     fixed.F64
	Accel     fixed.F64
	Radius    fixed.F64
	AngVel    fixed.Angle
	Angle     fixed.Angle
	RangeLeft fixed.F64
	Height    fixed.F64
	TurnRate  fixed.Angle

	WpStart int32
	WpLen   int32

	Cont   uint16
	CState [4]int64

	HitMask  uint16
	Pierce   int32
	Decay    uint16
	Payload  data.EffectList
	Packet   DamagePacket
	OnDone   uint16
	DoneMode MoverDoneMode
	Flags    uint8

	Owner EntityID
}

// NewMoverStore returns a pool of moverCap movers over a wpCap-slot spline
// waypoint arena.
func NewMoverStore(moverCap, wpCap int) *MoverStore {
	if moverCap <= 0 || moverCap >= 1<<24 {
		panic("sim: mover capacity must be in (0, 2^24)")
	}
	if wpCap < 0 {
		panic("sim: mover waypoint capacity must be >= 0")
	}
	n := moverCap + 1 // slot 0 reserved
	s := &MoverStore{
		Kind: make([]uint8, n), Target: make([]EntityID, n),
		Anchor: make([]EntityID, n), Goal: make([]fixed.Vec2, n), Dir: make([]fixed.Vec2, n),
		Speed: make([]fixed.F64, n), Accel: make([]fixed.F64, n), Radius: make([]fixed.F64, n),
		AngVel: make([]fixed.Angle, n), Angle: make([]fixed.Angle, n),
		RangeLeft: make([]fixed.F64, n), Height: make([]fixed.F64, n), TurnRate: make([]fixed.Angle, n),
		WpStart: make([]int32, n), WpLen: make([]int32, n), WpParam: make([]fixed.F64, n),
		Cont: make([]uint16, n), CState: make([][4]int64, n),
		HitMask: make([]uint16, n), Pierce: make([]int32, n), Decay: make([]uint16, n),
		Payload: make([]data.EffectList, n), Packet: make([]DamagePacket, n),
		OnDone: make([]uint16, n), DoneMode: make([]uint8, n), Flags: make([]uint8, n),
		Owner: make([]EntityID, n), Gen: make([]uint8, n), live: make([]bool, n),
		free:      make([]int32, 0, moverCap),
		waypoints: make([]fixed.Vec2, wpCap),
	}
	for i := moverCap; i >= 1; i-- {
		s.free = append(s.free, int32(i))
	}
	return s
}

// Cap is the number of usable mover slots (excludes reserved slot 0).
func (s *MoverStore) Cap() int { return len(s.live) - 1 }

// WaypointCap is the spline arena size.
func (s *MoverStore) WaypointCap() int { return len(s.waypoints) }

// Count is the number of live movers.
func (s *MoverStore) Count() int32 { return s.count }

// AddWaypoints appends pts to the shared spline arena and returns the
// (start,len) span for a MoverSpline. Returns ok=false (Dropped++) if the
// arena would overflow. Setup-time; the span is stable for the mover's life.
func (s *MoverStore) AddWaypoints(pts []fixed.Vec2) (start, length int32, ok bool) {
	if int(s.wpCount)+len(pts) > len(s.waypoints) {
		s.Dropped++
		return 0, 0, false
	}
	start = s.wpCount
	copy(s.waypoints[start:], pts)
	s.wpCount += int32(len(pts))
	return start, int32(len(pts)), true
}

// Waypoint returns the i-th point of a span (no bounds beyond the arena).
func (s *MoverStore) Waypoint(i int32) fixed.Vec2 { return s.waypoints[i] }

// Create allocates a mover from spec and returns its handle, or MoverID(0)
// (Dropped++) when the pool is exhausted. Zero alloc.
func (s *MoverStore) Create(spec MoverSpec) MoverID {
	n := len(s.free)
	if n == 0 {
		s.Dropped++
		return 0
	}
	r := s.free[n-1]
	s.free = s.free[:n-1]
	s.live[r] = true
	s.count++

	s.Kind[r] = uint8(spec.Kind)
	s.Target[r] = spec.Target
	s.Anchor[r] = spec.Anchor
	s.Goal[r] = spec.Goal
	s.Dir[r] = spec.Dir
	s.Speed[r] = spec.Speed
	s.Accel[r] = spec.Accel
	s.Radius[r] = spec.Radius
	s.AngVel[r] = spec.AngVel
	s.Angle[r] = spec.Angle
	s.RangeLeft[r] = spec.RangeLeft
	s.Height[r] = spec.Height
	s.TurnRate[r] = spec.TurnRate
	s.WpStart[r] = spec.WpStart
	s.WpLen[r] = spec.WpLen
	s.WpParam[r] = 0
	s.Cont[r] = spec.Cont
	s.CState[r] = spec.CState
	s.HitMask[r] = spec.HitMask
	s.Pierce[r] = spec.Pierce
	s.Decay[r] = spec.Decay
	s.Payload[r] = spec.Payload
	s.Packet[r] = spec.Packet
	s.OnDone[r] = spec.OnDone
	s.DoneMode[r] = uint8(spec.DoneMode)
	s.Flags[r] = spec.Flags
	s.Owner[r] = spec.Owner
	return makeMoverID(uint32(r), s.Gen[r])
}

// resolve maps a handle to its live slot, validating the generation.
func (s *MoverStore) resolve(id MoverID) (row int32, ok bool) {
	idx := id.Index()
	if idx == 0 || idx >= uint32(len(s.live)) {
		return 0, false
	}
	r := int32(idx)
	if !s.live[r] || s.Gen[r] != id.Generation() {
		return 0, false
	}
	return r, true
}

// Alive reports whether a handle refers to a live mover.
func (s *MoverStore) Alive(id MoverID) bool {
	_, ok := s.resolve(id)
	return ok
}

// Cancel frees a mover, bumping its generation so outstanding handles go
// stale. Idempotent. The spline waypoint span is NOT reclaimed (the arena
// is a bump allocator; spans are stable for the match — see #590 notes).
func (s *MoverStore) Cancel(id MoverID) bool {
	r, ok := s.resolve(id)
	if !ok {
		if s.DebugAssert != nil {
			s.DebugAssert("Cancel of stale/absent mover", id)
		}
		return false
	}
	s.live[r] = false
	s.Gen[r]++
	s.free = append(s.free, r)
	s.count--
	return true
}

// CancelOwnedBy frees every mover owned by a dying entity (R-MOV-10),
// called from the cleanup phase. Bounded by the dead list × live movers.
func (s *MoverStore) CancelOwnedBy(dead []EntityID) {
	if len(dead) == 0 || s.count == 0 {
		return
	}
	for r := int32(1); r < int32(len(s.live)); r++ {
		if !s.live[r] {
			continue
		}
		for _, d := range dead {
			if s.Owner[r] == d {
				s.live[r] = false
				s.Gen[r]++
				s.free = append(s.free, r)
				s.count--
				break
			}
		}
	}
}
