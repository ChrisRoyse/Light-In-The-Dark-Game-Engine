package sim

// Generic typed key-value store — PRD2 03-keyvalue-store. This file lands
// the foundation (#568): the tagged-union column layout, the packed Owner
// composite key, and the sorted-array binary-search core (locate / set /
// get / has / delete). Interning of key strings and string values (#569),
// the scoped public ops (#570), the UserDataStore retirement shim (#571),
// and hash/save (#572) build on this layout without changing it.
//
// The "map from (owner,key) to value" is realized as parallel arrays kept
// sorted by the composite (Owner, Key), searched by binary search and
// kept sorted by insert/delete shift. This gives O(log n) get, O(n)
// worst-case upsert, deterministic order by construction, and NO Go map
// in hashed state (R-KV-4).

// KVType is the closed value-type union (spec §1). Val/Val2 are raw int64
// bits; the type tag defines their interpretation. Only KVVec2 uses Val2.
type KVType uint8

const (
	KVInt    KVType = iota // int64 in Val
	KVFixed                // fixed.F64 bits in Val
	KVBool                 // 0/1 in Val
	KVString               // interned string-value id in Val (#569)
	KVEntity               // EntityID in Val
	KVVec2                 // Vec2.X in Val, Vec2.Y in Val2
	KVGroup                // GroupID in Val
	KVTimer                // TimerID in Val
	kvTypeCount
)

// KVScope is the owner namespace (spec §3), packed into the high byte of
// the Owner key so each owner's pairs stay contiguous in the sorted
// arrays (localized iteration + pruning).
type KVScope uint8

const (
	KVScopeEntity KVScope = iota // entityOrSlot = EntityID; pruned on death (R-KV-8)
	KVScopeGlobal                // entityOrSlot = 0; one shared namespace
	KVScopePlayer                // entityOrSlot = player slot
)

// makeOwner packs a scope + entity/slot into the composite owner key:
// [ scope:8 | reserved:8 | entityOrSlot:48 ].
func makeOwner(scope KVScope, entityOrSlot uint64) uint64 {
	return uint64(scope)<<56 | (entityOrSlot & 0x0000FFFFFFFFFFFF)
}

func ownerScope(o uint64) KVScope { return KVScope(o >> 56) }
func ownerEntity(o uint64) uint64 { return o & 0x0000FFFFFFFFFFFF }

// Exported owner-key builders for the public API (#573), which composes
// scoped KV access without reaching into the packing.
func EntityKVOwner(id EntityID) uint64  { return makeOwner(KVScopeEntity, uint64(id)) }
func GlobalKVOwner() uint64             { return makeOwner(KVScopeGlobal, 0) }
func PlayerKVOwner(slot uint8) uint64   { return makeOwner(KVScopePlayer, uint64(slot)) }

// internTable maps strings to stable 1-based uint32 ids (#569). Append-
// only within a match: an id, once assigned, never changes or recycles,
// so a serialized Key/string-value id resolves to the same string on
// load. The `list` (id order) is the serialized truth; `byStr` is a
// derived lookup index, rebuilt on load and used only for point lookups
// (never iterated in hashed/gameplay code), so it carries no determinism
// hazard. id 0 is the invalid/absent sentinel.
type internTable struct {
	list  []string          // list[id-1] = string
	byStr map[string]uint32 // string → id (derived)
}

func newInternTable() internTable {
	return internTable{byStr: make(map[string]uint32)}
}

// intern returns the id for s, assigning a fresh one on first sight.
func (t *internTable) intern(s string) uint32 {
	if id, ok := t.byStr[s]; ok {
		return id
	}
	t.list = append(t.list, s)
	id := uint32(len(t.list)) // 1-based
	t.byStr[s] = id
	return id
}

// lookup resolves an id to its string. ok=false for 0 or an out-of-range
// id (a defensive guard; a valid match never produces one).
func (t *internTable) lookup(id uint32) (string, bool) {
	if id == 0 || int(id) > len(t.list) {
		return "", false
	}
	return t.list[id-1], true
}

// id returns the id of s if already interned (0 if not). Read-only — does
// not assign, so a get-by-key on an unseen key string is a clean miss.
func (t *internTable) id(s string) uint32 { return t.byStr[s] }

func (t *internTable) count() int { return len(t.list) }

// reset clears the table to empty (load prelude).
func (t *internTable) reset() {
	t.list = t.list[:0]
	clear(t.byStr)
}

