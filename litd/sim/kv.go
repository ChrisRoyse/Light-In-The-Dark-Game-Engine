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

// KVStore is the sorted-parallel-array key-value store (spec §1). All
// arrays are sized once at construction (R-GC-2); count tracks the live
// prefix. Rows [0,count) are sorted ascending by (Owner, Key).
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
	}
}

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
