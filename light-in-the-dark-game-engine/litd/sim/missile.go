package sim

// Missile flight and impact (#158, ADR #295). Missiles fly at the
// movement-phase tail, BEFORE the bucket reconcile — combat (phase 5)
// sees same-tick post-flight positions for units and missiles alike,
// and no new top-level phase exists.
//
// Per tick per missile: refresh the goal (homing missiles track
// GuideEnt and update GuidePt to its live position — the last known
// position when it dies), then either arrive (snap, execute payload,
// deferred kill) or advance one speed-step. A dead guide target
// resolves per the MissileAoE flag: continue-and-detonate at the last
// known point, or expire payload-less. The launcher dying changes
// nothing — homing delivery does not need a living source.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// MissileSpec parameterizes SpawnMissile. Target != 0 spawns a homing
// missile; Target == 0 flies to Point. Payload.Len > 0 selects the
// compiled effect list, else Packet delivers (rolled at launch).
//
// A linear skillshot sets Flags|MissileLinear and fills Dir / Range /
// Pierce / Decay instead of Target/Point: it flies along Dir for Range
// world units, colliding with foes of Source's team, delivering the
// payload per hit (per-mille Decay between hits) until Pierce hits or
// range is spent.
type MissileSpec struct {
	Pos        fixed.Vec2
	Source     EntityID
	Target     EntityID
	Point      fixed.Vec2
	Speed      fixed.F64 // world units per tick, must be positive
	Accel      fixed.F64 // world units per tick^2, non-negative; applied after each tick
	Arc        fixed.F64
	Flags      uint8
	HitMask    uint16 // MissileHit* bits; 0 = enemy-only, no class filter
	GuidanceID uint16 // MissileGuidance*; 0 infers from Target/Point/Linear
	ImpactID   uint16 // MissileImpact*; 0 infers from flags/linear fields
	Payload    data.EffectList
	Packet     DamagePacket

	// Linear skillshot fields (Flags&MissileLinear).
	Dir    fixed.Vec2 // flight direction (need not be normalized; SpawnMissile normalizes)
	Range  fixed.F64  // max travel before expiry, must be positive
	Pierce int32      // max hits (≥1); 1 = single-target skillshot
	Decay  uint16     // per-mille payload decay between pierce hits (0 → 1000 = none)
}

// EvMissileImpact fires when a missile delivers its payload — Src is
// the missile, Dst the victim/target (0 for a point or AoE detonation).
// EvMissileExpired fires when a missile dies without delivering — Src
// is the missile, Dst is 0. Both reach scripts through the OnEvent
// dispatcher (#293); they are emitted alongside the OnMissile*
// presentation callbacks.
const (
	EvMissileImpact  uint16 = 22
	EvMissileExpired uint16 = 23
)

// missileHitRadius is the lateral collision half-width of a linear
// skillshot against a unit centre (world units). v1 constant until a
// per-unit collision size joins the hit test (#334 seam).
const missileHitRadius = 40

// dirIntScale: linear collision projects in integer world units; the
// unit direction is scaled to this integer length so the projection
// keeps sub-unit resolution without F64 overflow (det §2.1).
const dirIntScale = 1024

// SpawnMissile creates a missile entity. Fails deterministically
// (0, false) on pool exhaustion or a bad spec — NEVER a silent
// fire-as-instant fallback (ecs §2). As of #590 a missile is a mover-driven
// projectile: SpawnMissile delegates to spawnMoverProjectile, which builds a
// body entity + mover + render-only ProjRender record. The legacy MissileStore
// machinery remains dormant (never populated) pending removal (#590 follow-up).
func (w *World) SpawnMissile(m MissileSpec) (EntityID, bool) {
	return w.spawnMoverProjectile(m)
}

// flightUnits is the whole-world-unit distance between two points — the
// render-only metric behind missile arc progress (#528). Coordinates are floored
// to whole units before squaring so the sum stays inside uint64 for any real map
// (a fixed-precision squared distance needs 128 bits; this render metric does
// not). Deterministic: integer ops + the sim's bit-exact SqrtU64.
func flightUnits(a, b fixed.Vec2) int64 {
	dx := a.X.Floor() - b.X.Floor()
	dy := a.Y.Floor() - b.Y.Floor()
	return int64(fixed.SqrtU64(uint64(dx*dx + dy*dy)))
}

func normalizeMissileGuidance(m MissileSpec) (uint16, bool) {
	if m.GuidanceID != MissileGuidanceInfer {
		switch m.GuidanceID {
		case MissileGuidanceHoming, MissileGuidancePoint, MissileGuidanceLinear:
			return m.GuidanceID, true
		default:
			return 0, false
		}
	}
	if m.Flags&MissileLinear != 0 {
		return MissileGuidanceLinear, true
	}
	if m.Target != 0 {
		return MissileGuidanceHoming, true
	}
	return MissileGuidancePoint, true
}

func normalizeMissileImpact(m MissileSpec, guidanceID uint16) (uint16, bool) {
	if m.ImpactID != MissileImpactInfer {
		switch m.ImpactID {
		case MissileImpactDeliver, MissileImpactDetonate, MissileImpactPierce, MissileImpactExpire:
			if m.ImpactID == MissileImpactPierce && guidanceID != MissileGuidanceLinear {
				return 0, false
			}
			return m.ImpactID, true
		default:
			return 0, false
		}
	}
	if guidanceID == MissileGuidanceLinear && m.Pierce > 1 {
		return MissileImpactPierce, true
	}
	if m.Flags&MissileAoE != 0 {
		return MissileImpactDetonate, true
	}
	return MissileImpactDeliver, true
}

