package sim

// Per-unit custom value — WC3's GetUnitUserData/SetUnitUserData (#217).
// The dedicated UserDataStore is RETIRED (#571): the legacy API is now a
// thin shim over the generic key-value store (PRD2 03), using a reserved
// interned key under each unit's entity scope. This drops a bespoke store
// + its "userdata" hash sub + save block; the value now folds into the
// "kv" sub and serializes with everything else (R-KV-5). Behavior is
// unchanged — script bookkeeping, no sim consumer, deterministic, sparse
// (an absent unit reads 0), pruned with the entity on death (KV §8).

// kvUserDataKey is the reserved key string the userdata shim interns at
// world construction. Interning it first gives it a stable id (1) that is
// identical across a save/load (the keys table serializes in id order).
const kvUserDataKey = "__litd_userdata"

// UserData returns the unit's custom value, or 0 if none was ever set.
func (w *World) UserData(id EntityID) int32 {
	typ, val, _, ok := w.KV.KVGet(makeOwner(KVScopeEntity, uint64(id)), w.kvUserDataKey)
	if !ok || typ != KVInt {
		return 0
	}
	return int32(val)
}

// SetUserData assigns the unit's custom value. Returns false on a dead
// unit or a full KV store.
func (w *World) SetUserData(id EntityID, v int32) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	return w.KV.KVSet(makeOwner(KVScopeEntity, uint64(id)), w.kvUserDataKey, KVInt, int64(v), 0)
}