// rebuildIndex repopulates byStr from list after the list is restored
// (load). Ids are list-position + 1, matching intern's assignment.
func (t *internTable) rebuildIndex() {
	clear(t.byStr)
	for i, s := range t.list {
		t.byStr[s] = uint32(i + 1)
	}
}

// KVStore is the sorted-parallel-array key-value store (spec §1). All
// value arrays are sized once at construction (R-GC-2); count tracks the
// live prefix. Rows [0,count) are sorted ascending by (Owner, Key).
type KVStore struct {
	Owner []uint64 // packed scope+entity, primary sort key
	Key   []uint32 // interned key id, secondary sort key
	Type  []uint8  // KVType tag
	Val   []int64  // primary value bits
	Val2  []int64  // secondary value bits (Vec2.Y); 0 otherwise

	count int32

	// Dropped counts upserts refused because the store was full — hashed
	// state (#572) so a capacity divergence fails closed.
	Dropped uint32

	keys internTable // key strings → stable ids (#569), serialized
	strs internTable // KVString VALUES → stable ids (#569), serialized
}

// NewKVStore returns a store sized for cap pairs.
func NewKVStore(cap int) *KVStore {
	if cap <= 0 {
		panic("sim: kv capacity must be positive")
	}
	return &KVStore{
		Owner: make([]uint64, cap),
		Key:   make([]uint32, cap),
		Type:  make([]uint8, cap),
		Val:   make([]int64, cap),
		Val2:  make([]int64, cap),
		keys:  newInternTable(),
		strs:  newInternTable(),
	}
}

// InternKey returns the stable id for a key string, assigning one on
// first use. KeyString resolves an id back to its string.
func (s *KVStore) InternKey(key string) uint32        { return s.keys.intern(key) }
func (s *KVStore) KeyID(key string) uint32            { return s.keys.id(key) }
func (s *KVStore) KeyString(id uint32) (string, bool) { return s.keys.lookup(id) }

// InternStr interns a KVString VALUE, returning its stable id; StrValue
// resolves it back.
func (s *KVStore) InternStr(v string) uint32         { return s.strs.intern(v) }
func (s *KVStore) StrValue(id uint32) (string, bool) { return s.strs.lookup(id) }

// Cap is the maximum number of pairs the store can hold.
func (s *KVStore) Cap() int { return len(s.Owner) }

// Count is the number of live pairs.
func (s *KVStore) Count() int32 { return s.count }

// less reports whether (o1,k1) sorts before (o2,k2) in composite order.
func kvLess(o1 uint64, k1 uint32, o2 uint64, k2 uint32) bool {
	if o1 != o2 {
		return o1 < o2
	}
	return k1 < k2
}

// locate binary-searches for (owner,key). When found, idx is the row and
// ok is true. When absent, idx is the insertion point (the first row that
// sorts after the target) and ok is false. Branch-free of any map.
func (s *KVStore) locate(owner uint64, key uint32) (idx int32, ok bool) {
	lo, hi := int32(0), s.count // [lo, hi)
	for lo < hi {
		mid := lo + (hi-lo)/2
		if kvLess(s.Owner[mid], s.Key[mid], owner, key) {
			lo = mid + 1
		} else {
			hi = mid
		}
	}
	if lo < s.count && s.Owner[lo] == owner && s.Key[lo] == key {
		return lo, true
	}
	return lo, false
}

// set upserts (owner,key) → (typ,val,val2). An existing key is
// overwritten in place (last write wins, including a type change). A new
// key is inserted at its sorted slot (shift). Returns false and bumps
// Dropped if the store is full and the key is new. Zero-alloc.
func (s *KVStore) set(owner uint64, key uint32, typ KVType, val, val2 int64) bool {
	idx, ok := s.locate(owner, key)
	if ok {
		s.Type[idx] = uint8(typ)
		s.Val[idx] = val
		s.Val2[idx] = val2
		return true
	}
	if int(s.count) >= len(s.Owner) {
		s.Dropped++
		return false
	}
	// insert-shift: open a gap at idx, keeping rows sorted.
	copy(s.Owner[idx+1:s.count+1], s.Owner[idx:s.count])
	copy(s.Key[idx+1:s.count+1], s.Key[idx:s.count])
	copy(s.Type[idx+1:s.count+1], s.Type[idx:s.count])
	copy(s.Val[idx+1:s.count+1], s.Val[idx:s.count])
	copy(s.Val2[idx+1:s.count+1], s.Val2[idx:s.count])
	s.Owner[idx] = owner
	s.Key[idx] = key
	s.Type[idx] = uint8(typ)
	s.Val[idx] = val
	s.Val2[idx] = val2
	s.count++
	return true
}