func normalizeMissileHitMask(mask uint16) (uint16, bool) {
	if mask == 0 {
		return MissileDefaultHitMask, true
	}
	if mask&^MissileHitAllMask != 0 {
		return 0, false
	}
	if mask&MissileHitRelationMask == 0 {
		mask |= MissileHitEnemy
	}
	return mask, true
}

func (w *World) missileHitAllowed(target EntityID, sourceTeam uint8, mask uint16) bool {
	or := w.Owners.Row(target)
	if or == -1 {
		return false
	}
	sameTeam := w.Owners.Team[or] == sourceTeam
	if sameTeam {
		if mask&MissileHitAlly == 0 {
			return false
		}
	} else if mask&MissileHitEnemy == 0 {
		return false
	}
	classMask := mask & MissileHitClassMask
	if classMask == 0 {
		return true
	}
	return w.missileTargetClass(target)&classMask != 0
}

func (w *World) missileTargetClass(target EntityID) uint16 {
	if cr := w.Collisions.Row(target); cr != -1 {
		flags := w.Collisions.PathFlags[cr]
		if flags&PathBuild != 0 {
			return MissileHitStructure
		}
		if flags&PathAir != 0 {
			return MissileHitAir
		}
		if flags&PathGround != 0 {
			return MissileHitGround
		}
	}
	if ur := w.UnitTypes.Row(target); ur != -1 && int(w.UnitTypes.TypeID[ur]) < len(w.unitDefs) {
		def := &w.unitDefs[w.UnitTypes.TypeID[ur]]
		if def.Footprint > 0 || def.BuildTicks > 0 {
			return MissileHitStructure
		}
		if def.Pathing == data.PathingAir {
			return MissileHitAir
		}
		return MissileHitGround
	}
	return MissileHitGround
}

// DetonateMissile delivers a missile's payload at its current position
// immediately and removes it (deferred kill). Returns false on a
// non-missile or dead handle (R-API-5). Script-facing — Missile.Detonate
// (#293).
func (w *World) DetonateMissile(id EntityID) bool {
	if mr, ok := w.projMover(id); ok {
		// Mover-backed projectile (#590): deliver the payload once at the current
		// position, then consume the body — force DoneImpact so a linear/expire
		// projectile still delivers, matching impactMissile's immediate detonate.
		if !w.Ents.Alive(id) {
			return false
		}
		w.Movers.DoneMode[mr] = uint8(MoverDoneImpact)
		w.moverComplete(mr)
		return true
	}
	return false // not a live mover-backed projectile (#590)
}

// projMover resolves a projectile body entity to its driving mover row, or
// ok=false if id is not a live mover-backed projectile (#590).
func (w *World) projMover(id EntityID) (int32, bool) {
	pr := w.ProjRender.Row(id)
	if pr == -1 {
		return 0, false
	}
	return w.Movers.resolve(w.ProjRender.Mover[pr])
}

// ProjMover resolves a projectile body entity to its live mover row, for
// out-of-package readers (the api Missile wrapper). ok=false if the entity is
// not a live projectile body. Exported shim over projMover (#590).
func (w *World) ProjMover(id EntityID) (int32, bool) { return w.projMover(id) }

// ExpireMissile removes a missile payload-less and emits EvMissileExpired
// (plus the OnMissileExpire callback). Returns false on a non-missile or
// dead handle. Script-facing — Missile.Expire (#293).
func (w *World) ExpireMissile(id EntityID) bool {
	if mr, ok := w.projMover(id); ok {
		if !w.Ents.Alive(id) {
			return false
		}
		// DoneExpire -> moverComplete -> projectileExpire fires OnMissileExpire +
		// EvMissileExpired once (do not also fire here, #590 — that double-cued).
		w.Movers.DoneMode[mr] = uint8(MoverDoneExpire)
		w.moverComplete(mr)
		return true
	}
	return false // not a live mover-backed projectile (#590)
}

// SetMissileTarget retargets a missile mid-flight to home on target
// (refreshing the goal to its live position), or toward its current goal
// point when target == 0. Retargeting converts a linear skillshot into a
// guided missile. Returns false on a non-missile/dead handle or a dead
// target. Script-facing — Missile.SetTarget (#293).
func (w *World) SetMissileTarget(id, target EntityID) bool {
	if mr, ok := w.projMover(id); ok {
		if !w.Ents.Alive(id) || (target != 0 && !w.Ents.Alive(target)) {
			return false
		}
		ms := w.Movers
		ms.Flags[mr] &^= MoverSwept // a retargeted projectile is guided, not a skillshot
		ms.DoneMode[mr] = uint8(MoverDoneImpact)
		if target != 0 {
			ms.Kind[mr] = uint8(MoverHoming)
			ms.Anchor[mr] = target
			ms.TurnRate[mr] = 0
		} else {
			ms.Kind[mr] = uint8(MoverPoint)
			ms.Anchor[mr] = 0
			// A former skillshot has no goal point — aim at the current position
			// so it arrives rather than flying to the origin (degenerate legacy
			// linear->point case); a point mover keeps its existing Goal.
			if ms.Goal[mr] == (fixed.Vec2{}) {
				if tr := w.Transforms.Row(id); tr != -1 {
					ms.Goal[mr] = w.Transforms.Pos[tr]
				}
			}
		}
		return true
	}
	return false // not a live mover-backed projectile (#590)
}

func scalePermille(amt fixed.F64, permille uint16) fixed.F64 {
	return fixed.F64(int64(amt) * int64(permille) / 1000)
}

func minF(a, b fixed.F64) fixed.F64 {
	if a < b {
		return a
	}
	return b
}

func maxF(a, b fixed.F64) fixed.F64 {
	if a > b {
		return a
	}
	return b
}
