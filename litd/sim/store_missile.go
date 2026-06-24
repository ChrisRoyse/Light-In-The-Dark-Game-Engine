package sim

// Missile vocabulary (#158, ADR #295). The first-class MissileStore was retired
// in #590 — a missile is now a mover-driven projectile (a body entity + a mover
// + a render-only ProjRender record; see missile.go / projectile_spawn.go). The
// flag / hit-mask / guidance / impact constants survive: they parameterize the
// public MissileSpec authoring surface and the spawn-time normalization that
// maps a spec onto a mover.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

// Missile flag bits.
const (
	// MissileAoE: when the guide target invalidates mid-flight the
	// missile continues to the last known position and detonates;
	// without it the missile expires payload-less (WC3's expire-vs-
	// AoE flag, generalized).
	MissileAoE uint8 = 1 << 0
	// MissileLinear: a skillshot — no guide target, flies along Dir to
	// RangeLeft, collision-tested against foes each advance (#331).
	// GuideEnt is unused; the swept window resolves hits.
	MissileLinear uint8 = 1 << 1
)

// Missile hit-mask bits. A zero mask normalizes to MissileHitEnemy,
// preserving the original linear-skillshot behavior.
const (
	MissileHitGround    uint16 = data.TargetGround
	MissileHitAir       uint16 = data.TargetAir
	MissileHitStructure uint16 = data.TargetStructure
	MissileHitEnemy     uint16 = 1 << 8
	MissileHitAlly      uint16 = 1 << 9

	MissileDefaultHitMask  uint16 = MissileHitEnemy
	MissileHitClassMask    uint16 = MissileHitGround | MissileHitAir | MissileHitStructure
	MissileHitRelationMask uint16 = MissileHitEnemy | MissileHitAlly
	MissileHitAllMask      uint16 = MissileHitClassMask | MissileHitRelationMask
)

// Built-in missile guidance/impact IDs. Zero means "infer from the
// legacy structural fields" at spawn time.
const (
	MissileGuidanceInfer uint16 = iota
	MissileGuidanceHoming
	MissileGuidancePoint
	MissileGuidanceLinear
)

const (
	MissileImpactInfer uint16 = iota
	MissileImpactDeliver
	MissileImpactDetonate
	MissileImpactPierce
	MissileImpactExpire
)