// ---------------------------------------------------------------------
// Scoped public ops (#570). The owner is the packed composite key from
// makeOwner(scope, entityOrSlot); key is an interned id (InternKey). All
// are deterministic and zero-alloc. Stale/absent → safe zero/false.
// ---------------------------------------------------------------------

// KVSet upserts a typed value. Returns false (and bumps Dropped) only if
// the store is full and the key is new.
func (s *KVStore) KVSet(owner uint64, key uint32, typ KVType, val, val2 int64) bool {
	return s.set(owner, key, typ, val, val2)
}

// KVGet returns the stored type + value bits, ok=false when absent.
func (s *KVStore) KVGet(owner uint64, key uint32) (typ KVType, val, val2 int64, ok bool) {
	return s.get(owner, key)
}

// KVHas reports presence.
func (s *KVStore) KVHas(owner uint64, key uint32) bool { return s.has(owner, key) }

// KVDelete removes a pair, returning whether it was present.
func (s *KVStore) KVDelete(owner uint64, key uint32) bool { return s.del(owner, key) }

// ownerRange returns the contiguous row range [lo,hi) holding owner's
// pairs (empty when owner has none). Relies on the (Owner,Key) sort.
func (s *KVStore) ownerRange(owner uint64) (lo, hi int32) {
	lo, _ = s.locate(owner, 0) // insertion point at the owner's first key
	hi = lo
	for hi < s.count && s.Owner[hi] == owner {
		hi++
	}
	return lo, hi
}

// KVClearOwner drops every pair of owner in one shift (O(run) + the tail
// shift), keeping the arrays sorted. Returns the number removed.
func (s *KVStore) KVClearOwner(owner uint64) int {
	lo, hi := s.ownerRange(owner)
	n := hi - lo
	if n == 0 {
		return 0
	}
	copy(s.Owner[lo:s.count-n], s.Owner[hi:s.count])
	copy(s.Key[lo:s.count-n], s.Key[hi:s.count])
	copy(s.Type[lo:s.count-n], s.Type[hi:s.count])
	copy(s.Val[lo:s.count-n], s.Val[hi:s.count])
	copy(s.Val2[lo:s.count-n], s.Val2[hi:s.count])
	s.count -= n
	return int(n)
}

// KVEachOwner visits owner's pairs in key order. The range is snapshotted
// at entry, so fn must not insert/delete pairs of THIS owner mid-iteration
// (it may read freely and mutate other owners). Stale owner → no visits.
func (s *KVStore) KVEachOwner(owner uint64, fn func(key uint32, typ KVType, val, val2 int64)) {
	lo, hi := s.ownerRange(owner)
	for i := lo; i < hi; i++ {
		fn(s.Key[i], KVType(s.Type[i]), s.Val[i], s.Val2[i])
	}
}

// get returns the stored type + value bits for (owner,key), or ok=false
// when absent.
func (s *KVStore) get(owner uint64, key uint32) (typ KVType, val, val2 int64, ok bool) {
	idx, found := s.locate(owner, key)
	if !found {
		return 0, 0, 0, false
	}
	return KVType(s.Type[idx]), s.Val[idx], s.Val2[idx], true
}

// has reports whether (owner,key) is present.
func (s *KVStore) has(owner uint64, key uint32) bool {
	_, ok := s.locate(owner, key)
	return ok
}

// del removes (owner,key) by shift, keeping the arrays sorted. Returns
// true if the key was present.
func (s *KVStore) del(owner uint64, key uint32) bool {
	idx, ok := s.locate(owner, key)
	if !ok {
		return false
	}
	copy(s.Owner[idx:s.count-1], s.Owner[idx+1:s.count])
	copy(s.Key[idx:s.count-1], s.Key[idx+1:s.count])
	copy(s.Type[idx:s.count-1], s.Type[idx+1:s.count])
	copy(s.Val[idx:s.count-1], s.Val[idx+1:s.count])
	copy(s.Val2[idx:s.count-1], s.Val2[idx+1:s.count])
	s.count--
	return true
}
