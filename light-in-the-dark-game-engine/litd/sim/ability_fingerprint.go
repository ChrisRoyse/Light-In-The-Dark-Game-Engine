package sim

// Ability content fingerprint — PRD2 06 (epic #549, #596). Two peers with
// different ability files must not desync silently (R-ABL-2): the compiled
// AbilitySpec content folds into the data fingerprint the existing save/load
// (and join) path already validates. A mismatch is rejected at load with a
// clear reason — never a silent divergence mid-match.
//
// The hash is order-sensitive: reordering ops or specs changes behavior, so
// it MUST change the fingerprint (ability-model.md §5). Names are included so
// two files differing only by id/name still mismatch. Runtime-derived wiring
// (the Block index assigned by RegisterSpec) is NOT hashed — it is reproduced
// identically at setup and carries no authored content.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"

// Fingerprint hashes every registered spec's content in registration order.
// Deterministic and platform-independent (fixed-width little-endian via the
// statehash writer). An empty book hashes a stable non-zero constant so an
// abilities-on world is distinguishable from one that never registered any.
func (bk *AbilityBook) Fingerprint() uint64 {
	h := statehash.New()
	h.WriteBytes([]byte("litd.abilities.v1"))
	h.WriteU32(uint32(len(bk.specs)))
	for i := range bk.specs {
		hashAbilitySpec(h, &bk.specs[i])
	}
	return h.Sum64()
}

func hashAbilitySpec(h *statehash.Hasher, s *AbilitySpec) {
	writeFPString(h, s.ID)
	writeFPString(h, s.Name)
	h.WriteU8(uint8(s.CastType))
	h.WriteU8(uint8(s.Indicator))
	h.WriteI64(int64(s.CastRange))
	h.WriteI64(int64(s.ManaCost))
	h.WriteU16(s.Cooldown)
	h.WriteU16(s.Precast)
	h.WriteU16(s.CastPoint)
	h.WriteU16(s.Backswing)
	hashAbilityOps(h, s.OnCast)
}

func hashAbilityOps(h *statehash.Hasher, ops []AbilityOp) {
	h.WriteU32(uint32(len(ops)))
	for i := range ops {
		op := &ops[i]
		h.WriteU8(uint8(op.Kind))
		h.WriteU8(uint8(op.MoverKind))
		h.WriteU16(op.EffectList.Off)
		h.WriteU16(op.EffectList.Len)
		h.WriteU16(op.EventKind)
		h.WriteU32(op.KeyID)
		h.WriteU16(op.Cont)
		h.WriteI64(int64(op.Speed))
		h.WriteI64(int64(op.Range))
		h.WriteI64(int64(op.Radius))
		h.WriteI64(int64(op.Amount))
		h.WriteI64(op.Arg)
		h.WriteI64(int64(op.Count))
		h.WriteU16(op.HitMask)
		h.WriteI64(int64(op.Pierce))
		h.WriteU16(uint16(op.AngVel))
		h.WriteU16(uint16(op.TurnRate))
		h.WriteI64(int64(op.Height))
		h.WriteU16(op.Decay)
		h.WriteU8(uint8(op.DoneMode))
		h.WriteU16(op.OnDone)
		h.WriteU32(uint32(len(op.Waypoints)))
		for _, wp := range op.Waypoints {
			h.WriteI64(int64(wp.X))
			h.WriteI64(int64(wp.Y))
		}
		hashAbilityOps(h, op.Children)
	}
}

// writeFPString hashes a length-prefixed string so "ab"+"c" cannot collide
// with "a"+"bc".
func writeFPString(h *statehash.Hasher, s string) {
	h.WriteU32(uint32(len(s)))
	h.WriteBytes([]byte(s))
}

// JoinFingerprint combines the bound data-table fingerprint with the ability
// content fingerprint into the single value a host passes to SaveState /
// LoadState (and any future network join). Two worlds that load different
// ability files therefore fail the existing fingerprint gate at load instead
// of desyncing. Requires SetDataFingerprint to have run.
func (w *World) JoinFingerprint() uint64 {
	return mixFingerprint(w.dataFingerprint, w.AbilityDefs.Fingerprint())
}

// mixFingerprint folds two 64-bit hashes order-dependently (a finalizing
// splitmix step) so the combined value depends on both halves and on which
// half is which.
func mixFingerprint(a, b uint64) uint64 {
	x := a ^ (b + 0x9e3779b97f4a7c15 + (a << 6) + (a >> 2))
	x ^= x >> 30
	x *= 0xbf58476d1ce4e5b9
	x ^= x >> 27
	x *= 0x94d049bb133111eb
	x ^= x >> 31
	return x
}
